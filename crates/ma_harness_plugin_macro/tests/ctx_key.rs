//! 集成测试: ctx_key! macro
//!
//! 验证:
//! 1. snake_case 字面量通过
//! 2. CtxKey 类型推断正确
//! 3. CtxKey.name() 返回字面量
//!
//! 编译失败 case 用 trybuild (Week 2 加), 本测试只验 happy path.

use ma_harness_cordis::{Context, CtxKey};
use ma_harness_plugin_macro::ctx_key;

static SESSION_ID: CtxKey<String> = ctx_key!("session_id");
static MAX_TOKENS: CtxKey<u32> = ctx_key!("max_tokens");
static _UNDERSCORE_START: CtxKey<String> = ctx_key!("_private");
static WITH_DIGITS: CtxKey<i64> = ctx_key!("port_8080");

#[test]
fn snake_case_keys_compile() {
    // 上面 4 个 static 都能编译通过, 这个 test 只是触达
    assert_eq!(SESSION_ID.name(), "session_id");
    assert_eq!(MAX_TOKENS.name(), "max_tokens");
    assert_eq!(_UNDERSCORE_START.name(), "_private");
    assert_eq!(WITH_DIGITS.name(), "port_8080");
}

#[test]
fn ctx_key_works_with_context() {
    let ctx = Context::new();
    ctx.set(SESSION_ID, "abc".to_string());
    ctx.set(MAX_TOKENS, 1024u32);
    ctx.set(WITH_DIGITS, -1i64);

    assert_eq!(ctx.get(SESSION_ID), Some("abc".to_string()));
    assert_eq!(ctx.get(MAX_TOKENS), Some(1024u32));
    assert_eq!(ctx.get(WITH_DIGITS), Some(-1i64));
}

#[test]
fn const_context_works() {
    // 验证 ctx_key! 生成的 CtxKey 是真的 const-constructable
    const K: CtxKey<&'static str> = ctx_key!("const_key");
    assert_eq!(K.name(), "const_key");
}
