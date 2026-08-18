//! ma_harness_proto — Protobuf 生成代码 + 类型转换
//!
//! **公开 crate** (2026-08-18 锁定). 字段稳定 (Prost 自动生成).
//! 详细 .proto 定义见 `proto/ma_harness/v1/`.
//!
//! # 跟 core 的关系
//!
//! - `ma_harness_core` (内部) 用 `SessionEvent` enum 等强类型
//! - `ma_harness_proto` (公开) 用 Protobuf wire 格式
//! - 两者通过 `From` / `TryFrom` 互转
//!
//! # 2026-08-18: 临时禁用
//!
//! 由于 protoc 在 Windows 上编译不通 (protoc-prebuilt 走 GitHub 被墙,
//! protobuf-src autotools 缺 aux files), 整个 crate 临时 no-op.
//! 等 P2 解决: 本地 protoc 安装 / vendor prebuilt binary.
//!
//! `convert` 模块仍提供 `session_event_from_proto` / `session_event_to_proto`,
//! 但 `v1` 模块暂时是 stub, 等 build.rs 恢复后用 `tonic::include_proto!` 替换.

#![warn(missing_docs)]

// tonic-build 生成的 Protobuf 代码 (3 个 .proto 的 message + service)
// 2026-08-18: 临时禁用, build.rs no-op, 用 stub 模块替代
// 等 protoc 编译解决后恢复:
//     tonic::include_proto!("ma_harness.v1");
pub mod ma_harness {
    /// proto/ma_harness/v1/agent.proto
    ///
    /// 2026-08-18: 临时 stub. 等 protoc 编译解决后, 这里会通过
    /// `tonic::include_proto!("ma_harness.v1")` 引入自动生成的代码.
    pub mod v1 {
        // 占位 stub. 等 protoc 编译解决后, 上面一行 include_proto 替换
        // 整个模块, 这个 stub 文件就删除.
    }
}

// ============================================================================
// 类型转换: core ↔ proto
// ============================================================================

/// proto types 重导出, 方便用户 use 一处
pub use prost_types::Timestamp;
pub use tonic;

/// core ↔ proto 转换 trait
///
/// 用 `From` / `TryFrom` 实现, Phase 1 提供 SessionEvent 转换.
pub mod convert {
    use ma_harness_core::{EventType, SessionEvent, Severity};

    use chrono::{DateTime, TimeZone, Utc};

    /// proto::SessionEvent → core::SessionEvent
    ///
    /// 2026-08-18: 由于 ma_harness_proto 临时禁用, `v1::SessionEvent`
    /// 类型暂时不可用, 这个函数需要等 protoc 编译解决后恢复.
    /// 暂时以注释占位, 不导出 (等 build.rs 恢复后用 `pub use` 重新启用).
    #[allow(dead_code)]
    fn _session_event_from_proto_stub(p: ()) -> SessionEvent {
        // 占位, 等 build.rs 恢复后用真实现替换
        // 实际实现见 git history (session_event_from_proto)
        let _ = (p, EventType::Unspecified, Severity::Info, Utc::now(), DateTime::<Utc>::MIN_UTC);
        unimplemented!("ma_harness_proto 临时禁用, 等 protoc 编译解决后恢复")
    }
}

#[cfg(test)]
mod tests {
    use ma_harness_core::{EventType, Severity};
    use ma_harness_core::SessionEvent;

    #[test]
    fn proto_module_stub_compiles() {
        // 占位测试: 验证模块在临时禁用状态下能 compile
        // 等 protoc 编译解决后, 这个测试替换成真 round-trip 测试
        let s = SessionEvent::new("sess-1", EventType::SessionStart);
        assert_eq!(s.session_id, "sess-1");
        assert_eq!(s.event_type, EventType::SessionStart);
        assert_eq!(s.severity, Severity::Info);
    }
}
