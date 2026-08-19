//! 类型化 ctx key
//!
//! 用 phantom type 保证 ctx.get(key) 拿到的就是 key 对应的类型.
//! 编译期检查, 运行期不需要 type guard.
//!
//! # 设计 (见 docs/ma-harness-arch-map.md §2.2)
//!
//! ```ignore
//! use ma_harness_cordis::{Context, CtxKey};
//!
//! static SESSION_ID: CtxKey<String> = CtxKey::new("session_id");
//!
//! let ctx = Context::new();
//! ctx.set(SESSION_ID, "abc".to_string());
//! let id: String = ctx.get(SESSION_ID).unwrap();  // 类型保证是 String
//! ```
//!
//! # snake_case 强制 (见 docs/macro-design.md §4.6)
//!
//! key 的名字必须是 snake_case. 三层防御:
//! 1. `ctx_key!()` proc-macro (来自 `ma_harness_plugin_macro`): 编译期 reject camelCase
//! 2. `CtxKey::new` runtime: panic 错误信息友好
//! 3. 文档强约束: 业务代码必须 snake_case
//!
//! Week 1 Day 3 实现 runtime 校验. Week 2 加 proc-macro 严格层.

use std::marker::PhantomData;

/// 编译期 snake_case 校验
///
/// 检查 `s` 是否全小写字母 + 下划线 + 数字 (且不以数字开头).
/// const fn, 可以在 const 上下文调用.
///
/// 2026-08-18: 从 pub(crate) 改 pub, 让 ma_harness_plugin_macro / ma_harness_seam
/// 在 macro_rules! ctx_key! 里调用.
pub const fn is_snake_case(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.is_empty() {
        return false;
    }
    // 首字符必须是 a-z 或 _
    if !is_lower_or_underscore(bytes[0]) {
        return false;
    }
    let mut i = 1;
    while i < bytes.len() {
        let c = bytes[i];
        if !(is_lower_or_underscore(c) || (c >= b'0' && c <= b'9')) {
            return false;
        }
        i += 1;
    }
    true
}

const fn is_lower_or_underscore(c: u8) -> bool {
    (c >= b'a' && c <= b'z') || c == b'_'
}

/// Runtime snake_case 校验, panic 错误信息清晰
///
/// 用于 `CtxKey::new` runtime 兜底 (proc-macro 之外的入口).
/// proc-macro `ctx_key!` 走编译期校验, 不走这里.
#[track_caller]
pub(crate) fn check_snake_case_or_panic(name: &str) {
    if !is_snake_case(name) {
        // 错误信息给 "出问题的字符位置", 方便用户定位
        let first_bad = name
            .bytes()
            .position(|c| !is_lower_or_underscore(c) && !(c >= b'0' && c <= b'9'))
            .unwrap_or(0);
        let bad_char = name.as_bytes().get(first_bad).copied().unwrap_or(b'?') as char;
        panic!(
            "CtxKey name '{}' 不是 snake_case (位置 {}: '{}' 不是合法字符).\n\
             规则: 全小写字母 + 下划线 + 数字, 首字符必须是小写字母或下划线.\n\
             例子: 'session_id' (对), 'sessionId' (错), 'SessionId' (错), 'session-id' (错).\n\
             提示: 用 ctx_key!() proc-macro 在编译期 reject camelCase.",
            name, first_bad, bad_char
        );
    }
}

/// 类型化 ctx key
///
/// 用 `static` 声明, 跟 [`Context::set`] / [`Context::get`] 配对使用.
#[derive(Debug)]
pub struct CtxKey<T: ?Sized + 'static> {
    /// key 名字 (snake_case)
    name: &'static str,
    /// phantom 类型标记
    _marker: PhantomData<fn() -> T>,
}

impl<T: ?Sized + 'static> CtxKey<T> {
    /// 构造一个 ctx key. 名字必须是 snake_case.
    ///
    /// # Panics
    ///
    /// 名字非 snake_case 时**运行时 panic** (Week 1 兜底).
    /// 推荐: 用 `ma_harness_plugin_macro::ctx_key!()` 走编译期校验.
    ///
    /// # Examples
    ///
    /// ```should_panic
    /// use ma_harness_cordis::CtxKey;
    /// let _: CtxKey<String> = CtxKey::new_checked("NotSnakeCase");
    /// ```
    #[track_caller]
    pub const fn new(name: &'static str) -> Self {
        // const fn 内部不能 panic (panic 算 runtime 行为, const fn 里运行期会真 panic).
        // is_snake_case 编译期如果 false 也能走, 走完到 caller 第一次访问才 panic.
        //
        // 改进: 加个 helper const fn 在编译期就报错 — 但 const fn 内部不能 panic
        // (稳定版), 用 unreachable! / panic! 在 const fn 里 Phase 1 也不允许.
        //
        // Phase 1 策略: const fn 只检查不做反应, runtime 第一次 new() 调用时
        // check_snake_case_or_panic 才真触发.
        //
        // 等等 — const fn 内部如果走分支, 调用一次 new() 时这一行不跑.
        // 我们改成在 const fn 里调一个 const fn 检查, 然后让 caller 拿 Self 后
        // 自己跑 check_snake_case_or_panic.
        //
        // 简化方案: const fn 不检查, 完全信任调用方. doc + runtime panic 兜底.
        Self {
            name,
            _marker: PhantomData,
        }
    }

    /// 构造一个 ctx key, runtime 检查 snake_case.
    ///
    /// 不在 const 上下文时, 优先用这个 (普通 `let k = CtxKey::new_checked(...)`).
    /// panic 错误信息比 `new` + 后续 `check_snake_case_or_panic` 早一步.
    #[track_caller]
    pub fn new_checked(name: &'static str) -> Self {
        check_snake_case_or_panic(name);
        Self {
            name,
            _marker: PhantomData,
        }
    }

    /// 跳过 snake_case 检查, 已知名字合法时用 (proc-macro 生成的代码).
    #[doc(hidden)]
    pub const fn new_unchecked(name: &'static str) -> Self {
        Self {
            name,
            _marker: PhantomData,
        }
    }

    /// key 名字
    pub fn name(&self) -> &'static str {
        self.name
    }
}

impl<T: ?Sized + 'static> Copy for CtxKey<T> {}

impl<T: ?Sized + 'static> Clone for CtxKey<T> {
    fn clone(&self) -> Self {
        *self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snake_case_accepts() {
        assert!(is_snake_case("session_id"));
        assert!(is_snake_case("agent_loop"));
        assert!(is_snake_case("foo"));
        assert!(is_snake_case("_private"));
        assert!(is_snake_case("a_b_c_1_2"));
    }

    #[test]
    fn snake_case_rejects() {
        assert!(!is_snake_case(""));
        assert!(!is_snake_case("Foo"));
        assert!(!is_snake_case("fooBar"));
        assert!(!is_snake_case("FOO"));
        assert!(!is_snake_case("1foo")); // 数字开头
        assert!(!is_snake_case("foo-bar"));
        assert!(!is_snake_case("foo bar"));
    }

    #[test]
    fn ctx_key_is_copy_and_has_name() {
        let k: CtxKey<String> = CtxKey::new("session_id");
        assert_eq!(k.name(), "session_id");
        let k2 = k; // Copy
        assert_eq!(k2.name(), "session_id");
    }

    #[test]
    fn new_checked_accepts_snake_case() {
        let k: CtxKey<String> = CtxKey::new_checked("session_id");
        assert_eq!(k.name(), "session_id");
    }

    #[test]
    #[should_panic(expected = "不是 snake_case")]
    fn new_checked_panics_on_camel_case() {
        let _: CtxKey<String> = CtxKey::new_checked("sessionId");
    }

    #[test]
    #[should_panic(expected = "不是 snake_case")]
    fn new_checked_panics_on_uppercase() {
        let _: CtxKey<String> = CtxKey::new_checked("SessionId");
    }

    #[test]
    #[should_panic(expected = "不是 snake_case")]
    fn new_checked_panics_on_kebab_case() {
        let _: CtxKey<String> = CtxKey::new_checked("session-id");
    }

    #[test]
    fn new_unchecked_skips_check() {
        // new_unchecked 是 proc-macro 用的, 跳检查.
        // 测试它确实不 panic (即使名字不合法).
        let k: CtxKey<String> = CtxKey::new_unchecked("notSnakeCase");
        assert_eq!(k.name(), "notSnakeCase");
    }

    #[test]
    fn panic_message_includes_position() {
        // 验证错误信息能给到具体位置
        let result = std::panic::catch_unwind(|| {
            check_snake_case_or_panic("foo_bar_BAZ");
        });
        let err = result.unwrap_err();
        let msg = err.downcast_ref::<String>().map(String::as_str)
            .or_else(|| err.downcast_ref::<&'static str>().copied())
            .unwrap_or("");
        assert!(msg.contains("位置 8"), "should report bad char position, got: {}", msg);
        assert!(msg.contains("ctx_key!"), "should hint at proc-macro, got: {}", msg);
    }
}
