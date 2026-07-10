//! 滚动截屏会话状态与全局 static。

use std::sync::atomic::{AtomicBool, AtomicU64};

pub struct ScrollSession {
    pub sel_x: f64,
    pub sel_y: f64,
    pub sel_w: f64,
    pub sel_h: f64,
    #[allow(dead_code)]
    pub scale: f64,
    pub pw: usize,
    pub ph_per_frame: usize,
    pub buf: Vec<u8>,
    pub total_rows: usize,
    pub prev_frame: Vec<u8>,
    pub overlay_window_id: u32,
    pub ns_window_addr: usize,
    pub ignoring_mouse: bool,
    pub emit_seq: u64,
    pub static_mask: Vec<bool>,
    pub static_votes: Vec<u32>,
    pub toolbar_rect: Option<(f64, f64, f64, f64)>,
    pub static_streak: u32,
}

impl ScrollSession {
    pub fn row_bytes(&self) -> usize {
        self.pw * 4
    }
}

pub static SESSION: std::sync::Mutex<Option<ScrollSession>> = std::sync::Mutex::new(None);
pub static IS_RUNNING: AtomicBool = AtomicBool::new(false);
pub static LAST_EMIT_NS: AtomicU64 = AtomicU64::new(0);
pub static PENDING_TOOLBAR: std::sync::Mutex<Option<(f64, f64, f64, f64)>> =
    std::sync::Mutex::new(None);
