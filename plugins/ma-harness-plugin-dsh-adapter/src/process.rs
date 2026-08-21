//! Node.js 子进程 spawn + stderr bridge
//!
//! P13.1 范围:
//! - 探测 node 可执行文件 (env DSH_NODE_PATH / `which node` / `where.exe node`)
//! - spawn node 跑 dsh JSON-RPC server entry (临时 inline script, P13.2 改 plugin entry)
//! - 桥接子进程 stderr 到 `tracing::warn!` (dsh 用 stderr 打日志, 不污染 stdout JSON-RPC)
//! - capture stdin/stdout 给 JSON-RPC client
//!
//! P13.3 扩展: SIGTERM 优雅退出 / 3 次 respawn / config 加载

#![allow(dead_code)] // P13.1 留白方法 (e.g. SIGTERM handler) 给 P13.3

use std::path::{Path, PathBuf};
use std::process::Stdio;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};

use crate::{DshConfig, DshError};

/// dsh JSON-RPC server entry (P13.1 临时 inline script, P13.2+ 改 plugin entry)
///
/// 这个 script 是 ~30 行 Node.js, 跑 minimal JSON-RPC echo server:
/// - 读 stdin 一行 JSON
/// - 解 method, 回 result (initialize / tools/list / tools/call / shutdown)
/// - stderr 打日志
const DSH_RUNTIME_ENTRY: &str = r#"
// 2026-08-21 (Day 101+2): dsh-adapter P13.1 minimal JSON-RPC echo server
// 跑 stdin/stdout framed JSON-RPC 2.0, 实现 4 个 method:
// - initialize -> { name, version, capabilities }
// - tools/list -> { tools: [1 个 echo tool schema] }
// - tools/call (echo) -> { content: [{ type: text, text: ... }] }
// - shutdown -> {} 然后 exit
//
// P13.2 改: 调 @deepseek-ai/dsh-sdk-jsonrpc-server 真 server, 加载 user plugin
// P13.1 简化: 直接 echo, 跑通 wire protocol
const rl = require('readline').createInterface({ input: process.stdin });
process.stderr.write('[mock-dsh] started, protocol=0.1.0-rc.5\n');
rl.on('line', (line) => {
  let req;
  try { req = JSON.parse(line); } catch (e) {
    process.stderr.write('[mock-dsh] parse error: ' + e.message + '\n');
    return;
  }
  let resp;
  switch (req.method) {
    case 'initialize':
      resp = { jsonrpc: '2.0', id: req.id, result: {
        name: 'mock-dsh-server',
        version: '0.1.0',
        capabilities: ['tools', 'tools/list', 'tools/call', 'tools/cancel', 'shutdown'],
      }};
      break;
    case 'tools/list':
      resp = { jsonrpc: '2.0', id: req.id, result: { tools: [{
        name: 'echo',
        description: 'Echo the input back (P13.1 mock tool, P13.2 replaced by real dsh plugin tools)',
        parameters: {
          msg: { type: 'string', required: true, description: 'message to echo' },
        },
        output: { schema: { type: 'object', properties: { echoed: { type: 'string' } } } },
      }]}};
      break;
    case 'tools/call':
      if (req.params && req.params.name === 'echo') {
        const msg = (req.params.arguments && req.params.arguments.msg) || '';
        resp = { jsonrpc: '2.0', id: req.id, result: {
          content: [{ type: 'text', text: JSON.stringify({ echoed: msg }) }],
          isError: false,
        }};
      } else {
        resp = { jsonrpc: '2.0', id: req.id, error: {
          code: -32601, message: 'tool not found: ' + (req.params && req.params.name),
        }};
      }
      break;
    case 'shutdown':
      resp = { jsonrpc: '2.0', id: req.id, result: {} };
      process.stdout.write(JSON.stringify(resp) + '\n');
      process.exit(0);
      return;
    default:
      resp = { jsonrpc: '2.0', id: req.id, error: {
        code: -32601, message: 'method not found: ' + req.method,
      }};
  }
  process.stdout.write(JSON.stringify(resp) + '\n');
});
rl.on('close', () => { process.exit(0); });
"#;

/// 探测 node 可执行文件路径
///
/// 优先级: `DSH_NODE_PATH` env > `node_path` config > PATH 里找 `node` > "node" (PATH 解析)
///
/// P13.1 不引入 `which` crate, 自己解析 PATH (跨平台: `:` Unix, `;` Windows)
pub fn find_node(config: &DshConfig) -> PathBuf {
    // 1. config.node_path 优先
    if let Some(ref p) = config.node_path {
        if p.exists() {
            return p.clone();
        }
    }

    // 2. DSH_NODE_PATH env
    if let Ok(p) = std::env::var("DSH_NODE_PATH") {
        let path = PathBuf::from(p);
        if path.exists() {
            return path;
        }
    }

    // 3. 搜 PATH env 找 `node` (跨平台分隔符)
    if let Ok(path_env) = std::env::var("PATH") {
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
                let candidate = PathBuf::from(dir).join(format!("node{ext}"));
                if candidate.is_file() {
                    return candidate;
                }
            }
        }
    }

    // 4. fallback: 直接用 "node", 靠 PATH 解析 (最后兜底)
    PathBuf::from("node")
}

/// Spawn node 子进程跑 dsh JSON-RPC server
///
/// P13.1: 跑 inline mock script, 验证 wire protocol 跑通
/// P13.2+: 改跑 `@deepseek-ai/dsh-sdk-jsonrpc-server` + user plugin entry
pub async fn spawn_node(plugin_path: &Path, config: &DshConfig) -> Result<Child, DshError> {
    let node = find_node(config);

    // P13.1: 用 inline mock script, 不传 plugin_path
    // P13.2+: 改 `node --enable-source-maps $plugin_path` 跑 user plugin
    let mut cmd = Command::new(&node);
    cmd.arg("-e")
        .arg(DSH_RUNTIME_ENTRY)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // 透传 env (e.g. DEEPSEEK_API_KEY 给真 dsh)
        .envs(config.dsh_env.iter().cloned());

    // 也透传业务方标准 env
    for key in [
        "PATH",
        "HOME",
        "USERPROFILE", // Windows
        "TEMP",
        "TMP",
        "LANG",
        "LC_ALL",
    ] {
        if let Ok(val) = std::env::var(key) {
            cmd.env(key, val);
        }
    }

    // Kill on drop (兜底, 业务方忘了 shutdown 不留 zombie)
    cmd.kill_on_drop(true);

    let mut child = cmd.spawn().map_err(DshError::SpawnFailed)?;

    // stderr 桥接到 tracing::warn! (避免污染 stdout JSON-RPC)
    let _ = spawn_stderr_bridge(&mut child, plugin_path);

    Ok(child)
}

/// Spawn 后台 task 读子进程 stderr, 桥接到 tracing::warn!
///
/// P13.1 用 warn! 简化, P13.3 解析 dsh 日志格式 (e.g. [dsh] 2026-08-21 ...)
fn spawn_stderr_bridge(
    child: &mut Child,
    _plugin_path: &Path,
) -> Option<tokio::task::JoinHandle<()>> {
    let stderr = child.stderr.take()?;
    let handle = tokio::spawn(async move {
        let mut reader = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            // 简化: 全部 warn!, P13.3 解析 dsh 日志级别
            tracing::warn!(target: "dsh_adapter", "{}", line);
        }
    });
    Some(handle)
}
