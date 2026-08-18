//! build.rs — 调 tonic-build 把 3 个 .proto 编成 Rust
//!
//! 详细 .proto 定义见 `proto/ma_harness/v1/`.
//! 生成代码落 `OUT_DIR` (cargo build 临时目录, 不进 git).
//! 运行时通过 `ma_harness_proto::ma_harness::v1::*` 访问.

use std::io::Result;

fn main() -> Result<()> {
    // tonic-build 配置: 生成 server + client, 序列化 pb 消息
    let mut config = tonic_build::configure();
    config
        .build_server(true)
        .build_client(true)
        .compile_protos(
            &[
                "../../proto/ma_harness/v1/agent.proto",
                "../../proto/ma_harness/v1/session.proto",
                "../../proto/ma_harness/v1/event.proto",
            ],
            &["../../proto"],
        )?;

    // cargo 重新 build 信号: 3 个 .proto 改了重新跑 build
    println!("cargo:rerun-if-changed=../../proto/ma_harness/v1/agent.proto");
    println!("cargo:rerun-if-changed=../../proto/ma_harness/v1/session.proto");
    println!("cargo:rerun-if-changed=../../proto/ma_harness/v1/event.proto");

    Ok(())
}
