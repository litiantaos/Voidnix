#![allow(dead_code)]

pub mod toggle;
pub mod log;
pub mod obj_exception;
pub mod presentation;
pub mod throttling;
pub mod frame_animator;
pub mod emoji_warmer;

mod real_window;
mod entry;

#[cfg(test)]
pub(crate) mod test_support;

#[cfg(test)]
mod tests;

pub(crate) use entry::{Frame, WindowOps, PresentationBridge};

pub use entry::{install, install_screenshot, make_main_window_key, show_main, hide_main, resize_main, intercept_cmd_backspace};

#[cfg(any(test, feature = "webkit_tuning_mock"))]
pub(crate) use entry::{install_with, show_main_with, hide_main_with, resize_main_with};

#[cfg(test)]
pub(crate) use entry::uninstall_for_test;
