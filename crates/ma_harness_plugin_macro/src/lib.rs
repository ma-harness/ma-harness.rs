//! ma_harness_plugin_macro — 插件 proc-macro crate
//!
//! **公开 crate** (2026-08-18 锁定). proc-macro, API 锁.
//!
//! 提供 5 个 attribute / derive macro:
//!
//! | Macro | 形态 | 作用 | 复杂度 |
//! |---|---|---|---|
//! | `#[dsh_service]` | derive | 自动 impl Service trait | 薄糖 |
//! | `#[dsh_listener]` | derive + `#[dsh_listener::on]` | 自动 impl Listener trait | 薄糖 |
//! | `#[dsh_tool]` | attribute | 注册 model-callable 工具, 生成 schema | 重头 |
//! | `#[dsh_command]` | attribute | 注册 CLI 指令, 集成 clap | 重头 |
//! | `#[dsh_handler]` | attribute | 注册 model adapter | 重头 |
//!
//! `ctx_key!` macro_rules! 移到 `ma_harness_seam` (proc-macro crate 不能 export macro_rules!).
//!
//! 详细设计见 `docs/macro-design.md`.

#![deny(unsafe_code)]
#![warn(missing_docs)]
#![allow(missing_docs)] // 2026-08-18: 内部 crate, 暂不强制 doc (Phase 2 release 前补)

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{parse_macro_input, DeriveInput, ItemFn};

// ============================================================================
// ctx_key! — 已移到 ma_harness_seam (proc-macro crate 不允许 export macro_rules!)
// ============================================================================
//
// 使用方式:
//   use ma_harness_seam::ctx_key;
//   static MY_KEY: CtxKey<String> = ctx_key!("my_key");

// ============================================================================
// #[dsh_service] — derive
// ============================================================================

/// 自动实现 [`ma_harness_cordis::Service`] trait.
///
/// 用户**仍需**手写 `fn install(ctx) -> Result<Self, Error>` (无法 derive),
/// macro 只生成 trait impl 的 boilerplate.
///
/// # 用法
///
/// ```ignore
/// use ma_harness_plugin_macro::dsh_service;
/// use ma_harness_cordis::Service;
///
/// #[dsh_service]
/// pub struct MyService {
///     _field: String,
/// }
///
/// // 仍需手写 install + name:
/// impl MyService {
///     pub fn new(ctx: &Context) -> Result<Self, anyhow::Error> {
///         Ok(Self { _field: "hi".to_string() })
///     }
/// }
/// ```
#[proc_macro_derive(DshService, attributes(dsh_service))]
pub fn dsh_service(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as DeriveInput);
    let name = &ast.ident;
    let (impl_generics, ty_generics, where_clause) = ast.generics.split_for_impl();

    let expanded = quote! {
        impl #impl_generics ::ma_harness_cordis::Service for #name #ty_generics #where_clause {
            type Ctx = ::ma_harness_cordis::Context;
            // Error 由用户在自己 impl 里指定, 这里 default = anyhow::Error
            type Error = ::anyhow::Error;
            // install 由用户自己 impl, 这里只确保 Service trait bound 满足
        }
    };
    expanded.into()
}

// ============================================================================
// #[dsh_listener] — derive + #[dsh_listener::on(Event::X)] helper
// ============================================================================

/// 标记一个 struct 是 listener, 配合 `#[dsh_listener::on(Event::X)]` 标注方法
///
/// **双重宏**: struct 上 #[dsh_listener] + impl 内 fn 上 #[dsh_listener::on(...)]
/// 配对使用。
///
/// # 用法
///
/// ```ignore
/// use ma_harness_plugin_macro::{dsh_listener, dsh_listener::on};
/// use ma_harness_cordis::{Context, Listener, ListenerEvent};
///
/// #[derive(ListenerEvent)]
/// pub struct SessionStart;
///
/// #[dsh_listener]
/// pub struct MyListener;
///
/// impl MyListener {
///     #[on(SessionStart)]
///     async fn on_session_start(&self, _ctx: &Context, _ev: &SessionStart) -> anyhow::Result<()> {
///         // ...
///         Ok(())
///     }
/// }
/// ```
#[proc_macro_derive(DshListener, attributes(dsh_listener))]
pub fn dsh_listener(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as DeriveInput);
    let name = &ast.ident;
    let (impl_generics, ty_generics, where_clause) = ast.generics.split_for_impl();

    // Phase 1: macro 自身不展开 impl (因为没法静态扫描 impl 块的 #[on] 方法).
    // 用户需要自己 impl Service for MyListener, 或者 Week 2 加 build script.
    //
    // 简化: macro 只生成 "标记" impl, 让用户知道 struct 是 listener.
    let expanded = quote! {
        impl #impl_generics #name #ty_generics #where_clause {
            /// 标记: 这个 struct 是 listener. 配 #[dsh_listener::on(...)] 标注方法.
            #[allow(dead_code)]
            pub const __DSH_LISTENER: () = ();
        }
    };
    expanded.into()
}

/// `#[dsh_listener::on(Event::X)]` 标注 listener 方法订阅哪个 event
///
/// Phase 1: 标记用, 不生成额外代码 (用户自己写 ctx.on 注册).
#[proc_macro_attribute]
pub fn on(_attr: TokenStream, item: TokenStream) -> TokenStream {
    // 直接透传 fn
    item
}

// ============================================================================
// #[dsh_tool] — attribute (model-callable 工具)
// ============================================================================

/// 注册一个 model-callable 工具, 从函数签名提取 JSON Schema.
///
/// Phase 1 骨架: 生成 `tool_schema()` 和 `tool_invoke(json)` 入口,
/// 不依赖 schemars (签名简单手动提取). Phase 2 加 schemars 完整提取.
///
/// # 约束 (Phase 1)
///
/// - `async fn` (必填)
/// - 返回 `Result<T, E>` (T 走 serde, E 走 Display)
/// - 参数类型支持: String / 数字 / bool / Vec<T> (嵌套支持)
/// - 文档注释提取 (字段 doc → schema description)
///
/// # 完整规范
///
/// 见 `docs/macro-design.md` §4
#[proc_macro_attribute]
pub fn dsh_tool(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input_fn = parse_macro_input!(item as ItemFn);

    let fn_name = &input_fn.sig.ident;
    let fn_vis = &input_fn.vis;
    let fn_asyncness = &input_fn.sig.asyncness;
    let fn_inputs = &input_fn.sig.inputs;
    let fn_output = &input_fn.sig.output;
    let fn_body = &input_fn.block;
    let fn_attrs = &input_fn.attrs;

    // 提取 doc comment 作为 description
    let mut description = String::new();
    for attr in fn_attrs {
        if attr.path().is_ident("doc") {
            if let syn::Meta::NameValue(nv) = &attr.meta {
                if let syn::Expr::Lit(lit) = &nv.value {
                    if let syn::Lit::Str(s) = &lit.lit {
                        if !description.is_empty() {
                            description.push('\n');
                        }
                        description.push_str(s.value().trim());
                    }
                }
            }
        }
    }

    // 提取参数 (排除 receiver)
    let mut param_names: Vec<&syn::Ident> = Vec::new();
    let mut param_types: Vec<&syn::Type> = Vec::new();
    for input in fn_inputs.iter() {
        if let syn::FnArg::Typed(pat_type) = input {
            if let syn::Pat::Ident(pat_ident) = &*pat_type.pat {
                param_names.push(&pat_ident.ident);
                param_types.push(&pat_type.ty);
            }
        }
    }

    // 生成 schema 函数
    let schema_fn_name = quote::format_ident!("{}_schema", fn_name);
    let invoke_fn_name = quote::format_ident!("{}_invoke", fn_name);
    let register_fn_name = quote::format_ident!("{}_register", fn_name);

    // 参数 schema 字段 (Phase 1: 全当 string)
    // 2026-08-18: 用 collect() 避免后面重复使用 iterator 时 moved 错
    let param_schema_fields: Vec<_> = param_names.iter().zip(param_types.iter()).map(|(name, _ty)| {
        quote! {
            serde_json::json!({
                "name": stringify!(#name),
                "type": "string",
                "description": "",
            })
        }
    }).collect();

    let _params_json_value = quote! {
        serde_json::Value::Array(vec![#(#param_schema_fields),*])
    };

    // invoke 函数: 解析 JSON args, 调原函数
    // Phase 1 简化: 参数全当 String, 传原函数 (用 serde_json::from_value)
    let invoke_args = param_names.iter().map(|name| {
        quote! {
            let #name: serde_json::Value = args.get(stringify!(#name))
                .cloned()
                .ok_or_else(|| ::anyhow::anyhow!("missing arg: {}", stringify!(#name)))?;
        }
    });

    let invoke_call_args = param_names.iter().map(|name| {
        quote! { #name }
    });

    let expanded = quote! {
        // 1. 保留原函数 (用户写的不变)
        #(#fn_attrs)*
        #fn_vis #fn_asyncness fn #fn_name(#fn_inputs) #fn_output #fn_body

        // 2. schema 函数
        #fn_vis fn #schema_fn_name() -> ::ma_harness_core::ToolSchema {
            ::ma_harness_core::ToolSchema {
                name: stringify!(#fn_name).to_string(),
                description: #description.to_string(),
                parameters: ::serde_json::json!({
                    "type": "object",
                    "properties": serde_json::Value::Object(
                        [#(#param_schema_fields),*]
                            .into_iter()
                            .map(|v| {
                                let name = v["name"].as_str().unwrap().to_string();
                                (name, v)
                            })
                            .collect()
                    ),
                }),
            }
        }

        // 3. invoke 函数: JSON args -> 调原函数
        #fn_vis async fn #invoke_fn_name(
            args: ::serde_json::Value,
            ctx: &::ma_harness_cordis::Context,
        ) -> ::anyhow::Result<::serde_json::Value> {
            #(#invoke_args)*
            let result = #fn_name(#(#invoke_call_args),*).await?;
            ::serde_json::to_value(result).map_err(Into::into)
        }

        // 4. register 函数: 装进 ToolRegistry
        // Phase 1 简化: 提供 stub, 实际 ToolRegistry 还没建
        #fn_vis fn #register_fn_name(_registry: &mut ()) {
            // TODO: Week 2 接入 ma_harness_seam::ToolRegistry
            // registry.register(stringify!(#fn_name), #schema_fn_name(), invoke_fn);
        }
    };

    expanded.into()
}

// ============================================================================
// #[dsh_command] — attribute (clap 集成)
// ============================================================================

/// 注册一个 CLI/REPL 指令, 集成 clap 4.x
///
/// Phase 1 骨架: 生成 `clap::Command` + `dispatch(matches, ctx)` 入口.
///
/// 完整规范见 `docs/macro-design.md` §5
#[proc_macro_attribute]
pub fn dsh_command(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input_fn = parse_macro_input!(item as ItemFn);

    let fn_name = &input_fn.sig.ident;
    let fn_vis = &input_fn.vis;
    let fn_asyncness = &input_fn.sig.asyncness;
    let fn_inputs = &input_fn.sig.inputs;
    let fn_output = &input_fn.sig.output;
    let fn_body = &input_fn.block;
    let fn_attrs = &input_fn.attrs;

    // 提取 doc comment 作为 about
    let mut about = String::new();
    for attr in fn_attrs {
        if attr.path().is_ident("doc") {
            if let syn::Meta::NameValue(nv) = &attr.meta {
                if let syn::Expr::Lit(lit) = &nv.value {
                    if let syn::Lit::Str(s) = &lit.lit {
                        if !about.is_empty() {
                            about.push('\n');
                        }
                        about.push_str(s.value().trim());
                    }
                }
            }
        }
    }

    let clap_cmd_name = quote::format_ident!("{}_clap_cmd", fn_name);
    let dispatch_fn_name = quote::format_ident!("{}_dispatch", fn_name);

    // Phase 1 简化: 不处理 #[arg(...)] (需要用户自己用 clap::Command::arg 加)
    let expanded = quote! {
        #(#fn_attrs)*
        #fn_vis #fn_asyncness fn #fn_name(#fn_inputs) #fn_output #fn_body

        // 2. clap::Command 入口
        #fn_vis fn #clap_cmd_name() -> ::clap::Command {
            ::clap::Command::new(stringify!(#fn_name))
                .about(#about)
        }

        // 3. dispatch 函数
        #fn_vis async fn #dispatch_fn_name(
            matches: &::clap::ArgMatches,
            ctx: &::ma_harness_cordis::Context,
        ) -> ::anyhow::Result<()> {
            // Phase 1: 不解析参数, 直接调 (参数都走 ctx 拿)
            #fn_name(ctx).await
        }
    };

    expanded.into()
}

// ============================================================================
// #[dsh_handler] — attribute (model adapter)
// ============================================================================

/// 注册一个 model adapter (接 LLM API 的处理函数)
///
/// 完整规范见 `docs/macro-design.md` §6
///
/// Phase 1 骨架: macro 本身是标记, 不生成注册代码 (Phase 2 接入 AdapterRegistry).
#[proc_macro_attribute]
pub fn dsh_handler(_attr: TokenStream, item: TokenStream) -> TokenStream {
    // Phase 1: 直接透传, 不做处理
    item
}

// ============================================================================
// 内部辅助: 提取 ItemFn 公共部分
// ============================================================================

#[allow(dead_code)]
fn _unused() {
    let _f: TokenStream2 = quote! {};
}
