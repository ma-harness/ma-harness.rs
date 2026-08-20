//! build.rs — 调 protoc 生成 ma_harness.v1.rs
//!
//! 2026-08-18 (Day 52): 改用本地 vendor protoc
//! 2026-08-20 (Day 101+1): macOS / Linux 改用系统 protoc, ci.yml 装
//!
//! 之前 mental commit 走 protoc-prebuilt (crates.io 下载 zip) + protobuf-src (autotools
//! 编译 C++ 工具), 在 Windows 缺 sh + autotools aux files 不通. 改用本地 protoc (npmmirror.com
//! 镜像下载 25.1, 3.1MB), tonic-build 调它生成 stub.
//!
//! 二进制来源:
//!   Windows: vendor/protoc/bin/protoc.exe (本地, 已 commit)
//!   macOS:   系统 `protoc` (PATH, ci.yml 装 `brew install protobuf`)
//!   Linux:   系统 `protoc` (PATH, ci.yml 装 `apt-get install -y protobuf-compiler`)
//!
//! 原因: vendor 目录被 .gitignore, 多平台二进制 (25MB) 不适合塞 repo.
//! ci.yml 装系统 protoc 是行业标准做法, 维护成本低, 离线 build 也能用.
//!
//! PROTOC env 指向最终 protoc 路径, tonic-build 内部 spawn

#![allow(unsafe_code)] // std::env::set_var 1.83+ 是 unsafe

use std::path::PathBuf;
use std::process::Command;

fn main() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // crates/ma_harness_proto -> workspace root
    let workspace_root = manifest_dir.parent().unwrap().parent().unwrap();
    let bin_dir = workspace_root.join("vendor").join("protoc").join("bin");

    // Windows 走 vendor, 其他平台用系统 protoc (PATH)
    //   vendor 是 .gitignore, 多平台二进制不塞 repo
    //   ci.yml macos / linux 步骤先 apt / brew install protobuf-compiler
    let protoc: PathBuf = if cfg!(target_os = "windows") {
        let p = bin_dir.join("protoc.exe");
        if !p.exists() {
            panic!(
                "protoc not found at {}\n下载: https://registry.npmmirror.com/-/binary/protobuf/v25.1/protoc-25.1-win64.zip\n解压到 vendor/protoc/ (注意: Windows 二进制在 gitignore, 业务方自己放)",
                p.display()
            );
        }
        p
    } else {
        // macOS / Linux: 系统 protoc
        PathBuf::from("protoc")
    };

    // sanity check: protoc --version
    let output = Command::new(&protoc)
        .arg("--version")
        .output()
        .unwrap_or_else(|e| {
            panic!(
                "protoc --version failed: {}\nprotoc 路径: {}\nmacOS / Linux: ci.yml 已装系统 protoc 吗? `which protoc` 看一下",
                e, protoc.display()
            )
        });
    let version = String::from_utf8_lossy(&output.stdout);
    if !output.status.success() {
        panic!(
            "protoc --version exited with {:?}\nstdout: {}\nstderr: {}\nPATH 找得到吗?",
            output.status.code(),
            version,
            String::from_utf8_lossy(&output.stderr)
        );
    }
    println!(
        "cargo:warning=using protoc: {} ({})",
        version.trim(),
        protoc.display()
    );

    let proto_dir = workspace_root.join("proto").join("ma_harness").join("v1");
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());

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
        .compile(
            &[
                proto_dir.join("agent.proto"),
                proto_dir.join("session.proto"),
                proto_dir.join("event.proto"),
            ],
            &[workspace_root.join("proto")],
        )
        .expect("Failed to compile protos via tonic-build");
}
