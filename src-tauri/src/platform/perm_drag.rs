// 录屏等手动添加档权限的拖拽指引浮窗：独立原生小窗（NonactivatingPanel + 浮动
// 层级，悬浮于设置窗口底部外侧），显示应用图标 + 两行说明，图标即原生拖拽源
//（NSPasteboardTypeFileURL——WKWebView 内 HTML5 拖拽无法跨应用携带 file URL），
// 拖入设置列表等价点 + 添加。图标高度与两行文本高度一致（由 label sizeToFit 测得
// 动态决定）；文案由前端传参承载 i18n（含 \n 换行），显隐随授权会话驱动
//（startPermGrant / perm-session 起，perm-flow 终）。

use std::sync::Mutex;

use objc2::msg_send;
#[allow(deprecated)] // 0.6 中 retained 返回仍需 msg_send_id（msg_send 自动转换是 0.7 预告）
use objc2::msg_send_id;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObjectProtocol};
use objc2::MainThreadOnly;

use objc2_app_kit::{
    NSBackingStoreType, NSColor, NSDragOperation, NSDraggingItem, NSDraggingSource, NSEvent,
    NSFont, NSImageView, NSPanel, NSPasteboardItem, NSTextField, NSView, NSVisualEffectView,
    NSWindowCollectionBehavior, NSWindowStyleMask, NSWorkspace,
};
use objc2_foundation::{MainThreadMarker, NSArray, NSPoint, NSRect, NSSize, NSString, NSURL};

use crate::runtime::lock_or_recover;

const PAD: f64 = 12.0;
const GAP: f64 = 10.0;
/// 浮窗顶边距设置窗口底边的外侧间距
const GAP_OUTER: f64 = 8.0;
/// 系统设置左侧分类栏宽度估值（无 AX 读不到内部布局）：浮窗对准其右侧内容区
/// （「上方列表」所在），避免压住分类栏。
const SIDEBAR_EST: f64 = 220.0;

/// 已显示浮窗（裸指针承载 Retained，创建/关闭全在主线程）。panel 与 label 各持
/// 一份 into_raw 引用（label 另由视图树持有），hide 时显式引用先于 panel close
/// 释放，视图树那份随窗口 dealloc 回收。
struct HintPanel {
    panel: *mut AnyObject,
    label: *mut AnyObject,
}
unsafe impl Send for HintPanel {}

static PANEL: Mutex<Option<HintPanel>> = Mutex::new(None);

/// show/hide 代数：hide 递增；show 线程建窗前在主线程复核，换代即放弃——防 hide
/// 先到时（hide_on_main 落空）浮窗随后创建、无人收起的残窗。
static DRAG_GEN: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// 图标热区（= 拖拽热区）：正方形，边长 = 视图高 − 上下内边距（即两行文本高度），
/// 由视图 bounds 动态推导（浮窗高度随文案行数变化，勿用常量）。
fn icon_frame_of(view: &NSView) -> NSRect {
    let side = (view.bounds().size.height - PAD * 2.0).max(16.0);
    NSRect::new(NSPoint::new(PAD, PAD), NSSize::new(side, side))
}

/// mouseDragged：图标热区内的拖动开启原生拖拽会话；文字/空白区域不触发。
unsafe fn begin_drag(view: &NSView, ev: &NSEvent) {
    let f = icon_frame_of(view);
    let loc = ev.locationInWindow();
    let in_icon = loc.x >= f.origin.x
        && loc.x < f.origin.x + f.size.width
        && loc.y >= f.origin.y
        && loc.y < f.origin.y + f.size.height;
    if !in_icon {
        return;
    }
    let Some(bundle) = crate::platform::permission::app_bundle_path() else {
        return;
    };
    let mtm = MainThreadMarker::new_unchecked();
    let path_ns = NSString::from_str(&bundle.to_string_lossy());

    // 拖拽载荷：.app bundle 的 file URL
    let item = NSPasteboardItem::new();
    let url = NSURL::fileURLWithPath(&path_ns);
    let Some(url_str) = url.absoluteString() else {
        return;
    };
    item.setString_forType(&url_str, objc2_app_kit::NSPasteboardTypeFileURL);

    // 拖拽预览 = 热区同尺寸应用图标
    let icon = NSWorkspace::sharedWorkspace().iconForFile(&path_ns);
    icon.setSize(f.size);
    #[allow(deprecated)]
    let drag: Retained<NSDraggingItem> = msg_send_id![
        mtm.alloc::<NSDraggingItem>(),
        initWithPasteboardWriter: &*item
    ];
    let _: () = msg_send![&drag, setDraggingFrame: f, contents: &*icon];

    let items = NSArray::arrayWithObject(&*drag);
    let _: bool = msg_send![
        view,
        beginDraggingSessionWithItems: &*items,
        event: ev,
        source: view
    ];
}

objc2::define_class!(
    #[unsafe(super = NSView)]
    #[name = "VoidnixPermDragView"]
    /// 拖拽源视图：图标热区 mouseDragged 开启会话，自身作为 NSDraggingSource。
    struct PermDragView;

    impl PermDragView {
        #[unsafe(method(mouseDragged:))]
        fn mouse_dragged(&self, event: &NSEvent) {
            unsafe { begin_drag(self, event) }
        }
    }

    unsafe impl NSObjectProtocol for PermDragView {}

    unsafe impl NSDraggingSource for PermDragView {
        #[unsafe(method(draggingSession:sourceOperationMaskForDraggingContext:))]
        fn drag_mask(&self, _session: &AnyObject, _context: i64) -> NSDragOperation {
            NSDragOperation::Copy
        }
    }
);

/// 浮窗 frame：宽 = 内边距 + 图标(=文本高) + 间距 + 文本宽 + 内边距（文本宽钳进
/// 设置窗宽）；水平对准设置窗内容区（左分类栏右侧）中心，垂直位于设置窗底部
/// 外侧（GAP_OUTER 间距）；水平垂直均钳进设置所在屏（多屏翻转误差的防御性兜底）。
/// text_w / text_h 由 label sizeToFit 测得（两行文本）。
fn panel_frame(mtm: MainThreadMarker, text_w: f64, text_h: f64, settings: NSRect) -> NSRect {
    let text_w = text_w.max(0.0).min(settings.size.width - 24.0).max(120.0);
    let win_h = text_h + PAD * 2.0;
    let w = PAD + text_h + GAP + text_w + PAD;
    let content_x0 = settings.origin.x + SIDEBAR_EST;
    let content_x1 = settings.origin.x + settings.size.width;
    let mut x = content_x0 + ((content_x1 - content_x0) - w) / 2.0;
    if x + w > content_x1 {
        x = content_x1 - w;
    }
    if x < content_x0 {
        x = content_x0;
    }
    let mut y = settings.origin.y - GAP_OUTER - win_h;
    let cx = settings.origin.x + settings.size.width / 2.0;
    let cy = settings.origin.y + settings.size.height / 2.0;
    if let Some(vis) = crate::platform::window::screen_vis_containing(mtm, cx, cy) {
        x = x.clamp(
            vis.origin.x,
            (vis.origin.x + vis.size.width - w).max(vis.origin.x),
        );
        let max_y = (vis.origin.y + vis.size.height - win_h).max(vis.origin.y);
        y = y.clamp(vis.origin.y, max_y);
    }
    NSRect::new(NSPoint::new(x, y), NSSize::new(w, win_h))
}

/// 两行说明 label（12pt system，文案自带 \n 换行；sizeToFit 测得整块尺寸——
/// 宽 = 最长行、高 = 行数 × 行高）。
fn make_label(mtm: MainThreadMarker, text_ns: &NSString) -> Retained<NSTextField> {
    let label = NSTextField::labelWithString(text_ns, mtm);
    let font = NSFont::systemFontOfSize(12.0);
    label.setFont(Some(&font));
    label.setLineBreakMode(objc2_app_kit::NSLineBreakMode::ByWordWrapping);
    label.sizeToFit();
    label
}

/// 显示/更新浮窗（仅主线程）。已显示则更新文案与位置（授权会话重启场景）。
fn show_on_main(text: &str, settings: NSRect) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let text_ns = NSString::from_str(text);

    {
        let guard = lock_or_recover(&PANEL);
        if let Some(entry) = guard.as_ref() {
            unsafe {
                if let Some(label) = (entry.label as *mut NSTextField).as_ref() {
                    label.setStringValue(&text_ns);
                    label.sizeToFit();
                    let f = label.frame();
                    if let Some(panel) = (entry.panel as *mut NSPanel).as_ref() {
                        let frame = panel_frame(mtm, f.size.width, f.size.height, settings);
                        let _: () = msg_send![panel, setFrame: frame, display: true];
                        panel.orderFrontRegardless();
                    }
                }
            }
            return;
        }
    }

    let label = make_label(mtm, &text_ns);
    let text_w = label.frame().size.width;
    // 图标边长 = 两行文本高度（高度匹配一致），浮窗高 = 文本高 + 上下内边距
    let side = label.frame().size.height;
    let frame = panel_frame(mtm, text_w, side, settings);
    let panel = NSPanel::initWithContentRect_styleMask_backing_defer(
        mtm.alloc::<NSPanel>(),
        frame,
        NSWindowStyleMask::NonactivatingPanel,
        NSBackingStoreType::Buffered,
        false,
    );
    unsafe {
        panel.setReleasedWhenClosed(false);
        panel.setOpaque(false);
        panel.setBackgroundColor(Some(&NSColor::clearColor()));
        panel.setLevel(objc2_app_kit::NSFloatingWindowLevel);
        panel.setCollectionBehavior(
            NSWindowCollectionBehavior::CanJoinAllSpaces
                | NSWindowCollectionBehavior::FullScreenAuxiliary,
        );
        panel.setBecomesKeyOnlyIfNeeded(true);
        panel.setHasShadow(true);
    }

    // content view：自定义拖拽源（图标热区）
    #[allow(deprecated)] // retained 返回仍需 msg_send_id
    let view: Retained<PermDragView> =
        unsafe { msg_send_id![mtm.alloc::<PermDragView>(), initWithFrame: frame] };

    // 毛玻璃背景 + 圆角
    let effect = NSVisualEffectView::initWithFrame(
        NSVisualEffectView::alloc(mtm),
        NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(frame.size.width, frame.size.height),
        ),
    );
    effect.setMaterial(objc2_app_kit::NSVisualEffectMaterial::Popover);
    effect.setBlendingMode(objc2_app_kit::NSVisualEffectBlendingMode::BehindWindow);
    effect.setState(objc2_app_kit::NSVisualEffectState::Active);
    effect.setWantsLayer(true);
    if let Some(layer) = effect.layer() {
        layer.setCornerRadius(12.0);
    }
    view.addSubview(&effect);

    // 应用图标（拖拽热区，显式 setSize 铺满热区——NSImageView 默认按图标固有 32pt
    // 绘制会偏小）
    let path = crate::platform::permission::app_bundle_path()
        .map(|p| NSString::from_str(&p.to_string_lossy()))
        .unwrap_or_else(|| NSString::from_str(""));
    let icon = NSWorkspace::sharedWorkspace().iconForFile(&path);
    icon.setSize(NSSize::new(side, side));
    let iv = NSImageView::initWithFrame(
        mtm.alloc::<NSImageView>(),
        NSRect::new(NSPoint::new(PAD, PAD), NSSize::new(side, side)),
    );
    iv.setImage(Some(&icon));
    view.addSubview(&iv);

    // 两行说明：与图标等高并排（顶边对齐，同高即垂直居中一致）
    label.setFrame(NSRect::new(
        NSPoint::new(PAD + side + GAP, PAD),
        NSSize::new(text_w, side),
    ));
    view.addSubview(&label);

    panel.setContentView(Some(&view));
    panel.orderFrontRegardless();

    *lock_or_recover(&PANEL) = Some(HintPanel {
        panel: Retained::into_raw(panel) as *mut AnyObject,
        label: Retained::into_raw(label) as *mut AnyObject,
    });
}

/// 关闭浮窗（仅主线程）。
fn hide_on_main() {
    let Some(entry) = lock_or_recover(&PANEL).take() else {
        return;
    };
    unsafe {
        // label 与 panel 的 into_raw 引用独立于视图树持有，须一并回收
        drop(Retained::from_raw(entry.label as *mut NSTextField));
        if let Some(panel) = (entry.panel as *mut NSPanel).as_ref() {
            panel.close(); // releasedWhenClosed=false，释放由下方 Retained drop 完成
        }
        drop(Retained::from_raw(entry.panel as *mut NSPanel));
    }
}

/// 显示拖拽指引浮窗：等系统设置窗口映射（冷启动 1-2s）后定位其底部内容区中心
/// 外侧。建窗前在主线程复核代数（hide 先到即放弃，防残窗）。
pub fn show(app: &tauri::AppHandle, text: &str) {
    use std::sync::atomic::Ordering;
    let app = app.clone();
    let text = text.to_string();
    let gen = DRAG_GEN.load(Ordering::SeqCst);
    std::thread::spawn(move || {
        let mut frame = None;
        for _ in 0..40 {
            let (tx, rx) = std::sync::mpsc::channel::<Option<NSRect>>();
            if app
                .run_on_main_thread(move || {
                    let _ = tx.send(
                        MainThreadMarker::new()
                            .and_then(crate::platform::permission::settings_window_frame),
                    );
                })
                .is_err()
            {
                return;
            }
            if let Ok(Some(f)) = rx.recv_timeout(std::time::Duration::from_secs(2)) {
                frame = Some(f);
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        let Some(frame) = frame else { return };
        let text_for_main = text.clone();
        let _ = app.run_on_main_thread(move || {
            if DRAG_GEN.load(Ordering::SeqCst) != gen {
                return;
            }
            show_on_main(&text_for_main, frame);
        });
    });
}

/// 关闭拖拽指引浮窗。先换代再排队关闭：show 线程的主线程闭包复核代数即可放弃
/// 晚到的建窗。
pub fn hide(app: &tauri::AppHandle) {
    DRAG_GEN.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let _ = app.run_on_main_thread(hide_on_main);
}
