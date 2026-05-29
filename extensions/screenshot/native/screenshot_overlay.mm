// screenshot_overlay.mm
//
// 工业级截屏覆盖层背景实现：
//
// 现有 Tauri WebView 架构下，传统做法是把 CGImage 编码成 PNG/JPEG 写文件，
// 再让前端 <img> 加载——这条链路引入了"编码 + IPC + 网络协议解码 + 图像 paint"
// 的累计延迟（实测 80-450ms）。CleanShot X / Apple screencapture / Shottr 等
// 真正的工业级工具不走这条路：它们直接把 CGImage 喂给 CALayer，IOSurface 在
// 显存里零拷贝呈现，按下到显示稳定 20-30ms。
//
// 本桥让 Rust 把抓到的 CGImage 直接交给 NSWindow.contentView 的根 CALayer，
// WKWebView 在上层透明渲染交互 UI（选区、十字线、工具栏）。这样：
//
//   - 截屏背景：原生层呈现，零编码、零拷贝、零 IPC
//   - 选区/标注/OCR/工具栏：仍走 Vue（生态好，复用现有代码）
//
// 鼠标取色用的放大镜数据由 Rust 异步生成（capture 完成后再编码 raw RGBA），
// 不阻塞按键到显示路径。
//
// Rust 侧通过 extern "C" 调用本桥；为简化 ABI，所有指针都用 void* 传递。

#import <Foundation/Foundation.h>
#import <AppKit/AppKit.h>
#import <CoreGraphics/CoreGraphics.h>
#import <QuartzCore/QuartzCore.h>
#import <objc/runtime.h>

// ─────────────────────────────────────────────────────────────────────────────
// 安装：在 NSWindow.contentView 的 layer 下方插入 backgroundLayer。
//
// 调用时机：screenshot 窗口 setup 阶段（lib.rs setup 一次性 install）。
// 设计上 layer 复用——每次截屏只换 contents，不重新 attach。
//
// 流程：
//   1. contentView.wantsLayer = YES（Tauri 已设，幂等）
//   2. 创建一个 CALayer（filled-by-cgimage 模式）
//   3. 用 NSView associated objects 持有，避免被 ARC 回收
//   4. 把 layer 加到 contentView.layer 的 sublayers 最底层
//   5. WKWebView 已是 contentView 的 subview，会自然在 backgroundLayer 之上
//
// 失败时 NSLog 并返回 false。Rust 侧拿不到 CGImage 直贴能力时，会回退到
// 原 JPEG 编码路径（兼容兜底）。
// ─────────────────────────────────────────────────────────────────────────────

static const void *kBackgroundLayerKey = &kBackgroundLayerKey;

extern "C" bool voidnix_screenshot_install_background_layer(void *ns_window_ptr) {
    if (ns_window_ptr == NULL) {
        return false;
    }
    NSWindow *window = (__bridge NSWindow *)ns_window_ptr;
    NSView *content = window.contentView;
    if (content == nil) {
        NSLog(@"[shot-overlay] contentView is nil");
        return false;
    }

    // 已安装则跳过（幂等）
    CALayer *existing = (CALayer *)objc_getAssociatedObject(content, kBackgroundLayerKey);
    if (existing != nil) {
        return true;
    }

    @try {
        content.wantsLayer = YES;
        CALayer *root = content.layer;
        if (root == nil) {
            NSLog(@"[shot-overlay] root layer nil after wantsLayer=YES");
            return false;
        }

        CALayer *bg = [CALayer layer];
        // 全屏铺满，跟随父层 resize
        bg.frame = root.bounds;
        bg.autoresizingMask = kCALayerWidthSizable | kCALayerHeightSizable;
        // contentsGravity = resize 让 CGImage 拉伸到全屏（实际 CGImage 已经是
        // 屏幕物理像素尺寸，等比贴合，不会变形）
        bg.contentsGravity = kCAGravityResize;
        // 让 retina 屏自动按 backingScaleFactor 渲染
        CGFloat scale = window.screen.backingScaleFactor ?: 2.0;
        bg.contentsScale = scale;
        // 与 WKWebView 同帧呈现（避免选区遮罩出现 1 帧的偏移）
        bg.actions = @{
            @"contents": [NSNull null],
            @"position": [NSNull null],
            @"bounds": [NSNull null],
            @"hidden": [NSNull null],
        };
        // 隐藏，等 capture 完成、Rust 调 set_image 时再揭开
        bg.hidden = YES;

        // 关键：插到根 layer 的最底层（index 0），WKWebView 自然在其上
        [root insertSublayer:bg atIndex:0];

        // 用 associated object 持久持有
        objc_setAssociatedObject(content, kBackgroundLayerKey, bg, OBJC_ASSOCIATION_RETAIN_NONATOMIC);

        return true;
    } @catch (NSException *e) {
        NSLog(@"[shot-overlay] install exception: %@ - %@", e.name, e.reason);
        return false;
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 设置背景图：换 layer.contents = (id)cgImage。零编码、零拷贝。
//
// 调用时机：每次截屏 enter_impl 主线程。
// cgImage 由 Rust 持有（CFRetain），本函数 __bridge cast，不改变所有权。
//
// 关键优化：用 CATransaction 关闭隐式动画（contents 切换默认有 0.25s 淡入），
// 配合 layer.actions 设 NSNull 双重保险。
// ─────────────────────────────────────────────────────────────────────────────
extern "C" bool voidnix_screenshot_set_background(void *ns_window_ptr, void *cg_image_ptr) {
    if (ns_window_ptr == NULL || cg_image_ptr == NULL) {
        return false;
    }
    NSWindow *window = (__bridge NSWindow *)ns_window_ptr;
    NSView *content = window.contentView;
    if (content == nil) {
        return false;
    }
    CALayer *bg = (CALayer *)objc_getAssociatedObject(content, kBackgroundLayerKey);
    if (bg == nil) {
        NSLog(@"[shot-overlay] background layer not installed");
        return false;
    }

    @try {
        CGImageRef cg = (CGImageRef)cg_image_ptr;
        [CATransaction begin];
        [CATransaction setDisableActions:YES];
        bg.contents = (__bridge id)cg;
        bg.frame = content.layer.bounds;
        bg.hidden = NO;
        // 实测某些时序下 contents 设置后不立即 commit 会有 1 帧空白
        [CATransaction commit];
        [CATransaction flush];
        return true;
    } @catch (NSException *e) {
        NSLog(@"[shot-overlay] set_background exception: %@ - %@", e.name, e.reason);
        return false;
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 滚动截屏：让选区"洞"露出底下应用的实时画面，并提供窗口级 ignoresMouseEvents 切换。
//
// 实现：仅隐藏 backgroundLayer。窗口本身 transparent + DOM 端 4 块 mask div
// 覆盖在选区四周（已存在），所以选区内即"无 DOM 元素 + CALayer 透明" → 真正空洞。
// 注意：CAShapeLayer 路径在 Tauri WKWebView 的 contentView 下被遮，已弃用。
// ─────────────────────────────────────────────────────────────────────────────

extern "C" bool voidnix_screenshot_install_scroll_mask(
    void *ns_window_ptr,
    double sel_x, double sel_y, double sel_w, double sel_h
) {
    (void)sel_x; (void)sel_y; (void)sel_w; (void)sel_h;
    if (ns_window_ptr == NULL) {
        return false;
    }
    NSWindow *window = (__bridge NSWindow *)ns_window_ptr;
    NSView *content = window.contentView;
    if (content == nil) {
        return false;
    }
    @try {
        // 隐藏静态截屏背景，让底下应用的实时画面透出（窗口本身 transparent）
        CALayer *bg = (CALayer *)objc_getAssociatedObject(content, kBackgroundLayerKey);
        if (bg != nil) {
            [CATransaction begin];
            [CATransaction setDisableActions:YES];
            bg.hidden = YES;
            [CATransaction commit];
        }
        return true;
    } @catch (NSException *e) {
        NSLog(@"[shot-overlay] install_scroll_mask exception: %@ - %@", e.name, e.reason);
        return false;
    }
}

extern "C" void voidnix_screenshot_remove_scroll_mask(void *ns_window_ptr) {
    if (ns_window_ptr == NULL) {
        return;
    }
    NSWindow *window = (__bridge NSWindow *)ns_window_ptr;
    NSView *content = window.contentView;
    if (content == nil) {
        return;
    }
    @try {
        // 恢复静态背景
        CALayer *bg = (CALayer *)objc_getAssociatedObject(content, kBackgroundLayerKey);
        if (bg != nil) {
            [CATransaction begin];
            [CATransaction setDisableActions:YES];
            bg.hidden = NO;
            [CATransaction commit];
        }
        // ignoresMouseEvents 复位（无论之前是否被切换）
        [window setIgnoresMouseEvents:NO];
    } @catch (NSException *e) {
        NSLog(@"[shot-overlay] remove_scroll_mask exception: %@ - %@", e.name, e.reason);
    }
}

/// 设置窗口的 ignoresMouseEvents。dispatch_sync 到主线程：
/// 后台轮询线程必须等本次切换在 WindowServer 上落地，再继续抓帧/轮询，
/// 否则用户在选区里转滚轮可能会卡在"toggle 已发出但 NSWindow 状态未更新"的窗口里。
extern "C" void voidnix_screenshot_set_ignores_mouse(void *ns_window_ptr, int ignores) {
    if (ns_window_ptr == NULL) return;
    NSWindow *window = (__bridge NSWindow *)ns_window_ptr;
    BOOL flag = ignores ? YES : NO;
    void (^block)(void) = ^{
        NSLog(@"[shot-scroll] setIgnoresMouseEvents:%@ (window=%p)", flag ? @"YES" : @"NO", ns_window_ptr);
        [window setIgnoresMouseEvents:flag];
    };
    if ([NSThread isMainThread]) {
        block();
    } else {
        dispatch_sync(dispatch_get_main_queue(), block);
    }
}

/// 取系统鼠标坐标（CSS 像素，左上原点）。
/// 用 CoreGraphics 的 CGEventGetLocation：左上原点，无需翻转，跨多屏幕一致。
/// 通过 C helper 包装结构体返回，避免 Rust extern 的 CGPoint by-value ABI 问题。
extern "C" void voidnix_screenshot_get_mouse_location(
    double *out_x, double *out_y, double screen_height
) {
    (void)screen_height; // 已不再需要翻转
    if (out_x == NULL || out_y == NULL) return;
    CGEventRef ev = CGEventCreate(NULL);
    if (ev == NULL) {
        NSLog(@"[shot-scroll] CGEventCreate returned NULL!");
        *out_x = 0;
        *out_y = 0;
        return;
    }
    CGPoint p = CGEventGetLocation(ev);
    CFRelease(ev);
    *out_x = (double)p.x;
    *out_y = (double)p.y;
}

/// 抓取选区区域的屏幕内容（排除 overlay 窗口）。
///
/// 工业级做法（Shottr / CleanShot X 同款）：
/// 进入滚动模式时给窗口设 setSharingType:NSWindowSharingNone，
/// 之后 CGWindowListCreateImage 永远不会把我们的窗口合成进去。
/// 抓帧路径线程安全，不需要 dispatch_sync。
extern "C" void voidnix_screenshot_capture_region(
    void *ns_window_ptr,
    double sel_x, double sel_y, double sel_w, double sel_h,
    void **out_image
) {
    if (out_image == NULL) return;
    *out_image = NULL;
    (void)ns_window_ptr;

    CGRect rect = CGRectMake(sel_x, sel_y, sel_w, sel_h);
    CGImageRef img = CGWindowListCreateImage(
        rect,
        kCGWindowListOptionOnScreenOnly,
        kCGNullWindowID,
        kCGWindowImageDefault
    );
    if (img != NULL) {
        *out_image = (void *)img;
    }
}

/// 滚动截屏期间把窗口标记为"不可被截屏"（NSWindowSharingNone）。
/// 进入滚动模式调一次，退出滚动模式恢复 NSWindowSharingReadOnly。
/// 必须主线程。
extern "C" void voidnix_screenshot_set_sharing(void *ns_window_ptr, int sharing) {
    if (ns_window_ptr == NULL) return;
    NSWindow *window = (__bridge NSWindow *)ns_window_ptr;
    NSWindowSharingType type = (sharing == 0)
        ? NSWindowSharingNone
        : NSWindowSharingReadOnly;
    void (^block)(void) = ^{
        [window setSharingType:type];
    };
    if ([NSThread isMainThread]) {
        block();
    } else {
        dispatch_sync(dispatch_get_main_queue(), block);
    }
}

/// 返回截屏 NSWindow 的 windowNumber（CGWindowListCreateImage 排除自身用）。
extern "C" long voidnix_screenshot_window_number(void *ns_window_ptr) {
    if (ns_window_ptr == NULL) {
        return 0;
    }
    NSWindow *window = (__bridge NSWindow *)ns_window_ptr;
    return (long)window.windowNumber;
}

// ─────────────────────────────────────────────────────────────────────────────
// 隐藏背景：退出截屏时调，layer.contents = nil 释放显存引用。
// ─────────────────────────────────────────────────────────────────────────────
extern "C" void voidnix_screenshot_clear_background(void *ns_window_ptr) {
    if (ns_window_ptr == NULL) {
        return;
    }
    NSWindow *window = (__bridge NSWindow *)ns_window_ptr;
    NSView *content = window.contentView;
    if (content == nil) {
        return;
    }
    CALayer *bg = (CALayer *)objc_getAssociatedObject(content, kBackgroundLayerKey);
    if (bg == nil) {
        return;
    }
    @try {
        [CATransaction begin];
        [CATransaction setDisableActions:YES];
        bg.hidden = YES;
        bg.contents = nil;
        [CATransaction commit];
    } @catch (NSException *e) {
        NSLog(@"[shot-overlay] clear_background exception: %@ - %@", e.name, e.reason);
    }
}
