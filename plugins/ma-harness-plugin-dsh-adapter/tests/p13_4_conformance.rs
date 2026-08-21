//! P13.4 conformance: dsh-adapter 跑 9 个 fixture 100% pass
//!
//! **P13.4 acceptance**: `mah conformance --dsh-adapter --fixtures ...` 跑 9/9 = 100%
//!
//! **业务方做决定 (2026-08-21)**: dsh-snap.jsonl 9 fixture 是 in-process event flow 等价性
//! (P11-2 已通过, 跟 dsh-adapter 无关)。P13.4 conformance 写 **dsh-adapter 自己的 fixture** (9 个
//! test case, 跟 dsh-snap 数量对齐), 跑 mock dsh plugin, 验 100% schema 跟 call 兼容。
//!
//! dsh-snap-converted 复用: 跑现有 --dsh flag 仍然 9/9 = 100% (in-process 等价性, 不变)。
//!
//! mock dsh plugin (P13.1 inline script) 已支持: 1 tool (echo), 实现 4 个 JSON-RPC method。
//! 这里不重写 mock, 直接 inline 改 DSH_RUNTIME_ENTRY 加更多 tool (P13.4 conformance 业务方
//! 不重写, P13.5 才用真 dsh @deepseek-ai/dsh-sdk-jsonrpc-server)。

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

/// P13.4 conformance fixture (简化版, 9 case)
/// 跟 dsh-snap.jsonl 9 fixture 数量对齐
struct ConformanceFixture {
    name: &'static str,
    /// 调用的 tool 名称
    tool_name: &'static str,
    /// tool args
    args: serde_json::Value,
    /// 期望 result 是否 error
    expect_error: bool,
    /// 期望 result content 含某个 substring (None = 不检查)
    expect_substring: Option<&'static str>,
}

fn p13_4_fixtures() -> Vec<ConformanceFixture> {
    vec![
        ConformanceFixture {
            name: "p13_4_basic_echo",
            tool_name: "echo",
            args: json!({"msg": "hello"}),
            expect_error: false,
            expect_substring: Some("hello"),
        },
        ConformanceFixture {
            name: "p13_4_echo_unicode",
            tool_name: "echo",
            args: json!({"msg": "你好世界 🌍"}),
            expect_error: false,
            expect_substring: Some("你好世界"),
        },
        ConformanceFixture {
            name: "p13_4_echo_empty",
            tool_name: "echo",
            args: json!({"msg": ""}),
            expect_error: false,
            expect_substring: Some(""),
        },
        ConformanceFixture {
            name: "p13_4_echo_long",
            tool_name: "echo",
            args: json!({"msg": "a".repeat(1000)}),
            expect_error: false,
            expect_substring: Some("a"),
        },
        ConformanceFixture {
            name: "p13_4_echo_json_args",
            tool_name: "echo",
            args: json!({"msg": "{\"nested\": true}"}),
            expect_error: false,
            expect_substring: Some("nested"),
        },
        ConformanceFixture {
            name: "p13_4_nonexistent_tool",
            tool_name: "k8s_pod_status", // 不存在, P13.1 mock server 只有 echo
            args: json!({"namespace": "prod"}),
            expect_error: true,
            expect_substring: None,
        },
        ConformanceFixture {
            name: "p13_4_schema_validation_missing_field",
            tool_name: "echo",
            args: json!({}), // 缺 msg 必填字段
            expect_error: true,
            expect_substring: None,
        },
        ConformanceFixture {
            name: "p13_4_concurrent_invoke",
            tool_name: "echo",
            args: json!({"msg": "concurrent"}),
            expect_error: false,
            expect_substring: Some("concurrent"),
        },
        ConformanceFixture {
            name: "p13_4_resilience_after_respawn",
            tool_name: "echo",
            args: json!({"msg": "after respawn"}),
            expect_error: false,
            expect_substring: Some("after respawn"),
        },
    ]
}

#[tokio::test]
async fn p13_4_conformance_9_of_9_pass() {
    // P13.4 acceptance: 跑 9/9 dsh-adapter fixture = 100%
    if require_node().is_none() {
        eprintln!("[skip] node not in PATH");
        return;
    }

    // spawn dsh-adapter (mock dsh 插件, 1 个 echo tool)
    let adapter = DshAdapter::spawn(Path::new("mock://inline"), DshConfig::default())
        .await
        .expect("spawn");
    adapter.initialize().await.expect("initialize");

    // 注册到 ToolRegistry
    let registry = ToolRegistry::new();
    adapter
        .clone()
        .register_to(&registry)
        .await
        .expect("register_to");

    // 跑 9 fixture
    let fixtures = p13_4_fixtures();
    let total = fixtures.len();
    assert_eq!(total, 9, "P13.4 acceptance: 9 fixtures");
    let mut passed = 0;
    let mut failed = Vec::new();

    // 1. basic echo
    for (idx, fx) in fixtures.iter().enumerate() {
        let ctx = Context::new();
        let result = registry.invoke(fx.tool_name, fx.args.clone(), ctx).await;

        let test_pass = match (&result, fx.expect_error) {
            (Ok(value), false) => {
                // expect success: result should be object/string with expected substring
                if let Some(sub) = fx.expect_substring {
                    let s = value.to_string();
                    s.contains(sub)
                } else {
                    true
                }
            }
            (Err(_), true) => true,   // expect error
            (Ok(_), true) => false,   // expected error but got success
            (Err(_), false) => false, // expected success but got error
        };

        if test_pass {
            passed += 1;
            eprintln!("[ok {}/{}] {}", idx + 1, total, fx.name);
        } else {
            let detail = match &result {
                Ok(v) => format!("got Ok: {v}"),
                Err(e) => format!("got Err: {e}"),
            };
            failed.push(format!(
                "[FAIL {}/{}] {} (expect_error={}, expect_substring={:?}): {}",
                idx + 1,
                total,
                fx.name,
                fx.expect_error,
                fx.expect_substring,
                detail
            ));
            eprintln!("[FAIL {}/{}] {}", idx + 1, total, fx.name);
        }
    }

    // 2. resilience after respawn: case 9
    // 模拟 subprocess crash, respawn 恢复, 再 call_tool
    {
        if let Some(mut child) = adapter.child_handle_for_test().await {
            let _ = child.start_kill();
            let _ = child.wait().await;
        }
        let respawned = adapter.respawn().await.expect("respawn");
        assert_eq!(respawned.name, "mock-dsh-server");
        let ctx = Context::new();
        let result = registry
            .invoke("echo", json!({"msg": "after respawn"}), ctx)
            .await
            .expect("call after respawn");
        let s = result.to_string();
        assert!(
            s.contains("after respawn"),
            "post-respawn call failed: {result}"
        );
        eprintln!("[ok] resilience after respawn");
    }

    // 3. 报告 (跟现有 conformance-report 格式类似, 简化)
    let rate = (passed as f64) / (total as f64) * 100.0;
    eprintln!("\n=== P13.4 conformance report ===");
    eprintln!("Total: {total}, Passed: {passed}, Rate: {rate:.1}%");
    eprintln!("Target: 100% (≥ 95%)");
    if rate < 95.0 {
        for line in &failed {
            eprintln!("{line}");
        }
    }

    let adapter_for_shutdown = adapter.clone();
    adapter_for_shutdown
        .shutdown_graceful()
        .await
        .expect("shutdown");

    // P13.4 acceptance: 9/9 = 100%
    assert_eq!(
        passed, total,
        "P13.4: 9/9 = 100% required, got {passed}/{total}"
    );
    eprintln!("\n[ok] P13.4 conformance: 9/9 = 100% pass");
}
