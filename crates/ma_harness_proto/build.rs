//! build.rs — 临时 no-op, ma_harness_proto 整个 crate 禁用
//!
//! 2026-08-18: 等 protoc 编译问题解决再恢复
//!
//! 背景: protoc-prebuilt 走 GitHub (被墙), protobuf-src autotools 缺 aux files
//! Windows 上编译 protoc 暂时不通. 等 P2 解决 (本地 protoc 安装 / vendor).

fn main() {
    // no-op
}
