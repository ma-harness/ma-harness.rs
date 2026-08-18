# Code Mode — Phase 2 推迟备忘

> 目的: 把"Code Mode 推迟"这个决策单独成档,避免 12 周 PoC 期间被反复问"为什么没做 / 什么时候做"。

---

## 决策

**Code Mode (PTC 模式) 不在 Phase 1 (12 周 PoC) 范围内,推迟到 Phase 2。**

---

## 理由

### 1. Rust subprocess 的延迟问题

dsh 用 `node:worker_threads` 跑用户 JS 代码,启动 < 100ms。
如果 ma-harness 用 Rust subprocess (用户在 Rust 源码里写工具函数,`mah` spawn 子进程编译跑),要过 `rustc` 编译:
- 单文件 hello world: ~3-5s (release) / 0.5-1s (cached incremental)
- 真实业务工具 (10 个 crate 依赖): 10-30s 冷编译
- 增量编译: 1-3s (依赖 cargo metadata 缓存命中率)

**问题**: agent loop 里每写一个工具就卡 10-30s,体验比 dsh 差一个数量级。

### 2. wasmtime / deno_core 才是 Rust 这边的对等方案

要跟 dsh 的"快速执行用户代码"对齐,Rust 这边有两个对等方案:
- **wasmtime 20**: 用户写 Rust → 编 wasm → wasmtime 跑。冷启动 ~50ms,但需要 wasm 工具链 (rustup target add wasm32-wasi)
- **deno_core 0.290**: 用户写 TS/JS → deno_core 跑。冷启动 ~100ms,跟 dsh 体验对齐

两个都是 Phase 2 工作量:
- wasmtime: ~3 周 (含 sandbox + WASI 适配)
- deno_core: ~4 周 (含 TS 类型系统集成 + V8 编译)

### 3. 12 周 PoC 的容量有限

12 周要做的核心价值:
- 验证 Cordis 元框架的 Rust 表达力
- 验证 Protobuf 单协议 (控制面 + 数据面合一)
- 跑通 append-only 日志 + 6 个插件 + Default 模式
- 跑 dsh 现有 benchmark 拿到对比数字

加 Code Mode 直接挤掉一个 package 的开发时间。

---

## Phase 2 (后续) 做什么

**方案 A: wasmtime 优先**
- 用户用 Rust 写工具,编译到 wasm32-wasi
- mah runtime 用 wasmtime 加载 + 执行
- 沙箱靠 wasmtime 自带 capability + 我们加一层 landlock
- 优点: 跟 Rust 生态一致,工具可静态分析
- 缺点: 编译链复杂 (rustup target + wasm-opt),对纯写 TS 的用户不友好

**方案 B: deno_core 优先**
- 用户写 TS (类型提示来自我们生成的 .d.ts)
- mah runtime 用 deno_core 跑
- 沙箱靠 deno permissions API
- 优点: 跟 dsh 体验对齐,前端友好
- 缺点: 引入 V8 (binary 体积 +50MB),Rust 这边的"纯净"优势打折

**方案 C: 双引擎,用户选**
- plugin.toml 里写 `runtime: rust-wasm` 或 `runtime: ts-deno`
- mah 启动时按需加载
- 工作量 = A + B,~7 周

> PoC 期间不评估,留到 Phase 2 启动时单独评审。

---

## 跟 dsh 跑分对齐怎么处理

dsh 的 benchmark 里有 Code Mode 相关用例 (例 "spawn worker, execute 100 small tasks")。
**PoC 期间**: 这些用例 mark 为 `skipped: code_mode_phase_2`,benchmark 报告里单独一节 "Phase 1 not covered"。
**Phase 2**: 补齐。

---

## 复审触发条件

任何以下情况,本备忘录作废,需要重新评审:
1. 用户明确"Code Mode 必须 Phase 1 做"
2. dsh 弃用 Code Mode (那就不用做了)
3. Rust 生态出现新方案能把 cold compile 压到 < 1s (例如 swc 风格的 rustc jit)

---

## 变更记录

| 日期 | 变更 |
|---|---|
| 2026-08-18 | 初版,Code Mode 推迟到 Phase 2 决策落档 |
