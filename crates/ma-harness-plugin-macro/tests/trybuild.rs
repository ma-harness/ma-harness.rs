//! trybuild 编译成功 fixture 测试
//!
//! Phase 2.4: 验 proc-macro 真的能展开, 不爆 rustc 错.
//!
//! **范围 (Phase 2.4 PoC)**: 只测不引用 `ma_harness_seam` 的 derive (DshService / DshListener).
//! `dsh_service_dual` / `dsh_plugin_dual` attribute macro 展开时引用 `::ma_harness_seam::*`,
//! trybuild 1.x 默认只把 host crate (ma_harness_plugin_macro) 的 path deps 传 --extern 给 fixture.
//! ma_harness_plugin_macro 不 dep ma_harness_seam (cyclic: seam re-export 本 crate 的
//! DshService derive), 所以 trybuild 找不到 seam 编译报 E0433.
//! dual macro 的集成测试改在 `crates/ma_harness_demo/tests/dsh_dual_smoke.rs` 走真集成 (consumer crate).
//!
//! trybuild 内部跑 rustc, fixture 编译错误时 panic + 报 stderr, 跟 `cargo test` 的
//! `test result: ok` 是分开的: trybuild fail 时整个 test 进程 panic, `cargo test` 标 FAILED.

#[test]
fn ui() {
    let t = trybuild::TestCases::new();
    // 真 fixture: 验 DshService derive 展开 (纯 marker const)
    t.pass("tests/trybuild/dsh_service_derive_pass.rs");
    // 真 fixture: 验 DshListener derive 展开
    t.pass("tests/trybuild/dsh_listener_derive_pass.rs");
}
