//! ma_harness_plugin_macro — 插件 proc-macro crate
//!
//! **公开 crate** (2026-08-18 锁定). proc-macro, API 锁.
//! 5 个 macro (Week 2-3 实现): `#[dsh_service]` / `#[dsh_listener]` / `#[dsh_tool]` / `#[dsh_command]` / `#[dsh_handler]`.
//! 1 个 macro_rules! (Week 1 Day 3 实现): `ctx_key!`.
//!
//! 详细设计见 `docs/macro-design.md`.

#![deny(unsafe_code)]
#![warn(missing_docs)]

// ============================================================================
// Week 1 Day 3: ctx_key! macro_rules! (编译期 snake_case 校验)
// ============================================================================

/// 构造一个 [`ma_harness_cordis::CtxKey`], 编译期 reject 非 snake_case 名字.
///
/// # 用法
///
/// ```ignore
/// use ma_harness_cordis::CtxKey;
/// use ma_harness_plugin_macro::ctx_key;
///
/// static SESSION_ID: CtxKey<String> = ctx_key!("session_id");
/// static MAX_TOKENS: CtxKey<u32> = ctx_key!("max_tokens");
/// ```
///
/// # 编译期校验
///
/// 名字非 snake_case 时**编译错误** (不是运行时 panic):
///
/// ```ignore
/// // 编译失败: error[E0080]: evaluation of `main::BAD` failed
/// //   --> src/main.rs:5:30
/// //   note: index out of bounds: the len is 0 but the index is 1
/// //   说明: ctx_key! 名字 "sessionId" 不是 snake_case (位置 ctx_key! 调用点附近)
/// static BAD: CtxKey<String> = ctx_key!("sessionId");
/// ```
///
/// 规则: 全小写字母 + 下划线 + 数字, 首字符必须是小写字母或下划线.
///
/// **错误信息解读**: "index out of bounds" + 错误位置在 `ctx_key!` 调用点附近 = ctx_key! 名字不合法.
/// 真实错误信息由 `[(); 0]` / `[(); 1]` 越界 const-eval 产生, 没有漂亮文案. Week 2 加 trybuild
/// 给更友好的 error message.
///
/// # 类型推断
///
/// 类型 `T` 从使用点的 type annotation 推断, 宏本身不指定.
///
/// # 跟 `CtxKey::new` 区别
///
/// - `CtxKey::new("foo")` (const fn): 无校验, 信任调用方
/// - `CtxKey::new_checked("foo")` (runtime): panic 错误信息友好
/// - `ctx_key!("foo")` (macro): **编译期** reject, 最佳
#[macro_export]
macro_rules! ctx_key {
    ($name:expr) => {{
        // 编译期校验 snake_case.
        // 用 const block 强制在编译期跑 is_snake_case, 失败时触发 const-eval 错误.
        const __NAME: &str = $name;
        const __IS_VALID: bool = $crate::__is_snake_case(__NAME);
        // const 断言: 失败时编译错误.
        // 用 [(); N] index 越界 trick, 不用 const fn panic (更稳, 兼容更多 Rust 版本).
        //   valid (true): index 0 → ()
        //   invalid (false): index 1 → const-eval "index out of bounds" panic
        // 错误信息用户能看到. 提示信息放在变量名里.
        const _: () = [()][(!__IS_VALID) as usize];
        // 实际拿 key (unchecked, 因为已经 const 校验过).
        // 直接调 CtxKey::new_unchecked, type T 由使用点 type annotation 推断.
        ::ma_harness_cordis::CtxKey::new_unchecked(__NAME)
    }};
}

// ============================================================================
// Week 2-3 占位: 5 个 #[dsh_*] macro
// ============================================================================

use proc_macro::TokenStream;

/// 占位 proc-macro, 暂时让所有调用编译失败带清晰错误.
///
/// Week 2-3 会被真实实现替换.
#[proc_macro_attribute]
pub fn dsh_tool(_attr: TokenStream, _item: TokenStream) -> TokenStream {
    syn::Error::new(
        proc_macro2::Span::call_site(),
        "#[dsh_tool] 尚未实现, 预计 Week 2-3 落地 (见 docs/macro-design.md §4)",
    )
    .to_compile_error()
    .into()
}

/// 占位 proc-macro
#[proc_macro_derive(DshService, attributes(dsh_service))]
pub fn dsh_service(_item: TokenStream) -> TokenStream {
    syn::Error::new(
        proc_macro2::Span::call_site(),
        "#[dsh_service] 尚未实现, 预计 Week 2-3 落地 (见 docs/macro-design.md §2)",
    )
    .to_compile_error()
    .into()
}

// ============================================================================
// 内部 helper, 暴露给 ctx_key! 宏用
// ============================================================================

/// 内部: 委托给 `ma_harness_cordis::is_snake_case` (宏用, 不稳定)
#[doc(hidden)]
pub const fn __is_snake_case(s: &str) -> bool {
    ma_harness_cordis::is_snake_case(s)
}
