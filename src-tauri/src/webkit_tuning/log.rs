//! webkit_tuning 模块统一日志辅助。
//!
//! - [`component_status`]：单个组件 install 完成后的生效状态记录（Req 7.3）。
//! - [`event`]：一次 show/hide/resize 事件结束前的步骤列表记录（Req 7.4）。
//!
//! 全部走 `::log::info!(target: "webkit_tuning", ...)`；输出格式与 design.md
//! 「日志记录格式」一致：
//! - `component=<Name> status=<启用|已回退|已禁用>`
//! - `component=<Name> status=<启用|已回退|已禁用> reason=<...>`
//! - `event=<show|hide|resize> steps=[step1, step2, ...]`
//!
//! 模块名 `log` 与外部 `log` crate 同名；模块内对外部 crate 的访问统一使用
//! `::log::xxx` 绝对路径以避免歧义。
//!
//! 写入耗时上限：每次写入用 [`std::time::Instant`] 自查，超过 10ms 触发
//! `debug_assert`（Req 7.4），release 构建无运行期开销。

#![allow(dead_code)]

use std::time::Instant;

/// 单次 show/hide/resize 事件的步骤列表。
///
/// 使用 `Vec<&'static str>` 保持零依赖；步骤名一律取静态字符串字面量
/// （`pre-show` / `prepare-show` / `await-paint-ok` / `focus` 等）。
pub(crate) type Steps = Vec<&'static str>;

/// 组件 install 完成后的生效状态。design.md 「日志记录格式」要求三种取值，
/// 对应中文展示名由 [`Status::as_zh`] 返回。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Enabled,
    Fallback,
    Disabled,
}

impl Status {
    /// 中文展示名，对应 design.md `status=` 字段的合法取值集
    /// `{"启用","已回退","已禁用"}`。
    pub(crate) fn as_zh(self) -> &'static str {
        match self {
            Status::Enabled => "启用",
            Status::Fallback => "已回退",
            Status::Disabled => "已禁用",
        }
    }
}

/// 单次写入耗时上限（毫秒）。Req 7.4 规定单条事件日志写入耗时 ≤10ms。
const LOG_BUDGET_MS: u128 = 10;

/// 单次写入耗时自查；超阈值在 debug 构建中触发 assert，release 构建被编译掉。
#[inline]
fn check_budget(start: Instant) {
    let elapsed = start.elapsed();
    debug_assert!(
        elapsed.as_millis() < LOG_BUDGET_MS,
        "[webkit_tuning] log write took {:?} (>{}ms budget)",
        elapsed,
        LOG_BUDGET_MS
    );
}

/// 输出一条组件状态日志。
///
/// 格式（与 design.md「日志记录格式」严格对齐）：
/// - 无 reason：`component=<Name> status=<启用|已回退|已禁用>`
/// - 有 reason：`component=<Name> status=<启用|已回退|已禁用> reason=<...>`
pub fn component_status(name: &str, status: Status, reason: Option<&str>) {
    let start = Instant::now();
    match reason {
        Some(r) => ::log::info!(
            target: "webkit_tuning",
            "component={} status={} reason={}",
            name,
            status.as_zh(),
            r
        ),
        None => ::log::info!(
            target: "webkit_tuning",
            "component={} status={}",
            name,
            status.as_zh()
        ),
    }
    check_budget(start);
}

/// 输出一条事件日志。
///
/// 格式（与 design.md「日志记录格式」严格对齐）：
/// `event=<show|hide|resize> steps=[step1, step2, ...]`
///
/// 步骤顺序与传入的 [`Steps`] 顺序一致。
pub fn event(name: &str, steps: &Steps) {
    let start = Instant::now();
    ::log::info!(
        target: "webkit_tuning",
        "event={} steps=[{}]",
        name,
        steps.join(", ")
    );
    check_budget(start);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    /// 捕获日志记录的 sink：仅保留 target == "webkit_tuning" 的
    /// `(target, level, message)` 三元组，避免其他 crate 日志污染断言。
    struct CapturingLogger {
        records: Mutex<Vec<(String, ::log::Level, String)>>,
    }

    impl ::log::Log for CapturingLogger {
        fn enabled(&self, _: &::log::Metadata) -> bool {
            true
        }

        fn log(&self, record: &::log::Record) {
            if record.target() == "webkit_tuning" {
                let mut g = self.records.lock().unwrap();
                g.push((
                    record.target().to_string(),
                    record.level(),
                    record.args().to_string(),
                ));
            }
        }

        fn flush(&self) {}
    }

    /// 全进程仅安装一次的 sink。`log::set_logger` 全局一次性，
    /// 用 OnceLock 保证只尝试装载一次。
    static SINK: OnceLock<&'static CapturingLogger> = OnceLock::new();

    /// 测试串行锁。多个测试共享同一 sink，必须串行执行才能保证
    /// `clear() → 调用 log API → snapshot()` 的隔离性。
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    fn init_sink() -> &'static CapturingLogger {
        SINK.get_or_init(|| {
            let logger: &'static CapturingLogger = Box::leak(Box::new(CapturingLogger {
                records: Mutex::new(Vec::new()),
            }));
            // 进程内 set_logger 仅成功一次；若已有其他 logger 装载（极少见），
            // 忽略错误：OnceLock 仍只尝试一次，本 sink 不工作时测试会立刻可见地失败。
            let _ = ::log::set_logger(logger);
            ::log::set_max_level(::log::LevelFilter::Trace);
            logger
        })
    }

    fn clear(sink: &CapturingLogger) {
        sink.records.lock().unwrap().clear();
    }

    fn snapshot(sink: &CapturingLogger) -> Vec<(String, ::log::Level, String)> {
        sink.records.lock().unwrap().clone()
    }

    /// 单元测试 1：无 reason 的 Enabled，输出 `status=启用` 且不含 `reason=`。
    /// Validates: Requirements 7.3
    #[test]
    fn component_status_enabled_writes_zh() {
        // 用 ok() 容忍其它测试 panic 时的 poisoned 状态；本测试自身不写入共享数据。
        let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let sink = init_sink();
        clear(sink);

        component_status("Throttling_Suppressor", Status::Enabled, None);

        let recs = snapshot(sink);
        assert_eq!(recs.len(), 1, "应当恰好一条 webkit_tuning 日志");
        let (target, level, msg) = &recs[0];
        assert_eq!(target, "webkit_tuning");
        assert_eq!(*level, ::log::Level::Info);
        assert!(msg.contains("component=Throttling_Suppressor"), "msg = {msg}");
        assert!(msg.contains("status=启用"), "msg = {msg}");
        assert!(!msg.contains("reason="), "无 reason 时不应出现 reason= 字段");
    }

    /// 单元测试 2：Fallback + reason，输出 `status=已回退 reason=spi-missing`。
    /// Validates: Requirements 7.3
    #[test]
    fn component_status_fallback_with_reason() {
        let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let sink = init_sink();
        clear(sink);

        component_status(
            "Presentation_Coordinator",
            Status::Fallback,
            Some("spi-missing"),
        );

        let recs = snapshot(sink);
        assert_eq!(recs.len(), 1);
        let (_, _, msg) = &recs[0];
        assert!(msg.contains("component=Presentation_Coordinator"), "msg = {msg}");
        assert!(msg.contains("status=已回退"), "msg = {msg}");
        assert!(msg.contains("reason=spi-missing"), "msg = {msg}");
    }

    /// 单元测试 3：Disabled + reason，输出 `status=已禁用`。
    /// Validates: Requirements 7.3
    #[test]
    fn component_status_disabled_writes_zh() {
        let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let sink = init_sink();
        clear(sink);

        component_status("Emoji_Warmer", Status::Disabled, Some("warmer-failed"));

        let recs = snapshot(sink);
        assert_eq!(recs.len(), 1);
        let (_, _, msg) = &recs[0];
        assert!(msg.contains("component=Emoji_Warmer"), "msg = {msg}");
        assert!(msg.contains("status=已禁用"), "msg = {msg}");
        assert!(msg.contains("reason=warmer-failed"), "msg = {msg}");
    }

    /// 单元测试 4：status 中文展示值必须落在合法集 `{"启用","已回退","已禁用"}`。
    /// 这条不依赖日志 sink，纯函数断言。
    /// Validates: Requirements 7.3
    #[test]
    fn status_zh_value_in_allowed_set() {
        let allow = ["启用", "已回退", "已禁用"];
        for s in [Status::Enabled, Status::Fallback, Status::Disabled] {
            assert!(
                allow.contains(&s.as_zh()),
                "{:?} 的中文展示值 {} 不在合法集",
                s,
                s.as_zh()
            );
        }
    }

    /// 单元测试 5：event 输出严格保持步骤顺序。
    /// Validates: Requirements 7.4
    #[test]
    fn event_outputs_steps_in_order() {
        let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let sink = init_sink();
        clear(sink);

        let steps: Steps = vec!["pre-show", "prepare-show", "await-paint-ok", "focus"];
        event("show", &steps);

        let recs = snapshot(sink);
        assert_eq!(recs.len(), 1);
        let (target, level, msg) = &recs[0];
        assert_eq!(target, "webkit_tuning");
        assert_eq!(*level, ::log::Level::Info);
        // 严格匹配整条 message：steps=[...] 顺序 1:1 对应输入序列。
        assert_eq!(
            msg,
            "event=show steps=[pre-show, prepare-show, await-paint-ok, focus]"
        );
    }

    /// 单元测试 6：空步骤列表渲染为 `steps=[]`，不引入多余分隔符。
    /// Validates: Requirements 7.4
    #[test]
    fn event_empty_steps_renders_brackets() {
        let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let sink = init_sink();
        clear(sink);

        let steps: Steps = vec![];
        event("hide", &steps);

        let recs = snapshot(sink);
        assert_eq!(recs.len(), 1);
        let (_, _, msg) = &recs[0];
        assert_eq!(msg, "event=hide steps=[]");
    }
}
