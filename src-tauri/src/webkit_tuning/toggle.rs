#![allow(dead_code)]
//! Tuning_Toggle：决定整组 webkit_tuning 驯化逻辑是否启用。
//!
//! 读取环境变量 `VOIDNIX_DISABLE_WEBKIT_TUNING`，仅当字符串值精确等于 `"1"` 时
//! 视为禁用；未设置、空串、`"0"`、`"true"` 等其他任意取值一律启用（Req 7.1/7.2）。
//!
//! 由于 `is_enabled()` 通过 `Lazy<bool>` 一次性快照环境变量，运行期无法再被改动。
//! 为了让跨组件 PBT（T10）能在不同 toggle 状态下驱动 install/show/hide/resize，
//! 在 `cfg(test)` 下额外提供 `override_enabled` / `clear_override` 用于注入；
//! 生产构建路径仍走纯 `*ENABLED`，无锁开销。

use once_cell::sync::Lazy;

/// 决策纯函数：根据环境变量字符串值决定是否启用 webkit_tuning 驯化。
///
/// 仅当 `s == Some("1")` 时禁用，其他情形（None / "0" / "true" / 任意 Unicode 串）均启用。
/// 抽出 pure 函数便于 Property 12 直接覆盖决策逻辑，避免依赖一次性快照的 `ENABLED`。
pub(crate) fn decide(s: Option<&str>) -> bool {
    s != Some("1")
}

/// 进程级 Toggle 快照：启动时读取环境变量并固化。
static ENABLED: Lazy<bool> = Lazy::new(|| {
    decide(std::env::var("VOIDNIX_DISABLE_WEBKIT_TUNING").ok().as_deref())
});

/// 仅 cfg(test)：可被 PBT 注入的覆盖值。生产构建不编译此 static，确保零开销。
#[cfg(test)]
static OVERRIDE: std::sync::Mutex<Option<bool>> = std::sync::Mutex::new(None);

/// 当前是否启用 webkit_tuning 驯化逻辑。
///
/// 生产路径直接读取 `*ENABLED`；测试构建优先读取 `OVERRIDE`，未设置时回落到 `ENABLED`。
#[inline]
pub fn is_enabled() -> bool {
    #[cfg(test)]
    {
        if let Ok(guard) = OVERRIDE.lock() {
            if let Some(v) = *guard {
                return v;
            }
        }
    }
    *ENABLED
}

/// 仅 cfg(test)：覆盖 ENABLED 决策，供跨组件 PBT 在不同 toggle 状态下驱动主流程。
#[cfg(test)]
pub(crate) fn override_enabled(v: bool) {
    *OVERRIDE.lock().unwrap() = Some(v);
}

/// 仅 cfg(test)：清除 override，恢复读取 ENABLED 快照。
#[cfg(test)]
pub(crate) fn clear_override() {
    *OVERRIDE.lock().unwrap() = None;
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn decide_disabled_only_for_string_one() {
        // 仅 Some("1") 视为禁用，其它均启用。
        assert!(!decide(Some("1")));
        assert!(decide(Some("0")));
        assert!(decide(Some("true")));
        assert!(decide(Some("")));
        assert!(decide(Some(" 1")));
        assert!(decide(Some("1 ")));
        assert!(decide(None));
    }

    #[test]
    fn override_takes_precedence() {
        // 注入 false：is_enabled 必须返回 false。
        override_enabled(false);
        assert!(!is_enabled());
        // 注入 true：is_enabled 必须返回 true。
        override_enabled(true);
        assert!(is_enabled());
        // 清除后回落到 ENABLED 快照（不依赖具体值，只验证 API 不 panic 且可重复调用）。
        clear_override();
        let _ = is_enabled();
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        // Feature: webkit-presentation-tuning, Property 12: Tuning_Toggle 二值合同。
        // 对任意环境变量字符串值 s（含 None / 空串 / 任意非控制 Unicode 0~16 字符），
        // decide(s) ↔ s != Some("1")。
        // Validates: Requirements 7.1, 7.2
        #[test]
        fn property_12_decide_binary_contract(s in proptest::option::of("\\PC{0,16}")) {
            let expected = s.as_deref() != Some("1");
            prop_assert_eq!(decide(s.as_deref()), expected);
        }
    }
}
