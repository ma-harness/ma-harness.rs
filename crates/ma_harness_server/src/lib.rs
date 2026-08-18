//! ma_harness_server — 服务层 (salvo + tonic 拼装, 内部 crate)
//!
//! **内部 crate** (2026-08-18 锁定). salvo + tonic 拼装, 频繁变.
//! Week 7-9 起, 把 `ma_harness_seam` 的 5 个 registry 暴露成 gRPC service + HTTP endpoint.
//!
//! 2026-08-18: ma_harness_proto 临时禁用 (protoc 编译不通), gRPC service 模块
//! (agent_service / session_service) 也临时禁用, 只保留 HTTP (salvo) 部分.
//! 等 protoc 解决后恢复.

#![deny(unsafe_code)]
#![warn(missing_docs)]

use std::sync::Arc;

use ma_harness_cordis::Context;
use ma_harness_core::{EventLog, ModelAdapter, StubModelAdapter};

pub mod http;
// pub mod agent_service;     // 2026-08-18: 临时禁用 (依赖 ma_harness_proto)
// pub mod session_service;   // 2026-08-18: 临时禁用 (依赖 ma_harness_proto)

// pub use agent_service::AgentServiceImpl;     // 2026-08-18: 临时禁用
// pub use session_service::SessionServiceImpl; // 2026-08-18: 临时禁用

/// ServerBuilder — 拼装 server 所需的全部资源
///
/// 2026-08-18: gRPC service 构造方法临时禁用, 只保留 ctx 构造
pub struct ServerBuilder {
    /// 事件日志 (Phase 2 持久化)
    #[allow(dead_code)]
    log: EventLog,
    /// Model adapter (Phase 1 stub, Phase 2 OpenAI)
    adapter: Arc<dyn ModelAdapter>,
    /// 活跃 session 表 (Phase 2 多 session 管理)
    #[allow(dead_code)]
    sessions: dashmap::DashMap<String, ma_harness_core::SessionEvent>, // session_id -> 创建事件
}

impl ServerBuilder {
    /// 用 stub adapter 构造 (Phase 1 默认)
    pub fn with_stub(log: EventLog) -> Self {
        Self {
            log,
            adapter: Arc::new(StubModelAdapter),
            sessions: dashmap::DashMap::new(),
        }
    }

    /// 注入自定义 ModelAdapter
    pub fn with_adapter(mut self, adapter: Arc<dyn ModelAdapter>) -> Self {
        self.adapter = adapter;
        self
    }

    // 2026-08-18: 临时禁用, 等 ma_harness_proto 恢复
    // /// 构造 AgentServiceImpl
    // pub fn build_agent_service(&self) -> AgentServiceImpl {
    //     AgentServiceImpl::new(self.log.clone(), self.adapter.clone())
    // }
    //
    // /// 构造 SessionServiceImpl
    // pub fn build_session_service(&self) -> SessionServiceImpl {
    //     SessionServiceImpl::new(self.sessions.clone())
    // }

    /// 构造一个完整 ctx (Phase 1 占位, Week 5-6 加 plugin 装载)
    pub fn build_ctx(&self) -> Context {
        Context::new()
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_builder_with_stub() {
        let log = EventLog::open_in_memory().unwrap();
        let builder = ServerBuilder::with_stub(log);
        // 2026-08-18: gRPC service 构造临时禁用
        // let _agent = builder.build_agent_service();
        // let _session = builder.build_session_service();
        let _ctx = builder.build_ctx();
    }

    #[test]
    fn server_builder_with_custom_adapter() {
        // 构造一个 echo adapter 测 with_adapter
        use async_trait::async_trait;
        use ma_harness_core::{ModelRequest, ModelResponse, FinishReason};

        struct EchoAdapter;
        #[async_trait]
        impl ModelAdapter for EchoAdapter {
            fn name(&self) -> &str {
                "echo"
            }
            async fn complete(
                &self,
                req: &ModelRequest,
            ) -> anyhow::Result<ModelResponse> {
                Ok(ModelResponse {
                    model: req.model.clone(),
                    content: "echo".to_string(),
                    finish_reason: FinishReason::Stop,
                    prompt_tokens: 1,
                    completion_tokens: 1,
                })
            }
        }

        let log = EventLog::open_in_memory().unwrap();
        let builder = ServerBuilder::with_stub(log).with_adapter(Arc::new(EchoAdapter));
        // 2026-08-18: gRPC service 构造临时禁用
        // let agent = builder.build_agent_service();
        // drop(agent);
    }
}
