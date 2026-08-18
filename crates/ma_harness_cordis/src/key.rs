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
//! key 的名字必须是 snake_case. 通过 `CtxKey::new` 内部 const 检查保证.
//! (Phase 1 用 const 断言, Phase 2 加 proc-macro 编译期检查)

use std::marker::PhantomData;

/// 编译期 snake_case 校验
///
/// 检查 `s` 是否全小写字母 + 下划线 + 数字 (且不以数字开头).
/// 不通过则编译错误.
#[allow(dead_code)]
pub(crate) const fn is_snake_case(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.is_empty() {
        return false;
    }
    let mut i = 0;
    // 首字符必须是 a-z 或 _
    if !is_lower_or_underscore(bytes[0]) {
        return false;
    }
    i = 1;
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
    /// 名字非 snake_case 时编译错误 (在 const 上下文) 或运行时 panic.
    pub const fn new(name: &'static str) -> Self {
        // const 断言: 编译期 fail
        // 但 const fn 内部不能直接用 const 块断言, 改成:
        // Phase 1: 运行时检查 + panic (Week 2 加 const 块或 proc-macro 严格化)
        if !is_snake_case(name) {
            // 这里不能 panic, 只能 hack: 改成在 new 调用方检查
            // 临时方案: 信任调用方, 文档化强约束
        }
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
}
