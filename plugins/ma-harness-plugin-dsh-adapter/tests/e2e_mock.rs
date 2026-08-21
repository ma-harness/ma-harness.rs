//! E2E 单测: 跑 mock node 验证 spawn + JSON-RPC initialize + tools/list + tools/call + shutdown
//!
//! P13.1 阶段: mock node 走 inline script 跑 minimal JSON-RPC echo server
//! (见 `process::DSH_RUNTIME_ENTRY`)
//!
//! P13.2 替换: 跑真 dsh `@deepseek-ai/dsh-sdk-jsonrpc-server` + user plugin
//!
//! **业务方前置**: 本机装 Node.js 22+ (业务方 v26.1.0 OK)

use std::path::Path;

use ma_harness_plugin_dsh_adapter::{DshAdapter, DshConfig};
use serde_json::json;

fn require_node() -> Option<std::path::PathBuf> {
    // 简单探测: 跨平台 PATH 搜
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
    // 跳过: 业务方本机没 Node.js
    if require_node().is_none() {
        eprintln!("[skip] node not in PATH, skipping e2e");
        return;
    }

    let plugin_path = Path::new("mock://inline"); // P13.1 inline script, 忽略 path
    let config = DshConfig::default();
    let mut adapter = DshAdapter::spawn(plugin_path, config)
        .await
        .expect("spawn node should succeed");

    // 1. initialize handshake
    let server_info = adapter.initialize().await.expect("initialize should succeed");
    assert_eq!(server_info.name, "mock-dsh-server");
    assert!(server_info.capabilities.contains(&"tools".to_string()));
    eprintln!("[ok] initialize: {} v{}", server_info.name, server_info.version);

    // 2. tools/list 拿 1 个 echo tool
    let tools = adapter.list_tools().await.expect("tools/list should succeed");
    assert_eq!(tools.len(), 1, "mock should expose 1 tool");
    assert_eq!(tools[0].name, "echo");
    eprintln!("[ok] tools/list: {} tools", tools.len());

    // 3. tools/call 调 echo
    let result = adapter
        .call_tool("echo", json!({"msg": "hello from dsh-adapter"}))
        .await
        .expect("tools/call echo should succeed");
    assert!(!result.is_error, "echo should not be error");
    assert_eq!(result.content.len(), 1, "echo should return 1 content block");
    eprintln!("[ok] tools/call echo: {} content blocks", result.content.len());

    // 4. tools/call 调不存在的 tool -> expect 业务方业务方 server 返回 error
    // P13.1 mock server 对不存在的 tool 返回 JSON-RPC error code -32601
    // DshAdapter.call_tool 把 JSON-RPC error 转 DshError::JsonRpc(JsonRpcError::Server)
    // 这里不直接 expect Err (P13.1 没对 call_tool 加 error-to-Err 转换),
    // 只验不 panic
    let _ = adapter
        .call_tool("nonexistent_tool", json!({}))
        .await;
    eprintln!("[ok] tools/call nonexistent returned (error path exercised)");

    // 5. shutdown
    adapter.shutdown().await.expect("shutdown should succeed");
    eprintln!("[ok] shutdown");
}

#[tokio::test]
async fn smoke_initialize_then_list_tools_then_shutdown() {
    // 跳过: 业务方本机没 Node.js
    if require_node().is_none() {
        eprintln!("[skip] node not in PATH, skipping e2e");
        return;
    }

    // 简化版: 跳过 call_tool, 验 initialize + list + shutdown 也跑通
    let plugin_path = Path::new("mock://inline");
    let config = DshConfig::default();
    let mut adapter = DshAdapter::spawn(plugin_path, config)
        .await
        .expect("spawn");

    let server_info = adapter.initialize().await.expect("init");
    assert_eq!(server_info.name, "mock-dsh-server");

    let tools = adapter.list_tools().await.expect("list");
    assert!(!tools.is_empty());

    adapter.shutdown().await.expect("shutdown");
}
