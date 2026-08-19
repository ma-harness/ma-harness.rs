//! build.rs — 调用本地 vendor/protoc/bin/protoc.exe 生成 ma_harness.v1.rs
//!
//! 2026-08-18 (Day 52): 改用本地 vendor protoc
//!
//! 之前 mental commit 走 protoc-prebuilt (crates.io 下载 zip) + protobuf-src (autotools
//! 编译 C++ 工具), 在 Windows 缺 sh + autotools aux files 不通. 改用本地 protoc (npmmirror.com
//! 镜像下载 25.1, 3.1MB), tonic-build 调它生成 stub.
//!
//! PROTOC env 指向本地 protoc, tonic-build 内部 spawn

#![allow(unsafe_code)] // std::env::set_var 1.83+ 是 unsafe

use std::path::PathBuf;
use std::process::Command;

fn main() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // crates/ma_harness_proto -> workspace root
    let workspace_root = manifest_dir.parent().unwrap().parent().unwrap();
    let protoc = workspace_root
        .join("vendor")
        .join("protoc")
        .join("bin")
        .join("protoc.exe");

    if !protoc.exists() {
        panic!(
            "protoc not found at {}\n下载: https://npmmirror.com/mirrors/protobuf/v25.1/protoc-25.1-win64.zip\n解压到 vendor/protoc/",
            protoc.display()
        );
    }

    // sanity check: protoc --version
    let output = Command::new(&protoc)
        .arg("--version")
        .output()
        .expect("protoc --version failed");
    println!(
        "cargo:warning=using protoc: {}",
        String::from_utf8_lossy(&output.stdout).trim()
    );

    let proto_dir = workspace_root.join("proto").join("ma_harness").join("v1");
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());

    println!("cargo:rerun-if-changed={}", proto_dir.display());
    println!("cargo:rerun-if-changed=build.rs");

    // tonic-build 调 PROTOC env 找 protoc, 上面有现成的本地 protoc
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
