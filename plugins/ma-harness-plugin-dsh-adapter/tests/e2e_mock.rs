//! E2E 单测: 跑 mock node 验证 spawn + JSON-RPC initialize + tools/list + tools/call + shutdown
//!
//! P13.1: mock node inline script minimal JSON-RPC echo server
//! P13.2: 加 register_to + ToolRegistry::invoke 集成测试

use std::path::Path;

use ma_harness_core::ToolRegistry;
use ma_harness_cordis::Context;
use ma_harness_plugin_dsh_adapter::{DshAdapter, DshConfig};
use serde_json::json;

fn require_node() -> Option<std::path::PathBuf> {
    let path_env = std::env::var("PATH").ok()?;
    let sep = if cfg!(windows) { ';' } else { ':' };
    let exts: &[&str] = if cfg!(windows) {
        &["", ".exe", ".cmd", ".bat"]
    } else {
        &[""]
    };
    for dir in path_env.split(sep) {
        if dir.is_empty() {
            continue;
        }
        for ext in exts {
            let candidate = std::path::PathBuf::from(dir).join(format!("node{ext}"));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

#[tokio::test]
async fn smoke_spawn_initialize_list_call_shutdown() {
    if require_node().is_none() {
        eprintln!("[skip] node not in PATH");
        return;
    }
    let adapter =
        DshAdapter::spawn(Path::new("mock://inline"), DshConfig::default())
            .await
            .expect("spawn");
    let server_info = adapter.initialize().await.expect("initialize");
    assert_eq!(server_info.name, "mock-dsh-server");
    let tools = adapter.list_tools().await.expect("tools/list");
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].name, "echo");
    let result = adapter
        .call_tool("echo", json!({"msg": "hello"}))
        .await
        .expect("call");
    assert!(!result.is_error);
    let _ = adapter.call_tool("nonexistent", json!({})).await;
    adapter.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn smoke_register_to_tool_registry_and_invoke() {
    if require_node().is_none() {
        eprintln!("[skip] node not in PATH");
        return;
    }
    let adapter =
        DshAdapter::spawn(Path::new("mock://inline"), DshConfig::default())
            .await
            .expect("spawn");
    adapter.initialize().await.expect("initialize");
    let registry = ToolRegistry::new();
    let schemas = adapter
        .clone()
        .register_to(&registry)
        .await
        .expect("register_to");
    assert_eq!(schemas.len(), 1);

    let ctx = Context::new();
    let result = registry
        .invoke("echo", json!({"msg": "via registry"}), ctx)
        .await
        .expect("invoke echo");
    let obj = result.as_object().expect("object");
    assert_eq!(
        obj.get("echoed").and_then(|v| v.as_str()),
        Some("via registry")
    );

    let ctx = Context::new();
    let result = registry.invoke("echo", json!({}), ctx).await;
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("schema validation") || err.contains("missing required"));

    let ctx = Context::new();
    let result = registry.invoke("nonexistent", json!({}), ctx).await;
    assert!(result.is_err());

    adapter.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn smoke_initialize_then_list_tools_then_shutdown() {
    if require_node().is_none() {
        eprintln!("[skip] node not in PATH");
        return;
    }
    let adapter =
        DshAdapter::spawn(Path::new("mock://inline"), DshConfig::default())
            .await
            .expect("spawn");
    adapter.initialize().await.expect("init");
    let tools = adapter.list_tools().await.expect("list");
    assert!(!tools.is_empty());
    adapter.shutdown().await.expect("shutdown");
}
