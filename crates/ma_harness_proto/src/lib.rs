//! ma_harness_proto — Protobuf 生成代码
//!
//! **公开 crate** (2026-08-18 锁定). 字段稳定 (Prost 自动生成).
//! Week 2 加 tonic-build build script + 3 个 .proto (agent / session / event) 引用.
//!
//! 详细 .proto 定义见 `proto/ma_harness/v1/`.

#![deny(unsafe_code)]
#![warn(missing_docs)]

// Week 2 加:
// - build.rs 调 tonic_build
// - tonic::include_proto!("ma_harness.v1"); re-export
