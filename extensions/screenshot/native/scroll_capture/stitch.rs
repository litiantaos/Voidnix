//! 帧拼接：行签名对齐、静态区 mask、append_frame、预览 emit、capture_loop。

use std::sync::atomic::Ordering;
use tauri::Emitter;

use super::encode::{capture_below_overlay, encode_jpeg};
use super::state::{ScrollSession, IS_RUNNING, LAST_EMIT_NS, SESSION};

fn row_signatures(frame: &[u8], width: usize, height: usize) -> Vec<u64> {
    let row_bytes = width * 4;
    let mut sigs = Vec::with_capacity(height);
    for r in 0..height {
        let s = r * row_bytes;
        sigs.push(row_signature(&frame[s..s + row_bytes], width));
    }
    sigs
}

/// 多项式滚动哈希（base 31）：位置敏感，碰撞率远低于 RGB 求和。
/// 两行内容不同但亮度总和相同的场景（代码编辑器/终端纯色行）不再误判。
fn row_signature(row: &[u8], width: usize) -> u64 {
    let mut h: u64 = 0;
    let mut i = 0;
    let end = width * 4;
    while i < end {
        h = h.wrapping_mul(31).wrapping_add(row[i] as u64);
        h = h.wrapping_mul(31).wrapping_add(row[i + 1] as u64);
        h = h.wrapping_mul(31).wrapping_add(row[i + 2] as u64);
        i += 4;
    }
    h
}

pub fn find_offset_from_sigs(
    prev_sigs: &[u64],
    new_sigs: &[u64],
    height: usize,
) -> (usize, ScrollDir, f64) {
    let mut fwd_best_k: usize = 0;
    let mut fwd_best_ratio: f64 = 0.0;
    for k in 0..height {
        let n = height - k;
        if n < height / 4 {
            break;
        }
        let mut matches: usize = 0;
        for i in 0..n {
            if prev_sigs[k + i] == new_sigs[i] {
                matches += 1;
            }
        }
        let ratio = matches as f64 / n as f64;
        if ratio > fwd_best_ratio {
            fwd_best_ratio = ratio;
            fwd_best_k = k;
        }
    }

    let mut bwd_best_k: usize = 0;
    let mut bwd_best_ratio: f64 = 0.0;
    for k in 1..height {
        let n = height - k;
        if n < height / 4 {
            break;
        }
        let mut matches: usize = 0;
        for i in 0..n {
            if prev_sigs[i] == new_sigs[k + i] {
                matches += 1;
            }
        }
        let ratio = matches as f64 / n as f64;
        if ratio > bwd_best_ratio {
            bwd_best_ratio = ratio;
            bwd_best_k = k;
        }
    }

    if bwd_best_ratio > fwd_best_ratio && bwd_best_k > 0 {
        (bwd_best_k, ScrollDir::Backward, bwd_best_ratio)
    } else {
        (fwd_best_k, ScrollDir::Forward, fwd_best_ratio)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollDir {
    Forward,
    Backward,
}

fn update_static_mask(session: &mut ScrollSession, prev_sigs: &[u64], new_sigs: &[u64], k: usize) {
    if k == 0 {
        return;
    }
    let h = session.ph_per_frame;
    if h == 0 || session.static_votes.len() != h {
        return;
    }
    for i in 0..h {
        // 精确哈希匹配：内容完全相同 → 固定行
        if prev_sigs[i] == new_sigs[i] {
            session.static_votes[i] = session.static_votes[i].saturating_add(2);
        } else {
            session.static_votes[i] = session.static_votes[i].saturating_sub(1);
        }
        session.static_mask[i] = session.static_votes[i] >= 2;
    }
}

pub fn append_frame(session: &mut ScrollSession, new_frame: Vec<u8>) -> (usize, bool) {
    let h = session.ph_per_frame;
    let rb = session.row_bytes();
    let frame_bytes = h * rb;

    if session.prev_frame.is_empty() {
        session.buf.extend_from_slice(&new_frame);
        session.total_rows = h;
        session.prev_frame = new_frame;
        session.static_mask = vec![false; h];
        session.static_votes = vec![0; h];
        return (h, true);
    }

    let prev_sigs = row_signatures(&session.prev_frame, session.pw, h);
    let new_sigs = row_signatures(&new_frame, session.pw, h);
    let (k, dir, match_ratio) = find_offset_from_sigs(&prev_sigs, &new_sigs, h);

    const CONFIDENCE_THRESHOLD: f64 = 0.25;
    if match_ratio < CONFIDENCE_THRESHOLD {
        session.prev_frame = new_frame;
        return (0, false);
    }

    update_static_mask(session, &prev_sigs, &new_sigs, k);

    // 自动停止检测：连续无位移帧计数
    if k == 0 {
        session.static_streak = session.static_streak.saturating_add(1);
    } else {
        session.static_streak = 0;
    }

    let mut new_rows: usize = 0;
    let mut changed = false;

    match dir {
        ScrollDir::Backward => {
            let max_trim = session.total_rows.saturating_sub(h);
            if k <= max_trim {
                let new_total = session.total_rows - k;
                session.buf.truncate(new_total * rb);
                session.total_rows = new_total;
            } else {
                session.buf.clear();
                session.buf.extend_from_slice(&new_frame);
                session.total_rows = h;
            }
            changed = true;
        }
        ScrollDir::Forward => {
            if k > 0 {
                let start = (h - k) * rb;
                session
                    .buf
                    .extend_from_slice(&new_frame[start..start + k * rb]);
                session.total_rows += k;
                new_rows = k;
                changed = true;
            }
        }
    }

    // 末尾刷新:保证预览实时性。固定行跳过(顶部 toolbar 不被贴到末尾)。
    if session.total_rows >= h {
        let buf_len = session.buf.len();
        let bottom_start = buf_len - frame_bytes;
        let has_mask = session.static_mask.iter().any(|&m| m);
        if has_mask {
            for r in 0..h {
                if session.static_mask[r] {
                    continue;
                }
                let dst = bottom_start + r * rb;
                let src = r * rb;
                session.buf[dst..dst + rb].copy_from_slice(&new_frame[src..src + rb]);
            }
        } else {
            session.buf[bottom_start..buf_len].copy_from_slice(&new_frame);
        }
    }

    session.prev_frame = new_frame;
    (new_rows, changed)
}

pub fn should_emit() -> bool {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64;
    let last = LAST_EMIT_NS.load(Ordering::Relaxed);
    if now.saturating_sub(last) < 33_000_000 {
        return false;
    }
    LAST_EMIT_NS.store(now, Ordering::Relaxed);
    true
}

pub fn emit_preview(app: &tauri::AppHandle, session: &mut ScrollSession) {
    if !should_emit() {
        return;
    }
    if session.total_rows == 0 || session.pw == 0 {
        return;
    }
    let jpeg = match encode_jpeg(&session.buf, session.pw, session.total_rows, 0.65) {
        Ok(d) => d,
        Err(_) => return,
    };
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&jpeg);
    session.emit_seq += 1;
    let _ = app.emit(
        "screenshot-scroll-frame",
        serde_json::json!({
            "seq": session.emit_seq,
            "width": session.pw,
            "height": session.total_rows,
            "dataUrl": format!("data:image/jpeg;base64,{}", b64),
        }),
    );
}

pub fn capture_loop(app: tauri::AppHandle) {
    const FRAME_INTERVAL_MS: u64 = 12;
    let mut last_ignoring = false;

    while IS_RUNNING.load(Ordering::SeqCst) {
        let (sel_x, sel_y, sel_w, sel_h, overlay_id, cur_ignoring) = {
            let guard = SESSION.lock().unwrap_or_else(|e| e.into_inner());
            match guard.as_ref() {
                Some(s) => (
                    s.sel_x,
                    s.sel_y,
                    s.sel_w,
                    s.sel_h,
                    s.overlay_window_id,
                    s.ignoring_mouse,
                ),
                None => return,
            }
        };
        if cur_ignoring != last_ignoring {
            let _ = app.emit("screenshot-scroll-passthrough", cur_ignoring);
            last_ignoring = cur_ignoring;
        }

        let frame = capture_below_overlay(sel_x, sel_y, sel_w, sel_h, overlay_id);
        if let Some((fw, fh, fbuf)) = frame {
            let mut guard = SESSION.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(session) = guard.as_mut() {
                if session.pw == 0 {
                    session.pw = fw;
                    session.ph_per_frame = fh;
                    eprintln!(
                        "[shot-scroll] first frame: {}x{}, buf_len={}, overlay_win_id={}",
                        fw,
                        fh,
                        fbuf.len(),
                        overlay_id
                    );
                }
                if fw == session.pw && fh == session.ph_per_frame {
                    let (added, _changed) = append_frame(session, fbuf);
                    if session.emit_seq == 0 {
                        eprintln!(
                            "[shot-scroll] first append: added={} total_rows={}",
                            added, session.total_rows
                        );
                    }
                    emit_preview(&app, session);
                    // 自动停止：连续 ~240ms 无位移，通知前端
                    const AUTO_STOP_FRAMES: u32 = 20;
                    if session.static_streak == AUTO_STOP_FRAMES {
                        let _ = app.emit("screenshot-scroll-stopped", ());
                    }
                }
            }
        } else if last_ignoring == cur_ignoring {
            static LOGGED_NONE: std::sync::atomic::AtomicBool =
                std::sync::atomic::AtomicBool::new(false);
            if !LOGGED_NONE.swap(true, Ordering::Relaxed) {
                eprintln!(
                    "[shot-scroll] capture_below_overlay returned None (sel={},{},{},{} win={})",
                    sel_x, sel_y, sel_w, sel_h, overlay_id
                );
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(FRAME_INTERVAL_MS));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: usize = 24;
    const H: usize = 60;

    fn make_test_session(w: usize, h: usize) -> ScrollSession {
        ScrollSession {
            sel_x: 0.0,
            sel_y: 0.0,
            sel_w: 0.0,
            sel_h: 0.0,
            pw: w,
            ph_per_frame: h,
            buf: Vec::new(),
            total_rows: 0,
            prev_frame: Vec::new(),
            overlay_window_id: 0,
            ns_window_addr: 0,
            ignoring_mouse: false,
            emit_seq: 0,
            static_mask: Vec::new(),
            static_votes: Vec::new(),
            toolbar_rect: None,
            static_streak: 0,
        }
    }

    /// 构造合成帧:行 r 在固定区为常量色,滚动区颜色随 (r + offset) 变化。
    fn make_frame(
        w: usize,
        h: usize,
        offset: usize,
        fixed_top: usize,
        fixed_bottom: usize,
    ) -> Vec<u8> {
        let rb = w * 4;
        let mut buf = vec![0u8; rb * h];
        for r in 0..h {
            let is_fixed = r < fixed_top || r >= h - fixed_bottom;
            let v: u8 = if is_fixed {
                210
            } else {
                ((r - fixed_top + offset) as u32 * 5 + 20) as u8
            };
            for c in 0..w {
                let i = r * rb + c * 4;
                buf[i] = v;
                buf[i + 1] = v;
                buf[i + 2] = v;
                buf[i + 3] = 255;
            }
        }
        buf
    }

    fn row_value(buf: &[u8], w: usize, r: usize) -> u8 {
        buf[r * w * 4]
    }

    #[test]
    fn test_find_offset_no_fixed() {
        let prev = make_frame(W, H, 0, 0, 0);
        let new = make_frame(W, H, 12, 0, 0);
        let prev_sigs = row_signatures(&prev, W, H);
        let new_sigs = row_signatures(&new, W, H);
        let (k, dir, ratio) = find_offset_from_sigs(&prev_sigs, &new_sigs, H);
        assert_eq!(dir, ScrollDir::Forward);
        assert_eq!(k, 12);
        assert!(ratio > 0.5, "匹配率应足够高, ratio={}", ratio);
    }

    #[test]
    fn test_update_static_mask_top() {
        let fixed = 10;
        let mut session = make_test_session(W, H);
        session.static_mask = vec![false; H];
        session.static_votes = vec![0; H];
        let prev = make_frame(W, H, 0, fixed, 0);
        let new = make_frame(W, H, 8, fixed, 0);
        let prev_sigs = row_signatures(&prev, W, H);
        let new_sigs = row_signatures(&new, W, H);
        update_static_mask(&mut session, &prev_sigs, &new_sigs, 8);
        for r in 0..fixed {
            assert!(session.static_mask[r], "顶部行 {} 应标记为固定", r);
        }
    }

    #[test]
    fn test_update_static_mask_bottom() {
        let fb = 10;
        let mut session = make_test_session(W, H);
        session.static_mask = vec![false; H];
        session.static_votes = vec![0; H];
        let prev = make_frame(W, H, 0, 0, fb);
        let new = make_frame(W, H, 8, 0, fb);
        let prev_sigs = row_signatures(&prev, W, H);
        let new_sigs = row_signatures(&new, W, H);
        update_static_mask(&mut session, &prev_sigs, &new_sigs, 8);
        let detected = (H - fb..H).filter(|&r| session.static_mask[r]).count();
        assert_eq!(detected, fb, "底部固定区应全部被标记, 实际 {}", detected);
    }

    #[test]
    fn test_append_dedup_top_toolbar() {
        let fixed = 12;
        let mut session = make_test_session(W, H);
        let f1 = make_frame(W, H, 0, fixed, 0);
        append_frame(&mut session, f1);
        assert_eq!(session.total_rows, H);
        // 多帧滚动让 mask 充分生效
        let mut offset = 0;
        for step in 0..8 {
            offset += 6;
            let f = make_frame(W, H, offset, fixed, 0);
            append_frame(&mut session, f);
            // 顶部固定色 (210) 只应出现在首帧块(行 0..fixed)
            for r in H..session.total_rows {
                assert_ne!(
                    row_value(&session.buf, W, r),
                    210,
                    "body 行 {} (step={}) 含顶部固定区像素",
                    r,
                    step
                );
            }
        }
        // body 应有实质增长
        assert!(
            session.total_rows > H + 10,
            "body 应增长, total_rows={}",
            session.total_rows
        );
    }

    #[test]
    fn test_append_no_fixed_passthrough() {
        // 无固定区:行为与原算法一致, body 持续增长
        let mut session = make_test_session(W, H);
        let f1 = make_frame(W, H, 0, 0, 0);
        append_frame(&mut session, f1);
        let f2 = make_frame(W, H, 10, 0, 0);
        let (added, _) = append_frame(&mut session, f2);
        assert_eq!(added, 10);
        assert_eq!(session.total_rows, H + 10);
    }

    #[test]
    fn test_backward_trims_body() {
        let mut session = make_test_session(W, H);
        let f1 = make_frame(W, H, 0, 0, 0);
        append_frame(&mut session, f1);
        let f2 = make_frame(W, H, 10, 0, 0);
        append_frame(&mut session, f2);
        let after_fwd = session.total_rows;
        assert_eq!(after_fwd, H + 10);
        // 反向滚动:f3 = f1(回退到首帧)
        let f3 = make_frame(W, H, 0, 0, 0);
        append_frame(&mut session, f3);
        assert!(
            session.total_rows <= after_fwd,
            "反向滚动应缩减 body, total_rows={}",
            session.total_rows
        );
    }
}
