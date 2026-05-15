// webkit_tuning.mm
// 为 Voidnix 的 macOS 原生外壳提供 WKWebView 驯化所需的 SPI 调用与 Obj-C
// 异常拦截桥。Rust 侧在 src-tauri/src/webkit_tuning/ 通过 extern "C" 调用。

#import <Foundation/Foundation.h>
#import <AppKit/AppKit.h>
#import <WebKit/WebKit.h>
#include <stdbool.h>
#include <stdint.h>
#include <mach/mach_time.h>

// 在 Obj-C @try/@catch 内同步执行回调。
// 任何 Obj-C 异常（含 NSException 与少见的非 NSException 抛出物）被吞掉并 NSLog，
// 返回 false；正常返回 true。Rust 侧的 obj_exception::try_block 通过 C 函数指针 +
// 裸 ctx 把 FnOnce 桥到这里，避免跨 FFI 传递 Obj-C block 的 ABI 不确定性。
// trampoline 使用 extern "C-unwind" 允许 Obj-C 异常（C++ 异常机制）穿越 Rust 函数
// 边界，由此处的 @catch 捕获。
extern "C" bool voidnix_try_block(void (*fn)(void *), void *ctx) {
    if (fn == NULL) {
        return true;
    }
    @try {
        fn(ctx);
        return true;
    } @catch (NSException *e) {
        NSLog(@"[webkit_tuning] caught Obj-C exception: %@ - %@", e.name, e.reason);
        return false;
    } @catch (...) {
        NSLog(@"[webkit_tuning] caught non-NSException throw");
        return false;
    }
}

// 仅供 PBT 注入异常用：
//   kind = 0 不抛
//   kind = 1 NSGenericException
//   kind = 2 NSInvalidArgumentException
//   kind = 3 自定义 VoidnixCustomException
// 始终编译进静态库；release 不引用，profile.release.strip 会自然清理。
extern "C" void voidnix_test_throw(int kind) {
    switch (kind) {
        case 1:
            @throw [NSException exceptionWithName:NSGenericException
                                           reason:@"voidnix_test_throw generic"
                                         userInfo:nil];
        case 2:
            @throw [NSException exceptionWithName:NSInvalidArgumentException
                                           reason:@"voidnix_test_throw invalid argument"
                                         userInfo:nil];
        case 3:
            @throw [NSException exceptionWithName:@"VoidnixCustomException"
                                           reason:@"voidnix_test_throw custom"
                                         userInfo:nil];
        case 0:
        default:
            break;
    }
}

// _doAfterNextPresentationUpdate: 桥（T8 实装）。
// 通过 SPI selector 探测 + performSelector 调用 WKWebView 的私有方法，
// 并附带 dispatch_after 超时兜底。使用 __block bool fired + @synchronized
// 保证 once 语义，整段包在 @try/@catch 内。
extern "C" bool voidnix_do_after_next_presentation_update(
    WKWebView *web,
    NSWindow *window,
    uint64_t timeout_ms,
    void (^cb)(bool ok)) {
    (void)window;
    if (web == nil || cb == nil) {
        return false;
    }
    SEL sel = NSSelectorFromString(@"_doAfterNextPresentationUpdate:");
    if (![web respondsToSelector:sel]) {
        return false;
    }
    __block bool fired = false;
    void (^onceCb)(bool) = ^(bool ok) {
        @synchronized (web) {
            if (fired) return;
            fired = true;
        }
        cb(ok);
    };
    @try {
        [web performSelector:sel withObject:^{ onceCb(true); }];
    } @catch (NSException *e) {
        NSLog(@"[webkit_tuning] _doAfterNextPresentationUpdate threw: %@", e);
        return false;
    }
    dispatch_after(
        dispatch_time(DISPATCH_TIME_NOW, (int64_t)timeout_ms * NSEC_PER_MSEC),
        dispatch_get_main_queue(),
        ^{ onceCb(false); }
    );
    return true;
}

// C 函数指针版本：供 RealPresentationBridge 使用，避免 block2 跨 FFI 的 ABI 问题。
// cb_fn(ctx, ok) 在 presentation update 完成或超时时被调用一次。
extern "C" bool voidnix_do_after_next_presentation_update_fn(
    WKWebView *web,
    NSWindow *window,
    uint64_t timeout_ms,
    void (*cb_fn)(void *ctx, bool ok),
    void *ctx) {
    (void)window;
    if (web == nil || cb_fn == NULL) {
        return false;
    }
    SEL sel = NSSelectorFromString(@"_doAfterNextPresentationUpdate:");
    if (![web respondsToSelector:sel]) {
        return false;
    }
    __block bool fired = false;
    void (^onceCb)(bool) = ^(bool ok) {
        @synchronized (web) {
            if (fired) return;
            fired = true;
        }
        cb_fn(ctx, ok);
    };
    @try {
        [web performSelector:sel withObject:^{ onceCb(true); }];
    } @catch (NSException *e) {
        NSLog(@"[webkit_tuning] _doAfterNextPresentationUpdate_fn threw: %@", e);
        return false;
    }
    dispatch_after(
        dispatch_time(DISPATCH_TIME_NOW, (int64_t)timeout_ms * NSEC_PER_MSEC),
        dispatch_get_main_queue(),
        ^{ onceCb(false); }
    );
    return true;
}

// 系统 emoji 字体预热（T9 实装）。
// 把若干 emoji 探针绘制到 1×1 离屏 NSBitmapImageRep，触发 CoreText 字体加载，
// 使后续首次渲染不出现字体回退停顿（Req 4.3）。
// 分片执行：每片用 mach_absolute_time 自查，超过 8ms 则 dispatch_async 让出主线程（Req 4.2）。
// 最外层 @try/@catch 兜底，任何异常不向上传播（Req 4.4）。
extern "C" void voidnix_warm_emoji_font(void) {
    @try {
        NSArray<NSString *> *probes = @[
            @"😀", @"👋🏽", @"👨‍👩‍👧‍👦", @"🇨🇳", @"🧑‍💻", @"❤️", @"🎉"
        ];
        NSDictionary *attrs = @{
            NSFontAttributeName: [NSFont systemFontOfSize:14.0]
        };

        // 创建 1×1 离屏画布
        NSBitmapImageRep *rep = [[NSBitmapImageRep alloc]
            initWithBitmapDataPlanes:NULL
                          pixelsWide:1
                          pixelsHigh:1
                       bitsPerSample:8
                     samplesPerPixel:4
                            hasAlpha:YES
                            isPlanar:NO
                      colorSpaceName:NSDeviceRGBColorSpace
                         bytesPerRow:0
                        bitsPerPixel:32];
        if (rep == nil) {
            return;
        }

        NSGraphicsContext *ctx = [NSGraphicsContext graphicsContextWithBitmapImageRep:rep];
        if (ctx == nil) {
            return;
        }

        [NSGraphicsContext saveGraphicsState];
        [NSGraphicsContext setCurrentContext:ctx];

        // 分片绘制：每个 emoji 一片，超过 8ms 则 dispatch_async 让出主线程继续
        __block NSUInteger idx = 0;
        // 用 __block void (^drawNext)(void) 实现递归 block
        __block __weak void (^weakDrawNext)(void);
        void (^drawNext)(void) = ^{
            if (idx >= probes.count) {
                [NSGraphicsContext restoreGraphicsState];
                return;
            }
            uint64_t start = mach_absolute_time();
            while (idx < probes.count) {
                @try {
                    [probes[idx] drawAtPoint:NSZeroPoint withAttributes:attrs];
                } @catch (...) {}
                idx++;
                // 检查是否超过 8ms（mach_absolute_time 在 Apple Silicon 上 1 tick ≈ 1ns）
                uint64_t elapsed = mach_absolute_time() - start;
                if (elapsed > 8000000ULL) { // 8ms in nanoseconds
                    void (^strongDrawNext)(void) = weakDrawNext;
                    if (strongDrawNext) {
                        dispatch_async(dispatch_get_main_queue(), strongDrawNext);
                    }
                    return;
                }
            }
            [NSGraphicsContext restoreGraphicsState];
        };
        weakDrawNext = drawNext;
        drawNext();
    } @catch (...) {
        NSLog(@"[webkit_tuning] voidnix_warm_emoji_font: caught exception");
    }
}
