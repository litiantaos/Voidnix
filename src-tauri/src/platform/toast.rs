// 原生 toast 浮窗：每行一个独立 NSPanel（NonactivatingPanel，不抢焦点 + 点击
// 穿透），独立于主窗 WebView 生命周期——主窗隐藏（alpha=0）不影响其展示，反馈
// 类动作得以在触发后立即关窗。锚定所在屏 visibleFrame 顶部中心（HUD 位，与
// 主窗显隐/坐标无关）；最新在顶、水平居中、向下生长（macOS 通知堆叠语义）。
// 进出场与堆叠补位动画全走 **window 级 animator**（NSAnimationContext + 窗口
// animator 的 alpha/frame，与 platform/window.rs::animate_panel 同路径——snap-panel
// 生产验证可靠；view 层动画在该场景实测不提交渲染）。到期调度 NSTimer 主线程
// 原生定时。视觉常量对齐 web 版 toast：px-3 / py-1.5 / gap-2 / radius-panel /
// 13pt medium。

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use objc2::msg_send;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::MainThreadOnly;

use objc2_app_kit::{
    NSAnimationContext, NSBackingStoreType, NSColor, NSFont, NSFontWeightMedium, NSImage,
    NSImageView, NSPanel, NSTextField, NSView, NSVisualEffectBlendingMode, NSVisualEffectMaterial,
    NSVisualEffectState, NSVisualEffectView, NSWindowCollectionBehavior, NSWindowStyleMask,
};
use objc2_foundation::{MainThreadMarker, NSPoint, NSRect, NSSize, NSString, NSTimer};
use objc2_quartz_core::CAMediaTimingFunction;

use crate::runtime::lock_or_recover;

/// 行内水平内边距（px-3）
const PAD_X: f64 = 12.0;
/// 行内垂直内边距（py-1.5）
const PAD_Y: f64 = 6.0;
/// 图标-文本间距与行距（gap-2）
const GAP: f64 = 8.0;
/// 顶部锚定边距（行区顶距 visibleFrame 顶）
const MARGIN: f64 = 12.0;
/// SF Symbol 图标边长
const ICON: f64 = 16.0;
/// 文本最大宽度（web 版 max-w-96 = 384 − 内边距与图标）
const MAX_TEXT_W: f64 = 336.0;
/// 卡片圆角（radius-panel）
const RADIUS: f64 = 10.0;
/// 文本字号（text-sm）
const FONT_SIZE: f64 = 13.0;
/// 同屏行数上限，超限淘汰最旧
const MAX_ROWS: usize = 3;
/// 进场动画：250ms easeOut，自屏幕顶缘下滑入 + 淡入
const ANIM_ENTER: f64 = 0.25;
/// 离场动画：180ms easeIn，淡出 + 下滑退出
const ANIM_LEAVE: f64 = 0.18;
/// 进场自上方滑入 / 离场向下退出的位移幅度（窗口级动画可越屏缘，从屏幕顶滑入）
const SLIDE_OFFSET: f64 = 12.0;
/// 离场动画后销毁收尾的调度间隔（秒）
const FINISH_DELAY_SEC: f64 = 0.3;

struct ToastRow {
    id: u64,
    /// 行 NSPanel（Retained into_raw；离场收尾时 close + 回收）
    panel: *mut AnyObject,
    w: f64,
    h: f64,
}
unsafe impl Send for ToastRow {}

/// 在场行（时间序，旧→新）与离场中行（动画播完前保留，帧不动）
static ROWS: Mutex<Vec<ToastRow>> = Mutex::new(Vec::new());
static FADING: Mutex<Vec<ToastRow>> = Mutex::new(Vec::new());
static NEXT_ID: AtomicU64 = AtomicU64::new(1);
/// 锚定屏快照（行增减重排时保持锚点稳定）
static VIS: Mutex<Option<NSRect>> = Mutex::new(None);

/// 裸指针 → panel 引用（生命周期锚定行借用，行由 ROWS/FADING 持有；全模块仅主线程访问）。
fn panel_of(row: &ToastRow) -> Option<&NSPanel> {
    unsafe { (row.panel as *mut NSPanel).as_ref() }
}

/// 行 panel 屏幕布局（纯函数，Cocoa 坐标）：最新（末位）在顶、水平居中、行区顶
/// 自 vis 顶 - MARGIN 向下排；fading_h 为底部离场行保留的占位高（离场行 frame
/// 原位不动，动画在占位区内完成，收尾销毁后占位回收）。
pub fn row_panel_frames(rows: &[(f64, f64)], fading_h: f64, vis: NSRect) -> Vec<NSRect> {
    let vis_top = vis.origin.y + vis.size.height;
    let mut y_top = vis_top - MARGIN - fading_h;
    // 极宽行防御：clamp 进屏保 MARGIN 边距（正常 toast 远窄于屏，不会触发）
    let max_x = (vis.origin.x + vis.size.width - MARGIN).max(vis.origin.x + MARGIN);
    let mut frames = Vec::with_capacity(rows.len());
    for (w, h) in rows.iter().rev() {
        y_top -= h;
        let x = (vis.origin.x + (vis.size.width - w) / 2.0).clamp(vis.origin.x + MARGIN, max_x);
        frames.push(NSRect::new(NSPoint::new(x, y_top), NSSize::new(*w, *h)));
        y_top -= GAP;
    }
    frames.reverse();
    frames
}

/// fading（离场中）行的占位高：各行高 + 各自一个行距（销毁后行距一并回收）。
fn fading_height(fading: &[(f64, f64)]) -> f64 {
    fading.iter().map(|r| r.1 + GAP).sum::<f64>()
}

/// window 级动画（NSAnimationContext + 窗口 animator）：alpha 与 frame 同时过渡。
/// 与 platform::window::animate_panel 同路径（snap-panel 生产验证）——窗口 alpha/
/// 位移由窗口服务器合成，不经 AppKit view 重绘管线。
fn animate_panel_to(
    panel: &NSPanel,
    alpha_to: Option<f64>,
    frame_to: NSRect,
    duration: f64,
    ease_out: bool,
) {
    let timing = CAMediaTimingFunction::functionWithName(if ease_out {
        objc2_foundation::ns_string!("easeOut")
    } else {
        objc2_foundation::ns_string!("easeIn")
    });
    NSAnimationContext::beginGrouping();
    let ctx = NSAnimationContext::currentContext();
    ctx.setDuration(duration);
    ctx.setTimingFunction(Some(&timing));
    unsafe {
        let animator: *mut AnyObject = msg_send![panel, animator];
        if let Some(a) = alpha_to {
            let _: () = msg_send![animator, setAlphaValue: a];
        }
        let _: () = msg_send![animator, setFrame: frame_to, display: true];
    }
    NSAnimationContext::endGrouping();
}

/// appearance 跟随主题缓存（toast 瞬态，每次 show 读取即可，不做监听）。
fn apply_appearance(panel: &NSPanel) {
    use objc2_app_kit::{NSAppearance, NSAppearanceCustomization};
    use objc2_foundation::ns_string;

    let appearance = match crate::platform::window::cached_appearance().as_deref() {
        Some("light") => NSAppearance::appearanceNamed(ns_string!("NSAppearanceNameAqua")),
        Some("dark") => NSAppearance::appearanceNamed(ns_string!("NSAppearanceNameDarkAqua")),
        // auto / 未设置：None = 跟随系统
        _ => None,
    };
    panel.setAppearance(appearance.as_deref());
}

/// SF Symbol 图标（降级链：填充变体 → 裸名 → None 无图标收缩布局）。
fn symbol_image(kind_error: bool) -> Option<Retained<NSImage>> {
    let (primary, fallback) = if kind_error {
        ("exclamationmark.triangle.fill", "exclamationmark")
    } else {
        ("checkmark.circle.fill", "checkmark")
    };
    NSImage::imageWithSystemSymbolName_accessibilityDescription(&NSString::from_str(primary), None)
        .or_else(|| {
            NSImage::imageWithSystemSymbolName_accessibilityDescription(
                &NSString::from_str(fallback),
                None,
            )
        })
}

/// 多行文本测量：cellSizeForBounds 按 MAX_TEXT_W 宽换行测得 (宽, 高)。
fn measure_label(label: &NSTextField) -> (f64, f64) {
    let Some(cell) = label.cell() else {
        return (MAX_TEXT_W, 18.0);
    };
    let size = cell.cellSizeForBounds(NSRect::new(
        NSPoint::new(0.0, 0.0),
        NSSize::new(MAX_TEXT_W, 10_000.0),
    ));
    (size.width.clamp(1.0, MAX_TEXT_W), size.height.max(1.0))
}

/// 构造一行（独立 NSPanel + 毛玻璃卡片 + SF Symbol 图标 + 13pt medium 文本）。
/// 初始 alpha 0（进场动画淡入），frame 由调用方定位。id 由调用方回填。
fn make_row(mtm: MainThreadMarker, message: &str, kind_error: bool) -> ToastRow {
    let label = NSTextField::labelWithString(&NSString::from_str(message), mtm);
    label.setFont(Some(&NSFont::systemFontOfSize_weight(
        FONT_SIZE,
        // AppKit 导出的字重常量（extern static，读取安全、值恒定）
        unsafe { NSFontWeightMedium },
    )));
    label.setLineBreakMode(objc2_app_kit::NSLineBreakMode::ByWordWrapping);
    let (lw, lh) = measure_label(&label);

    let icon = symbol_image(kind_error);
    let (icon_w, icon_gap) = if icon.is_some() {
        (ICON, GAP)
    } else {
        (0.0, 0.0)
    };
    let w = PAD_X * 2.0 + icon_w + icon_gap + lw;
    let h = lh.max(icon_w) + PAD_Y * 2.0;

    let frame = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(w, h));
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
        // 主窗隐藏会 deactivate（还原前台 app），panel 必须不随之隐藏
        panel.setHidesOnDeactivate(false);
        panel.setIgnoresMouseEvents(true);
        panel.setHasShadow(true);
        panel.setAlphaValue(0.0);
    }
    apply_appearance(&panel);

    let view = NSView::new(mtm);
    let effect = NSVisualEffectView::initWithFrame(
        NSVisualEffectView::alloc(mtm),
        NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(w, h)),
    );
    effect.setMaterial(NSVisualEffectMaterial::Popover);
    effect.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
    effect.setState(NSVisualEffectState::Active);
    effect.setWantsLayer(true);
    if let Some(layer) = effect.layer() {
        layer.setCornerRadius(RADIUS);
    }
    view.addSubview(&effect);

    if let Some(image) = &icon {
        let iv = NSImageView::initWithFrame(
            NSImageView::alloc(mtm),
            NSRect::new(
                NSPoint::new(PAD_X, (h - ICON) / 2.0),
                NSSize::new(ICON, ICON),
            ),
        );
        iv.setImage(Some(image));
        let tint = if kind_error {
            NSColor::systemRedColor()
        } else {
            NSColor::controlAccentColor()
        };
        iv.setContentTintColor(Some(&tint));
        view.addSubview(&iv);
    }

    label.setFrame(NSRect::new(
        NSPoint::new(PAD_X + icon_w + icon_gap, (h - lh) / 2.0),
        NSSize::new(lw, lh),
    ));
    view.addSubview(&label);
    panel.setContentView(Some(&view));

    ToastRow {
        id: 0,
        panel: Retained::into_raw(panel) as *mut AnyObject,
        w,
        h,
    }
}

/// 重排（仅主线程）：按锚定屏与堆叠序给每行 panel 定 frame；新行（new_id）自
/// 目标上方 SLIDE_OFFSET 淡入滑进；既有行平移补位（旧行下移一位）；fading 行
/// frame 原位不动（其动画自行播放）。
fn relayout(new_id: Option<u64>) {
    let vis = *lock_or_recover(&VIS);
    let Some(vis) = vis else { return };
    let frames = {
        let rows = lock_or_recover(&ROWS);
        let fading = lock_or_recover(&FADING);
        let sizes: Vec<(f64, f64)> = rows.iter().map(|r| (r.w, r.h)).collect();
        let fsizes: Vec<(f64, f64)> = fading.iter().map(|r| (r.w, r.h)).collect();
        row_panel_frames(&sizes, fading_height(&fsizes), vis)
    };
    let rows = lock_or_recover(&ROWS);
    for (row, f) in rows.iter().zip(&frames) {
        let Some(panel) = panel_of(row) else {
            continue;
        };
        if Some(row.id) == new_id {
            // 进场：起始 frame 在目标上方（窗口级动画可越屏缘，从屏幕顶缘滑入）
            let mut from = *f;
            from.origin.y += SLIDE_OFFSET;
            unsafe {
                let _: () = msg_send![panel, setFrame: from, display: false];
            }
            panel.orderFrontRegardless();
            animate_panel_to(panel, Some(1.0), *f, ANIM_ENTER, true);
        } else if panel.frame().origin != f.origin {
            // 旧行平移补位
            animate_panel_to(panel, None, *f, ANIM_ENTER, true);
        }
    }
}

/// 到期调度（仅主线程）：NSTimer 主 runloop 原生定时。到期 fire → fade_out_row
///（+ 收尾再调度）；淘汰行（show 时已在淡出）只调度销毁收尾。
fn schedule_expiry(id: u64, delay_sec: f64, finish: bool) {
    let block = block2::RcBlock::new(move |_timer: std::ptr::NonNull<NSTimer>| {
        if finish {
            finish_remove_row(id);
        } else {
            fade_out_row(id);
            schedule_expiry(id, FINISH_DELAY_SEC, true);
        }
    });
    drop(unsafe {
        NSTimer::scheduledTimerWithTimeInterval_repeats_block(delay_sec, false, &block)
    });
}

/// 显示一行 toast（仅主线程）。duration_ms 为 0 表示不自动离场。超上限淘汰
/// 最旧行（当场开始淡出，销毁收尾已调度）。
pub fn show_row(
    mtm: MainThreadMarker,
    message: &str,
    kind_error: bool,
    duration_ms: u64,
    vis: NSRect,
) {
    *lock_or_recover(&VIS) = Some(vis);

    let evicted = {
        let mut rows = lock_or_recover(&ROWS);
        if rows.len() >= MAX_ROWS {
            Some(rows.remove(0))
        } else {
            None
        }
    };
    if let Some(old) = evicted {
        let evicted_id = old.id;
        if let Some(panel) = panel_of(&old) {
            let mut to = panel.frame();
            to.origin.y -= SLIDE_OFFSET;
            animate_panel_to(panel, Some(0.0), to, ANIM_LEAVE, false);
        }
        lock_or_recover(&FADING).push(old);
        schedule_expiry(evicted_id, FINISH_DELAY_SEC, true);
    }

    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let mut row = make_row(mtm, message, kind_error);
    row.id = id;
    lock_or_recover(&ROWS).push(row);
    relayout(Some(id));
    if duration_ms > 0 {
        schedule_expiry(id, duration_ms as f64 / 1000.0, false);
    }
}

/// 行离场（仅主线程）：window 级淡出 + 下滑退出（帧原位语义——其余行不重排，
/// 离场行销毁后占位由收尾重排回收）。id 不存在时幂等 no-op。
pub fn fade_out_row(id: u64) {
    let removed = {
        let mut rows = lock_or_recover(&ROWS);
        rows.iter().position(|r| r.id == id).map(|p| rows.remove(p))
    };
    let Some(row) = removed else {
        return;
    };
    if let Some(panel) = panel_of(&row) {
        let mut to = panel.frame();
        to.origin.y -= SLIDE_OFFSET;
        animate_panel_to(panel, Some(0.0), to, ANIM_LEAVE, false);
    }
    lock_or_recover(&FADING).push(row);
}

/// 行销毁收尾（仅主线程）：close + 回收 Retained + 重排回收 fading 占位（剩余行
/// 上移补位）。幂等。
pub fn finish_remove_row(id: u64) {
    {
        let mut fading = lock_or_recover(&FADING);
        let Some(pos) = fading.iter().position(|r| r.id == id) else {
            return;
        };
        let row = fading.remove(pos);
        if let Some(panel) = panel_of(&row) {
            panel.close(); // releasedWhenClosed=false，释放由下方 Retained drop 完成
        }
        drop(unsafe { Retained::from_raw(row.panel as *mut NSPanel) });
    }
    relayout(None);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vis(x: f64, y: f64, w: f64, h: f64) -> NSRect {
        NSRect::new(NSPoint::new(x, y), NSSize::new(w, h))
    }

    #[test]
    fn layout_newest_on_top_centered() {
        // 3 行（旧→新）：最新（末位）在顶（行区顶 = vis 顶 - 12）、旧行向下叠、
        // 水平居中、行距 GAP
        let rows = [(100.0, 28.0), (200.0, 32.0), (150.0, 28.0)];
        let frames = row_panel_frames(&rows, 0.0, vis(0.0, 0.0, 1920.0, 1055.0));
        assert_eq!(frames.len(), 3);
        // 最新（末位）顶边贴行区顶
        assert_eq!(frames[2].origin.y, 1055.0 - 12.0 - 28.0);
        assert_eq!(frames[2].origin.x, (1920.0 - 150.0) / 2.0);
        // 中间行叠其下
        assert_eq!(frames[1].origin.y, 1055.0 - 12.0 - 28.0 - GAP - 32.0);
        assert_eq!(frames[1].origin.x, (1920.0 - 200.0) / 2.0);
        // 最旧在底
        assert_eq!(
            frames[0].origin.y,
            1055.0 - 12.0 - 28.0 - GAP - 32.0 - GAP - 28.0
        );
        assert_eq!(frames[0].origin.x, (1920.0 - 100.0) / 2.0);
    }

    #[test]
    fn layout_keeps_fading_space() {
        // fading 占位：行区整体上移让出底部空间（离场行原位、在场行位置上抬）
        let frames = row_panel_frames(&[(150.0, 28.0)], 28.0 + GAP, vis(0.0, 0.0, 1000.0, 800.0));
        assert_eq!(frames[0].origin.y, 800.0 - 12.0 - 36.0 - 28.0);
    }

    #[test]
    fn layout_empty() {
        assert!(row_panel_frames(&[], 0.0, vis(0.0, 0.0, 1000.0, 800.0)).is_empty());
        assert_eq!(fading_height(&[]), 0.0);
        assert_eq!(fading_height(&[(100.0, 28.0)]), 28.0 + GAP);
    }
}
