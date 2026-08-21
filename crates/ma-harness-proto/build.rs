//! build.rs — 调 protoc 生成 ma_harness.v1.rs
//!
//! 2026-08-18 (Day 52): 改用本地 vendor protoc (Windows only)
//! 2026-08-20 (Day 101+1): 改用系统 protoc, ci.yml 跨平台装
//!
//! 之前 mental commit 走 protoc-prebuilt (crates.io 下载 zip) + protobuf-src (autotools
//! 编译 C++ 工具), 在 Windows 缺 sh + autotools aux files 不通. 改用本地 vendor protoc
//! 镜像 (npmmirror.com 25.1), 但 vendor/ 在 .gitignore, CI runner clone 不到.
//!
//! 最终方案: 全平台都用系统 PATH 的 `protoc`:
//!   Windows: ci.yml `choco install -y protoc`
//!   macOS:   ci.yml `brew install protobuf`
//!   Linux:   ci.yml `apt-get install -y protobuf-compiler`
//!
//! 本地开发:
//!   Windows: `choco install protoc` 或 `scoop install protobuf` 或 `cargo install protobuf`
//!   macOS:   `brew install protobuf`
//!   Linux:   `apt install protobuf-compiler`
//!
//! PROTOC env 指向系统 protoc, tonic-build 内部 spawn

#![allow(unsafe_code)] // std::env::set_var 1.83+ 是 unsafe

use std::path::{Path, PathBuf};
use std::process::Command;

/// Walk up from `start` until finding a directory whose Cargo.toml contains
/// `[workspace]`. Works for both normal cargo build (manifest at
/// `crates/ma-harness-proto/`) and cargo publish (manifest copied to
/// `target/package/ma-harness-proto-<ver>/`).
fn find_workspace_root(start: &Path) -> PathBuf {
    let mut p = start.to_path_buf();
    loop {
        let cargo_toml = p.join("Cargo.toml");
        if cargo_toml.is_file() {
            if let Ok(content) = std::fs::read_to_string(&cargo_toml) {
                if content.contains("[workspace]") {
                    return p;
                }
            }
        }
        match p.parent() {
            Some(pp) => p = pp.to_path_buf(),
            None => panic!(
                "workspace root not found from {} (no Cargo.toml with [workspace] in any parent)",
                start.display()
            ),
        }
    }
}

fn main() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // 2026-08-21 (Day 101+2): 找 workspace 根 (含 [workspace] 的 Cargo.toml)。
    //
    // 三种 cargo 子命令 build.rs 跑在不同位置:
    //   cargo build  : crates/ma-harness-proto/             -> 2 层上
    //   cargo test   : crates/ma-harness-proto/             -> 2 层上
    //   cargo publish: target/package/ma-harness-proto-0.1.1/  -> 3 层上
    //
    // 之前用 CARGO_WORKSPACE_DIR env (cargo 1.64+), 但发现 cargo publish
    // 时这个 env 指向 target/ 而非真正 workspace 根, tonic-build 报
    // "Could not make proto path relative: target/proto/..."。
    //
    // 最稳: walk-up 找含 [workspace] 的 Cargo.toml 目录, 所有 cargo 子命令都准。
    let workspace_root = find_workspace_root(&manifest_dir);
    let proto_dir = workspace_root.join("proto").join("ma_harness").join("v1");
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());

    // 全平台都用系统 protoc (PATH)
    let protoc = PathBuf::from("protoc");

    // sanity check: protoc --version
    let output = Command::new(&protoc)
        .arg("--version")
        .output()
        .unwrap_or_else(|e| {
            panic!(
                "protoc --version failed: {}\nprotoc 找不到? 系统装了吗?\n  Windows: choco install -y protoc\n  macOS:   brew install protobuf\n  Linux:   apt-get install -y protobuf-compiler",
                e
            )
        });
    if !output.status.success() {
        panic!(
            "protoc --version exited with {:?}\nstdout: {}\nstderr: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let version = String::from_utf8_lossy(&output.stdout);
    println!("cargo:warning=using protoc: {}", version.trim());

    println!("cargo:rerun-if-changed={}", proto_dir.display());
    println!("cargo:rerun-if-changed=build.rs");

    // tonic-build 调 PROTOC env 找 protoc
    unsafe {
        std::env::set_var("PROTOC", &protoc);
    }

    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .file_descriptor_set_path(out_dir.join("ma_harness_descriptor.bin"))
        .out_dir(&out_dir)
        .compile_protos(
            &[
                proto_dir.join("agent.proto"),
                proto_dir.join("session.proto"),
                proto_dir.join("event.proto"),
            ],
            &[workspace_root.join("proto")],
        )
        .expect("Failed to compile protos via tonic-build");
}
