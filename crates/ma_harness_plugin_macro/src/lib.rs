//! ma_harness_plugin_macro — 插件 proc-macro crate
//!
//! **公开 crate** (2026-08-18 锁定). proc-macro, API 锁.
//! 5 个宏: `#[dsh_service]` / `#[dsh_listener]` / `#[dsh_tool]` / `#[dsh_command]` / `#[dsh_handler]`.
//! 详细设计见 `docs/macro-design.md`.

#![deny(unsafe_code)]
#![warn(missing_docs)]

// Week 2 起实现, 当前是空 proc-macro crate, 5 个 macro 暂未导出.
// 至少 1 个 token 导出 (否则 crate 是空 lib, proc-macro 编译不过).
//
// 占位: 抛 "not yet implemented" 给编译期 caller
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

// 暂不引入 syn/quote/proc-macro2 为硬依赖, 因为 syn 解析器还没写.
// 等 Week 2 第一个真实 macro 落地, 一次性加完整依赖.
