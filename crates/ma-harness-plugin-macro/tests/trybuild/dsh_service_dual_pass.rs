// 占位: dsh_service_dual 跳过 trybuild (见 tests/trybuild/dsh_service_dual_pass.rs 历史版本).
// 原因: macro 展开时引用 ::ma_harness_seam::Service, trybuild 1.x 不传 non-host-dep --extern.
// dsh_service_dual 集成测试改在 crates/ma_harness_demo/tests/dsh_dual_smoke.rs 走.
