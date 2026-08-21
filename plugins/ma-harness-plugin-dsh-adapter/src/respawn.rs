//! Respawn state machine + graceful shutdown
//!
//! P13.3 范围:
//! - 指数 backoff respawn (1s/2s/4s), 最多 N 次
//! - Graceful shutdown: 发 shutdown RPC -> 等 5s -> kill 兜底
//! - call_tool 失败检测: child exit / pipe close -> 自动 respawn + retry

#![allow(dead_code)]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use tokio::sync::Mutex;

use crate::jsonrpc::{JsonRpcClient, JsonRpcError, JsonRpcRequest};
use crate::process;
use crate::{DshAdapter, DshError, ServerInfo};

/// P13.3 respawn 状态
pub struct RespawnState {
    /// 当前 respawn 次数 (P13.3 限制 max_respawn)
    pub count: AtomicUsize,

    /// 上次 respawn 时刻 (用于指数 backoff)
    pub last_respawn: Mutex<Option<std::time::Instant>>,
}

impl RespawnState {
    /// 构造新 respawn 状态 (count=0, last_respawn=None)
    pub fn new() -> Self {
        Self {
            count: AtomicUsize::new(0),
            last_respawn: Mutex::new(None),
        }
    }
}

impl Default for RespawnState {
    fn default() -> Self {
        Self::new()
    }
}

/// P13.3 指数 backoff helper
pub fn next_backoff(attempt: usize) -> Duration {
    let secs = 1u64 << attempt.min(5);
    Duration::from_secs(secs.min(30))
}

impl DshAdapter {
    /// 指数 backoff 后 respawn 子进程 (P13.3 lifecycle)
    pub async fn respawn(&self) -> Result<ServerInfo, DshError> {
        let count = self.respawn.count.load(Ordering::SeqCst);
        if count >= self.config.max_respawn {
            return Err(DshError::PluginCrashed(format!(
                "max_respawn {} reached",
                self.config.max_respawn
            )));
        }
        let backoff = next_backoff(count);
        tracing::warn!(target: "dsh_adapter",
            respawn_count = count, backoff_secs = backoff.as_secs(),
            "respawning dsh subprocess after backoff");
        tokio::time::sleep(backoff).await;

        let mut child = process::spawn_node(&self.plugin_path, &self.config).await?;
        let stdin = child.stdin.take().ok_or_else(|| {
            DshError::SpawnFailed(std::io::Error::other(
                "child stdin not captured after respawn",
            ))
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            DshError::SpawnFailed(std::io::Error::other(
                "child stdout not captured after respawn",
            ))
        })?;
        let new_client = JsonRpcClient::new(stdin, stdout);

        {
            let mut client_guard = self.client.lock().await;
            *client_guard = Some(new_client);
        }
        {
            let mut child_guard = self.child.lock().await;
            if let Some(mut old) = child_guard.take() {
                let _ = old.start_kill();
            }
            *child_guard = Some(child);
        }

        let init_request = JsonRpcRequest::new(
            "initialize",
            Some(serde_json::json!({
                "protocolVersion": crate::DSH_PROTOCOL_VERSION,
                "clientInfo": {
                    "name": "ma-harness",
                    "version": env!("CARGO_PKG_VERSION"),
                },
            })),
        );
        let init_result = {
            let mut guard = self.client.lock().await;
            let client = guard.as_mut().ok_or_else(|| {
                DshError::JsonRpc(JsonRpcError::Client("client missing after respawn".into()))
            })?;
            client.request(init_request).await
        };

        match init_result {
            Ok(response) => {
                let result = response.into_result().map_err(DshError::JsonRpc)?;
                let server_info: ServerInfo = serde_json::from_value(result)?;
                *self.server_info.lock().await = Some(server_info.clone());
                self.respawn.count.store(0, Ordering::SeqCst);
                tracing::info!(target: "dsh_adapter",
                    "dsh subprocess respawned successfully, server: {} v{}",
                    server_info.name, server_info.version);
                Ok(server_info)
            }
            Err(e) => {
                self.respawn.count.fetch_add(1, Ordering::SeqCst);
                Err(DshError::JsonRpc(e))
            }
        }
    }

    /// Graceful shutdown (P13.3 lifecycle)
    pub async fn shutdown_graceful(self: std::sync::Arc<Self>) -> Result<(), DshError> {
        {
            let mut guard = self.client.lock().await;
            if let Some(client) = guard.as_mut() {
                let _ = client.request(JsonRpcRequest::new("shutdown", None)).await;
            }
        }

        let mut child_guard = self.child.lock().await;
        if let Some(mut child) = child_guard.take() {
            match tokio::time::timeout(Duration::from_secs(5), child.wait()).await {
                Ok(Ok(_status)) => {
                    tracing::info!(target: "dsh_adapter", "dsh subprocess exited gracefully");
                    return Ok(());
                }
                _ => {
                    tracing::warn!(target: "dsh_adapter", "dsh subprocess didn't exit gracefully, sending kill");
                    let _ = child.start_kill();
                    let _ = tokio::time::timeout(Duration::from_secs(5), child.wait()).await;
                }
            }
        }
        Ok(())
    }

    /// 检测是否需要 respawn
    pub fn needs_respawn(err: &DshError) -> bool {
        matches!(err, DshError::PluginCrashed(_) | DshError::Io(_))
    }
}

impl DshAdapter {
    /// test-only: 拿 child handle (for crash 模拟, integration test 用)
    #[doc(hidden)]
    pub async fn child_handle_for_test(&self) -> Option<tokio::process::Child> {
        let mut guard = self.child.lock().await;
        guard.take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_progression() {
        assert_eq!(next_backoff(0), Duration::from_secs(1));
        assert_eq!(next_backoff(1), Duration::from_secs(2));
        assert_eq!(next_backoff(2), Duration::from_secs(4));
        assert_eq!(next_backoff(3), Duration::from_secs(8));
        assert_eq!(next_backoff(4), Duration::from_secs(16));
        assert_eq!(next_backoff(5), Duration::from_secs(30));
        assert_eq!(next_backoff(10), Duration::from_secs(30));
    }

    #[test]
    fn needs_respawn_detects_crash() {
        assert!(DshAdapter::needs_respawn(&DshError::PluginCrashed(
            "test".into()
        )));
        let io_err = DshError::Io(std::io::Error::other("pipe closed"));
        assert!(DshAdapter::needs_respawn(&io_err));
        assert!(!DshAdapter::needs_respawn(&DshError::Timeout(
            Duration::from_secs(1)
        )));
    }
}
