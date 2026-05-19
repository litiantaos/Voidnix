#![allow(dead_code)]
//! Obj-C 异常拦截 + selector 探测。
//!
//! - [`try_block`]：把 Rust 闭包通过 C 函数指针 + 裸 ctx 传到 native 的
//!   `@try/@catch` 内同步执行。任何 Obj-C 异常都被吞掉返回 false；正常返回 true。
//!   这是 Req 5.3 的唯一可行路径——objc2 不能跨语言 catch Obj-C 异常。
//! - [`responds_to_sel`]：基于 `try_block` 包装 `respondsToSelector:`，对任意
//!   selector 字符串都安全返回布尔值。Req 5.4 的运行时探测入口。

use objc2::runtime::{AnyObject, Sel};

extern "C" {
    fn voidnix_try_block(
        f: extern "C-unwind" fn(*mut std::ffi::c_void),
        ctx: *mut std::ffi::c_void,
    ) -> bool;
}

#[cfg(test)]
extern "C" {
    fn voidnix_test_throw(kind: i32);
}

/// 单调用 trampoline：从裸 ctx 还原 RefCell<Option<F>>，take 出闭包并调用一次。
/// 使用 `extern "C-unwind"` 允许 C++ 异常（Obj-C @throw 在 ObjC++ 中通过 C++ 异常
/// 机制实现）穿越此函数边界，由 native 侧 voidnix_try_block 的 @catch 捕获。
extern "C-unwind" fn trampoline<F: FnOnce()>(ctx: *mut std::ffi::c_void) {
    // SAFETY: ctx 指向 try_block 栈帧上的 RefCell<Option<F>>，由 native 侧
    // voidnix_try_block 同步调用一次，调用结束后 ctx 不再被使用。
    let cell = unsafe { &*(ctx as *const std::cell::RefCell<Option<F>>) };
    if let Some(f) = cell.borrow_mut().take() {
        f();
    }
}

/// 在 Obj-C `@try/@catch` 内执行闭包 `f`。
///
/// 返回值：
/// - `true`  闭包正常执行返回。
/// - `false` 闭包内触发了 Obj-C 异常（已被 native 侧吞掉并 NSLog），调用方应当
///   把这次操作视为失败并走 fallback 路径。
pub fn try_block<F: FnOnce()>(f: F) -> bool {
    let cell: std::cell::RefCell<Option<F>> = std::cell::RefCell::new(Some(f));
    let ctx = &cell as *const _ as *mut std::ffi::c_void;
    unsafe { voidnix_try_block(trampoline::<F>, ctx) }
}
/// 安全探测 obj 是否响应 sel：内部包在 [`try_block`] 中，任何异常返回 false；
/// `obj` 为空指针时直接返回 false，不进 native 路径。
pub fn responds_to_sel(obj: *mut AnyObject, sel: Sel) -> bool {
    if obj.is_null() {
        return false;
    }
    let mut answer = false;
    let answer_ref = &mut answer;
    let _ = try_block(|| {
        // SAFETY: obj 非空，sel 由调用方保证有效。respondsToSelector: 在
        // 接收者已释放或类型不对时会抛 NSInvalidArgumentException，由
        // try_block 拦截。
        let r: bool = unsafe { objc2::msg_send![obj, respondsToSelector: sel] };
        *answer_ref = r;
    });
    answer
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    /// EvilOp：PBT 用来构造各种异常种类。
    #[derive(Debug, Clone, Copy)]
    enum EvilOp {
        None,
        ThrowGeneric,
        ThrowInvalid,
        ThrowCustom,
    }

    impl EvilOp {
        fn kind(self) -> i32 {
            match self {
                EvilOp::None => 0,
                EvilOp::ThrowGeneric => 1,
                EvilOp::ThrowInvalid => 2,
                EvilOp::ThrowCustom => 3,
            }
        }
    }

    fn arb_evil_op() -> impl Strategy<Value = EvilOp> {
        prop_oneof![
            Just(EvilOp::None),
            Just(EvilOp::ThrowGeneric),
            Just(EvilOp::ThrowInvalid),
            Just(EvilOp::ThrowCustom),
        ]
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        // Feature: webkit-presentation-tuning, Property 8: try_block 对任意被
        // 包裹代码恒不向上抛。对任意异常种类序列，try_block 都返回布尔值不
        // panic，且后续 try_block 仍能继续工作。
        // Validates: Requirements 5.3
        #[test]
        fn property_8_try_block_does_not_propagate(
            errors in proptest::collection::vec(arb_evil_op(), 1..16)
        ) {
            for op in &errors {
                let kind = op.kind();
                let ok = try_block(|| unsafe { voidnix_test_throw(kind) });
                let expected_ok = matches!(op, EvilOp::None);
                prop_assert_eq!(ok, expected_ok);
            }
            // 异常被吞掉之后，try_block 仍可正常工作：执行一次空闭包应当返回 true。
            let after_ok = try_block(|| {});
            prop_assert_eq!(after_ok, true);
        }

        // Feature: webkit-presentation-tuning, Property 9: responds_to_sel 对
        // 任意 selector 名总不抛；对运行时不存在的方法返回 false。
        // Validates: Requirements 5.4
        #[test]
        fn property_9_responds_to_sel_safe_for_any_string(
            names in proptest::collection::vec("[A-Za-z_:0-9 ]{0,64}", 1..32)
        ) {
            use objc2::class;
            let cls = class!(NSObject);
            let obj = cls as *const _ as *mut AnyObject;
            for name in &names {
                if name.is_empty() {
                    continue;
                }
                // CString::new 在含 NUL 字节时返回 Err，跳过。
                let cstring = match std::ffi::CString::new(name.as_str()) {
                    Ok(c) => c,
                    Err(_) => continue,
                };
                // SAFETY: cstring 在 sel 使用期间存活。
                // objc2 0.6.4 的 Sel::register 接受 &CStr，不需要 unsafe。
                let sel = Sel::register(cstring.as_c_str());
                // 不 panic 即视为通过；返回值由具体 selector 决定。
                let _ = responds_to_sel(obj, sel);
            }
            // 显式覆盖空指针分支：必须返回 false 且不进 native 路径。
            prop_assert_eq!(
                responds_to_sel(std::ptr::null_mut(), objc2::sel!(description)),
                false
            );
        }
    }

    #[test]
    fn try_block_normal_returns_true() {
        let mut counter = 0;
        let counter_ref = &mut counter;
        let ok = try_block(|| {
            *counter_ref += 1;
        });
        assert!(ok);
        assert_eq!(counter, 1);
    }

    #[test]
    fn try_block_after_throw_still_works() {
        let ok1 = try_block(|| unsafe { voidnix_test_throw(1) });
        assert!(!ok1);
        let mut counter = 0;
        let counter_ref = &mut counter;
        let ok2 = try_block(|| {
            *counter_ref += 1;
        });
        assert!(ok2);
        assert_eq!(counter, 1);
    }

    #[test]
    fn responds_to_sel_existing_returns_true() {
        use objc2::class;
        let cls = class!(NSObject);
        let obj = cls as *const _ as *mut AnyObject;
        let sel = objc2::sel!(description);
        assert!(responds_to_sel(obj, sel));
    }
}
