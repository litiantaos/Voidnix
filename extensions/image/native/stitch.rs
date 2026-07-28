//! 图片拼接：多图横向/纵向合成为长图。
//!
//! gap 正值=间距，负值=重叠（如电影截图台词拼接，重叠避免截断字幕）。
//! 纵向模式可统一宽度、横向模式可统一高度（等比缩放），消除异型图参差。
//! 使用 CoreGraphics 位图上下文合成 → PNG 编码。
//!
//! 管道：逐张加载 → （可选）统一尺寸 → 计算布局 → 位图上下文逐张绘制 → PNG。

use objc2::rc::autoreleasepool;
use objc2_foundation::{NSPoint, NSRect, NSSize};
use serde::Deserialize;
use std::ffi::c_void;

use super::shared::{self, ExtractedImage, Loaded};

/// kCGImageAlphaPremultipliedLast（RGBA 透明背景）。
const ALPHA_PREMULTIPLIED_LAST: u32 = 1;

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    Vertical,
    Horizontal,
}

/// 尺寸统一策略。
/// Width(n) = 统一缩放到宽度 n（纵向模式，等比缩放高度）；
/// Height(n) = 统一缩放到高度 n（横向模式，等比缩放宽度）。
#[derive(Deserialize, Clone)]
#[serde(tag = "mode", content = "value", rename_all = "lowercase")]
pub enum Resize {
    Width(u32),
    Height(u32),
}

// CoreGraphics 位图上下文（符号经 AppKit 传递链接）
extern "C" {
    fn CGColorSpaceCreateDeviceRGB() -> *mut c_void;
    fn CGBitmapContextCreate(
        data: *mut c_void,
        width: usize,
        height: usize,
        bits_per_component: usize,
        bytes_per_row: usize,
        space: *mut c_void,
        bitmap_info: u32,
    ) -> *mut c_void;
    fn CGContextDrawImage(ctx: *mut c_void, rect: NSRect, image: *mut c_void);
    fn CGBitmapContextCreateImage(ctx: *mut c_void) -> *mut c_void;
}

/// 执行图片拼接。
///
/// 在 autoreleasepool 内执行所有 Objective-C 操作。调用方应在 `spawn_blocking` 中调用。
pub fn stitch(
    paths: &[String],
    direction: Direction,
    gap: i64,
    resize: Resize,
) -> Result<shared::ImageResult, String> {
    autoreleasepool(|_| -> Result<shared::ImageResult, String> {
        let extracted = unsafe { run_stitch_pipeline(paths, direction, gap, resize)? };
        shared::build_result(extracted.png_bytes, extracted.width, extracted.height)
    })
}

/// 拼接管道核心。
unsafe fn run_stitch_pipeline(
    paths: &[String],
    direction: Direction,
    gap: i64,
    resize: Resize,
) -> Result<ExtractedImage, String> {
    if paths.len() < 2 {
        return Err("至少需要 2 张图片".into());
    }

    // ── 1. 逐张加载（Loaded 持有 NSImage，保持 CGImage 有效直至绘制完成）──
    let mut images: Vec<Loaded> = Vec::with_capacity(paths.len());
    for path in paths {
        images.push(shared::load_image(path)?);
    }

    // ── 2. 统一尺寸（等比缩放）──
    let dims: Vec<(u32, u32)> = resize_all(&images, &resize);

    // ── 3. 计算画布尺寸与各图坐标 ──
    let layout = compute_layout(&dims, &direction, gap)?;
    let (canvas_w, canvas_h) = (layout.canvas_w, layout.canvas_h);

    // ── 4. 创建 RGBA 位图上下文（透明背景）──
    let color_space = CGColorSpaceCreateDeviceRGB();
    if color_space.is_null() {
        return Err("无法创建色彩空间".into());
    }
    let ctx = CGBitmapContextCreate(
        std::ptr::null_mut(),
        canvas_w as usize,
        canvas_h as usize,
        8,
        0,
        color_space,
        ALPHA_PREMULTIPLIED_LAST,
    );
    shared::release_cf(color_space);
    if ctx.is_null() {
        return Err("无法创建位图上下文（图片过大？）".into());
    }

    // ── 5. 逆序绘制（序号小的在上层：painter's algorithm 后绘制者覆盖先绘制者，
    //         逆序使首张图最后绘制 = 最上层，重叠时后续图仅露出底部台词）──
    for i in (0..images.len()).rev() {
        let loaded = &images[i];
        let (x, top_y) = layout.positions[i];
        let (dw, dh) = dims[i];
        let cg_y = canvas_h as i64 - top_y - dh as i64;
        let rect = NSRect {
            origin: NSPoint {
                x: x as f64,
                y: cg_y as f64,
            },
            size: NSSize {
                width: dw as f64,
                height: dh as f64,
            },
        };
        CGContextDrawImage(ctx, rect, loaded.cg_image);
    }

    // ── 6. 取合成结果 CGImage → PNG ──
    let cg_out = CGBitmapContextCreateImage(ctx);
    shared::release_cf(ctx);
    if cg_out.is_null() {
        return Err("合成图像生成失败".into());
    }

    let png_result = shared::encode_png(cg_out);
    extern "C" {
        fn CGImageRelease(image: *mut c_void);
    }
    CGImageRelease(cg_out);

    png_result.map(|png_bytes| ExtractedImage {
        png_bytes,
        width: canvas_w,
        height: canvas_h,
    })
}

/// 等比缩放所有图片到统一宽度或高度。
///
/// Width(n)：所有图缩放到 width=n，高度按比例缩放。
/// Height(n)：所有图缩放到 height=n，宽度按比例缩放。
unsafe fn resize_all(images: &[Loaded], resize: &Resize) -> Vec<(u32, u32)> {
    match resize {
        Resize::Width(target_w) => images
            .iter()
            .map(|l| {
                if l.width == 0 {
                    return (*target_w, 0u32);
                }
                let h = (*target_w as u64 * l.height as u64 / l.width as u64) as u32;
                (*target_w, h)
            })
            .collect(),
        Resize::Height(target_h) => images
            .iter()
            .map(|l| {
                if l.height == 0 {
                    return (0u32, *target_h);
                }
                let w = (*target_h as u64 * l.width as u64 / l.height as u64) as u32;
                (w, *target_h)
            })
            .collect(),
    }
}

/// 布局计算结果。
struct Layout {
    canvas_w: u32,
    canvas_h: u32,
    /// 各图 top-left 原点坐标 (x, y)
    positions: Vec<(i64, i64)>,
}

/// 计算画布尺寸与各图 top-left 坐标。
///
/// 纵向：宽度取最大值，图片水平居中；横向：高度取最大值，图片垂直居中。
fn compute_layout(
    images: &[(u32, u32)],
    direction: &Direction,
    gap: i64,
) -> Result<Layout, String> {
    match direction {
        Direction::Vertical => {
            let canvas_w = images.iter().map(|(w, _)| *w).max().unwrap_or(0);
            let mut positions = Vec::with_capacity(images.len());
            let mut y: i64 = 0;
            for &(w, h) in images {
                let x = (canvas_w as i64 - w as i64) / 2;
                positions.push((x, y));
                y += h as i64 + gap;
            }
            let canvas_h = positions
                .iter()
                .zip(images)
                .map(|(&(_, top), &(_, h))| top + h as i64)
                .max()
                .unwrap_or(0);
            if canvas_w == 0 || canvas_h <= 0 {
                return Err("画布尺寸无效".into());
            }
            Ok(Layout {
                canvas_w,
                canvas_h: canvas_h as u32,
                positions,
            })
        }
        Direction::Horizontal => {
            let canvas_h = images.iter().map(|(_, h)| *h).max().unwrap_or(0);
            let mut positions = Vec::with_capacity(images.len());
            let mut x: i64 = 0;
            for &(w, h) in images {
                let y = (canvas_h as i64 - h as i64) / 2;
                positions.push((x, y));
                x += w as i64 + gap;
            }
            let canvas_w = positions
                .iter()
                .zip(images)
                .map(|(&(left, _), &(w, _))| left + w as i64)
                .max()
                .unwrap_or(0);
            if canvas_w <= 0 || canvas_h == 0 {
                return Err("画布尺寸无效".into());
            }
            Ok(Layout {
                canvas_w: canvas_w as u32,
                canvas_h,
                positions,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vertical_no_gap() {
        let l = compute_layout(&[(100, 50), (100, 30)], &Direction::Vertical, 0).unwrap();
        assert_eq!(l.canvas_w, 100);
        assert_eq!(l.canvas_h, 80);
        assert_eq!(l.positions, vec![(0, 0), (0, 50)]);
    }

    #[test]
    fn vertical_with_gap() {
        let l = compute_layout(&[(100, 50), (100, 30)], &Direction::Vertical, 10).unwrap();
        assert_eq!(l.canvas_w, 100);
        assert_eq!(l.canvas_h, 90);
        assert_eq!(l.positions, vec![(0, 0), (0, 60)]);
    }

    #[test]
    fn vertical_with_overlap() {
        let l = compute_layout(&[(100, 50), (100, 30)], &Direction::Vertical, -10).unwrap();
        assert_eq!(l.canvas_w, 100);
        assert_eq!(l.canvas_h, 70);
        assert_eq!(l.positions, vec![(0, 0), (0, 40)]);
    }

    #[test]
    fn excessive_overlap_not_rejected() {
        // 重叠超过图高度不报错，超出部分自然裁剪
        let l = compute_layout(&[(100, 10), (100, 10)], &Direction::Vertical, -20).unwrap();
        assert_eq!(l.canvas_h, 10);
        assert_eq!(l.positions[1].1, -10);
    }

    #[test]
    fn horizontal_no_gap() {
        let l = compute_layout(&[(50, 100), (30, 100)], &Direction::Horizontal, 0).unwrap();
        assert_eq!(l.canvas_w, 80);
        assert_eq!(l.canvas_h, 100);
        assert_eq!(l.positions, vec![(0, 0), (50, 0)]);
    }

    #[test]
    fn center_align_mixed_widths() {
        let l = compute_layout(&[(100, 50), (60, 30)], &Direction::Vertical, 0).unwrap();
        assert_eq!(l.positions[0].0, 0);
        assert_eq!(l.positions[1].0, 20);
    }

    #[test]
    fn three_images_vertical() {
        let l =
            compute_layout(&[(100, 50), (100, 50), (100, 50)], &Direction::Vertical, 0).unwrap();
        assert_eq!(l.canvas_w, 100);
        assert_eq!(l.canvas_h, 150);
        assert_eq!(l.positions[2], (0, 100));
    }
}
