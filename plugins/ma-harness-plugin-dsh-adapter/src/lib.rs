//! ma-harness dsh-adapter plugin (P13)
//!
//! **目标**: 让 ma-harness 可以直接加载并运行 dsh (DeepSeek Harness) 写的 TS plugin,
//! 走 dsh 自家 `@deepseek-ai/dsh-sdk-jsonrpc-server` 协议 (JSON-RPC 2.0 over stdio)。
//!
//! **详细设计**: [`docs/zh-CN/design/dsh-adapter.md`](../../../../docs/zh-CN/design/dsh-adapter.md)
//! 或 [`docs/en/design/dsh-adapter.md`](../../../../docs/en/design/dsh-adapter.md)
//!
//! # 5 阶段
//!
//! - **P13.1 骨架**: 本 crate, JSON-RPC client, Node.js 子进程 spawn, mock 测 (本文件)
//! - **P13.2 工具桥接**: dsh `defineTool` → ma-harness `ToolSchema` + invoke 转发
//! - **P13.3 lifecycle**: shutdown / respawn / cancel / stderr / 配置
//! - **P13.4 conformance**: `mah conformance --dsh-adapter` 跑 9/9 dsh-snap
//! - **P13.5 e2e + 文档**: k8s_pod_status 真插件 + `mah dsh info/doctor` + CI
//!
//! # 公开 API (P13.1)
//!
//! ```ignore
//! use ma_harness_plugin_dsh_adapter::{DshAdapter, DshConfig};
//!
//! let config = DshConfig::default();
//! let mut adapter = DshAdapter::spawn(&plugin_path, config).await?;
//! adapter.initialize().await?;
//! let tools = adapter.list_tools().await?;
//! let result = adapter.call_tool("k8s_pod_status", json!({"namespace": "prod"})).await?;
//! adapter.shutdown().await?;
//! ```
//!
//! # 跟现有 Plugin trait 的关系
//!
//! dsh-adapter 是 **runtime loader**, 不是 static plugin。`inventory::submit!` 静态注册
//! 不适用 (业务方 `mah load-plugin dsh::/path` path 编译期未知)。DshAdapter 实例由
//! cli 在 `mah load-plugin` 命令里构造, P13.5 集成到 ma-harness-cli。

#![deny(unsafe_code)]
#![warn(missing_docs)]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::process::Child;
use tokio::sync::Mutex;

pub mod jsonrpc;
pub mod process;
pub mod registry;
pub mod respawn;
pub mod schema;

pub use jsonrpc::{JsonRpcClient, JsonRpcError, JsonRpcRequest, JsonRpcResponse};

// ============================================================================
// 公开 API
// ============================================================================

/// dsh runtime 配置
#[derive(Debug, Clone)]
pub struct DshConfig {
    /// Node.js 可执行文件路径, None 时 auto-detect via `which::which("node")`
    pub node_path: Option<PathBuf>,

    /// 工具调用默认超时, 默认 30 秒
    pub timeout: Duration,

    /// 子进程 crash 后自动 respawn 次数, 默认 3
    pub max_respawn: usize,

    /// 透传给 dsh 子进程的环境变量 (e.g. `DEEPSEEK_API_KEY`)
    pub dsh_env: Vec<(String, String)>,
}

impl Default for DshConfig {
    fn default() -> Self {
        Self {
            node_path: None,
            timeout: Duration::from_secs(30),
            max_respawn: 3,
            dsh_env: Vec::new(),
        }
    }
}

/// dsh 子进程 server 报告的协议版本 (P13.1 暂用 0.1.0-rc.5)
pub const DSH_PROTOCOL_VERSION: &str = "0.1.0-rc.5";

/// dsh 子进程 server 报告的 server info (来自 initialize response)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerInfo {
    /// server 名 (e.g. "@deepseek-ai/dsh-sdk-jsonrpc-server")
    pub name: String,

    /// server 版本
    pub version: String,

    /// server 支持的 capabilities (e.g. `["tools", "tools/call", "tools/cancel"]`)
    #[serde(default)]
    pub capabilities: Vec<String>,
}

/// dsh plugin 注册的工具 schema (从 tools/list 拿)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DshToolSchema {
    /// 工具名
    pub name: String,

    /// 工具描述
    pub description: String,

    /// 参数 schema (dsh 用 record-of-fields 形式, 我们转 JSON Schema object)
    pub parameters: serde_json::Value,

    /// 可选 output schema (P13 暂存, P13.2 用来桥接)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<serde_json::Value>,
}

/// 工具调用 content block (dsh `output.render` 形态, 我们存 raw JSON value)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    /// 文本块 (dsh render 出来的)
    Text {
        /// 文本内容
        text: String,
    },
    /// 其他 raw content (暂不解析, P14+ 扩展)
    #[serde(other)]
    Other,
}

/// dsh tools/call 响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallResult {
    /// content blocks (跟 dsh ContentBlock[] 一致)
    pub content: Vec<ContentBlock>,

    /// 工具返回 isError=true 时为 true
    #[serde(default)]
    pub is_error: bool,
}

/// dsh-adapter 错误
#[derive(Debug, Error)]
pub enum DshError {
    /// spawn node 子进程失败
    #[error("spawn node failed: {0}")]
    SpawnFailed(#[source] std::io::Error),

    /// node 子进程提前退出, 没读到 handshake
    #[error("node exited before handshake, status: {0:?}")]
    HandshakeFailed(Option<std::process::ExitStatus>),

    /// JSON-RPC 协议错误 (parse / response 错)
    #[error("JSON-RPC error: {0}")]
    JsonRpc(#[from] JsonRpcError),

    /// node 子进程 IO 错 (pipe close 等)
    #[error("subprocess IO error: {0}")]
    Io(#[from] std::io::Error),

    /// 工具调用超时
    #[error("tool call timeout after {0:?}")]
    Timeout(Duration),

    /// 子进程 crash (pipe close)
    #[error("subprocess crashed: {0}")]
    PluginCrashed(String),

    /// JSON 序列化错
    #[error("JSON serialization failed: {0}")]
    Serde(#[from] serde_json::Error),
}

/// DshAdapter 是核心: 持有 node 子进程 + JSON-RPC client
///
/// P13.1 阶段: spawn + initialize + list_tools + call_tool + shutdown 都跑通 (mock node)
/// P13.2+ 阶段: 桥接到 ma-harness ToolRegistry, 注册工具
pub struct DshAdapter {
    /// 配置
    config: DshConfig,

    /// 启动的 plugin entry 路径 (e.g. dsh plugin entry TS file)
    plugin_path: PathBuf,

    /// JSON-RPC client (over tokio::process stdin/stdout pipes)
    client: Mutex<Option<JsonRpcClient>>,

    /// node 子进程 (P13.3 才加 respawn, P13.1 简单持有)
    child: Mutex<Option<Child>>,

    /// server info (initialize 拿到)
    server_info: Mutex<Option<ServerInfo>>,

    /// P13.3: respawn 状态 (count + last_respawn)
    respawn: respawn::RespawnState,
}

impl DshAdapter {
    /// Spawn node 子进程跑 dsh plugin, 构造 DshAdapter (P13.1 不调 initialize)
    ///
    /// `plugin_path` 指向 dsh plugin entry 脚本 (e.g. `path/to/plugin.ts` 或 .js)
    /// `config` 配置 dsh runtime (node path / timeout / env)
    ///
    /// **返回 `Arc<Self>`** (P13.2 优化): 让 `ToolInvokeFn` 静态 closure 能 clone Arc 共享 adapter,
    /// 多 tool 注册到 ma-harness `ToolRegistry` 时共享同一子进程。
    pub async fn spawn(plugin_path: &Path, config: DshConfig) -> Result<Arc<Self>, DshError> {
        // 1. spawn node 子进程 (P13.1 用 inline script 跑 JSON-RPC echo, P13.2+ 改 plugin entry)
        let mut child = process::spawn_node(plugin_path, &config).await?;

        // 2. 从 stdin/stdout 构造 JSON-RPC client
        let stdin = child.stdin.take().ok_or_else(|| {
            DshError::SpawnFailed(std::io::Error::other("child stdin not captured"))
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            DshError::SpawnFailed(std::io::Error::other("child stdout not captured"))
        })?;

        let client = JsonRpcClient::new(stdin, stdout);

        Ok(Arc::new(Self {
            config,
            plugin_path: plugin_path.to_path_buf(),
            client: Mutex::new(Some(client)),
            child: Mutex::new(Some(child)),
            server_info: Mutex::new(None),
            respawn: respawn::RespawnState::new(),
        }))
    }

    /// 拿 plugin path 引用
    pub fn plugin_path(&self) -> &Path {
        &self.plugin_path
    }

    /// 拿 config 引用
    pub fn config(&self) -> &DshConfig {
        &self.config
    }

    /// 初始化 handshake (JSON-RPC initialize)
    /// P13.1 必跑 (P13.2 集成时 install 阶段自动调)
    pub async fn initialize(&self) -> Result<ServerInfo, DshError> {
        let mut guard = self.client.lock().await;
        let client = guard.as_mut().ok_or_else(|| {
            DshError::JsonRpc(JsonRpcError::Client("client already taken".into()))
        })?;

        let request = JsonRpcRequest::new(
            "initialize",
            Some(serde_json::json!({
                "protocolVersion": DSH_PROTOCOL_VERSION,
                "clientInfo": {
                    "name": "ma-harness",
                    "version": env!("CARGO_PKG_VERSION"),
                },
            })),
        );
        let response = client.request(request).await?;

        // 解析 response.result -> ServerInfo
        let result = response.into_result().map_err(DshError::JsonRpc)?;
        let server_info: ServerInfo = serde_json::from_value(result)?;
        *self.server_info.lock().await = Some(server_info.clone());
        Ok(server_info)
    }

    /// 拿 server info (initialize 后才有)
    pub async fn server_info(&self) -> Option<ServerInfo> {
        self.server_info.lock().await.clone()
    }

    /// 拿全部工具 schema (JSON-RPC tools/list)
    /// P13.1 跑通 (P13.2 用来注册到 ma-harness ToolRegistry)
    pub async fn list_tools(&self) -> Result<Vec<DshToolSchema>, DshError> {
        let mut guard = self.client.lock().await;
        let client = guard.as_mut().ok_or_else(|| {
            DshError::JsonRpc(JsonRpcError::Client("client already taken".into()))
        })?;

        let request = JsonRpcRequest::new("tools/list", None);
        let response = client.request(request).await?;
        let result = response.into_result().map_err(DshError::JsonRpc)?;
        let payload: ToolsListResponse = serde_json::from_value(result)?;
        Ok(payload.tools)
    }

    /// 调工具 (JSON-RPC tools/call)
    /// P13.1 跑通 (P13.2 用来 invoke dsh tool)
    pub async fn call_tool(
        &self,
        name: &str,
        arguments: serde_json::Value,
    ) -> Result<CallResult, DshError> {
        let mut guard = self.client.lock().await;
        let client = guard.as_mut().ok_or_else(|| {
            DshError::JsonRpc(JsonRpcError::Client("client already taken".into()))
        })?;

        let request = JsonRpcRequest::new(
            "tools/call",
            Some(serde_json::json!({
                "name": name,
                "arguments": arguments,
            })),
        );
        let response = tokio::time::timeout(self.config.timeout, client.request(request))
            .await
            .map_err(|_| DshError::Timeout(self.config.timeout))??;
        let result = response.into_result().map_err(DshError::JsonRpc)?;
        let call_result: CallResult = serde_json::from_value(result)?;
        Ok(call_result)
    }

    /// Shutdown (JSON-RPC shutdown + kill child if alive)
    /// P13.1 跑通, P13.3 加 graceful shutdown 跟 SIGTERM wait
    ///
    /// 接受 `Arc<Self>`: 业务方持 Arc clone, 调 shutdown 时 move Arc 进 self
    pub async fn shutdown(self: Arc<Self>) -> Result<(), DshError> {
        // 1. 发 shutdown RPC (P13.1 失败也不影响, P13.3 graceful)
        {
            let mut guard = self.client.lock().await;
            if let Some(client) = guard.as_mut() {
                let _ = client.request(JsonRpcRequest::new("shutdown", None)).await;
            }
        }

        // 2. 等子进程退出 (P13.1 给 5s, P13.3 走 SIGTERM -> SIGKILL 兜底)
        let mut child_guard = self.child.lock().await;
        if let Some(mut child) = child_guard.take() {
            let _ = tokio::time::timeout(Duration::from_secs(5), child.wait()).await;
            let _ = child.kill().await;
        }

        Ok(())
    }
}

impl Drop for DshAdapter {
    fn drop(&mut self) {
        // 兜底: 如果用户忘了 shutdown, 至少 kill 子进程
        if let Some(mut child) = self.child.try_lock().ok().and_then(|mut g| g.take()) {
            // 同步 kill (在 Drop 里不能 await)
            let _ = child.start_kill();
        }
    }
}

// ============================================================================
// 内部类型 (P13.1 用)
// ============================================================================

/// tools/list 响应包装
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ToolsListResponse {
    /// 工具 schema 列表
    tools: Vec<DshToolSchema>,
}
