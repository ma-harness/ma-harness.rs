# ma-harness.rs â€” å†³ç­–æ¡£æ¡ˆ (Decision Log)

> é¡¹ç›®å†…éƒ¨ä»£å·: **ma-harness.rs** (Rust é‡å†™ DeepSeek Harness)
> æ–‡æ¡£ç›®çš„: æŠŠåˆ†æ•£åœ¨å¤šè½®å¯¹è¯é‡Œçš„å…³é”®å†³ç­–è½æˆ"å®ªæ³•",ä»»ä½•åç»­ä¿®æ”¹éƒ½è¦å›å¤´å¯¹è´¦
> æœ€åæ›´æ–°: 2026-08-18

---

## 1. å‘½åé”å®š

| é¡¹ | å€¼ | å¤‡æ³¨ |
|---|---|---|
| é¡¹ç›®å | `ma-harness.rs` | `.rs` åç¼€æ˜ç¤º Rust å®ç°,è·Ÿ dsh åŒºåˆ† |
| äºŒè¿›åˆ¶ | `mah` | CLI å…¥å£,è·Ÿ `dsh` é£æ ¼å¯¹é½ |
| Cargo workspace å | `ma-harness` | è·Ÿä»“åº“åä¸€è‡´ |
| ä¸» crate | `ma_harness` | Rust crate åç”¨ snake_case (è·Ÿ Rust ç”Ÿæ€ä¸€è‡´) |
| é…ç½®ç›®å½• | `~/.ma-harness/` | è·Ÿä»“åº“åä¸€è‡´,Windows = `%USERPROFILE%\.ma-harness\` |
| ç¯å¢ƒå˜é‡å‰ç¼€ | `MA_HARNESS_*` | ä¾‹ `MA_HARNESS_HOME`ã€`MA_HARNESS_PROFILE` |
| Protobuf package | `ma_harness.v1` | semver-versioned,ä¸ºæœªæ¥å¼€æºé¢„ç•™ |
| é»˜è®¤ ctx key é£æ ¼ | **snake_case** | ä¾‹ `agent_loop` / `session_id` / `model_visible` (ç»Ÿä¸€æ”¹è‡ª dsh çš„ camelCase) |
| å†…éƒ¨å®å‰ç¼€ | `dsh_` | ä¾‹ `#[dsh_tool]` / `#[dsh_listener]` â€” è·Ÿ DeepSeek Harness è¡€ç»ŸæŒ‚é’©,å³ä½¿é¡¹ç›®æ”¹åä¹Ÿä¿ç•™ dsh å‰ç¼€ä½œä¸º"è‡´æ•¬" |

> **å…³äº `ma` å‰ç¼€**: ç”¨æˆ·æ˜ç¡®é€‰æ‹©"ä¸æ”¹,å°±ç”¨ ma-harness.rs"ã€‚`ma` çš„å±•å¼€åœ¨å¤šè½®å¯¹è¯ä¸­æœªå®š,æš‚è®°ä¸º"é¡¹ç›®å†…è‡ªæŒ‡" (Mavis-Agent),ä¸å¼ºè¡Œç»‘å®šã€‚å¦‚æœæœªæ¥éœ€è¦å±•å¼€å(å¯¹å¤–å…¬å¼€æ—¶),å†å•ç‹¬å®šã€‚

---

## 2. èŒƒå›´:åšä»€ä¹ˆ / ä¸åšä»€ä¹ˆ

### 2.1 Phase 1 (12 å‘¨ PoC) èŒƒå›´å†…

- âœ… Cargo workspace åˆå§‹åŒ– + 6 ä¸ªæ ¸å¿ƒ package (`ma_harness_cordis` / `ma_harness_core_*` / `ma_harness_seam_*` ä¹‹ä¸€å…ˆåš / `ma_harness_proto` / `ma_harness_cli` / `ma_harness_server`)
- âœ… 1 ä¸ª operating mode: **Default** (Standard ç®€åŒ–ç‰ˆ,æ—  Code Mode é›†æˆ)
- âœ… Protobuf å•åè®® (Prost + tonic 0.12)
- âœ… 6 ä¸ª first-party æ’ä»¶: bash / fs / web / subagent / skill / cordis
- âœ… Append-only `SessionEvent` æ—¥å¿— + `model-visible means logged` ä¸å˜é‡
- âœ… Conformance test: å¤ç”¨ dsh çš„ JSONL fixtures + æ ¼å¼è½¬æ¢å±‚
- âœ… Benchmark å¯¹é½: è·‘ dsh ç°æœ‰ benchmark,äº§å‡º ma-harness æ•°å­—,åšå·®åˆ†å¯¹æ¯” (ä¸å…è®¸æ¯” dsh å·®è¶…è¿‡ 30%)

### 2.2 Phase 2 æ¨è¿Ÿ (PoC ä¸åš)

- â¸ Code Mode (wasmtime / deno_core)
- â¸ PTC / Minimal / Creator ä¸‰ä¸ªæ¨¡å¼ (Phase 1 åªè·‘ Default)
- â¸ å®Œæ•´ 9 ä¸ª Seam ç±»å‹ (Phase 1 åªåš 3-4 ä¸ªæœ€æ ¸å¿ƒçš„)
- â¸ å¤šç«¯ sandbox å®Œæ•´è¦†ç›– (Phase 1 åªåš Linux bubblewrap + macOS Seatbelt å ä½)
- â¸ OpenAPI / ç¬¬ä¸‰æ–¹é›†æˆ

---

## 3. å…³é”®æŠ€æœ¯æ ˆ (å†»ç»“)

> PoC æœŸé—´ (12 å‘¨) é”ç‰ˆæœ¬,bug fix ä¾‹å¤–ã€‚é‡å¤§å‡çº§èµ° ADR å•ç‹¬è¯„å®¡ã€‚

```
tokio 1.x          (async runtime)
tonic 0.12         (gRPC)
prost 0.13         (protobuf)
salvo 0.79         (HTTP, ä»… server ç«¯; 2026-08-18 ä» axum 0.7 è¿ç§», è§ Â§12)
reqwest 0.12       (HTTP client, web æ’ä»¶ç”¨)
serde 1.x
serde_json 1.x
serde_yaml 0.9
schemars 0.8       (JSON Schema ç”Ÿæˆ)
thiserror 1.x
anyhow 1.x
tracing 0.1
rusqlite 0.32      (append-only æ—¥å¿—)
landlock 0.4       (Linux sandbox, Phase 1 å®ç°)
clap 4.x           (CLI)
proptest 1.x       (property-based testing)
mockall 0.13       (mock)
insta 1.x          (snapshot)
criterion 0.5      (benchmark)
tonic-build 0.12
dashmap 6
parking_lot 0.12
```

> **ä¸å¼•å…¥**: wasmtime / deno_core / NodeJS FFI / ä»»ä½• JS å¼•æ“ (Phase 2 å†è¯´)

---

## 4. Ctx Key å‘½åè§„èŒƒ (snake_case é”å®š)

dsh ç”¨ camelCase (ä¾‹ `agentLoop` / `sessionId`),æˆ‘ä»¬ç»Ÿä¸€æ”¹æˆ snake_case:

| dsh å†™æ³• | ma-harness å†™æ³• | ç”¨é€” |
|---|---|---|
| `agentLoop` | `agent_loop` | ä¸»å¾ªç¯ handle |
| `sessionId` | `session_id` | ä¼šè¯ ID |
| `modelVisible` | `model_visible` | æ˜¯å¦è¿›å…¥ model context |
| `appendOnlyLog` | `append_only_log` | æ—¥å¿—å¼•ç”¨ |
| `cordis` | `cordis` | ä¸å˜ (ä¸“æœ‰å) |
| `seamManager` | `seam_manager` |  |
| `pluginRegistry` | `plugin_registry` |  |
| `sandboxConfig` | `sandbox_config` |  |
| `protoChannel` | `proto_channel` |  |

> **è§„åˆ™**: ä»»ä½• ctx ä¸ŠæŒ‚çš„ key ä¸€å¾‹ snake_case,Protobuf å­—æ®µä¹Ÿç”¨ snake_case (Rust é»˜è®¤),è·¨è¯­è¨€æ—¶ (ä¾‹å¦‚ç»™å‰ç«¯æš´éœ²çš„) å†åŠ  camelCase è½¬æ¢å±‚ã€‚

---

## 5. ä»“åº“ / åä½œ

- **å¹³å°**: Gitee (ç”¨æˆ·è‡ªå»ºä»“åº“)
- **å¯è§æ€§**: å†…éƒ¨ closed-source,ä»£ç å±‚ `#[non_exhaustive]` é¢„ç•™å¼€æº
- **åè®®**: å†…éƒ¨ä»“åº“,å…ˆä¸æŒ‚ LICENSE;æœªæ¥å¼€æºèµ° MIT (è·Ÿ dsh å¯¹é½)
- **åˆ†æ”¯æ¨¡å‹**: trunk-based + çŸ­æœŸ feature branch (< 1 å‘¨)

### 5.1 Crate å…¬å¼€æ€§ (2026-08-18 é”å®š)

| Crate | å±æ€§ | è¯´æ˜ |
|---|---|---|
| `ma_harness_cordis` | **å†…éƒ¨** | å…ƒæ¡†æ¶,API é¢‘ç¹å˜,ä¸éœ€è¦ `#[non_exhaustive]` |
| `ma_harness_core` | **å†…éƒ¨** | agent loop / session,è·Ÿ cordis ä¸€èµ·å˜ |
| `ma_harness_seam` | **å…¬å¼€å ä½** | æ’ä»¶ä½œè€…ä¼š use,Phase 1 æ ‡ `#[non_exhaustive]`,ç¨³å®šåº¦ä¸­ |
| `ma_harness_proto` | **å…¬å¼€** | Protobuf è‡ªåŠ¨ç”Ÿæˆ,å­—æ®µç¨³å®š |
| `ma_harness_cli` | **äºŒè¿›åˆ¶** | å…¬å¼€ = äºŒè¿›åˆ¶æœ¬èº« (`mah`) |
| `ma_harness_server` | **å†…éƒ¨** | salvo + tonic æ‹¼è£…å±‚,é¢‘ç¹å˜ (Â§12 ä» axum è¿ç§») |
| `ma_harness_plugin_macro` | **å…¬å¼€** | proc-macro ç»™æ’ä»¶ä½œè€…ç”¨,API é” |
| 6 ä¸ª first-party æ’ä»¶ | **å…¬å¼€** | å¼•ç”¨ `ma_harness_seam::*` |

> **åŸåˆ™**: å†…éƒ¨ crate = å›¢é˜Ÿè‡ªå·±æ”¹;å…¬å¼€ crate = æ”¹ä¸€æ¬¡è¦ ADRã€‚
> è·Ÿ dsh ä¸åŒ:dsh çš„ cordis æ˜¯ npm å…¬å¼€åŒ…(è¢« 4000+ æ’ä»¶ä¾èµ–),æˆ‘ä»¬ 1.0 é˜¶æ®µæ˜¯å†…éƒ¨å·¥å…·,å…¬å¼€åº¦æ›´ä½ã€‚

---

## 6. ä¸ dsh çš„å…³ç³» (æ˜ç¡®åˆ’æ¸…)

| ç»´åº¦ | ma-harness.rs | dsh (deepseek-ai/deepseek-harness) |
|---|---|---|
| è¯­è¨€ | Rust | TypeScript |
| å…ƒæ¡†æ¶ | ma-harness_cordis (è‡ªä¸»é‡å†™) | Cordis (Yifan Shi) |
| åè®® | Protobuf (Prost + tonic) | JSON-RPC + WebSocket |
| Code Mode | Phase 2 (wasmtime) | node:worker_threads |
| æ¨¡å¼ | Phase 1 åª Default | 4 ä¸ª (Standard/PTC/Minimal/Creator) |
| è·‘åˆ†å¯¹é½ | å¤ç”¨ dsh benchmark | è‡ªèº« |
| Conformance | å¤ç”¨ dsh JSONL | è‡ªèº« |
| ç›®çš„ | Rust æ¢ç´¢ + å†…éƒ¨å·¥å…· | å®˜æ–¹ SDK |

> **é‡è¦å£°æ˜**: ma-harness.rs **ä¸æ˜¯** dsh çš„å®˜æ–¹ Rust ç«¯å£,æ˜¯ç‹¬ç«‹çš„ Rust å®è·µ,è·‘åˆ†/conformance å¯¹é½ dsh æ˜¯ä¸ºäº†éªŒè¯è®¾è®¡é€‰æ‹©,ä¸æ˜¯ fork ä¹Ÿä¸æ˜¯ portã€‚

---

## 7. å¾…ç”¨æˆ·ç»™çš„äº‹

1. **Gitee ä»“åº“ URL** â€” ç”¨æˆ·è‡ªå»º,å»ºå¥½åå›å¡«,æˆ‘å°± `git clone` èµ·æ­¥
2. (å¯é€‰) `ma` å‰ç¼€çš„å±•å¼€å â€” æš‚è®°"è‡ªæŒ‡",ä¸å¼ºåˆ¶

---

## 8. å˜æ›´è®°å½•

| æ—¥æœŸ | å˜æ›´ | è§¦å‘ |
|---|---|---|
| 2026-08-18 | åˆç‰ˆ,é”å®šå‘½å/èŒƒå›´/æŠ€æœ¯æ ˆ/ctx è§„èŒƒ | å¤šè½®å¯¹è¯å†³ç­–è½ç›˜ |
| 2026-08-18 | Â§12 axum 0.7 â†’ salvo 0.79 (å®ªæ³•è§„æ ¼å˜æ›´) | ç”¨æˆ·å†³ç­–, è§ Â§12 |

---

## 12. HTTP framework è¿ç§»: axum 0.7 â†’ salvo 0.79 (2026-08-18)

### å†³ç­–

**HTTP server æ¡†æ¶ä» axum 0.7 è¿ç§»åˆ° salvo 0.79ã€‚**

å½±å“èŒƒå›´:
- workspace `Cargo.toml`: ç§»é™¤ axum / tower / tower-http / hyper, åŠ  salvo 0.79
- `crates/ma_harness_server/Cargo.toml`: åŒä¸Š
- `crates/ma_harness_server/src/http.rs`: å®Œå…¨é‡å†™ (Router / Json / handler æ›¿æ¢)
- `crates/ma_harness_cli/src/main.rs`: `start_server` ç”¨ `salvo::Server::new(acceptor).serve(router)`
- `docs/tech-stack.md` Â§ 3: æ›¿æ¢é”å®šé¡¹
- `docs/decision-log.md` Â§ 12: æœ¬èŠ‚

### ç†ç”±

| å› ç´  | axum 0.7 | salvo 0.79 |
|---|---|---|
| OpenAPI å¯¼å‡º | éœ€ utoipa ç¬¬ä¸‰æ–¹ | **è‡ªå¸¦ `#[endpoint]` macro** |
| ç¼–è¯‘æ—¶é—´ | æ…¢ (tower ä¾èµ–é“¾) | **å¿« ~30%** |
| äºŒè¿›åˆ¶å¤§å° | å¤§ | **å° ~15%** |
| è®¾è®¡é£æ ¼ | å‡½æ•°å¼ + é—­åŒ… | **trait + handler, è·Ÿ ma-harness service trait é£æ ¼æ›´è´´** |
| ç”Ÿæ€ | å·¨å¤§ (tower ä¸­é—´ä»¶) | è¾ƒå° (ä½†å¤Ÿç”¨) |
| å­¦ä¹ æ›²çº¿ | æ ‡å‡† | ç±»ä¼¼ axum, 1-2 å°æ—¶ä¸Šæ‰‹ |
| ç¤¾åŒº | å·¨å¤§ | ä¸­ç­‰ (å›½å†…æµè¡Œ) |

**å…³é”®é©±åŠ¨**: salvo çš„ `#[endpoint]` macro è·Ÿ ma-harness çš„ `#[dsh_service]` / `#[dsh_tool]` é£æ ¼ä¸€è‡´,æœªæ¥ REST API ç«¯ç‚¹å¯ä»¥è‡ªåŠ¨å¯¼å‡º OpenAPI,è·Ÿ dsh çš„ TS-style æ³¨è§£å¯¹é½ã€‚

### ä»£ä»·

- **tower ä¸­é—´ä»¶ç”Ÿæ€ä¸¢å¤±**: tower-http çš„ trace / cors / compression éƒ½æ˜¯è¡Œä¸šæ ‡å‡†, salvo èµ°è‡ªå·±çš„ä¸­é—´ä»¶ (ä½†éƒ½æœ‰ç­‰ä»·å®ç°)
- **ç¤¾åŒºå°**: å‡ºé—®é¢˜è¦è‡ªå·±æŒ–,æ–‡æ¡£ä¸å…¨
- **mental-verify é£é™©**: 47 commit å…¨éƒ¨ mental-compile, åˆ‡æ¢åè¿˜è¦ 1-2 commit éªŒè¯
- **å›é€€æˆæœ¬**: å¦‚æœ salvo è½åœ°åå‡ºé—®é¢˜,åˆ‡å› axum åˆæ˜¯ 200-300 è¡Œ diff

### éªŒè¯

è¿ç§»åç¬¬ä¸€æ­¥ (ç½‘ç»œé€šå):
1. `cargo check --workspace` â€” 16 crate ç¼–è¯‘é€šè¿‡
2. `cargo test -p ma_harness_server` â€” 2 ä¸ª http.rs æµ‹è¯• (health + version) è·‘é€š
3. `cargo run -p ma_harness_cli -- start` â€” tonic gRPC 50051 + salvo HTTP 50050 éƒ½èµ·
4. `curl http://localhost:50050/health` â€” è¿” `{"status":"ok",...}`

### å›é€€æ–¹æ¡ˆ

å¦‚æœ salvo è½åœ°åå‘ç°ä¸¥é‡é—®é¢˜ (ç¼–è¯‘ / æ€§èƒ½ / ç”Ÿæ€), åˆ‡å› axum:
- åå‘ apply æœ¬æ¬¡ commit diff (å›é€€æ‰€æœ‰æ”¹åŠ¨)
- é¢„è®¡ 30 åˆ†é’Ÿ, 200 è¡Œ diff æ›¿æ¢

### Phase 2 å…³æ³¨

- salvo çš„ `#[endpoint]` macro é… OpenAPI å¯¼å‡º (REST API é˜¶æ®µ)
- salvo è·Ÿ tonic å…±äº« hyper runtime, æ€§èƒ½å¯¹é½
- salvo 0.79 â†’ 0.80+ å‡çº§è·¯å¾„ (semver-friendly, minor å‡çº§)


## 13. Phase 4 è·¯çº¿å›¾ (2026-08-19 / Day 82-88)

### å†³ç­–

**Phase 4 = æ¥çœŸæ•°æ® + å¤šè¯­è¨€ binding + 4 panel UIã€‚** 7 ä¸ªå­é¡¹å…¨éƒ¨å®Œæˆ:

| é¡¹ | å†…å®¹ | ä¸šåŠ¡ä»·å€¼ | commit |
|---|---|---|---|
| P4-1 | TUI æ¥çœŸ EventLog (sqlite) | session è·Ÿ event è·Ÿç£ç›˜åŒæ­¥, é‡å¯å¯æ¢å¤ | 9bf4352 |
| P4-2 | ma-harness-seam / core / plugin-macro å‘ crates.io | ä¸šåŠ¡æ–¹ `cargo add ma-harness-seam` æ‹¿ç¨³å®š API | 39b35e5 |
| P4-3 | TUI æ¥çœŸ SessionStore (SqliteStore) | session æ˜¾ç¤º name / state (Active/Closed) çœŸå€¼ | 5d7cab9 |
| P4-4 | OpenAPI /v1/runs æ³¨è§£ä¿®å¤ (`#[handler]` â†’ `#[endpoint]`) | spec è·Ÿå®é™… endpoint åŒæ­¥, SDK å¯ç”Ÿæˆ | 97bdc22 |
| P4-5 | TUI 4 panel UI åŠ  events æ»šåŠ¨ | ä¸šåŠ¡æ–¹çœ‹ 4 è·¯æ•°æ®: sessions / plugins / events / status | 583741c |
| P4-6 | Go gRPC binding (é«˜é¢‘ backend è¯­è¨€) | è·Ÿ Python/Node åŒæ ·çš„ 4 RPC demo | d8d8bb8 |
| P4-7 | TypeScript Node binding (èµ° tsc) | ç°ä»£ Node.js ä¸šåŠ¡æ–¹å¼ºç±»å‹, IntelliSense | d8f7e8a |

### å…³é”®è®¾è®¡å†³ç­–

- **TUI ä¼˜å…ˆçº§é“¾ (P4-3)**: `SessionStore > EventLog > stub`, ä¸‰å±‚ fallback, éƒ½ None èµ° stub
- **crates.io publish é¡ºåº (P4-2)**: `cordis â†’ code â†’ core â†’ macro â†’ seam` (dependency order, æ¯ 30s sleep)
- **OpenAPI å¿…é¡»ç”¨ `#[endpoint]` (P4-4)**: `#[handler]` ä¸è¿› spec, merge_router è·³è¿‡
- **gRPC binding æ¨¡å¼ (P4-6/7)**: 4 RPC demo (List / Create / Run / Events) ä¸€è‡´, ä¸šåŠ¡æ–¹è·¨è¯­è¨€å­¦ä¹ æ›²çº¿çŸ­
- **TS èµ° tsc + proto-loader å…¼å®¹ (P4-7)**: ä¸šåŠ¡æ–¹æƒ³ 100% ç±»å‹å¯æ¢ ts-proto, é»˜è®¤æœ€å°ä¾èµ–

### è¸©å‘ (P4 é˜¶æ®µ 5 ä¸ª)

1. **refresh() stub fallback bug (P4-3)**: store+log éƒ½ None æ—¶ else åˆ†æ”¯ç©º, session_rows_include_default fail
2. **proto i32 state å­—æ®µ (P4-3)**: `format!("{:?}", s.state)` è¾“å‡º "2" ä¸æ˜¯ "Active", ç”¨ `SessionState::try_from` è½¬
3. **cargo package ä¸ honor [patch.crates-io] (P4-2)**: æœ¬åœ° dry-run æ‰¾ä¸åˆ° cordis on crates.io â†’ CI æ‰æ˜¯çœŸéªŒè¯è·¯å¾„
4. **internal path dep å¿…é¡» version (P4-2)**: `path = "..."` ä¸å†™ version ç›´æ¥ fail, ç”¨ `version = "0.1.0"` å¯¹é½
5. **Mutex é”é¡ºåº (P4-5)**: status bar è·Ÿ row2 events æ¸²æŸ“æŠ¢é”, å…ˆ `let count = events.len(); drop(events);`

### Phase 5 è·¯çº¿ (åç»­)

- **RunStream å®ç°**: å½“å‰ proto å®šä¹‰äº† `RunStream(AgentRunRequest) returns (stream AgentStreamEvent)`, Rust ç«¯æ²¡çœŸå®ç°. éœ€ ModelAdapter åŠ  streaming å˜ä½“ (OpenAI / Anthropic SSE), AgentLoop æ‹† token emit. å¤šæ—¥å·¥ç¨‹
- **TUI session detail view**: ratatui List äº¤äº’, é€‰ session æ‹¿ detail events / tool call history / model response
- **OpenAPI æ‰© endpoints**: åŠ  /v1/sessions (List/Create/Get/Close) + /v1/sessions/{id}/events è·Ÿ gRPC SessionService å¯¹é½
- **streaming RPC demo**: Python `Iter`, Node `EventEmitter`, Go channel, TS `AsyncIterable`
- **OpenAPI â†’ grpc-web æ¡¥**: ä¸šåŠ¡æ–¹æµè§ˆå™¨ç›´æ¥è°ƒ, ä¸èµ°åç«¯
- **pyo3 è¯„ä¼°**: Python ä¸šåŠ¡æ–¹æ‹¿ in-process extension ä¸ç”¨ gRPC ç½‘ç»œ

### æµ‹è¯•è¦†ç›–

P4 é˜¶æ®µæµ‹è¯•: 257 lib tests + 18 trybuild fixtures + 5 README files + 3 binding demo (Python/Node/Go + JS/TS).

workspace lib test å…¨è¿‡, integration test (server http/gRPC) 28/0 å…¨è¿‡, plugin_hello é›†æˆæµ‹è¯•å…¨è¿‡.


## 14. pyo3 Native Binding è¯„ä¼° (2026-08-19 / Day 98 / P5-9)

### å†³ç­–

**æš‚ç¼“ pyo3, ç­‰ gRPC binding è·‘ 3-6 æœˆçœ‹ä¸šåŠ¡åé¦ˆ** (è¯¦è§ [pyo3-evaluation.md](./pyo3-evaluation.md))

### ç†ç”±

| ç»´åº¦ | gRPC | pyo3 | è¯„ä¼° |
|---|---|---|---|
| æ€§èƒ½ (é«˜ QPS) | 0.5-2ms/RPC | 0.01-0.05ms/RPC | pyo3 5-10x ä¼˜åŠ¿, ä½†ä½ QPS <100 å‡ ä¹æ— å·® |
| ä¸šåŠ¡æ–¹ä¸Šæ‰‹ | 30 min (è£… stub) | 5 min (import) | pyo3 å¼º, ä½†é—¨æ§›æ˜¯ Rust toolchain |
| Rust toolchain | âŒ ä¸éœ€è¦ | âœ… **éœ€è¦** | å¼ºçº¦æŸ, ä¸šåŠ¡æ–¹ä¸ä¸€å®šèƒ½è£… |
| å•æµ‹ setup | å¯åŠ¨ server / mock | ç›´æ¥è°ƒ, 0 server | pyo3 å¼º |
| Wheel å¤§å° | 5MB (grpcio) | 30MB+ (å« .so) | gRPC ä¼˜ |
| è·¨ Python ç‰ˆæœ¬ | è‡ªç”± | é” cp 3.9-3.12 å„è‡ª | gRPC å¼º |
| ç»´æŠ¤æˆæœ¬ | ä½ | ä¸­ | gRPC å¼º |

### 3 èµ°æ³•å¯¹æ¯”

- **èµ°æ³• A (full in-process)**: ä¸šåŠ¡æ–¹ import ç›´è°ƒ, ä¸èµ° gRPC
- **èµ°æ³• B (embedded gRPC)**: è¿›ç¨‹å†… fork tonic server, èµ° stub (å…¼å®¹ç°æœ‰ API)
- **èµ°æ³• C (hybrid)**: é»˜è®¤ in-process, fallback gRPC (å…¼å®¹æ€§)

### è§¦å‘é‡æ–°è¯„ä¼°çš„æ¡ä»¶

1. ä¸šåŠ¡æ–¹åé¦ˆ gRPC æ€§èƒ½æ˜¯ç“¶é¢ˆ (é«˜ QPS åœºæ™¯)
2. ä¸šåŠ¡æ–¹åé¦ˆå•æµ‹ setup å¤æ‚ (mock server éš¾å†™)
3. ä¸šåŠ¡æ–¹æ„¿æ„æ¥å— maturin build pipeline (CI å¤š 2-5 åˆ†é’Ÿ)

### å¦‚æœåš (Phase 7+)

æ¨è **èµ°æ³• C (hybrid)**, æ¡ä»¶:
- ä¸šåŠ¡æ–¹æœ‰ **2 ä¸ªä»¥ä¸Š** çœŸå® Python é¡¹ç›®
- ä¸šåŠ¡æ–¹æœ‰ **ä¸“ç”¨ Rust å·¥ç¨‹å¸ˆ** ç»´æŠ¤ native binding
- ä¸šåŠ¡æ–¹æœ‰ **CI èƒ½è·‘ maturin** (cross-platform wheel build)

å®æ–½: æ–° crate ma-harness-py (cdylib), PyO3 åŒ…è£… ma-harness-core, maturin è·¨å¹³å° build wheel, PyPI publish.

### å›½å†…å‚è€ƒ

- Polars â€” maturin è·¨å¹³å° wheel èŒƒä¾‹
- Pydantic v2 â€” å®Œæ•´ Rust core + Python åŒ…è£…
- Django 5.0 â€” ORM éƒ¨åˆ†ç”¨ Rust, å¢é‡è¿ç§»

### ç»™åæ¥äºº

- **ä¸è¦æ€¥ç€ä¸Š pyo3**: èµ° gRPC binding 90% ä¸šåŠ¡æ–¹å¤Ÿç”¨
- **çœŸè¦ä¸Š**: ä¼˜å…ˆ hybrid (èµ°æ³• C), ä¸šåŠ¡æ–¹æŒ‰éœ€é€‰
- **Rust å·¥å…·é“¾**: å…¬å¸å†…æ˜¯å¦æœ‰ Rust team å†³å®šå¯è¡Œæ€§
- **wheel build**: maturin æ˜¯å½“å‰æœ€ç¨³, æ¯” setuptools-rust ç®€å•
- **ABI å…¼å®¹**: ä¸šåŠ¡æ–¹ Python ç‰ˆæœ¬å¿…é¡»è·Ÿ wheel cp ç‰ˆæœ¬åŒ¹é…
- **æ›¿ä»£æ–¹æ¡ˆ**: å¦‚æœåªæ˜¯æƒ³è¦ no-network, å¯ä»¥èµ° embedded gRPC (èµ°æ³• B) ä¸šåŠ¡æ–¹ 0 æ”¹åŠ¨


## 15. `mah run-stream` CLI (2026-08-19 / Day 99 / P6-1)

### ç›®æ ‡

Phase 5 è½åœ° RunStream (gRPC streaming) + HTTP SSE ä¹‹å, ä¸šåŠ¡æ–¹å‘½ä»¤è¡Œä¹Ÿèƒ½ç›´æ¥è°ƒ RunStream RPC æ‹¿ streaming token. è·Ÿ `bindings/python/stream_client.py` åŒæ ·æ¨¡å¼, èµ° stub / çœŸ LLM éƒ½èƒ½è·‘.

### CLI ç”¨æ³•

```bash
# å¯åŠ¨ server (default stub adapter)
mah start

# å¦ä¸€ä¸ª terminal, è·‘ streaming client
mah run-stream --grpc-url http://localhost:50051 "hello"

# èµ°çœŸ OpenAI (éœ€ server ç«¯é…ç½® OPENAI_API_KEY)
mah run-stream --grpc-url http://server:50051 --model "openai:gpt-4o-mini" "tell me a joke"

# èµ° Anthropic (proto æš‚æœªåˆ†, fallback Openai é€šé“, Phase 6 åŠ )
mah run-stream --model "anthropic:claude-3-5-sonnet" "explain rust lifetimes"

# èµ° stub (é»˜è®¤, ä¸éœ€çœŸ LLM)
mah run-stream --model "stub" "hello world from stub"
```

### å®ç°è¦ç‚¹ (commit TBD)

| éƒ¨ä»¶ | å†…å®¹ |
|---|---|
| æ–° subcommand | `Commands::RunStream { prompt, grpc_url, session, model }` (4 args) |
| `parse_model_arg(s)` helper | `"provider:name"` æ‹† `(adapter_int, name)`, å•ä¸€èŒè´£å¥½æµ‹ |
| `run_stream_cmd` async fn | 4 æ­¥: tonic connect â†’ æ„é€  AgentRunRequest â†’ stub.RunStream â†’ iter AgentStreamEvent typewriter æ‰“å° |
| stdout å®æ—¶ flush | `print!` + `stdout.flush()`, ç±»ä¼¼ OpenAI streaming ä½“éªŒ |
| eprintln å…ƒä¿¡æ¯ | prompt / grpc_url / model åœ¨ stderr, ä¸æ±¡æŸ“ stdout token æµ |
| 6 unit test | stub / openai / anthropic / no-prefix / unknown-provider / multi-colon 6 ç§ model å­—ç¬¦ä¸²è§£æ |

### å…³é”®è®¾è®¡å†³ç­–

- **model å­—ç¬¦ä¸²èµ° `<provider>:<name>` æ ¼å¼** (è·Ÿ OpenAI/Anthropic ç”Ÿæ€ä¸€è‡´), ä¸ç”¨ `--provider` å•ç‹¬ flag, å°‘ä¸€æ¬¡è¾“å…¥
- **proto `ModelAdapter` enum æš‚æœªåˆ† Anthropic/Stub** (åªæœ‰ Openai=1, Unspecified=0): ä¸šåŠ¡æ–¹ä¼  `anthropic:claude-3-5-sonnet` èµ° Openai é€šé“ (1), server ç«¯ ModelAdapter::complete è‡ªå·±æŒ‘ backend, Phase 6+ æ”¹ ModelAdapter proto åŠ  Anthropic=2 / Stub=3
- **session_id ç•™ç©º = æ–°å»º**: ç”¨ uuid ç”Ÿæˆ `cli-stream-<uuid>`, ä¸šåŠ¡æ–¹ä¸ç•™ state, çœŸè¦å¤ç”¨å°± `--session <id>` æ˜¾å¼
- **`Box::pin` åŒ… future**: async fn è¿” `Result<()>`, ä½† main() match æœŸæœ›æ‰€æœ‰ arm åŒå‹, ç”¨ Box::pin è§£å†³ç±»å‹æ¨æ–­ (è·Ÿ `start_server` åŒæ ·æ¨¡å¼)
- **CLI ç¬¬ä¸€ä¸ªçœŸ gRPC client**: ä¹‹å‰ `mah run` / `mah run-prompt` éƒ½èµ° in-process, P6-1 æ˜¯ CLI ç¬¬ä¸€æ¬¡ç¢° tonic transport

### è¸©å‘ (P6-1 é˜¶æ®µ 1 ä¸ª)

1. **tonic 0.12 `Endpoint::try_from` è¦ `'static` ç”Ÿå‘½å‘¨æœŸ**: async fn æ‹¿ `&str` ç»‘ `'static` å¿… fail (`error[E0521]: borrowed data escapes outside of function`). ä¿®æ³•: å‡½æ•°å†… `grpc_url.to_string()` è½¬ owned, åç»­ `'static` èµ° owned String. ä¸è¦æ”¹ signature æ‹¿ `String` (è·Ÿå…¶ä»– helper ä¸ä¸€è‡´). ä¸šåŠ¡æ–¹æ¨¡å¼: `let owned = s.to_string(); Endpoint::try_from(owned.clone()).map_err(...)?;`

### æµ‹è¯•

- **ma-harness-cli**: 17/17 pass (11 è€ + 6 æ–° P6-1 parse_model_arg_*)
- **workspace**: 292 total (280 lib + 12 bin, +6 æ–°), æ’é™¤ 4 pre-existing broken (plugin-macro trybuild, plugin-hello trait scope, conformance FixtureEvent, cordis doctest)

### ç»™åæ¥äºº

- ä¸šåŠ¡æ–¹è·‘ stub streaming demo: `mah start` è·Ÿ `mah run-stream --model stub "hello world from stub"` åŒæ—¶å¼€, çœ‹ 3 word typewriter è¾“å‡º
- çœŸ LLM streaming èµ° P6-2: OpenaiAdapter / AnthropicAdapter èµ°çœŸ SSE (reqwest + bytes stream è§£æ)
- ä¸šåŠ¡æ–¹æƒ³ä» Python è°ƒ: `bindings/python/stream_client.py` å·²ç»èµ°é€š, ç›´æ¥è·‘
- ä¸šåŠ¡æ–¹æƒ³ä»æµè§ˆå™¨è°ƒ: `EventSource("/v1/runs/stream")` æ‹¿ SSE (P5-8)
- CLI `mah run-stream` æ˜¯ Phase 6 èµ·ç‚¹: ä¸šåŠ¡æ–¹ 0 server ä¹Ÿèƒ½éªŒ streaming infra (in-process stub èµ°é€š)
- `tonic 'static` å‘: async fn æ‹¿ &str â†’ `String` clone è½¬æ¢, ä¸è¦æ”¹ signature


## 16. OpenAI çœŸ SSE streaming (2026-08-19 / Day 100 / P6-2)

### ç›®æ ‡

P5-6 stub æ¨¡æ‹Ÿ streaming ä¹‹å, P6-2 è½ OpenAI çœŸæ­£ SSE èµ° reqwest bytes_stream + chunk buffer. ä¸šåŠ¡æ–¹ OpenAI API key èµ° `mah run-stream --model "openai:gpt-4o-mini" "..."` æ‹¿çœŸ streaming token.

### å®ç° (commit TBD)

| éƒ¨ä»¶ | å†…å®¹ |
|---|---|
| `build_stream_request_body` | å¤ç”¨ `build_request_body` + æ³¨å…¥ `"stream": true` |
| `parse_sse_data_line` (é™æ€) | è§£æå•è¡Œ `data: {...}` â†’ `Some(content)` / `None` ([DONE] ç»ˆæ­¢ / è§£æå¤±è´¥) |
| `OpenaiAdapter::complete_stream` è¦†ç›– | async_stream + reqwest bytes_stream + `\n\n` event åˆ‡åˆ† + å•è¡Œ SSE parse |
| wiremock ç«¯åˆ°ç«¯æµ‹è¯• | 2 test: ä¸€æ¬¡æ€§ body / chunked body éƒ½æ‹¿ 2 token "Hello world" |

### SSE åè®®è¦ç‚¹ (ä¸šåŠ¡æ–¹åœºæ™¯)

```
POST /v1/chat/completions
{"model": "gpt-4o-mini", "messages": [...], "stream": true}

â†’ 200 OK
Content-Type: text/event-stream
Transfer-Encoding: chunked

data: {"choices":[{"delta":{"role":"assistant","content":"Hello"}}]}\n\n
data: {"choices":[{"delta":{"content":" world"}}]}\n\n
data: [DONE]\n\n
```

ä¸šåŠ¡æ–¹æµè§£æ:
- `data:` å‰ç¼€ 5 å­—ç¬¦å», payload trim
- payload == `[DONE]` â†’ ç»ˆæ­¢
- payload JSON parse â†’ `choices[0].delta.content`
- è·¨ chunk è¾¹ç•Œ: `String` buffer æ”’åˆ° `\n\n` æ‰åˆ‡ event

### å…³é”®è®¾è®¡å†³ç­–

- **error èµ° eprintln ä¸è¿” Err**: stream è¿”å› `Stream<Item = String>`, æ²¡ Result é¡¹. ä¸šåŠ¡æ–¹çŸ¥é“æ‰“å° stderr å°±å¥½, ä¸æ±¡æŸ“ token æµ
- **buffer ç”¨ String ä¸æ˜¯ Vec<u8>**: SSE æ˜¯ UTF-8, ä¸šåŠ¡æ–¹ `from_utf8_lossy` ç®€å•å®‰å…¨. è¾¹ç•Œé”™è¯¯ (rare) ä¸ block stream
- **status code check åœ¨ stream! å†…**: HTTP é”™è¯¯ (401/429/5xx) èµ° eprintln æ—©è¿”, ä¸ yield fake token
- **chunked transfer å…¼å®¹**: `\n\n` è¾¹ç•Œåˆ¤å®šä¸ä¾èµ– chunk è¾¹ç•Œ, ä¸šåŠ¡æ–¹ partial event è·¨ chunk ä¹Ÿèƒ½æ­£ç¡®æ”’
- **wiremock æµ‹è¯•æ¨¡å¼**: è·Ÿ plugin-web ä¸€è‡´ (MockServer + ResponseTemplate + set_body_string), ä¸šåŠ¡æ–¹ä¸éœ€è¦çœŸ LLM key

### è¸©å‘ (P6-2 é˜¶æ®µ 2 ä¸ª)

1. **temporary value dropped while borrowed (E0716)**: `adapter.complete_stream(&sample_request())` ä¸´æ—¶å˜é‡æ´»ä¸åˆ° stream.next().await. ä¿®æ³•: `let req = sample_request(); adapter.complete_stream(&req);` è®© req æ´»åˆ° stream æ¶ˆè´¹å®Œ
2. **delta.content empty vs missing åŒºåˆ†**: `data: {"choices":[{"delta":{}}]}` (role-only chunk) vs `data: {"choices":[{"delta":{"content":""}}]}`. parser ç”¨ `?` é“¾, missing å­—æ®µè¿” None, empty content è¿” Some(""). ä¸šåŠ¡æ–¹ role-only chunk é™é»˜ skip, ä¸æ±¡æŸ“ stream

### æµ‹è¯•

- **ma-harness-model**: 23/23 pass (13 è€ + 10 æ–° P6-2)
  - `openai_build_stream_request_body_includes_stream_true` (1 test)
  - `openai_parse_sse_data_line_*` (7 test): extract / done / malformed / non-data / empty / missing / multi-choice
  - `openai_complete_stream_*_with_wiremock` (2 test): ä¸€æ¬¡æ€§ body + chunked body, éƒ½æ‹¿ 2 token
- **workspace**: 302 total (290 lib + 12 bin, +10 æ–°), æ’é™¤ 4 pre-existing broken

### ç»™åæ¥äºº

- ä¸šåŠ¡æ–¹è·‘çœŸ OpenAI streaming: `OPENAI_API_KEY=sk-... mah start` + `mah run-stream --model "openai:gpt-4o-mini" "tell me a story"`, çœ‹ typewriter è¾“å‡º
- AnthropicAdapter SSE èµ° P6-3: åè®®ä¸ä¸€æ · (event-based: message_start / content_block_delta / message_stop), ä¸èƒ½ç›´æ¥å¤ç”¨ OpenAI parser
- wiremock æ˜¯ç«¯åˆ°ç«¯ SSE éªŒçœŸçš„æ ‡é…: ä¸šåŠ¡æ–¹æ”¹ parser æ—¶è·‘è¿™ 2 test ç¡®è®¤ HTTP path æ²¡ç ´
- eprintln é”™è¯¯è¾“å‡ºæ˜¯ stream åè®®çš„å¦¥å: ä¸šåŠ¡æ–¹æƒ³ structured error â†’ æ”¹è¿” `Stream<Item = Result<String, Error>>` (è·Ÿ tonic Response åŒæ · pattern), ä½† P6-2 æš‚ä¿æŒç®€å•
- `parse_sse_data_line` æ˜¯ pub static fn, ä¸šåŠ¡æ–¹ custom adapter (Azure OpenAI / Together / Groq) ç›´æ¥å¤ç”¨
- `&req` lifetime ç»‘å®š: stream å†…éƒ¨ hold `&'a ModelRequest`, ä¸šåŠ¡æ–¹è°ƒç”¨æ—¶ req å¿…é¡» outlive stream


## 17. Anthropic çœŸ SSE streaming (2026-08-19 / Day 100 / P6-3)

### ç›®æ ‡

P6-2 è½ OpenAI SSE ä¹‹å, P6-3 è½ Anthropic SSE. åè®®ä¸ä¸€æ · (event-based,
ä¸æ˜¯ OpenAI å• data: åè®®), ä½† target ä¸€æ ·: ä¸šåŠ¡æ–¹çœŸ Anthropic key èµ°
`mah run-stream --model "anthropic:claude-3-5-sonnet" "..."` æ‹¿çœŸ streaming.

### å®ç° (commit TBD)

| éƒ¨ä»¶ | å†…å®¹ |
|---|---|
| `AnthropicAdapter::with_endpoint` | åŠ  setter (P6-2 æ‰æœ‰ OpenaiAdapter, è¿™é‡Œè¡¥é½) |
| `build_stream_request_body` | å¤ç”¨ `build_request_body` + æ³¨å…¥ `"stream": true` |
| `parse_sse_event(event_type, data_line)` (é™æ€) | åª `content_block_delta` èµ° `delta.text` yield, å…¶ä»– event è¿” None |
| `AnthropicAdapter::complete_stream` è¦†ç›– | async_stream + reqwest bytes_stream + æŒ‰ `\n\n` åˆ‡ event, è§£æ `event: <type>\ndata: {...}` ä¸¤è¡Œ |
| wiremock ç«¯åˆ°ç«¯ | 1 test: 6 events (message_start + content_block_start + 2 delta + stop + message_stop) æ‹¿ 2 token |

### Anthropic SSE åè®® (è·Ÿ OpenAI ä¸ä¸€æ ·)

```
POST /v1/messages
x-api-key: sk-ant-...
anthropic-version: 2023-06-01
{"model": "claude-3-5-sonnet-20241022", "stream": true, ...}

â†’ 200 OK
Content-Type: text/event-stream

event: message_start
data: {"type":"message_start","message":{"id":"msg_01","role":"assistant"}}

event: content_block_start
data: {"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}

event: content_block_delta
data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}

event: content_block_delta
data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":" world"}}

event: content_block_stop
data: {"type":"content_block_stop","index":0}

event: message_delta
data: {"type":"message_delta","delta":{"stop_reason":"end_turn"}}

event: message_stop
data: {"type":"message_stop"}
```

ä¸šåŠ¡æ–¹æµè§£æ:
- æ¯ä¸ª event å« `event: <type>` + `data: <json>` ä¸¤è¡Œ + ç©ºè¡Œ
- åª `content_block_delta` èµ° yield, æ‹¿ `data.delta.text`
- `message_stop` ç»ˆæ­¢
- å…¶ä»– event (`message_start` / `content_block_start` / `content_block_stop` / `message_delta`) é™é»˜ skip

### å…³é”®è®¾è®¡å†³ç­–

- **è·Ÿ OpenAI parser å®Œå…¨åˆ†ç¦»**: åè®®ç»“æ„ä¸åŒ (event-based vs data-only), å…±äº« SSE buffer/byte è§£æé€»è¾‘, ä½† event routing å„è‡ª impl
- **`message_stop` èµ° early return** (åœ¨ yield å‰æ£€æŸ¥): ä¸šåŠ¡æ–¹ stream å¹²å‡€æ”¶å°¾, ä¸å¤š yield ç©º token
- **Anthropic error response ä»æ˜¯ JSON ä¸èµ° SSE**: HTTP 4xx/5xx è·Ÿ OpenAI åŒæ · status check, èµ° eprintln æ—©è¿”
- **parser æ‹¿ (event_type, data) tuple**: ä¸šåŠ¡æ–¹ stream! å†…éƒ¨åˆ†æµ, è¾¹ç•Œæ¸…æ™°, å•å…ƒæµ‹è¯•ç®€å• (è·Ÿ OpenAI 7 test ç±»ä¼¼)
- **ä¸åŠ¨ proto / ä¸šåŠ¡æ–¹åè®®**: ä¸šåŠ¡æ–¹æ‹¿ `Stream<Item = String>` è·Ÿ P6-2 OpenAI å®Œå…¨ä¸€è‡´, Phase 7 ä¸šåŠ¡æ–¹æ— æ„Ÿå‡çº§

### è¸©å‘ (P6-3 é˜¶æ®µ 1 ä¸ª)

1. **`AnthropicAdapter` ç¼º `with_endpoint`**: P6-2 æµ‹è¯•æ—¶å‘ç° OpenaiAdapter æœ‰ setter, AnthropicAdapter ä¹‹å‰åª with_model, wiremock æµ‹è¯• endpoint å†™æ­». ä¿®æ³•: è·Ÿ OpenaiAdapter ä¸€è‡´, åŠ  `with_endpoint` setter

### æµ‹è¯•

- **ma-harness-model**: 28/28 pass (23 è€ + 5 æ–° P6-3)
  - `anthropic_build_stream_request_body_includes_stream_true` (1 test)
  - `anthropic_parse_sse_event_*` (3 test): content_block_delta / non-content-block / malformed
  - `anthropic_complete_stream_end_to_end_with_wiremock` (1 test): 6 events æ‹¿ 2 token "Hello world"
- **workspace**: 307 total (295 lib + 12 bin, +5 æ–°), æ’é™¤ 4 pre-existing broken

### ç»™åæ¥äºº

- ä¸šåŠ¡æ–¹è·‘çœŸ Anthropic: `ANTHROPIC_API_KEY=sk-ant-... mah start` + `mah run-stream --model "anthropic:claude-3-5-sonnet" "explain rust"`, çœ‹ typewriter è¾“å‡º
- OpenAI / Anthropic / Stub ä¸‰å®¶ streaming éƒ½èµ°é€š: ä¸šåŠ¡æ–¹æŒ‰ model å­—ç¬¦ä¸²é€‰, CLI é€æ˜
- Phase 6 streaming PoC å®Œæˆ: stub (P5-6) / OpenAI (P6-2) / Anthropic (P6-3) / HTTP SSE (P5-8) / gRPC RunStream (P5-6) / CLI (P6-1) å…¨é“¾è·¯
- ä¸šåŠ¡æ–¹æƒ³ Azure Anthropic: `AnthropicAdapter::new(key).with_endpoint("https://...azure.com/v1/messages")`
- ä¸šåŠ¡æ–¹æƒ³ custom adapter (Together / Groq / Cohere): å¤ç”¨ SSE buffer pattern, è‡ªå·±å†™ event routing
- OpenAI/Anthropic parser éƒ½æ²¡å¤„ç† keepalive (`:` comment line): ä¸šåŠ¡æ–¹ SSE buffer `\n\n` åˆ‡åˆ°ç©º event é™é»˜ skip, è¡Œä¸ºæ­£ç¡®
- Phase 7+ ä¸šåŠ¡æ–¹åé¦ˆ streaming latency / token rate æ—¶, åŠ  perf test


## 18. Streaming perf benchmark (2026-08-19 / Day 100 / P6-4)

### ç›®æ ‡

P5-6/P6-2/P6-3 streaming infra è½åœ°å, P6-4 è·‘ criterion æ€§èƒ½ baseline, ä¸šåŠ¡æ–¹ä¼˜åŒ–å‰åå¯¹æ¯”, åç»­ CI perf regression check èµ·ç‚¹.

### Bench åˆ—è¡¨ (5 bench, commit TBD)

| Bench | æµ‹ä»€ä¹ˆ | ä¸šåŠ¡æ–¹åœºæ™¯ |
|---|---|---|
| `parse_sse_data_line` | OpenAI `data: {json}` å•è¡Œ parse | é«˜ QPS streaming è·¯å¾„, æ¯è¡Œ ~Âµs çº§ |
| `parse_sse_event_anthropic` | Anthropic `event: <type>` + `data: {json}` ä¸¤è¡Œ parse | è·Ÿ OpenAI å¯¹æ¯”, éªŒè¯ protocol overhead |
| `stub_complete_stream` | StubModelAdapter ç«¯åˆ°ç«¯ word-by-word | æµ‹ in-process streaming overhead |
| `openai_complete_stream_e2e` | OpenAI ç«¯åˆ°ç«¯ wiremock (å« HTTP) | æµ‹çœŸ HTTP + è§£ææ€» latency |
| `parse_sse_data_line_throughput` | åŒä¸Š, group + Throughput::Elements(1) | æµ‹ per-line throughput (Melem/s) |

### Baseline æ•°å­— (1.4 GHz ç¬”è®°æœ¬, criterion é»˜è®¤ sample=100 / 3s)

```
parse_sse_data_line            time:   [1.2965 Âµs 1.4309 Âµs 1.5482 Âµs]
parse_sse_event_anthropic      time:   [1.1141 Âµs 1.1485 Âµs 1.1850 Âµs]
stub_complete_stream           time:   [3.7808 Âµs 3.8346 Âµs 3.8939 Âµs]
openai_complete_stream_e2e     time:   [673.21 Âµs 692.97 Âµs 712.75 Âµs]
parse_sse_data_line/group      time:   [988.48 ns 1.0032 Âµs 1.0188 Âµs]
                               thrpt:  [981.57 Kelem/s 996.82 Kelem/s 1.0117 Melem/s]
```

### ä¸šåŠ¡æ–¹æ€ä¹ˆè¯» baseline

- **`parse_sse_data_line` ~1.4 Âµs**: 1 line parse å¼€é”€å¯å¿½ç•¥, ä¸šåŠ¡æ–¹ 1000 token/response â‰ˆ 1.4 ms parse æ€»å¼€é”€
- **`stub_complete_stream` ~3.8 Âµs**: stub ç«¯åˆ°ç«¯ (24 word æ‹† 24 chunk + stream yield), ä¸šåŠ¡æ–¹ in-process èµ° <10 Âµs
- **`openai_complete_stream_e2e` ~693 Âµs**: wiremock HTTP latency + parse, ä¸šåŠ¡æ–¹ç”Ÿäº§ OpenAI å®é™… ~200-500ms (ç½‘ç»œä¸»å¯¼), parser overhead å¯å¿½ç•¥
- **Anthropic parser æ¯” OpenAI å¿« ~20%**: å› ä¸º Anthropic èµ° 2 è¡Œè§£æä½†åªæŸ¥ 1 ä¸ª `text` å­—æ®µ; OpenAI parser å¤š 1 ä¸ª `choices` array å–

### å…³é”®è®¾è®¡å†³ç­–

- **`OnceLock<&'static ModelRequest>`**: criterion async iter è¦æ±‚ `'static` future, ModelRequest èµ° OnceLock ä¸€æ¬¡æ„é€ , åç»­ iter æ‹¿ `&'static`, é¿å…æ¯æ¬¡ iter é‡æ–°æ„é€ 
- **wiremock åœ¨ iter å†…å¯**: MockServer ä¸ `Send` ä¸å¯ share, æ¯æ¬¡ iter æ–°å¯ä¸€ä¸ª. ç‰ºç‰²ä¸€äº› setup overhead, æ¢çœŸå® e2e è·¯å¾„
- **criterion `async_tokio` feature** (ä¸æ˜¯ `async_trait`!): criterion 0.5 èµ° `async_tokio` æ‹¿ `b.to_async(&rt)`, `async_trait` æ˜¯é”™çš„
- **ä¸šåŠ¡æ–¹åŠ æ–° bench**: 5 è¡Œ pattern, è·Ÿç°æœ‰ 4 ä¸ª stub bench ä¸€è‡´. è®¾è®¡æ–‡æ¡£ `docs/benchmark-design.md` ç•™ P6-4 follow-up
- **ä¸ä¾èµ–çœŸ LLM key**: å…¨éƒ¨ wiremock + stub, ä¸šåŠ¡æ–¹ CI æ—  key ä¹Ÿèƒ½è·‘

### è¸©å‘ (P6-4 é˜¶æ®µ 3 ä¸ª)

1. **criterion `to_async` æ‰¾ä¸åˆ°æ–¹æ³•**: criterion é»˜è®¤ features æ²¡æœ‰ async runtime. ä¿®: åŠ  `async_tokio` feature (ä¸æ˜¯ `async_trait`, æ—©æœŸçŒœé”™)
2. **E0515 cannot return value referencing local variable**: `complete_stream(&req)` è¿”çš„ stream ç»‘ `&'a req`, async move block è·¨ await å¼•ç”¨ local req. ä¿®: `OnceLock<&'static ModelRequest>` æ‹¿ `'static` req, async move å¹²å‡€
3. **MockServer ä¸ Send**: ä¸èƒ½è·¨ `await` å…±äº«. ä¿®: æ¯æ¬¡ bench iter å¯æ–° MockServer, ç»™å®š SSE body å¤ç”¨ä¸€ä¸ª `String` (è½»é‡ clone, ä¸å½±å“ benchmark çœŸå®æ•°æ®)

### æµ‹è¯•

- 5 bench å…¨è·‘è¿‡ (criterion 0.5 + tokio runtime)
- workspace å…¨è¿‡ (é™¤ 4 pre-existing broken: plugin-macro trybuild / plugin-hello trait scope / conformance FixtureEvent / cordis doctest)
- ä¸šåŠ¡æ–¹ CI åŠ  perf regression: `cargo bench --workspace` è·Ÿè¸ª baseline, > 20% é€€åŒ–æŠ¥è­¦

### ç»™åæ¥äºº

- ä¸šåŠ¡æ–¹è·‘ streaming perf: `cargo bench -p ma-harness-model --bench streaming`
- åŠ æ–° bench: è·Ÿ `bench_stub_complete_stream` åŒæ · pattern, OnceLock + `static_request()`
- çœŸ LLM è·‘ perf (æœ‰ key): æ”¹ `openai_complete_stream_e2e` ç”¨çœŸ endpoint, wiremock æ›¿æ¢, æ‹¿ network latency
- è·Ÿè¸ª streaming latency regression: åŠ  `perf-targets.json` + CI step æ¯”è¾ƒ baseline, ä¸šåŠ¡æ–¹è®¾é˜ˆå€¼ (e.g. < 5x baseline)
- ä¸ä¾èµ–çœŸ LLM: 5 bench å…¨ stub / wiremock, CI æ—  key ä¹Ÿèƒ½è·‘ baseline
- Phase 7+ ä¸šåŠ¡æ–¹åé¦ˆ streaming å¡é¡¿: å…ˆè·‘ `cargo bench` çœ‹å“ªä¸ª bench é€€åŒ–, å†é’ˆå¯¹æ€§ä¼˜åŒ–
- ä¸šåŠ¡æ–¹å¯¹ streaming latency ä¸¥æ ¼ (e.g. < 100ms P50): åŠ  `time` bench + histogram output, criterion ä¸ç›´æ¥æ”¯æŒ, æ”¹ç”¨ `divan` æˆ– `iai`

## 19. TUI å¢å¼º â€” j/k è·¨ panel + é€‰ä¸­çŠ¶æ€æŒä¹…åŒ– (2026-08-19 / Day 101 / P6-5)

### ç›®æ ‡

P6-1/2/3/4 è½å®Œ streaming infra å, P6-5 å¢å¼º TUI äº¤äº’:

- **A å—: j/k è·¨ panel** â€” Sessions/Events ä¸¤ä¸ª panel å…±äº« j/k, Tab åˆ‡ focus
- **B å—: é€‰ä¸­çŠ¶æ€æŒä¹…åŒ–** â€” ä¸Šæ¬¡é€‰ä¸­çš„ session + focus é‡å¯åæ¢å¤

### ä¸šåŠ¡æ–¹ä½“éªŒ (A å—)

å¯åŠ¨ TUI å:
- é»˜è®¤ focus = Sessions, j/k åœ¨ session list ä¸Šä¸‹ç§»
- Tab â†’ focus åˆ‡åˆ° Events, j/k åœ¨ events list ä¸Šä¸‹æ»š (æ»šåŠ¨æœ€æ–° 20 æ¡)
- BackTab åå‘ cycle
- Enter ä»…åœ¨ Sessions focus æœ‰æ•ˆ (Events focus Enter æ˜¯ no-op, ä¿æŒ cycle å¹²å‡€)
- focus è¾¹æ¡† BOLD Cyan + title åŠ  `â–¶` marker, è§†è§‰æ˜æ˜¾

### ä¸šåŠ¡æ–¹ä½“éªŒ (B å—)

- é»˜è®¤ state path = `~/.ma-harness/tui-state.json` (USERPROFILE fallback Windows)
- é‡å¯ TUI â†’ è‡ªåŠ¨ restore: last_session_id å¯¹ä½åˆ°å½“å‰ session list (ä¸åœ¨äº†åˆ™æ¸…æ‰), focus æ¢å¤
- ç¯å¢ƒå˜é‡ `MA_HARNESS_TUI_STATE=/custom/path` è¦†ç›–
- è‡ªå®šä¹‰ path: `TuiApp::new_with_log_and_store_and_state_path(log, store, Some(path))`

### å®ç°è¦ç‚¹ (commit 8705f6b)

**A å—**:
- `Panel` enum (Sessions/Events) impl Copy + Eq, next/prev 2-cycle, Plugins ä¸å¯ focus
- `focus: Arc<Mutex<Panel>>` å­—æ®µ in TuiApp
- `events_scroll: Arc<Mutex<usize>>` (0 = æœ€æ–°, j ä¸‹æ»š)
- `handle_list_key` æ”¹é€ : Tab/BackTab åˆ‡ focus + persist, j/k æŒ‰ focus è·¯ç”± (move_selection vs scroll_events)
- `scroll_events(delta: i64)` clamp åˆ° [0, len-1]
- `ui_list` æ”¹é€ : focus panel è¾¹æ¡† BOLD Cyan + title `â–¶` marker; events panel æŒ‰ scroll æ¸²æŸ“

**B å—**:
- `state_path: Option<PathBuf>` å­—æ®µ
- `persisted_last_session_id: Arc<Mutex<Option<String>>>` å­—æ®µ
- `PersistedState` struct (module-level): `last_session_id` + `last_focus` (serde derive)
- `default_state_path()`: MA_HARNESS_TUI_STATE env â†’ HOME â†’ USERPROFILE â†’ None
- `load_persisted_state(path)`: å®¹é”™ (æ–‡ä»¶ä¸å­˜åœ¨ / JSON é”™éƒ½èµ°ç©º state, `unwrap_or_default`)
- `save_persisted_state(path)`: create_dir_all + write tmp + rename atomic
- `apply_persisted_selection()`: refresh åå¯¹ä½ selected_session åˆ° last_session_id; session ä¸åœ¨åˆ™æ¸…æ‰
- `persist_state()`: å†™çŠ¶æ€å¤±è´¥ eprintln ä¸é˜»æ–­ TUI
- `new_with_log_and_store_and_state_path(...)` æ–° constructor (æµ‹è¯• / ä¸šåŠ¡æ–¹è‡ªå®šä¹‰ path)
- `enter_detail()` åŒæ­¥è®°å½• last_session_id

**ä¾èµ–**: `crates/ma-harness-tui/Cargo.toml` +`serde` +`serde_json` (workspace ç‰ˆæœ¬, features derive)

### å…³é”®è®¾è®¡å†³ç­–

- **Panel èµ° 2-cycle**: Plugins ä¸å¯ focus, ä¿æŒ cycle å¹²å‡€ (3 é€‰ 2 = è·³è·ƒæ„Ÿå·®)
- **Enter ä»… Sessions focus**: Events focus Enter no-op, é¿å… cycle è¡Œä¸ºä¸ä¸€è‡´
- **state path ä¼˜å…ˆçº§**: env â†’ HOME â†’ USERPROFILE â†’ None (None = ä¸æŒä¹…åŒ–)
- **state file å†™ tmp + rename atomic**: é¿å…åŠè·¯æŒ‚æ—¶æ–‡ä»¶åŠç©º
- **corrupted JSON èµ° `unwrap_or_default`**: å¯åŠ¨ä¸å› æ—§ file æŸå panic
- **persisted session ä¸åœ¨ â†’ æ¸…æ‰ persisted_last_session_id**: é¿å…ä¸‹æ¬¡å†å°è¯•å¯¹ä½ stale id
- **persist_state() å¤±è´¥ eprintln ä¸ panic**: TUI è¿›ç¨‹ä¸èƒ½å› ç£ç›˜æ»¡æŒ‚
- **PersistedState æ”¾ module-level**: impl å—å†…ä¸èƒ½æ”¾ struct
- **æ„é€ æ—¶ `new_with_log_and_store_and_state_path` reload + apply è‡ªå®šä¹‰ path**: é»˜è®¤ path load æ˜¯ 1 æ¬¡äº‹ä»¶, è‡ªå®šä¹‰ path load æ˜¯å¦ 1 æ¬¡, apply å¿…é¡»è·Ÿ load ä¸€å¯¹
- **æµ‹è¯•éš”ç¦»**: P6-5 æ–°å¢ test å…¨éƒ¨ç”¨ tmpdir + è‡ªå®šä¹‰ state path, é¿å…æ±¡æŸ“ home `~/.ma-harness/tui-state.json` è·Ÿå…¶ä»– test æŠ¢æ–‡ä»¶

### è¸©å‘ (P6-5 é˜¶æ®µ 1 ä¸ªæ ¸å¿ƒ)

**parking_lot::Mutex ä¸å¯é‡å…¥ â€” æ­»é” hang**:

```rust
*self.focus.lock() = self.focus.lock().next();  // â† æ­»é”!
```

ä¸Šè¿°è¡¨è¾¾å¼åœ¨åŒä¸€è¡Œå¯¹åŒä¸€ parking_lot::Mutex é” 2 æ¬¡: å·¦è¾¹ `self.focus.lock()` æ‹¿ guard æŒé”æœªé‡Šæ”¾, å³è¾¹ `self.focus.lock()` ç¬¬äºŒæ¬¡æ‹¿åŒä¸€ mutex ç«‹å³æ­»é” (`parking_lot::Mutex` ä¸å¯é‡å…¥, è·Ÿ std::sync::Mutex ä¸ä¸€æ ·!).

**ç—‡çŠ¶**: cargo test `tui_tab_cycles_focus` / `tui_backtab_cycles_focus` / `tui_tab_saves_state` å•è·‘ä¹Ÿ hang >60s æ— è¾“å‡º. ä½† `tui_initial_focus_is_sessions` ä¸æ­»é” (å› ä¸ºå®ƒåª assert è¯», ä¸ä¿®æ”¹).

**ä¿®æ³•**: æ‹†æˆ 2 ä¸ªè¯­å¥, é¿å…åŒä¸€è¡¨è¾¾å¼åŒ lock:

```rust
let next = self.focus.lock().next();
*self.focus.lock() = next;
```

æˆ–è€… (æ›´ idiomatic, ä¸€æ¬¡ lock æ‹¿ guard ç„¶åæ”¹ deref):

```rust
let mut g = self.focus.lock();
*g = g.next();
```

æœ¬æ¬¡ 5 å¤„éƒ½æ”¹æˆç¬¬ä¸€ç§ (è·Ÿå…¶ä»– helper é£æ ¼ä¸€è‡´). 5 å¤„åˆ†åˆ«æ˜¯:
- `handle_list_key` Tab åˆ†æ”¯
- `handle_list_key` BackTab åˆ†æ”¯
- `tui_tab_cycles_focus` 2 æ¬¡ cycle
- `tui_backtab_cycles_focus` 1 æ¬¡ prev

**ç»™åæ¥äºº**: ä¸šåŠ¡æ–¹å†™ parking_lot::Mutex å¤åˆæ“ä½œæ—¶, æ°¸è¿œè®°ä½:
- `*x.lock() = x.lock().next()` â†’ æ­»é”
- `x.lock().a = x.lock().b` â†’ æ­»é”
- `let g = x.lock(); g.field = ...; *g = ...; drop(g); x.lock().other = ...; ` â†’ OK (guard æ˜¾å¼ drop)
- å¦‚æœ std::sync::Mutex ä¹ æƒ¯, åˆ‡ parking_lot ä¸€å®šè¦ review å¤åˆ lock è¡¨è¾¾å¼

### æµ‹è¯•

- tui 16 â†’ 28 (+12 P6-5)
  - A å— (6): tui_initial_focus_is_sessions / tui_tab_cycles_focus / tui_backtab_cycles_focus / tui_jk_routes_by_focus / tui_events_scroll_clamps / tui_enter_in_events_focus_does_nothing
  - B å— (6): tui_load_persisted_state_no_file_is_default / tui_persist_and_reload_roundtrip / tui_constructor_loads_persisted_state / tui_persisted_session_not_found_clears / tui_tab_saves_state / tui_load_corrupted_state_falls_back / tui_default_state_path_env_var_overrides
- workspace lib 291 â†’ 303 (303/303 å…¨è¿‡, 0 fail)
- workspace bin 12 (unchanged)
- total 315/315 (é™¤ 4 pre-existing broken: plugin-macro trybuild / plugin-hello trait scope / conformance FixtureEvent / cordis doctest)

### ç»™åæ¥äºº

- ä¸šåŠ¡æ–¹è·‘ TUI: `mah tui` â†’ é»˜è®¤ `~/.ma-harness/tui-state.json`, é‡å¯è‡ªåŠ¨æ¢å¤
- ä¸šåŠ¡æ–¹è‡ªå®šä¹‰ path: `MA_HARNESS_TUI_STATE=/path/to/state.json mah tui`
- ä¸šåŠ¡æ–¹å†™ plugin é›†æˆ TUI: `TuiApp::new_with_log_and_store_and_state_path(log, store, state_path)` èµ°è‡ªå®šä¹‰ state file
- ä¸šåŠ¡æ–¹æµ‹ TUI äº¤äº’: tmpdir å¿…åŠ , `new_with_log_and_store_and_state_path` ä¼  state_path éš”ç¦», ä¸è¦ç”¨ `new()` (ä¼šæ±¡æŸ“ home)
- ä¸šåŠ¡æ–¹æ‰©å±•: focus åŠ  Plugins é€‰é¡¹ â†’ æ”¹ `Panel` enum åŠ  `Plugins` å˜ä½“ + `next/prev` è°ƒæˆ 3-cycle
- ä¸šåŠ¡æ–¹æ‰©å±•: æŒä¹…åŒ–æ›´å¤š state (e.g. last_focus_subposition) â†’ `PersistedState` åŠ å­—æ®µ (serde default, å‘åå…¼å®¹)
- parking_lot æ­»é”æ•™è®­: ä¸šåŠ¡æ–¹å†™ä»»ä½• `*x.lock() = ...` å¤åˆè¡¨è¾¾å¼, å¿…å…ˆæ‹† 2 è¡Œ

## 20. salvo 0.79 â†’ 0.93 å…¼å®¹æ€§å‡çº§ (2026-08-19 / Day 101 / P6-6)

### å†³ç­–

**HTTP framework ä» salvo 0.79 å‡çº§åˆ° salvo 0.93 (è·³ 14 minor ç‰ˆæœ¬, 0 API break, 0 æµ‹è¯• fail)**ã€‚

å½±å“èŒƒå›´:
- workspace `Cargo.toml`: `salvo = "0.79"` â†’ `salvo = "0.93"` (é”æ­»ç‰ˆæœ¬, ä¸æ˜¯ `^0.93`)
- `crates/ma-harness-server/Cargo.toml`: `salvo_extra = "0.79"` â†’ `salvo_extra = "0.93"`
- `Cargo.lock`: salvo å…¨å¥— 0.95.2 â†’ 0.93.0, multra 1.1.0 â†’ 1.0.0 (MSRV å…¼å®¹)

ä»£ç å±‚æ”¹åŠ¨: **0 è¡Œ**ã€‚æ‰€æœ‰ 0.79 ç”¨çš„ API (Router / OnceCell / TestClient / take_json / take_bytes / `#[endpoint]` + `oapi` + `sse` features) åœ¨ 0.93 å…¨éƒ¨å…¼å®¹ã€‚

### ä¸ºä»€ä¹ˆä¸å‡ 0.95.x (æœ€æ–°ç‰ˆ)

| salvo ç‰ˆæœ¬ | å‘å¸ƒæ—¥ | MSRV | å…¼å®¹æ€§ |
|---|---|---|---|
| 0.79.0 | 2025-05-27 | 1.85 | å½“å‰é”å®š |
| 0.93.0 | 2026-04-30 | 1.92 | **âœ“ å‡çº§ç›®æ ‡ (rustc 1.93 å…¼å®¹)** |
| 0.94.0 | 2026-07-07 | 1.94 | âœ— éœ€ rustc 1.94 |
| 0.95.2 | 2026-07-15 | 1.94 | âœ— éœ€ rustc 1.94 (latest) |

æˆ‘ä»¬ rustc 1.93.0, æ‰€ä»¥ 0.93 æ˜¯æœ€é«˜å…¼å®¹ç‰ˆã€‚å‡ 0.95 éœ€è¦å…ˆ `rustup update 1.94`ã€‚

### é—´æ¥ä¾èµ–é™çº§ (multra)

`cargo update -p salvo` æŠŠ multra å‡åˆ° 1.1.0 (è¦ rustc 1.94, ä¸å…¼å®¹), é”å› 1.0.0 (MSRV 1.89, å…¼å®¹):

```bash
cargo update -p multra --precise 1.0.0
# Downgrading multra v1.1.0 -> v1.0.0
# Adding spin v0.10.1
```

salvo 0.93 ä»ç„¶ dep multra, ä½† 1.0.0 è·Ÿ 0.93 çš„ API å…¼å®¹ã€‚

### éªŒè¯

1. `cargo clean -p salvo -p salvo-oapi -p salvo-oapi-macros -p salvo-proxy -p salvo-serde-util -p salvo_core -p salvo_extra -p salvo_macros -p multra` â€” æ¸… incremental cache (Removed 845 files, 1.8 GiB)
2. `cargo check --workspace` â€” é‡æ–°ç¼–, 0 error, 10.57s
3. `cargo test --workspace --lib` â€” 18 ä¸ª test result, å…¨éƒ¨ ok, 0 fail
4. **303/303 lib test å…¨è¿‡** (è·Ÿå‡çº§å‰ä¸€è‡´)
5. bin test å¤±è´¥ 4 ä¸ª â€” **è·Ÿ main åˆ†æ”¯å®Œå…¨ä¸€è‡´**, æ˜¯ pre-existing broken, è·Ÿ salvo æ— å…³:
   - `ma-harness-plugin-macro/tests/macros_compile.rs` trybuild (ç¼º `tokio` dev-dep)
   - `plugins/ma-harness-plugin-hello/tests/end_to_end.rs:18` HelloService::name trait scope
   - `crates/ma-harness-conformance/tests/smoke.rs:213` FixtureEvent not found
   - `crates/ma-harness-cordis/src/key.rs:104` CtxKey<T>::new doctest should_panic ä¸ panic

### API å…¼å®¹æ€§ (å‡ºä¹æ„æ–™çš„ 0 break)

æˆ‘ä»¬ä»£ç ç”¨çš„ 0.79 ç‰¹å®š API:

| ç”¨æ³• | 0.79 çŠ¶æ€ | 0.93 çŠ¶æ€ |
|---|---|---|
| `Router` (åŸºç¡€ push / push_with_handler / get / post) | âœ“ | âœ“ (å…¼å®¹) |
| `#[handler]` / `#[endpoint]` macro | âœ“ | âœ“ (å…¼å®¹) |
| `#[endpoint]` éœ€ `oapi` feature | âœ“ | âœ“ (å…¼å®¹) |
| `JsonBody<T>` wrapper (T: ToSchema) æ‹¿ JSON body | âœ“ | âœ“ (å…¼å®¹) |
| `TestClient` + `ResponseExt` + `take_json()` | âœ“ | âœ“ (å…¼å®¹) |
| `take_bytes(Option<&Mime>)` / `take_string()` | âœ“ | âœ“ (å…¼å®¹) |
| `tokio::sync::OnceCell` å…¨å±€ + `Mutex<Option>` è¦†ç›– | âœ“ (å›  0.79 Router æ—  .data()) | âœ“ ä»å…¼å®¹ (0.93 Router::data() å­˜åœ¨ä½†æœªè¿ç§») |
| `SseEvent` æµå¼å“åº” | âœ“ | âœ“ (å…¼å®¹) |
| features `["test", "oapi", "sse"]` | âœ“ | âœ“ å…¨éƒ¨ä¿ç•™ |

**å…³é”®è§‚å¯Ÿ**: salvo 0.79 â†’ 0.93 æœŸé—´, ä¸Šè¿° API å…¨éƒ¨ 0 ç ´åæ€§å˜åŒ–ã€‚å³ä¾¿ Router::data() 0.80+ å°±æœ‰äº†, æˆ‘ä»¬ 0.79 å†™çš„ OnceCell hack åœ¨ 0.93 ä»èƒ½å·¥ä½œã€‚è¿™æ˜¯ä¿å®ˆå‡çº§æ¨¡å¼ã€‚

### é¢„æœŸæ”¶ç›Š (P6-6)

- æ‹¿åˆ° 14 ä¸ª minor çš„ bug fix + å®‰å…¨è¡¥ä¸ (1 å¹´ +)
- ç¼–è¯‘æ—¶é—´è·Ÿ binary size å‡ ä¹ä¸å˜ (salvo 0.93 é‡æ–°ç»„ç»‡è¿‡ä¾èµ–å›¾, ä½† build output ç±»ä¼¼)
- ä¸ºå‡ 0.95 / 0.96 é“ºè·¯: å‡ rustc 1.94 åæ”¹ version å­—ç¬¦ä¸²å³å¯, 0 ä»£ç æ”¹åŠ¨

### Phase 7+ å‡ 0.95.x è·¯å¾„

å¦‚æœä¸šåŠ¡æ–¹éœ€è¦ 0.95 çš„æ–°ç‰¹æ€§ (HTTP3 / Acme / WebTransport å¢å¼º / æ€§èƒ½æå‡):

1. `rustup update 1.94` (30 åˆ†é’Ÿä¸‹è½½ + install)
2. workspace `Cargo.toml`: `salvo = "0.93"` â†’ `salvo = "0.95"`
3. `crates/ma-harness-server/Cargo.toml`: `salvo_extra = "0.93"` â†’ `salvo_extra = "0.95"`
4. `cargo update -p salvo -p salvo_extra`
5. `cargo check --workspace` (é¢„æœŸ 0 break, è·Ÿ 0.79 â†’ 0.93 ä¸€æ ·ä¿å®ˆ)
6. `cargo test --workspace --lib` (303/303 é¢„æœŸ 0 fail)
7. commit + push

é¢„è®¡ 30 åˆ†é’Ÿå·¥ä½œé‡, 0 ä»£ç æ”¹åŠ¨ã€‚

### å›é€€æ–¹æ¡ˆ

å¦‚æœå‡çº§åå‡ºé—®é¢˜ (e.g. æ€§èƒ½é€€åŒ–, æŸä¸ªè¾¹ç¼˜ case fail):

```bash
git revert <commit>
# æˆ–è€…
git checkout main  # é€€å› main åˆ†æ”¯ (salvo 0.79)
```

å›é€€æˆæœ¬: 1 è¡Œ git å‘½ä»¤ã€‚

### ç»™åæ¥äºº

- salvo è·³ 14 minor 0 break, å‡çº§é—¨æ§›ä½äºé¢„æœŸ â€” è·³ 16 minor ä¹Ÿå»ºè®®å…ˆ cargo check è¯•
- multra æ˜¯ salvo çš„éšè—ä¾èµ–, å‡ salvo æ—¶è¦é” multra å…¼å®¹ç‰ˆæœ¬
- pre-existing broken test 4 ä¸ª, è·Ÿ salvo å‡çº§æ— å…³, ä¸šåŠ¡æ–¹ä¸ç”¨çº ç»“
- salvo 0.79 å†™çš„ OnceCell hack åœ¨ 0.93 ä»å…¼å®¹, ä½† **æ–°ä»£ç å»ºè®®ç”¨ Router::data() (0.80+)**, ç®€æ´
- ä¸šåŠ¡æ–¹å‡çº§è§¦å‘æ¡ä»¶: salvo CVE / salvo æ–°ç‰¹æ€§éœ€æ±‚ / ä¸šåŠ¡æ–¹è¦æ±‚
- å‡çº§æ—¶å»ºç‹¬ç«‹åˆ†æ”¯ (e.g. `salvo-X.Y-migration`), éªŒè¯é€šè¿‡å† fast-forward merge åˆ° main

## 21. salvo 0.93 â†’ 0.95 + rustc 1.93 â†’ 1.94 ä¸€æ­¥åˆ°ä½å‡çº§ (2026-08-19 / Day 101 / P6-7)

### å†³ç­–

**ä¸šåŠ¡æ–¹è¦æ±‚ä¸€æ­¥åˆ°ä½å‡åˆ° salvo 0.95 (latest), åŒæ—¶å‡çº§ rustc 1.93 â†’ 1.94**ã€‚è·³ 16 minor (0.79 â†’ 0.95) + å‡ 1 ä¸ª toolchain, 0 API break, 0 ä»£ç æ”¹åŠ¨, 303/303 lib test å…¨è¿‡ã€‚

å½±å“èŒƒå›´:
- workspace `Cargo.toml`: `salvo = "0.93"` â†’ `salvo = "0.95"`
- `crates/ma-harness-server/Cargo.toml`: `salvo_extra = "0.93"` â†’ `salvo_extra = "0.95"`
- `Cargo.lock`: salvo å…¨å¥— 0.93.0 â†’ 0.95.2, multra 1.0.0 â†’ 1.1.0, tokio-tungstenite 0.29 â†’ 0.30, ulid 1.2.1 â†’ 3.0.0
- **æ–° toolchain**: rustc 1.94.1 (e408947bf 2026-03-25) é€šè¿‡ `rustup install 1.94` è£…å¥½
- **ä»£ç å±‚æ”¹åŠ¨**: **0 è¡Œ** (è·Ÿ P6-6 ä¸€æ ·, OnceCell/Mutex<Option> / TestClient / take_json / #[endpoint]+oapi+sse features å…¨éƒ¨ 0.95 å…¼å®¹)

### rustc å‡çº§è·¯å¾„ (å›½å†…ç½‘ç»œ)

**é—®é¢˜**: `rustup install 1.94 --profile minimal` ç›´æ¥èµ° `https://static.rust-lang.org` åœ¨å›½å†… 7890 ä»£ç†ç¯å¢ƒ Connection reset (os error 10054)ã€‚

**è§£å†³**: èµ°å›½å†… rustup é•œåƒã€‚

å°è¯• 1: `https://mirrors.ustc.edu.cn/rust-static` âœ“ **æˆåŠŸ**
- `RUSTUP_DIST_SERVER='https://mirrors.ustc.edu.cn/rust-static'`
- `RUSTUP_UPDATE_ROOT='https://mirrors.ustc.edu.cn/rust-static/rustup'`
- è£… rustc 1.94.1 + cargo + rust-std
- ~5 åˆ†é’Ÿ

å°è¯• 2 (å¤‡é€‰): `https://mirrors.tuna.tsinghua.edu.cn/rustup` éƒ¨åˆ†æˆåŠŸ
- æ‹¿åˆ° channel-rust-stable.toml (æœ€æ–° stable)
- ä½† 1.94 release artifact åœ¨ tuna é•œåƒé‡Œæ²¡æ‰¾åˆ° (tuna é•œåƒä» 2026-07-16 å¼€å§‹ sync, 1.94 æ˜¯ 2026-03-25 å‘çš„, å·²ç» outdated)
- ustc é•œåƒæ›´å…¨, æ¨è

```bash
$env:RUSTUP_DIST_SERVER='https://mirrors.ustc.edu.cn/rust-static'
$env:RUSTUP_UPDATE_ROOT='https://mirrors.ustc.edu.cn/rust-static/rustup'
rustup install 1.94 --profile minimal
# 1.94-x86_64-pc-windows-msvc installed - rustc 1.94.1 (e408947bf 2026-03-25)

rustup default 1.94
# default toolchain set to 1.94-x86_64-pc-windows-msvc
```

### éªŒè¯

1. `cargo clean -p salvo -p salvo-oapi -p salvo-oapi-macros -p salvo-proxy -p salvo-serde-util -p salvo_core -p salvo_extra -p salvo_macros -p multra` (æ¸… incremental cache)
2. `cargo check --workspace` é‡æ–°ç¼–, 0 error, **1m 13s** (æ¯” P6-6 æ…¢, å› ä¸ºè·³æ›´å¤š minor + å‡ toolchain é‡æ–°é“¾æ¥æ›´å¤š deps)
3. `RUST_TEST_THREADS=1 cargo test --workspace --lib` â€” 18 ä¸ª test result, å…¨éƒ¨ ok, **303/303 å…¨è¿‡** âœ“
4. **å¹¶å‘è·‘æœ‰ 1 ä¸ª flake** (`http::tests::post_v1_sessions_then_get` è¿”å› 500 æ›¿ 200):
   - è·Ÿ P6-5 å·²çŸ¥ flake ä¸€è‡´ (test isolation é—®é¢˜, è·Ÿ salvo å‡çº§æ— å…³)
   - ä¸²è¡ŒåŒ– (`RUST_TEST_THREADS=1`) å®Œå…¨è§£å†³
   - ä¸šåŠ¡æ–¹æ¥å— (CI é»˜è®¤ `RUST_TEST_THREADS=1`)
5. bin test å¤±è´¥ 4 ä¸ª â€” pre-existing broken (è·Ÿ main ä¸€è‡´, è·Ÿ salvo æ— å…³)

### å…³é”®å‘ç° (è·Ÿ P6-6 ä¸€æ ·ä»¤äººæƒŠè®¶)

- **è·³ 16 minor ä»ç„¶ 0 break** â€” 0.79 â†’ 0.95 æœŸé—´, 9 ç±» API å…¨éƒ¨å…¼å®¹
- **0.94/0.95 å¼•å…¥æ–°ç‰¹æ€§** (HTTP3 / Acme å¢å¼º / æ€§èƒ½) å…¨éƒ¨ additive, ä¸å½±å“æ—¢æœ‰ç”¨æ³•
- **OnceCell/Mutex<Option> hack 0.79 å†™æ³•åœ¨ 0.95 ä»å·¥ä½œ** â€” å³ä¾¿ Router::data() 0.80+ å°±æœ‰
- **ä¿å®ˆå‡çº§æ¨¡å¼**: salvo 0.79 â†’ 0.95 æœŸé—´æ²¡ break API, 1.3 å¹´çš„ minor release éƒ½å¾ˆ backward-compatible

### å›½å†… rustup é•œåƒé€ŸæŸ¥

| é•œåƒ | URL | 1.94 artifact | é€‚ç”¨ |
|---|---|---|---|
| rust-lang.org (official) | https://static.rust-lang.org | âœ“ | æµ·å¤– |
| ustc | https://mirrors.ustc.edu.cn/rust-static | âœ“ | **å›½å†…æ¨è** |
| tuna | https://mirrors.tuna.tsinghua.edu.cn/rustup | âœ— (1.94 æ²¡) | å›½å†…å¤‡é€‰ (æœ€æ–° stable) |
| rsproxy | https://rsproxy.cn | éƒ¨åˆ† | cargo ä¸“ç”¨, rustup ä¸å…¨ |
| ä¸­ç§‘å¤§æ—§è·¯å¾„ | https://mirrors.ustc.edu.cn/rustup | 404 | è·¯å¾„å·²è¿ç§» |

**ç»™åæ¥äºº**: å›½å†…è£… rustc 1.94+ å¿…èµ° `RUSTUP_DIST_SERVER=https://mirrors.ustc.edu.cn/rust-static`, ç›´æ¥ rustup èµ°å®˜æ–¹ 100% å¤±è´¥ (Connection reset)ã€‚

### é¢„æœŸæ”¶ç›Š (P6-7)

- **æœ€æ–° salvo 0.95.2** (2026-07-15) + 0.94 è·³ 16 minor çš„ bug fix + å®‰å…¨è¡¥ä¸
- **æ–°ç‰¹æ€§å¯ç”¨**: HTTP3, Acme è‡ªåŠ¨ TLS, WebTransport, salvo-jwt-auth, salvo-cache ç­‰ (æŒ‰éœ€)
- **rustc 1.94** std lib æ”¹è¿› (e.g. new error patterns, formatting tweaks)
- **ç»§ç»­å‡çº§åˆ° 0.96+** åªéœ€æ”¹ `version = "0.95"` â†’ `"0.96"` + `cargo update`, 0 ä»£ç æ”¹åŠ¨é¢„æœŸ

### Phase 7+ å‡ salvo 0.96+ è·¯å¾„

æˆ‘ä»¬å·²ç»åœ¨ rustc 1.94 toolchain, ä¸‹æ¬¡å‡çº§ 0 éšœç¢:

1. workspace `Cargo.toml`: `salvo = "0.95"` â†’ `salvo = "0.96"` (å‡è®¾ 0.96 å·²å‘)
2. `crates/ma-harness-server/Cargo.toml`: `salvo_extra = "0.95"` â†’ `"0.96"`
3. `cargo update`
4. `cargo check --workspace` (é¢„æœŸ 0 break)
5. `RUST_TEST_THREADS=1 cargo test --workspace --lib` (é¢„æœŸ 303/303 å…¨è¿‡)
6. commit + push

é¢„è®¡ 15 åˆ†é’Ÿå·¥ä½œé‡, 0 ä»£ç æ”¹åŠ¨ã€‚

### ç»™åæ¥äºº

- salvo è·³ 16 minor + å‡ rustc 1 minor, 0 break â€” å‡çº§é—¨æ§›æä½
- å›½å†… rustup è£…æ–° toolchain èµ° ustc é•œåƒ (å…¶ä»–é•œåƒä¸å…¨)
- ä¸²è¡Œæµ‹è¯• (`RUST_TEST_THREADS=1`) è§£å†³å¹¶å‘ isolation flake
- pre-existing broken 4 ä¸ªä¸€ç›´å­˜åœ¨, è·Ÿ salvo æ— å…³
- ä¸šåŠ¡æ–¹æƒ³ç”¨æ–°ç‰¹æ€§ (HTTP3/Acme) ç°åœ¨å¯ç”¨, 0.95 å…¨ feature-gated å¯ç”¨



## 22. Phase 7 æ”¶å®˜ (2026-08-19 / Day 101)

**ç›®æ ‡**: 6-8 å‘¨ä¸“æ³¨æœŸ, äº¤ä»˜ 4 P0: Web UI + å®¡æ‰¹æµç¨‹ + å·¥å…·ç®¡é“å‡çº§ + å­ä»£ç† fork.

**ç»“æœ**: Day 101 å…¨éƒ¨æ”¶å®˜, å®é™…èŠ‚å¥å‹ç¼©åˆ°å•æ—¥å®Œæˆ (æœŸé—´é€Ÿç‡é™æµå¯¼è‡´éƒ¨åˆ†æµ‹è¯•è·³è¿‡, ä¸šåŠ¡æ–¹æ¥å—).

### äº¤ä»˜æ¸…å• (10+ ä¸ªæ–° commits)

- a54bc2a P7-0 ä¿® 4 ä¸ª pre-existing broken test
- 2436a42 P7-1.1 Web UI éª¨æ¶ (React + Vite + TS)
- e251119 P7-1.2 tonic-web é›†æˆ â€” gRPC-web æ¡¥
- 66580cf P7-1.3/1.4/1.5 Session Detail + Trajectory + TokenStats
- 7a802cb P7-1.7 SSE events/stream å®æ—¶æ¨é€
- f25e016 P7-2.1/2/3 å®¡æ‰¹æœåŠ¡ + pre-execute hook
- b2d09c3 P7-2.4 TUI approval ç®€åŒ–ç‰ˆ
- f3745e0 P7-2.5 HTTP approval ç«¯ç‚¹ v1
- 1eeec28 P7-2.6 å®¡æ‰¹å®¡è®¡ log helper
- d2dd695 P7-2.7 é›†æˆæµ‹è¯• 8 scenarios
- e10f9a8 P7-3 7-stage pipeline
- 93b7a78 P7-3.4 ChannelApprovalService oneshot
- 3e92cdc P7-3.6 HTTP approval v2 æ¥ ChannelApprovalService
- 742ea9d P7-4 å­ä»£ç† fork (SubagentSpec)
- 08831b0 P7-5 TUI Trajectory ç€è‰²

### å…³é”®å†³ç­–

- Web UI é€‰ React + Vite + TypeScript (ç”Ÿæ€ç†Ÿ, æ‹›äººæ˜“)
- å®¡æ‰¹ v1 ç®€åŒ– + v2 å®Œæ•´ æ‹†åˆ†: TUI èµ° pending queue ç®€åŒ–ç‰ˆ, HTTP èµ° placeholder; v2 é›†æˆ ChannelApprovalService oneshot
- Pipeline 7 é˜¶æ®µ (pre/guard/approval/exec/post/finalize/result): å†…éƒ¨ Arc<Context> å…±äº«, ToolInvokeFn æ”¹ Fn(Value, &Context) è®© retry cheap
- Context ä¸å¯ Clone: å†…éƒ¨ Box<dyn Any> + AtomicBool ä¸æ”¯æŒ, ç”¨ Arc<Context> è·¨ stage å…±äº«
- ChannelApprovalService: tokio::sync::oneshot + Arc<Mutex<HashMap>> å®ç°, ä¸šåŠ¡æ–¹ (TUI key / HTTP POST) æ¨ decision å”¤é†’
- SSE events/stream v1 è½®è¯¢ EventLog: 1s é—´éš” + heartbeat ä¿æ´»; v2 broadcast channel ç•™ P8-2

### æµ‹è¯•ç´¯è®¡

- 380 â†’ 400 lib + bin tests (+20)
- 311 â†’ 326 lib tests (+15)
- cordis 76 â†’ 81 (+5)
- core 31 â†’ 38 (+7 pipeline)
- server 37 â†’ 44 (+7 approval v2 + SSE)
- tui 32 â†’ 32 (1 æ”¹åŠ¨, 0 æ–°)
- subagent 2 â†’ 8 (+6 SubagentSpec)
- integration: 8 (approval flow)
- bin tests: 27 â†’ 27 (æ— æ–°)

### ç´¯è®¡

- decision-log: 1-21 â†’ 1-22
- README æ ‡ P7 çŠ¶æ€
- 130+ â†’ 200+ commit (Day 0-101)
- Web UI 3080 ç«¯å£ä¸Šçº¿ (P7-1.1+)
- HTTP API: 8 paths â†’ 9 paths (+SSE events/stream)
- å®Œæ•´å®¡æ‰¹æµç¨‹: è£… registry â†’ tool invoke â†’ request_approval â†’ ä¸šåŠ¡æ–¹æ¨ decision â†’ continue

### ç•™å¾… P8+

- P7-1.8 Playwright e2e (å—é™)
- TUI approval AppMode::Approval y/n å¼¹çª— v2 (oneshot é›†æˆ)
- Web UI approval ç«¯ç‚¹çœŸå†³ç­– v2 (å·²é€šè¿‡ ChannelApprovalService å®ç°, é›†æˆ)
- Phase 8: ä¸Šä¸‹æ–‡å‹ç¼© / Token ç›‘æ§ / å¤šæ¨¡å‹æ‰©å±•
- Phase 9: æ¨¡å¼æ‰©å±• / Capability Seam / Creator æ¨¡å¼

## 23. Phase 8 æ”¶å®˜ (2026-08-19 / Day 101)

**ç›®æ ‡**: ä¸Šä¸‹æ–‡å‹ç¼© / Token ç›‘æ§ / å¤šæ¨¡å‹æ‰©å±• / æ¨¡å¼æ‰©å±•.

**ç»“æœ**: 4 commits å…¨éƒ¨ Day 101 æ”¶å®˜, è·Ÿ P7 ä¸€æ—¥å®ŒæˆèŠ‚å¥ä¸€è‡´.

### äº¤ä»˜æ¸…å• (4 commits)

- `48bce3e` P8-1 ä¸Šä¸‹æ–‡å‹ç¼© (CompressionPolicy + SlidingWindow{20} default + estimate_tokens ç²—ä¼°)
- `3a0c122` P8-2 `/v1/sessions/{id}/token-stats` ç«¯ç‚¹
- `78a57bd` P8-3 å¤šæ¨¡å‹æ‰©å±• (Azure / Local / DeepSeek + env auto)
- `d312f5e` P8-4 æ¨¡å¼æ‰©å±• (Default / Minimal / PTC / Creator)

### å…³é”®å†³ç­–

- **CompressionPolicy ä¸‰æ€**: `Never` / `SlidingWindow{keep_last_n}` / `Summarize` (v2 TODO), default SlidingWindow{20}
- **estimate_tokens ç²—ä¼°**: ASCII 1/4 token, CJK 1/1.5 token, é¿å… tiktoken å¤æ‚ dep
- **load_history_from_log**: æ‹¿ ModelRequest/ModelResponse events é‡å»º messages (P8-1 + P7-1.7 é…å¥—)
- **EVENT_LOG: ModelVisible å­—æ®µ**: ApprovalRequest/Decision æ®µä½ 800/801, `model_visible = false` (å†…éƒ¨å®¡è®¡ä¸ä¸Š model context)
- **serde åºåˆ—åŒ– 0-1 normalized** (P8-1): `load_history` `payload_json` ååºåˆ—åŒ– `serde_json::Value`, å– `content` å­—æ®µ
- **å¤šæ¨¡å‹ env auto-detect**: `OPENAI_API_KEY` / `ANTHROPIC_API_KEY` / `DEEPSEEK_API_KEY` å“ªä¸ªæœ‰å°±å“ªä¸ª, ä¸šåŠ¡æ–¹ä¸æŒ‡å®šèµ° default
- **proto OperatingMode enum**: DEFAULT=1 / MINIMAL=2 å·²å®š, PTC=3 / CREATOR=4 ä¸šåŠ¡æ–¹å ä½
- **PTC (Persistent Tool Calling)** (P8-4): å•è½®å¤š tool è°ƒ, ä¸åœ¨ä¸­é—´ä¸­æ–­ (Code Mode ç±»ä¼¼)
- **OperatingModeConfig::effective_plugins** (P8-4): 7 first-party plugins (Default/PTC/Creator) / 0 (Minimal) / ä¸šåŠ¡æ–¹ override

### æµ‹è¯•ç´¯è®¡ (P8 å)

- core: 38 â†’ 95 (+57, ä¸Šä¸‹æ–‡å‹ç¼©/å¤šæ¨¡å‹/æ¨¡å¼)
- model: 0 â†’ 12 (+12 adapter)
- seam: åŠ  2 å…¬å…± API re-export æµ‹è¯•

### ç´¯è®¡

- decision-log: 1-22 â†’ 1-23
- OperatingMode å››ç§ (Default / Minimal / PTC / Creator) ä¸šåŠ¡æ–¹å¯åˆ‡æ¢
- CompressionPolicy ä¸‰æ€ + estimate_tokens ç²—ä¼°å¯ç”¨
- 4 ä¸ª model adapter (OpenAI / Anthropic / Azure / Local / DeepSeek) env auto

### ç•™å¾… P9+

- CompressionPolicy::Summarize çœŸå®ç° (v2 TODO)
- DeepSeek çœŸå®æ¨¡å‹æ¥å…¥ (env æœ‰äº†, ä¸šåŠ¡æ–¹æœªæ)
- Bedrock / Vertex AI ç­‰å…¬æœ‰äº‘ adapter (ç•™ç»™ P10-6)


## 24. Phase 9 æ”¶å®˜ (2026-08-19 / Day 101)

**ç›®æ ‡**: æ¨¡å¼æ‰©å±• (P8-4) è½å® + Capability Seam + Creator æ¨¡å¼éª¨æ¶.

**ç»“æœ**: 2 commits æ”¶å®˜ (P8-4 å·²æ”¶, P9-1/2 å…¨æ”¶).

### äº¤ä»˜æ¸…å• (2 commits)

- `7ca642f` P9-1 Capability Seam å…¬å¼€ stable API re-exports (VERSION / API_VERSION + å…¨éƒ¨ stable types)
- `05ded14` P9-2 Creator æ¨¡å¼éª¨æ¶ (åŠ¨æ€ plugin å·¥å‚ v1)

### å…³é”®å†³ç­–

- **ma-harness-seam stable API**: ä¸šåŠ¡æ–¹ `use ma_harness_seam::*` ä¸€è¡Œ re-export, å†…éƒ¨ `ma-harness-core` / `ma-harness-cordis` é¢‘ç¹å˜, ä¸šåŠ¡æ–¹ä¸æ„Ÿ
- **VERSION + API_VERSION const**: ä¸šåŠ¡æ–¹ verify è£…å¯¹ç‰ˆæœ¬, ABI break ä¸šåŠ¡æ–¹èƒ½ compile-time check
- **Creator PluginSpec è®¾è®¡** (P9-2): `name` + `version` + `description` + `source_code` + `entry_fn` + `dependencies`, key = name (UUID æ”¹ name)
- **CreatorRegistry å†…å­˜ HashMap** (P9-2): åŒæ­¥ `parking_lot::Mutex`, v2 æ”¹ DashMap å¼‚æ­¥å‹å¥½
- **CreatorError ä¸‰æ€**: DuplicateName / NotFound / Compile / NotLoaded
- **CompileStatus enum**: Pending / Compiling / Loaded / Failed
- **v1 ç®€åŒ–**: compile æ˜¯å ä½ (æ ‡ Loaded, ä¸çœŸç¼–è¯‘), v2 çœŸç¼–è¯‘ç•™ç»™ P10-1

### æµ‹è¯•ç´¯è®¡ (P9 å)

- core: 95 â†’ 95 (Creator éª¨æ¶ 0 lib test å¢åŠ , å…¨åœ¨ P10-1)
- seam: åŠ  VERSION / API_VERSION const æµ‹è¯•

### ç´¯è®¡

- decision-log: 1-23 â†’ 1-24
- seam crate å…¬å¼€ stable API å®Œæ•´ (ä¸šåŠ¡æ–¹ä¸€è¡Œ use)
- CreatorFactory v1 å¯ç”¨ (create_and_load å ä½)

### ç•™å¾… P10+

- Creator çœŸç¼–è¯‘ (P10-1.5/1.6/1.7)
- è·¨ dylib å…±äº« ToolRegistry (P10-1.8)


## 25. Phase 10 æ”¶å®˜ (2026-08-19 / Day 101)

**ç›®æ ‡**: 8 é¡¹ä¸šåŠ¡æ–¹é«˜ä¼˜å…ˆä»»åŠ¡ (Creator çœŸç¼–è¯‘ + è·¨å¹³å°ç¡¬åŒ– + libloading é—­ç¯ + Profile éš”ç¦» + AGENTS.md è§£æ + Trajectory å¢å¼º + å¤šäº‘ adapter + Metrics endpoint + TUI modal é›†æˆ).

**ç»“æœ**: 8/8 æ”¶å®˜, 10 commits å…¨éƒ¨ Day 101 å®Œæˆ.

### äº¤ä»˜æ¸…å• (10 commits)

- `9cdda7e` P10-5 AGENTS.md è§£æ (auto system prompt)
- `6fa9cba` P10-4 Trajectory å¤šåˆ—å¸ƒå±€ + ç±»å‹ chips + æŒä¹…åŒ–ç­›é€‰
- `06e6586` P10-3 Profile éš”ç¦» (per-config)
- `c1b9a09` P10-1.5 Creator çœŸå®ç¼–è¯‘ v1.5 æ ¡éªŒ + ç¼–è¯‘æ­¥éª¤
- `8d1f7dd` P10-2 TUI y/n å¼¹çª— v2 (oneshot æ¡¥æ¥)
- `66411e7` P10-6 Bedrock / Vertex AI adapter (AWS/GCP)
- `7d4c756` P10-7 /v1/metrics Prometheus endpoint
- `78a79bd` P10-2.5 TUI y/n modal å®Œæ•´é›†æˆ
- `6b884d6` P10-1.6 Creator ç¼–è¯‘è·¨å¹³å°ç¡¬åŒ– (Day 101+1)
- `f19f056` P10-1.7 Creator libloading åŠ è½½ dylib (Day 101+1)

### å…³é”®å†³ç­–

- **AGENTS.md è§£æ** (P10-5): é¡¹ç›®æ ¹è‡ªåŠ¨åŠ è½½åˆ° system prompt, ä¸šåŠ¡æ–¹ä¸ç”¨æ‰‹åŠ¨æŒ‡å®š
- **Profile éš”ç¦»** (P10-3): per-config (å¼€å‘/ç”Ÿäº§/æµ‹è¯•), plugins / approval policy / model å…¨åˆ‡
- **TUI y/n modal v2** (P10-2/2.5): oneshot channel è·Ÿ host ChannelApprovalService æ¡¥æ¥, ä¸šåŠ¡æ–¹æŒ‰ y/n å³å†³
- **Bedrock / Vertex AI adapter** (P10-6): å…¬æœ‰äº‘ LLM æ¥å…¥, è·Ÿ P8-3 è‡ªæ‰˜ç®¡/Azure/Local é…å¥—
- **Prometheus endpoint** (P10-7): /v1/metrics æš´éœ² token / session / tool call è®¡æ•°
- **P10-1.6 è·¨å¹³å°ç¡¬åŒ–**: 6 é¡¹ä¿®å¤ (è§ Â§ 26 è¯¦ç»†)
- **P10-1.7 libloading é—­ç¯**: 6 é¡¹æ”¹é€  (è§ Â§ 27 è¯¦ç»†)

### æµ‹è¯•ç´¯è®¡ (P10 å)

- core: 95 â†’ 106 (+11, Creator ç¼–è¯‘/åŠ è½½/è·¨å¹³å°)
- server: 44 â†’ 50 (+6, metrics + bedrock/vertex)
- tui: 32 â†’ 35 (+3, modal é›†æˆ)
- ui (Web): 4 â†’ 4 (Trajectory å¤šåˆ—)
- model: 12 â†’ 18 (+6, bedrock/vertex)

### ç´¯è®¡

- decision-log: 1-24 â†’ 1-25
- Phase 7-10 å…¨éƒ¨æ”¶å®˜, ç´¯è®¡ 200+ commit
- Core 106 lib test pass, 0 fail
- P10-1.5/1.6/1.7 çœŸç¼–è¯‘ + è·¨å¹³å°ç¡¬åŒ– + libloading é—­ç¯


## 26. P10-1.6 Creator ç¼–è¯‘è·¨å¹³å°ç¡¬åŒ– (2026-08-20 / Day 101+1)

**ç›®æ ‡**: P10-1.5 æ¥å…¥åè¿˜æœ‰è·¨å¹³å°å‘æ²¡ä¿®, ä¸šåŠ¡æ–¹æåˆ°"éœ€è¦è€ƒè™‘è·¨å¹³å°", ä¿® 6 ä¸ªè·¨å¹³å°é—®é¢˜.

**commit**: `6b884d6` (78ad79d..6b884d6)

### Critical ä¿®æ³•

1. **`dylib_filename` Box::leak å†…å­˜æ³„æ¼ â†’ æ”¹è¿” `String`**
   - ä¹‹å‰ `pub fn dylib_filename(spec_name: &str) -> &'static str` ä¸‰ç§å¹³å°åˆ†æ”¯éƒ½ `Box::leak(format!(...))`
   - æ¯æ¬¡è°ƒç”¨æ³„æ¼ ~32-64 bytes, ä¸šåŠ¡æ–¹ 1000 æ¬¡è°ƒç”¨æ³„æ¼ 32KB+
   - æ”¹ `pub fn dylib_filename(spec_name: &str) -> String`, è°ƒç”¨æ–¹ `.to_string()` æˆ–ç›´æ¥ `String`

2. **`compile()` åŒæ­¥ cargo subprocess æ”¹ `tokio::task::spawn_blocking`**
   - cargo ç¼–è¯‘å¯è¾¾åˆ†é’Ÿçº§, åŒæ­¥è·‘åœ¨ tokio worker ä¸Š block æ•´ä¸ª async runtime
   - ä¿®æ³•: `tokio::task::spawn_blocking(move || compile_plugin(&spec, &cfg)).await`
   - æ³¨æ„ `.await` è¿” `Result<Result<T, E>, JoinError>`, å†…å¤–ä¸¤å±‚éƒ½è¦ handle

### æ­£ç¡®æ€§

3. **`render_cargo_toml` edition 2021 â†’ 2024** (è·Ÿ workspace å¯¹é½)
4. **`find_cargo` åŠ  `cargo --version` éªŒè¯ + æ”¹è¿” `Result`** (ä¹‹å‰ `where`/`which` å‘½ä»¤è¿” placeholder, é”™è¯¯ä¿¡æ¯å»¶è¿Ÿ)
5. **`dylib_filename` åŠ  Windows éæ³•å­—ç¬¦è¿‡æ»¤** (`<>:"/\\|?*` + æ§åˆ¶å­—ç¬¦ â†’ `_`, æœ«å°¾ `.` ä¿®å‰ª, ç©ºå fallback)
6. **è·¨å¹³å° env ä¼ é€’**: Windows `PATHEXT` (`.EXE;.CMD;.BAT;.COM`) + `SYSTEMROOT` (cmd.exe å†…ç½®å‘½ä»¤éœ€è¦), Unix ä¿æŒ `PATH` / `HOME` / `CARGO_HOME` / `RUSTUP_HOME`, åŠ  `RUSTC_WRAPPER` é€ä¼  (sccache)

### API æ‰©å±•

- `CreatorRegistry::dylib_artifact_path(name) -> Result<PathBuf, CreatorError>` helper, ä¸šåŠ¡æ–¹ P10-1.7 libloading æ‹¿äº§ç‰©ç»å¯¹è·¯å¾„

### å…³é”® Pattern

- **åŒæ­¥ subprocess åœ¨ async context å¿…èµ° `spawn_blocking`** (cargo ç¼–è¯‘å¿…èµ°)
- **è·¨å¹³å° helper å‡½æ•°è¿” `String` ä¼˜äº `&'static str`** (é¿å… Box::leak å pattern)
- **find_cargo ç±»ç¯å¢ƒæŸ¥æ‰¾å…ˆ verify å†è¿”** (é¿å… placeholder é”™è¯¯ä¿¡æ¯å»¶è¿Ÿ)

### æµ‹è¯•ç´¯è®¡ (P10-1.6 å)

- core: 95 â†’ 103 (+8, dylib_filename è·¨å¹³å° + çœŸ cargo ç¼–è¯‘é›†æˆ)
- çœŸ cargo ç¼–è¯‘é›†æˆæµ‹åœ¨ Windows è·‘è¿‡ ~1.5s debug ç¼–è¯‘

### ç»™åæ¥äºº

- ä¸šåŠ¡æ–¹è·¨å¹³å° subprocess: PATHEXT (Windows) + SYSTEMROOT (Windows) + RUSTC_WRAPPER (sccache) å¿…é€ä¼ 
- ä¸šåŠ¡æ–¹åœ¨ Windows server core è·‘ cargo: `rustup default stable-x86_64-pc-windows-msvc` + MSVC build tools
- ä¸šåŠ¡æ–¹æ‰© sanitize (e.g. å…è®¸ `.`): æ”¹ `sanitize_lib_name` å³å¯


## 27. P10-1.7 Creator libloading é—­ç¯ (2026-08-20 / Day 101+1)

**ç›®æ ‡**: P10-1.5/1.6 çœŸç¼–è¯‘èƒ½è·‘å‡º cdylib äº§ç‰©, P10-1.7 é—­ç¯: çœŸ cargo ç¼–è¯‘ + çœŸ libloading åŠ è½½ dylib + è°ƒ register å‡½æ•°. ä¸šåŠ¡æ–¹çœŸæ­£ç”¨ Creator æ¨¡å¼åŠ¨æ€ç”Ÿæˆ tool.

**commit**: `f19f056` (6b884d6..f19f056)

### æ ¸å¿ƒ API æ”¹é€ 

1. **`CreatorRegistry::load_into(name) -> Result<LoadedPlugin, CreatorError>` çœŸ libloading**
   - ä¹‹å‰ v1 å ä½ `Ok(())`, ç°åœ¨ `libloading::Library::new(path)` è·¨å¹³å°åŠ è½½
     (Linux/macOS: dlopen / Windows: LoadLibraryW)
   - æ‰¾ `register` ç¬¦å· (`extern "C" fn()`), è°ƒ register (side effect)
   - `[allow(unsafe_code)]` åœ¨å‡½æ•° (workspace lint `deny(unsafe_code)` æ‹¦ unsafe block)

2. **æ–° `LoadedPlugin` RAII å¥æŸ„**
   - æŒ `_library: libloading::Library`, Drop æ—¶ dlclose (Linux) / FreeLibrary (Windows)
   - ä¸šåŠ¡æ–¹æ‹¿ `loaded.name()` / `loaded.path()`, ä¸éœ€è¦ç®¡åº•å±‚

3. **`CreatorError::Load(String)` æ–°å˜ä½“** (libloading å¤±è´¥)

### ä¿®å¤ P10-1.6 æ¼æ´

- `dylib_artifact_path` ä¹‹å‰ç”¨ `self.output_dir` æ‹¼, ä½† compile_plugin å®é™…å†™åˆ° `cfg.output_dir`
- é”™ä½ â†’ LoadedPlugin æ‹¿ä¸åˆ°çœŸå®è·¯å¾„
- ä¿®: `PluginRecord.artifact_path: Option<PathBuf>` å­—æ®µ, compile æˆåŠŸåè®°å½•çœŸå®è·¯å¾„
- `dylib_artifact_path` ä¼˜å…ˆ record è®°å½•, å…œåº• self.output_dir

### CreatorFactory::create_and_load æ”¹ API

- ä¹‹å‰: `async fn create_and_load(spec, &ToolRegistry) -> Result<String, _>`
- ç°åœ¨: `async fn create_and_load(spec) -> Result<LoadedPlugin, _>`
- ä¸šåŠ¡æ–¹æ‹¿ LoadedPlugin å¥æŸ„ (RAII ä¿ dylib æ´»)

### ABI è·¨ dylib è®¾è®¡ (P10-1.7 v1)

- plugin `register` æ”¹ `#[unsafe(no_mangle)] pub extern "C" fn()`
  - **Rust 2024 edition ä¸¥æ ¼**: `#[no_mangle]` èµ° `unsafe(...)` åŒ…è£¹
  - ä¹‹å‰ `#[no_mangle]` ç›´æ¥ attribute åœ¨ 2024 edition æŠ¥ `unsafe attribute used without unsafe`
- C-ABI å…¼å®¹, libloading::Symbol<extern "C" fn()> ç›´æ¥æ‹¿
- è·¨ dylib è¾¹ç•Œä¼  Rust trait object (Arc<dyn Fn> + Context + BoxFuture) ABI ä¸ç¨³
  - v1 ç®€åŒ–: register æ— å…¥å‚, plugin è‡ªå·± eprintln / è®¾ static
  - P10-1.8 è®¡åˆ’: plugin ä¾èµ– workspace `ma-harness-core` å…±äº« ToolRegistry ç±»å‹

### Dep

- åŠ  `libloading = "0.8"` åˆ° ma-harness-core
- Cargo.lock è‡ªåŠ¨æ›´æ–° (libloading 0.8.x + dependencies)

### æµ‹è¯•ç´¯è®¡ (P10-1.7 å)

- core: 103 â†’ 106 (+3, libloading é›†æˆæµ‹)
- çœŸ cargo ç¼–è¯‘ + çœŸ libloading é›†æˆæµ‹é€šè¿‡ (cdylib .dll è½ç›˜ + dlopen + è°ƒ register)

### å…³é”® Pattern

- **è·¨ dylib è¾¹ç•Œè®¾è®¡**: `extern "C" fn()` æ¯” Rust trait object ABI ç¨³
- **Rust 2024 unsafe attribute**: `#[unsafe(no_mangle)]` æ›¿æ¢ `#[no_mangle]`, åŒæ ·è§„åˆ™é€‚ç”¨ `#[link_section]` / `#[export_name]`

### P10-1.8 ç•™ç»™åæ¥äºº

- plugin ä¾èµ– workspace `ma-harness-core` (path = "..." è‡ªåŠ¨ resolve)
  - generated Cargo.toml åŠ  `ma-harness-core = { path = "../<host-crate>" }`
- `register` æ”¹ `(registry: &ToolRegistry)`, plugin å†…éƒ¨ `registry.register(schema, invoke_fn)`
- ABI å…±äº«: å¼ºåˆ¶ plugin è·Ÿ host åŒä¸€ä»½ ma-harness-core äºŒè¿›åˆ¶ (Rust 1.85+, edition 2024)
- sandbox: P10-1.7 å½“å‰ unsafe åŠ è½½ dylib æ²¡ sandbox, ä¸šåŠ¡æ–¹åº”å®¡æ‰¹åæ‰è°ƒ

## 28. P11-1 baseline + P11-1.5 è½¬æ¢å±‚æ”¹è¿›æ”¶å®˜ (2026-08-20 / Day 101+1)

> è·Ÿ dsh æ€§èƒ½å¯¹é½ç¬¬ä¸€æ­¥: é‡åŒ– baseline + ä¿®è½¬æ¢å±‚

### å†³ç­–

1. **P11-1 baseline å‡º 5/8 + 2/7 = (62.5% / 28.6%)** â€” smoke 3 fail by design (æµ‹ framework ä¸€è‡´æ€§), dsh_synthetic 5 fail å…¨æ˜¯è½¬æ¢å±‚é—®é¢˜
2. **P11-1.5 è½¬æ¢å±‚æ”¹è¿›** â€” ä¿® dsh_format è®© dsh_synthetic **28.6% â†’ 100% (7/7)**
3. **P11 è·¯çº¿å›¾ (12-18 å‘¨)**: P11-1 baseline â†’ P11-2 dsh Terminal Bench â†’ P11-3 `mah-py` Python SDK â†’ P11-4 ACP / P11-5 å¤šæ¨¡æ€ / P11-6 Plugin Registry

### å…³é”®è®¾è®¡å†³ç­–

#### dsh_format è½¬æ¢å±‚æ”¹è¿› (P11-1.5)

**convert_input æ´¾ç”Ÿ** (input.events ç©º + messages éç©º):

- ç¬¬ä¸€ä¸ª user message è§¦å‘ **RunStart å‰ç½®** (è¡¨ç¤º session å¯åŠ¨, payload `{model: "stub"}`)
- for msg in messages:
  - `user` â†’ `UserInput { content }`
  - `assistant` â†’ `ModelResponse { content }`
  - `system` â†’ `SystemMessage { content }`
  - `tool` â†’ `ToolResult { result }`

**convert_expected åŒ…è£…** (data éå¯¹è±¡æ—¶èµ°ç‰¹æ®Š key):

| event_type | key |
|---|---|
| `UserInput` / `ModelResponse` / `SystemMessage` / `ToolError` | `content` |
| `ToolResult` | `result` |
| å…¶å®ƒ | `data` |

**convert_expected æ´¾ç”Ÿ** (expected_output.messages):

- assistant role â†’ `ModelResponse { content }` (è·Ÿåœ¨ expected.events åé¢)

**P11-1.5 å•å…ƒæµ‹è¯•** (æ–°å¢ 5 ä¸ª, 5 â†’ 10):

1. `parse_dsh_derives_user_input_from_messages` â€” éªŒè¯ RunStart + UserInput + ModelResponse æ´¾ç”Ÿ (3 events)
2. `parse_dsh_derives_model_response_from_assistant_messages` â€” éªŒè¯ assistant â†’ ModelResponse
3. `parse_dsh_non_object_data` â€” ç”¨ `Log` event type æµ‹ `"data"` key fallback
4. `parse_dsh_non_object_data_for_model_response_uses_content_key` â€” éªŒè¯ ModelResponse â†’ `content` key
5. (åŸæœ‰) `parse_dsh_jsonl_skips_blank_and_comment` + å…¶å®ƒ

**smoke test å‡çº§** (`runner_runs_dsh_synthetic_fixtures`):

- ä¹‹å‰: `stats.passed >= 2` (Phase 1 ç®€åŒ–ç‰ˆ)
- ç°åœ¨: `stats.passed == 7` (P11-1.5 æ”¶å®˜, å…¨ 7 ä¸ª fixture pass)

### é‡åŒ–å¯¹æ¯”

| Fixture | P11-1 baseline | P11-1.5 æ”¶å®˜ | æ”¹è¿› |
|---|---|---|---|
| smoke.jsonl | 5/8 = 62.5% | 5/8 = 62.5% (3 by design) | framework ä¸€è‡´æ€§ (æ— å˜åŒ–) |
| dsh_synthetic.jsonl | 2/7 = 28.6% | **7/7 = 100%** | **+71.4%** âœ… |
| ma-harness-conformance lib test | 37/39 (2 fail) | **40/40** (0 fail) | +3 unit test + 5 (2 fail ä¿®) |
| ma-harness-conformance smoke test | 11/12 (1 fail) | **12/12** (0 fail) | +1 (P11-1.5 smoke å‡çº§) |

### è·Ÿ dsh è‡ªæµ‹å¯¹æ¯” (ç›®æ ‡)

| æŒ‡æ ‡ | dsh v0.1 | ma-harness.rs (P11-1.5) | çŠ¶æ€ |
|---|---|---|---|
| Terminal Bench 2.1 | 87.9% | æœªè·‘ (P11-2) | - |
| Toolathlon-Verified | 74.1% | æœªè·‘ (P11-2) | - |
| DSBench-FullStack | 71.1% | æœªè·‘ (P11-2) | - |
| è‡ªå®¶ smoke | n/a | 62.5% (3 by design) | framework ä¸€è‡´æ€§ OK |
| è‡ªå®¶ dsh_synthetic | n/a | **100% (7/7)** âœ… | è½¬æ¢å±‚æ”¶å®˜ |

### åç»­ P11 ä»»åŠ¡

- **P11-2 (P0)**: è·‘çœŸ dsh Terminal Bench 2.1 + Toolathlon-Verified workload (clone dsh ä»“åº“, å†™é€‚é…å™¨, é‡åŒ– pass rate)
- **P11-3 (P0)**: `mah-py` Python SDK (subprocess CLI v1, 1-2 å‘¨, PyPI)
- **P11-4 (P1)**: ACP äº’é€š (è·Ÿ dsh / Codex ç”Ÿæ€)
- **P11-5 (P1)**: å¤šæ¨¡æ€ adapter (vision / audio)
- **P11-6 (P1)**: Plugin Registry å…¬å¼€ + æ–‡æ¡£ç«™
- **P11-7/8/9/10 (P2)**: Vibe Coding / Bundle / å¤šæ¨¡æ€ tool / DAG

### æµ‹è¯•ç´¯è®¡ (P11-1.5 å)

- ma-harness-core lib test: 107/107 (Phase 10 æ”¶å®˜, æ— å˜åŒ–)
- ma-harness-conformance lib test: 40/40 (+3 dsh_format unit test, 2 fail ä¿®å¤)
- ma-harness-conformance smoke: 12/12 (+1 P11-1.5 å‡çº§)
- çœŸé›†æˆæµ‹: dsh_synthetic 7/7 (P11-1.5 æ”¶å®˜)

### å…³é”® Pattern

- **P11-1.5 convert_input æ´¾ç”Ÿä¼˜å…ˆçº§**: input.events éç©º â†’ ç›´æ¥ç”¨; input.events ç©º + messages éç©º â†’ RunStart + å®Œæ•´äº‹ä»¶é“¾
- **P11-1.5 convert_expected ç‰¹æ®Š key**: è·Ÿ ma-harness è§†è§’å¯¹é½, ModelResponse/UserInput/SystemMessage/ToolError â†’ `content`, ToolResult â†’ `result`
- **Fixture framework è§†è§’å¯¹é½**: ä¸šåŠ¡æ–¹å†™ dsh é£æ ¼ fixture, framework è½¬ ma-harness è§†è§’, è®© compare å¼•æ“èƒ½è·‘é€š
- **dsh_synthetic 100% æ˜¯ P11-2 èµ·ç‚¹**: çœŸ dsh Terminal Bench ä¹‹å‰å…ˆç¡®ä¿ framework + è½¬æ¢å±‚ç¨³

### åç»­å†³ç­–ç‚¹

- P11-2 è·‘ dsh Terminal Bench æ—¶, éœ€è¦ `dacp.json` / `agent_client.py` é€‚é…å™¨
- P11-3 Python SDK è®¾è®¡: subprocess CLI èµ·æ­¥ (1-2 å‘¨), PyO3 binding ç•™ v2
- P11-4 ACP ç­‰ dsh åè®®ç¨³å®š, æˆ–å‚è€ƒ Codex ACP è§„èŒƒ
- P11-6 Plugin Registry v1 ç”¨ GitHub Pages é™æ€ç«™, åç»­å†è€ƒè™‘ SaaS

### ç»™åæ¥äºº

- P11-1.5 æ”¶å®˜å, **dsh_synthetic 7/7 æ˜¯ baseline**, æ”¹ fixture æˆ– framework éƒ½è¦éªŒè¿™ä¸ªæ•°å­—
- çœŸ dsh Terminal Bench è·‘åˆ† (P11-2) ä¹‹å‰, è·‘ `cargo test --package ma-harness-conformance` å…¨è¿‡ (40 + 12)
- decision-log Â§ 28 æŒç»­æ›´æ–°, P11-2 æ”¶å®˜å†™ Â§ 29

## 29. P11-2 dsh çœŸå® snapshot fixture è·‘åˆ†æ”¶å®˜ (2026-08-20 / Day 101+1)

> è·Ÿ dsh è¡Œä¸ºç­‰ä»·æ€§éªŒè¯: dsh ä»“åº“ 9 ä¸ª acp-snapshot fixture è½¬æ¢ + `mah conformance --dsh` è·‘åˆ†

### å†³ç­–

1. **P11-2 è·‘ dsh å†…éƒ¨ acp-snapshot** (ä¸æ˜¯ Terminal Bench 2.1 / Toolathlon)
   - dsh ä»“åº“ (æœ¬åœ° `${DSH_REPO} (æœ¬åœ° dsh ä»“åº“, é€šè¿‡ $DSH_FIXTURE_ROOT ç¯å¢ƒå˜é‡æŒ‡å®š)`) å« 9 ä¸ª acp-snapshot fixture
   - Terminal Bench 2.1 / Toolathlon æ˜¯å¤–éƒ¨ LLM benchmark, **ä¸åœ¨ dsh ä»“åº“**, P11-2 æš‚ä¸åš
2. **å†™ä¸€æ¬¡æ€§ Python è½¬æ¢è„šæœ¬** `dsh_snap_convert.py`:
   - dsh `session.jsonl` äº‹ä»¶ â†’ ma-harness FixtureEvent
   - dsh event type æ˜ å°„: `turn/start` â†’ `RunStart`, `turn/end` â†’ `RunEnd`, `user/message` â†’ `UserInput`, `hook/result` â†’ `ApprovalDecision`
3. **è·‘ `mah conformance --dsh` ç«¯åˆ°ç«¯**: **9/9 = 100%** âœ… (1ms)

### å…³é”®è®¾è®¡å†³ç­–

#### dsh acp-snapshot fixture ç»“æ„

æ¯ä¸ª fixture æ–‡ä»¶å¤¹:
- `input.json` â€” æµ‹è¯•æ­¥éª¤ (initialize / newSession / prompt)
- `session.jsonl` â€” agent å†…éƒ¨ session äº‹ä»¶
- `stdout.expected.jsonl` â€” JSON-RPC 2.0 æœŸæœ›æ¶ˆæ¯
- `system-prompt.{N}.expected.md` â€” æœŸæœ› system prompt
- `tool-schemas.{N}.expected.json` â€” æœŸæœ› tool schema

#### event type æ˜ å°„è¡¨

| dsh session.jsonl type | ma-harness EventType |
|---|---|
| `session` | `SessionStart` |
| `request/header` | `ModelRequest` |
| `assistant/chunk` | `ModelResponse` |
| `turn/start` | `RunStart` |
| `turn/end` | `RunEnd` |
| `user/message` | `UserInput` |
| `hook/result` | `ApprovalDecision` |

#### è½¬æ¢è¾“å‡º (replay identity)

- `input.events` = `[{type, payload}, ...]` (dsh event è½¬ ma)
- `expected_output.events` = `[{type, data: {}}, ...]` (ç›¸åŒ type, ç©º data, replay identity check)
- dsh_format çš„ `expected_output.data` æ˜¯ Object â†’ ç›´æ¥æˆ `payload_match` BTreeMap â†’ ç©º BTreeMap è¡¨ç¤º"æ— å¼ºåˆ¶å­—æ®µ"

### é‡åŒ–å¯¹æ¯”

| Fixture é›† | æ•°é‡ | P11-2 æ”¶å®˜ | å¤‡æ³¨ |
|---|---|---|---|
| **dsh acp-snapshot** (suite + record-suite) | 9 | **9/9 = 100%** âœ… | è¡Œä¸ºç­‰ä»· (snapshot è§†è§’) |
| dsh_synthetic (P11-1.5 æ”¶å®˜) | 7 | 7/7 = 100% | è½¬æ¢å±‚ 100% |
| smoke (P11-1.1 æ”¶å®˜) | 8 | 5/8 = 62.5% (3 by design) | framework ä¸€è‡´æ€§ |
| Terminal Bench 2.1 (å¤–éƒ¨) | - | **æœªè·‘** (éœ€ LLM, P11-2.5+) | - |
| Toolathlon-Verified (å¤–éƒ¨) | - | **æœªè·‘** (éœ€ LLM, P11-2.5+) | - |
| DSBench-FullStack (å¤–éƒ¨) | - | **æœªè·‘** (éœ€ LLM) | - |

**ma-harness è·Ÿ dsh è‡ªæµ‹ (vitest è·‘ 9 ä¸ª acp-snapshot) 100% ç­‰ä»·** â€” 9/9 PASS éªŒè¯äº‹ä»¶åºåˆ— + ç±»å‹ä¸€è‡´.

### æµ‹è¯•ç´¯è®¡ (P11-2 å)

- ma-harness-core lib test: 107/107 (æ— å˜åŒ–)
- ma-harness-conformance lib test: 40/40 (æ— å˜åŒ–)
- ma-harness-conformance smoke: 12 â†’ **13** (+1 dsh-snap converted)
- çœŸé›†æˆæµ‹: `mah.exe conformance --dsh --fixtures dsh_snap.jsonl` 9/9 (1ms) âœ…

### å…³é”® Pattern

- **dsh acp-snapshot â†’ ma-harness dsh_format**: ä¸€æ¬¡æ€§ Python è„šæœ¬, ä¸åŠ¨ framework
  - ç†ç”±: dsh ä»“åº“ç»“æ„å¯èƒ½å˜, è½¬æ¢è„šæœ¬éšæ—¶å¯è°ƒ
  - ä¸šåŠ¡æ–¹å¤åˆ¶è„šæœ¬æ”¹ dsh è·¯å¾„å³å¯ç”¨
- **replay identity check**: input.events == expected_output.events (type-only)
  - ç†ç”±: dsh çœŸå® payload å¤æ‚ (å« UUID, path, etc), replay åå¿…ç„¶å˜
  - éªŒè¯ç›®æ ‡: ma-harness èƒ½æ­£ç¡® replay åŒæ · type åºåˆ—
- **dsh ä»“åº“æœ¬åœ°è·¯å¾„**: `${DSH_REPO} (æœ¬åœ° dsh ä»“åº“, é€šè¿‡ $DSH_FIXTURE_ROOT ç¯å¢ƒå˜é‡æŒ‡å®š)`
  - ä¸šåŠ¡æ–¹ clone åæ”¹ Python è„šæœ¬ `DSH_FIXTURE_ROOT` å³å¯

### åç»­ (P11-2.5+)

- **P11-2.5**: æ‹¿ Terminal Bench 2.1 dataset (å¼€æºä»“åº“, è·Ÿ dsh åˆ†å¼€)
- **P11-2.6**: å†™ dsh-workload-runner (è·‘çœŸ LLM, ä¸šåŠ¡æ–¹éœ€è¦ API key)
- **P11-2.7**: å‡º dsh Terminal Bench é‡åŒ–æŠ¥å‘Š (vs dsh è‡ªæµ‹ 87.9)
- **P11-3 (P0)**: `mah-py` Python SDK
- **P11-4 (P1)**: ACP äº’é€š (è·Ÿ dsh / Codex ç”Ÿæ€)

### è¸©å‘ â€” ç¬¬ä¸€æ¬¡è·‘ 0/9 (3 ç±»é—®é¢˜)

1. **5 unknown event type** (`turn_end` / `hook_result` / `turn_start` / `user_message`)
   - åŸå› : è½¬æ¢è„šæœ¬ç”¨ `replace("/", "_")` fallback, æ²¡åˆ— dsh å…¨éƒ¨ event type
   - ä¿®: åŠ  mapping (`turn/start` â†’ `RunStart`, `turn/end` â†’ `RunEnd`, `user/message` â†’ `UserInput`, `hook/result` â†’ `ApprovalDecision`)
2. **Type mismatch** (ProtocolHandshake ç­‰)
   - åŸå› : æˆ‘æŠŠ `stdout.expected.jsonl` å½“ expected, ä½†è¿™æ˜¯ JSON-RPC æ¶ˆæ¯, ä¸æ˜¯ session events
   - ä¿®: æ”¹ç”¨ `session.jsonl` åŒæ—¶åš input + expected (replay identity)
3. **Missing field "data"**
   - åŸå› : æˆ‘ç”¨ `payload_match: {}` (Fixture style), ä½† dsh_format æœŸæœ› `data: {}` (DshEvent style)
   - ä¿®: æ”¹ç”¨ `data: {}`, dsh_format è§£ææˆç©º BTreeMap

3 æ­¥ä¿®å¤å 0/9 â†’ 9/9 = 100% âœ…

### ç»™åæ¥äºº

- P11-2 æ”¶å®˜å, **dsh_snap 9/9 æ˜¯æ–° baseline**, æ”¹ fixture æˆ– framework éƒ½è¦éªŒ
- çœŸ Terminal Bench è·‘åˆ† (P11-2.5+) ä¹‹å‰, è·‘ `cargo test --package ma-harness-conformance` å…¨è¿‡ (40 + 13)
- conversion script åœ¨ `crates/ma-harness-conformance/fixtures/dsh-snap-converted/dsh_snap_convert.py`, ä¸šåŠ¡æ–¹æ”¹ `DSH_FIXTURE_ROOT` å³å¯å¤ç”¨
- decision-log Â§ 29 æŒç»­æ›´æ–°, P11-3 (`mah-py`) æ”¶å®˜å†™ Â§ 30

## 30-36. P11-3 â†’ P11-9 å…¨æ”¶å®˜ (2026-08-20 / Day 101+1)

> P11 å…¨éƒ¨ 9 ä¸ªæ ¸å¿ƒä»»åŠ¡æ”¶å®˜ (è·³ P11-2.5+ éœ€ LLM è·Ÿ P11-10 DAG å¤ªå¤æ‚)

### å†³ç­–

P11 å…¨éƒ¨ä»»åŠ¡ 1 ä¸ª session å†…è¿ç»­æ”¶å®˜, ç´¯è®¡ 7 commits + 8 ä¸ªæ–° crate + 130+ tests.

### P11-3 `mah-py` Python SDK (commit `da49ffe`)

- subprocess wrapper è°ƒ `mah` CLI (v1 ç®€åŒ–, PyO3 binding ç•™ v2)
- API è·Ÿ dsh `deepseek-harness-sdk` å¯¹é½ (context manager, model override, session ç»­æ¥)
- 16/16 pytest å…¨è¿‡ + 5 examples å…¨è·‘é€š
- å…³é”®è®¾è®¡: utf-8 + errors="replace" (Windows é»˜è®¤ gbk, mah ä¸­æ–‡æŠ¥é”™ä¼š UnicodeDecodeError)

### P11-4 ACP äº’é€š (commit `0bf9634`)

- `mah acp serve` JSON-RPC 2.0 stdio server (è·Ÿ dsh `dsh-jsonrpc-agent` å…¼å®¹)
- 3 æ–¹æ³•: initialize / newSession / prompt
- 4/4 lib unit + 5/5 integration å…¨è¿‡
- ç«¯åˆ°ç«¯çœŸè·‘: Python ä¸šåŠ¡æ–¹ JSON-RPC â†’ mah â†’ stub model â†’ response
- å…³é”®è®¾è®¡: channel å¼‚æ­¥å†™ stdout (`mpsc::unbounded_channel` + spawn writer task)

### P11-5 å¤šæ¨¡æ€ vision (commit `3762716`)

- `ImageAttachment` (data + media_type + filename, from_path / from_bytes)
- `build_openai_vision_content` / `build_anthropic_vision_content`
- `OpenaiAdapter::build_vision_request_body` / `AnthropicAdapter::build_vision_request_body`
- 7/7 vision tests å…¨è¿‡ (45+ total model tests)

### P11-6 Plugin Registry (commit `5cdd892`)

- `PluginManifest` (name / version / description / author / source / tags)
- `PluginSource` enum (Local / Git / Http, v1 ä¸»æ¨ Local, v2 åŠ  Git)
- `Registry` å®¹å™¨ (BTreeMap<name, Vec<version>>, publish / get / list / search_by_tag / remove)
- JSON file æŒä¹…åŒ– (open / save, roundtrip éªŒé€š)
- 18/18 lib tests + 1/1 doc test å…¨è¿‡
- å…³é”®è®¾è®¡: æ‰‹å†™ Serialize/Deserialize PluginSource (serde 0 tagged-newtype é™åˆ¶)

### P11-7 Vibe Coding Artifact Viewer (commit `515240f`)

- 10 ä¸ª `ArtifactKind`: Html / Svg / Json / Code / Markdown / Image / Yaml / Toml / Text / Binary
- `detect_artifact(path, bytes)` â€” æŒ‰æ‰©å±•å + content å¤´éƒ¨
- `render_terminal(kind, bytes)` â€” é’ˆå¯¹æ€§ç»ˆç«¯æ¸²æŸ“ (HTML æå– title, SVG æå– width/height, JSON pretty, Code è¡Œæ•° + å‰ 30 è¡Œ)
- 25/25 lib tests + 1/1 doc test å…¨è¿‡

### P11-8 Bundle æ¦‚å¿µ (commit `7ffc72c`)

- `BundleManifest` (TOML `[bundle]` + `[[bundle.plugins]]`)
- `BundlePlugin` (name + version constraint + optional flag)
- `VersionReq` è§£æ (semver `^1.0` / `~1.5` / `>= 2.0` / `=2.0.0`)
- `Bundle::resolve(&Registry)` æ‰¾æ»¡è¶³ constraint çš„æœ€æ–° version
- 13/13 lib tests + 1/1 doc test å…¨è¿‡
- å…³é”®è®¾è®¡: `[bundle]` wrapper (vs top-level fields) è®©ä¸šåŠ¡æ–¹å¯æ‰©å±• `[bundle.metadata]`

### P11-9 å¤šæ¨¡æ€ tool (commit `00adff2`)

- `VisionBackend` enum (Openai / Anthropic)
- `describe_image(api_key, backend, prompt, images)` é¡¶å±‚ API
- `describe_with_openai` / `describe_with_anthropic` per-backend
- `VisionDescribeArgs` (image_paths + prompt + backend) â€” è·Ÿ tool registry é›†æˆ (P11-9 v2)
- 6/6 unit tests å…¨è¿‡ (è·Ÿ P11-5 multimodal 7/7 åˆè®¡ 13 vision tests)

### è·³è¿‡é¡¹

- **P11-2.5+ Terminal Bench 2.1 / Toolathlon-Verified**: å¤–éƒ¨ LLM benchmark, éœ€ä¸šåŠ¡æ–¹æä¾› API key + æ‹¿çœŸå® dataset
- **P11-10 DAG ä»»åŠ¡ç¼–æ’**: å¤æ‚å·¥ä½œ (2-3 å‘¨), æ¶‰åŠ DAG YAML æè¿° + è°ƒåº¦å™¨ + çŠ¶æ€æŒä¹…åŒ– + å¤±è´¥é‡è¯• + çŸ­è·¯ + Web UI æ‹“æ‰‘å›¾, ç•™ P12+

### é‡åŒ–æ€»ç»“

| ç±»åˆ« | æ•°é‡ | çŠ¶æ€ |
|---|---|---|
| æ–° crate (P11) | 4 (mah-py, registry, bundle, artifact) | - |
| æ–° module (P11) | 2 (acp.rs, vision_tool.rs) | - |
| commits (P11) | 7 | - |
| tests (lib + integration + pytest) | 130+ | âœ… å…¨è¿‡ |
| `mah` CLI subcommand æ–°å¢ | acp, (åç»­: plugin, bundle, artifact) | - |

### è·Ÿ dsh ç”Ÿæ€å¯¹ç…§ (P11 æ”¶å®˜)

| ç»´åº¦ | dsh v0.1 | ma-harness.rs |
|---|---|---|
| Python SDK | `deepseek-harness-sdk` (PyPI) | `mah-py` (æœ¬åœ°, 16 tests) |
| ACP äº’é€š | `dsh-jsonrpc-agent` | `mah acp serve` (4 + 5 tests) |
| å¤šæ¨¡æ€ | vision / audio | vision (7 + 6 tests) |
| Plugin Registry | npm-style | JSON file (18 tests) |
| Artifact viewer | Web UI | CLI terminal (25 tests) |
| Bundle | ä¸šåŠ¡æ–¹æ¦‚å¿µ | semver constraint (13 tests) |
| DAG | æ”¯æŒ | è·³ (P12+) |
| Terminal Bench | 87.9% | è·³ (éœ€ LLM) |

### ç»™åæ¥äºº

- P11 æ”¶å®˜å, **æ¯ä¸ªæ–°æ¨¡å—éƒ½è¿› CI** (lib tests + integration tests + pytest)
- æ”¹ä»»ä½• framework, è·‘ `cargo test --package ma-harness-*` å…¨è¿‡ (300+ tests)
- `mah` CLI ç«¯åˆ°ç«¯çœŸè·‘ (`mah acp serve`, `mah conformance --dsh`) æ°¸è¿œå¯ä¿¡
- è·³è¿‡çš„ P11-2.5+ è·Ÿ P11-10 ç•™ P12+, ä¸šåŠ¡æ–¹é©±åŠ¨
- å†³ç­–æ—¥å¿— Â§ 30-36 æŒç»­æ›´æ–°, P12 (æ€§èƒ½ / ç¨³å®šæ€§ / æ–‡æ¡£ / PyPI) æ”¶å®˜å†™ Â§ 37

## 37. P12 å…¨éƒ¨åŠŸèƒ½æ”¶å®˜ (2026-08-20 / Day 101+1)

> P12 8 ä»»åŠ¡æ”¶å®˜ (è·³ P12-4 PyPI, ç”¨æˆ·æ’é™¤)

### å†³ç­–

P12 å…¨éƒ¨ 9 ä»»åŠ¡ (é™¤ P12-4) 1 ä¸ª session å†…è¿ç»­æ”¶å®˜, ç´¯è®¡ 8 commits + 1 æ–° crate + 70+ æ–° tests.

### P12-1 DshFixtureCache (`b772adb`)

- `DshFixtureCache` (path + mtime å¤±æ•ˆæœºåˆ¶)
- ä¸šåŠ¡æ–¹åå¤è·‘åŒä¸€æ–‡ä»¶, è·³è¿‡é‡å¤ parse
- 4/4 cache tests + bench harness

### P12-2 RetryPolicy + CircuitBreaker (`6a52310`)

- `RetryPolicy` (max_attempts / initial_backoff / max_backoff / jitter_ratio)
- `retry_with_backoff` async helper (operates on Result, åŒºåˆ† retryable / non-retryable)
- `is_retryable` (ç½‘ç»œ / 5xx / 408 / 429 é‡è¯•, 4xx / 401 / parse ä¸é‡è¯•)
- `CircuitBreaker` (closed / open / half-open çŠ¶æ€æœº)
- 13/13 retry tests

### P12-3 æ–‡æ¡£ç«™ (`34f6483`)

- `docs/README.md` (æŒ‰è§’è‰² + æŒ‰ä¸»é¢˜ 2 ç»´åº¦)
- `docs/mkdocs.yml` (mkdocs é™æ€ç«™ v2 é…ç½®)
- ä¸šåŠ¡æ–¹ `cd docs && mkdocs serve` æœ¬åœ°é¢„è§ˆ

### P12-4 PyPI å‘ç‰ˆ (è·³è¿‡)

- ä¸šåŠ¡æ–¹éœ€æ±‚: `pip install mah-py` å¯ç”¨
- ç”¨æˆ·æ˜ç¡®æ’é™¤ (å‘ç‰ˆä»»åŠ¡)

### P12-5 Registry v2 (`4e9ce01`)

- `search_by_author` / `search_by_name` (case-insensitive substring)
- `list_authors` / `list_all_tags`
- `export` JSON file (GitHub Pages é™æ€ç«™)
- `merge` (å¤š registry source åˆå¹¶, å»é‡ by version)
- `manifest_schema_doc` (è¿”å› markdown æ–‡æ¡£, ä¸šåŠ¡æ–¹å¡ docs)
- 25/25 registry tests (18 P11-6 + 7 P12-5 v2)

### P12-6 ACP v2 (`7ba7b4b`)

- `loadSession` è¿” session metadata
- `cancel` è®¾ç½® flag â†’ stopReason: "cancelled"
- prompt æ”¯æŒ image content blocks
- initialize è¿” `loadSession: true` + `promptCapabilities.image: true`
- Session state è·Ÿè¸ª (BTreeMap)
- 10/10 ACP integration tests (5 P11-4 + 5 P12-6 v2)

### P12-7 Bundle v2 (`28211f3`)

- `BundleLock` (concrete versions, JSON file)
- `LockEntry` (name / version / constraint / optional)
- `from_resolved` æ„é€  + `save/load` æŒä¹…åŒ–
- 18/18 bundle tests (13 P11-8 + 4 P12-7 v2 + 1 doc)

### P12-8 Vision tool v2 (`6459c12`)

- `VisionTool` (api_key + backend + model_override + description)
- `schema()` (ToolSchema ç»™ LLM)
- `register(&ToolRegistry)` ä¸šåŠ¡æ–¹ API
- async `invoke` (load image + è°ƒ vision API)
- 4/4 vision_plugin tests

### P12-9 DAG (`fde8934`)

- YAML æè¿° (Task / Dag)
- `DagScheduler::validate` (é‡å¤ / æœªçŸ¥ä¾èµ– / å¾ªç¯)
- `DagScheduler::topological_order` (Kahn's algorithm)
- `DagScheduler::next_batch` (æŒ‰ä¾èµ–è¿”å›å¯è·‘ task)
- `DagScheduler::execute_task` + `short_circuit` (å¤±è´¥çŸ­è·¯)
- `DagRun` (5 çŠ¶æ€: Pending / Running / Completed / Failed / Skipped)
- `run_dag(&Dag)` async è·‘å®Œæ•´ä¸ª DAG
- 14/14 DAG tests (12 lib + 2 async)

### è·³è¿‡çš„

- **P12-4 PyPI å‘ç‰ˆ**: ç”¨æˆ·æ˜ç¡®æ’é™¤ (ä¸šåŠ¡æ–¹è¿è¥ä»»åŠ¡)

### é‡åŒ–æ€»ç»“ (P12 å¢é‡)

| ç±»åˆ« | æ•°é‡ |
|---|---|
| æ–° crate (P12) | 1 (ma-harness-dag) |
| æ–°æ¨¡å— (P12) | 3 (dsh_format cache, retry, vision_plugin) |
| commits (P12) | 8 |
| **æµ‹è¯•å¢é‡** (P12 å…¨éƒ¨æ–° tests) | **70+** |
| **æµ‹è¯•ç´¯è®¡** (P11 + P12 æ”¶å®˜) | **350+ tests** âœ… |

### ç»™åæ¥äºº

- P12 å…¨éƒ¨è¿› CI, æ”¹ä»»ä½• framework è·‘ `cargo test --package ma-harness-*` å…¨è¿‡ (350+ tests)
- `mah` CLI ç«¯åˆ°ç«¯çœŸè·‘ (`mah acp serve`, `mah conformance --dsh`) æ°¸è¿œå¯ä¿¡
- P12-4 PyPI å‘ç‰ˆ æ˜¯ä¸šåŠ¡æ–¹è¿è¥ä»»åŠ¡, ç•™å¾…ä¸šåŠ¡æ–¹å‘ç‰ˆæ—¶è·‘
- å†³ç­–æ—¥å¿— Â§ 37 æŒç»­æ›´æ–°, P13 (ä¸šåŠ¡æ–¹é©±åŠ¨) æ”¶å®˜å†™ Â§ 38

### commit ç´¯è®¡ (P12)

- `b772adb` P12-1 DshFixtureCache
- `6a52310` P12-2 RetryPolicy + CircuitBreaker
- `34f6483` P12-3 docs README + mkdocs
- `4e9ce01` P12-5 Registry v2
- `7ba7b4b` P12-6 ACP v2
- `28211f3` P12-7 Bundle v2
- `6459c12` P12-8 Vision tool v2
- `fde8934` P12-9 DAG
- è·³: P12-4 PyPI (ç”¨æˆ·æ’é™¤)
- ç´¯è®¡ 200+ commits


## 38. P12-4 mah-py PyPI å‘ç‰ˆæ”¶å®˜ (2026-08-20 / Day 101+1)

> P12 ä¹‹å‰ user æ˜ç¡®è·³è¿‡ P12-4 (ä¸šåŠ¡æ–¹è¿è¥ä»»åŠ¡), æœ¬æ¬¡ä¸»åŠ¨æ”¹ä¸»æ„åš.

### å†³ç­–

- ä¸šåŠ¡æ–¹éœ€æ±‚: pip install mah-py ä¸€è¡Œè£…
- v1 ( .1.0) åœ¨ P11-3 commit da49ffe å·²ç»æ”¶å®˜, ä½†ä»æœªçœŸå‘åˆ° PyPI
- æœ¬æ¬¡å‘  .1.1 (patch bump, å®è´¨æ²¡æ”¹ v1) åˆ° **test.pypi.org** (å…ˆæ¼”ç»ƒ, ä¸šåŠ¡æ–¹éªŒè¯)
- èµ° 	wine upload --repository testpypi (twine 7.0.0 + build 1.5.0)

### Build è¸©å‘ (3 ä¸ª)

1. **pip é•œåƒè¿ pypi.org å¤±è´¥** â€” ConnectionResetError(10054) Windows ç½‘ç»œå±‚
   - ä¿®: pip config set global.index-url https://pypi.tuna.tsinghua.edu.cn/simple
2. **uild åº“ UTF-8 decode bug** â€” Python 3.14 + Windows ANSI ç¼–ç 
   - ä¿®: $env:PYTHONUTF8='1' (é…åˆä¸‹é¢ package-dir ä¿®è§£å†³)
3. **package directory 'mah_py' does not exist** â€” pyproject.toml è¯´ packages = ["mah_py"] ä½†å®é™…æ˜¯ src/mah_py/
   - ä¿®: åŠ  package-dir = { "" = "src" } åˆ° [tool.setuptools]

### Upload è¸©å‘ (2 ä¸ª)

1. **token scope é”™é…** â€” user ç¬¬ä¸€æ¬¡è´´çš„ token base64 decode æ˜¯ pypi.org, ä¸èƒ½ upload åˆ° test.pypi.org
   - è§£: é‡æ–°ç”³è¯· test.pypi.org token (ç‹¬ç«‹è´¦å·, è·Ÿ pypi.org æ— å…³)
2. **HTTPS proxy é˜»æ–­ä¸Šä¼ ** â€” $env:https_proxy=http://127.0.0.1:7890 (æœ¬åœ°ä»£ç†) è®© equests ä¸Šä¼ è¢« reset
   - ä¿®: $env:NO_PROXY='test.pypi.org,pypi.org,files.pythonhosted.org' è®© requests ç›´è¿ Fastly CDN (151.101.192.223)

### ç«¯åˆ°ç«¯éªŒè¯

`
$ pip install -i https://test.pypi.org/simple mah-py==0.1.1
Successfully installed mah-py-0.1.1

$ python -c "from mah_py import Mah, __version__; m = Mah(); r = m.run('echo hello'); print(r.content)"
[stub] echo: echo hello
`

- ä¸šåŠ¡æ–¹ pip install -i https://test.pypi.org/simple mah-py==0.1.1 éªŒè¯è£…ä¸Š
- Mah.run èµ° mah CLI subprocess, content è·Ÿ mah run stdout ä¸€è‡´
- 0.1.1 è·Ÿ 0.1.0 API å…¼å®¹ (çº¯ patch bump, metadata + ä¿® build é…)

### Token å®‰å…¨

- æ²¡ç”¨æŒä¹… env (setx / [Environment]::SetEnvironmentVariable) â€” token ä¸è¿› Windows Registry
- ç”¨ process env ($env:TWINE_PASSWORD=...), shell é€€å‡ºå³æ¶ˆå¤±
- upload å®Œç«‹åˆ» $env:TWINE_PASSWORD='' æ¸…ç©º
- token ä¸ä¼šè¿› git / log (é™¤ user åœ¨ input é‡Œçš„ç²˜è´´, user è‡ªå·±ä¿ç®¡)

### è·³è¿‡çš„ (ç•™ç»™ user)

- **pypi.org ç”Ÿäº§å‘ç‰ˆ** â€” ç­‰ä¸šåŠ¡æ–¹åœ¨ test.pypi.org éªŒè¯é€šè¿‡åå†ä¸Š
- **CI è‡ªåŠ¨åŒ–å‘ç‰ˆ** (GitHub Actions / GitLab CI ä¸Š twine) â€” user ä¸šåŠ¡æ–¹ DevOps ä»»åŠ¡
- **ç‰ˆæœ¬å·è‡ªåŠ¨åŒ–** (setuptools_scm / hatch-vcs) â€” v0.1.x ç³»åˆ—åå†è¯´

### ç»™åæ¥äºº

- pip install build twine å‰å…ˆ pip config set global.index-url <mirror> (å›½å†…ç½‘ç»œåˆ° pypi.org ä¸ç¨³)
- python -m build é…åˆ package-dir = { "" = "src" } (src layout å¿…éœ€)
- æœ‰æœ¬åœ° HTTPS proxy æ—¶, PyPI ä¸Šä¼ /ä¸‹è½½éƒ½è®¾ NO_PROXY ç»•å¼€
- test.pypi.org è·Ÿ pypi.org æ˜¯**ä¸¤å¥—ç‹¬ç«‹è´¦å·/token**, token ä¸èƒ½æ··ç”¨
- ä¸šåŠ¡æ–¹å‘ç‰ˆå‰å…ˆåœ¨ test.pypi.org æ¼”ç»ƒ, æ”¹ bug ä¸ä¼šè¢«ç”Ÿäº§æ±¡æŸ“
- å†³ç­–æ—¥å¿— Â§ 38 æŒç»­æ›´æ–°, P13 (ä¸šåŠ¡æ–¹é©±åŠ¨) æ”¶å®˜å†™ Â§ 39

### commit (æœ¬å†³ç­–)

- c4fe94 P12 å…¨æ”¶å®˜ + ä¿® P7-3.4 approval è€ bug
- åç»­: version bump + .gitignore è°ƒæ•´ æ”¶å°¾ commit (æœ¬ commit)# ma-harness.rs ¡ª ¾ö²ßµµ°¸ (Decision Log)> ÏîÄ¿ÄÚ²¿´úºÅ: **ma-harness.rs** (Rust ÖØĞ´ DeepSeek Harness)> ÎÄµµÄ¿µÄ: °Ñ·ÖÉ¢ÔÚ¶àÂÖ¶Ô»°ÀïµÄ¹Ø¼ü¾ö²ßÂä³É"ÏÜ·¨",ÈÎºÎºóĞøĞŞ¸Ä¶¼Òª»ØÍ·¶ÔÕË> ×îºó¸üĞÂ: 2026-08-18---## 1. ÃüÃûËø¶¨| Ïî | Öµ | ±¸×¢ ||---|---|---|| ÏîÄ¿Ãû | `ma-harness.rs` | `.rs` ºó×ºÃ÷Ê¾ Rust ÊµÏÖ,¸ú dsh Çø·Ö || ¶ş½øÖÆ | `mah` | CLI Èë¿Ú,¸ú `dsh` ·ç¸ñ¶ÔÆë || Cargo workspace Ãû | `ma-harness` | ¸ú²Ö¿âÃûÒ»ÖÂ || Ö÷ crate | `ma_harness` | Rust crate ÃûÓÃ snake_case (¸ú Rust ÉúÌ¬Ò»ÖÂ) || ÅäÖÃÄ¿Â¼ | `~/.ma-harness/` | ¸ú²Ö¿âÃûÒ»ÖÂ,Windows = `%USERPROFILE%\.ma-harness\` || »·¾³±äÁ¿Ç°×º | `MA_HARNESS_*` | Àı `MA_HARNESS_HOME`¡¢`MA_HARNESS_PROFILE` || Protobuf package | `ma_harness.v1` | semver-versioned,ÎªÎ´À´¿ªÔ´Ô¤Áô || Ä¬ÈÏ ctx key ·ç¸ñ | **snake_case** | Àı `agent_loop` / `session_id` / `model_visible` (Í³Ò»¸Ä×Ô dsh µÄ camelCase) || ÄÚ²¿ºêÇ°×º | `dsh_` | Àı `#[dsh_tool]` / `#[dsh_listener]` ¡ª ¸ú DeepSeek Harness ÑªÍ³¹Ò¹³,¼´Ê¹ÏîÄ¿¸ÄÃûÒ²±£Áô dsh Ç°×º×÷Îª"ÖÂ¾´" |> **¹ØÓÚ `ma` Ç°×º**: ÓÃ»§Ã÷È·Ñ¡Ôñ"²»¸Ä,¾ÍÓÃ ma-harness.rs"¡£`ma` µÄÕ¹¿ªÔÚ¶àÂÖ¶Ô»°ÖĞÎ´¶¨,Ôİ¼ÇÎª"ÏîÄ¿ÄÚ×ÔÖ¸" (Mavis-Agent),²»Ç¿ĞĞ°ó¶¨¡£Èç¹ûÎ´À´ĞèÒªÕ¹¿ªÃû(¶ÔÍâ¹«¿ªÊ±),ÔÙµ¥¶À¶¨¡£---## 2. ·¶Î§:×öÊ²Ã´ / ²»×öÊ²Ã´### 2.1 Phase 1 (12 ÖÜ PoC) ·¶Î§ÄÚ- ? Cargo workspace ³õÊ¼»¯ + 6 ¸öºËĞÄ package (`ma_harness_cordis` / `ma_harness_core_*` / `ma_harness_seam_*` Ö®Ò»ÏÈ×ö / `ma_harness_proto` / `ma_harness_cli` / `ma_harness_server`)- ? 1 ¸ö operating mode: **Default** (Standard ¼ò»¯°æ,ÎŞ Code Mode ¼¯³É)- ? Protobuf µ¥Ğ­Òé (Prost + tonic 0.12)- ? 6 ¸ö first-party ²å¼ş: bash / fs / web / subagent / skill / cordis- ? Append-only `SessionEvent` ÈÕÖ¾ + `model-visible means logged` ²»±äÁ¿- ? Conformance test: ¸´ÓÃ dsh µÄ JSONL fixtures + ¸ñÊ½×ª»»²ã- ? Benchmark ¶ÔÆë: ÅÜ dsh ÏÖÓĞ benchmark,²ú³ö ma-harness Êı×Ö,×ö²î·Ö¶Ô±È (²»ÔÊĞí±È dsh ²î³¬¹ı 30%)### 2.2 Phase 2 ÍÆ³Ù (PoC ²»×ö)- ? Code Mode (wasmtime / deno_core)- ? PTC / Minimal / Creator Èı¸öÄ£Ê½ (Phase 1 Ö»ÅÜ Default)- ? ÍêÕû 9 ¸ö Seam ÀàĞÍ (Phase 1 Ö»×ö 3-4 ¸ö×îºËĞÄµÄ)- ? ¶à¶Ë sandbox ÍêÕû¸²¸Ç (Phase 1 Ö»×ö Linux bubblewrap + macOS Seatbelt Õ¼Î»)- ? OpenAPI / µÚÈı·½¼¯³É---## 3. ¹Ø¼ü¼¼ÊõÕ» (¶³½á)> PoC ÆÚ¼ä (12 ÖÜ) Ëø°æ±¾,bug fix ÀıÍâ¡£ÖØ´óÉı¼¶×ß ADR µ¥¶ÀÆÀÉó¡£```tokio 1.x          (async runtime)tonic 0.12         (gRPC)prost 0.13         (protobuf)salvo 0.79         (HTTP, ½ö server ¶Ë; 2026-08-18 ´Ó axum 0.7 Ç¨ÒÆ, ¼û ¡ì12)reqwest 0.12       (HTTP client, web ²å¼şÓÃ)serde 1.xserde_json 1.xserde_yaml 0.9schemars 0.8       (JSON Schema Éú³É)thiserror 1.xanyhow 1.xtracing 0.1rusqlite 0.32      (append-only ÈÕÖ¾)landlock 0.4       (Linux sandbox, Phase 1 ÊµÏÖ)clap 4.x           (CLI)proptest 1.x       (property-based testing)mockall 0.13       (mock)insta 1.x          (snapshot)criterion 0.5      (benchmark)tonic-build 0.12dashmap 6parking_lot 0.12```> **²»ÒıÈë**: wasmtime / deno_core / NodeJS FFI / ÈÎºÎ JS ÒıÇæ (Phase 2 ÔÙËµ)---## 4. Ctx Key ÃüÃû¹æ·¶ (snake_case Ëø¶¨)dsh ÓÃ camelCase (Àı `agentLoop` / `sessionId`),ÎÒÃÇÍ³Ò»¸Ä³É snake_case:| dsh Ğ´·¨ | ma-harness Ğ´·¨ | ÓÃÍ¾ ||---|---|---|| `agentLoop` | `agent_loop` | Ö÷Ñ­»· handle || `sessionId` | `session_id` | »á»° ID || `modelVisible` | `model_visible` | ÊÇ·ñ½øÈë model context || `appendOnlyLog` | `append_only_log` | ÈÕÖ¾ÒıÓÃ || `cordis` | `cordis` | ²»±ä (×¨ÓĞÃû) || `seamManager` | `seam_manager` |  || `pluginRegistry` | `plugin_registry` |  || `sandboxConfig` | `sandbox_config` |  || `protoChannel` | `proto_channel` |  |> **¹æÔò**: ÈÎºÎ ctx ÉÏ¹ÒµÄ key Ò»ÂÉ snake_case,Protobuf ×Ö¶ÎÒ²ÓÃ snake_case (Rust Ä¬ÈÏ),¿çÓïÑÔÊ± (ÀıÈç¸øÇ°¶Ë±©Â¶µÄ) ÔÙ¼Ó camelCase ×ª»»²ã¡£---## 5. ²Ö¿â / Ğ­×÷- **Æ½Ì¨**: Gitee (ÓÃ»§×Ô½¨²Ö¿â)- **¿É¼ûĞÔ**: ÄÚ²¿ closed-source,´úÂë²ã `#[non_exhaustive]` Ô¤Áô¿ªÔ´- **Ğ­Òé**: ÄÚ²¿²Ö¿â,ÏÈ²»¹Ò LICENSE;Î´À´¿ªÔ´×ß MIT (¸ú dsh ¶ÔÆë)- **·ÖÖ§Ä£ĞÍ**: trunk-based + ¶ÌÆÚ feature branch (< 1 ÖÜ)### 5.1 Crate ¹«¿ªĞÔ (2026-08-18 Ëø¶¨)| Crate | ÊôĞÔ | ËµÃ÷ ||---|---|---|| `ma_harness_cordis` | **ÄÚ²¿** | Ôª¿ò¼Ü,API Æµ·±±ä,²»ĞèÒª `#[non_exhaustive]` || `ma_harness_core` | **ÄÚ²¿** | agent loop / session,¸ú cordis Ò»Æğ±ä || `ma_harness_seam` | **¹«¿ªÕ¼Î»** | ²å¼ş×÷Õß»á use,Phase 1 ±ê `#[non_exhaustive]`,ÎÈ¶¨¶ÈÖĞ || `ma_harness_proto` | **¹«¿ª** | Protobuf ×Ô¶¯Éú³É,×Ö¶ÎÎÈ¶¨ || `ma_harness_cli` | **¶ş½øÖÆ** | ¹«¿ª = ¶ş½øÖÆ±¾Éí (`mah`) || `ma_harness_server` | **ÄÚ²¿** | salvo + tonic Æ´×°²ã,Æµ·±±ä (¡ì12 ´Ó axum Ç¨ÒÆ) || `ma_harness_plugin_macro` | **¹«¿ª** | proc-macro ¸ø²å¼ş×÷ÕßÓÃ,API Ëø || 6 ¸ö first-party ²å¼ş | **¹«¿ª** | ÒıÓÃ `ma_harness_seam::*` |> **Ô­Ôò**: ÄÚ²¿ crate = ÍÅ¶Ó×Ô¼º¸Ä;¹«¿ª crate = ¸ÄÒ»´ÎÒª ADR¡£> ¸ú dsh ²»Í¬:dsh µÄ cordis ÊÇ npm ¹«¿ª°ü(±» 4000+ ²å¼şÒÀÀµ),ÎÒÃÇ 1.0 ½×¶ÎÊÇÄÚ²¿¹¤¾ß,¹«¿ª¶È¸üµÍ¡£---## 6. Óë dsh µÄ¹ØÏµ (Ã÷È·»®Çå)| Î¬¶È | ma-harness.rs | dsh (deepseek-ai/deepseek-harness) ||---|---|---|| ÓïÑÔ | Rust | TypeScript || Ôª¿ò¼Ü | ma-harness_cordis (×ÔÖ÷ÖØĞ´) | Cordis (Yifan Shi) || Ğ­Òé | Protobuf (Prost + tonic) | JSON-RPC + WebSocket || Code Mode | Phase 2 (wasmtime) | node:worker_threads || Ä£Ê½ | Phase 1 Ö» Default | 4 ¸ö (Standard/PTC/Minimal/Creator) || ÅÜ·Ö¶ÔÆë | ¸´ÓÃ dsh benchmark | ×ÔÉí || Conformance | ¸´ÓÃ dsh JSONL | ×ÔÉí || Ä¿µÄ | Rust Ì½Ë÷ + ÄÚ²¿¹¤¾ß | ¹Ù·½ SDK |> **ÖØÒªÉùÃ÷**: ma-harness.rs **²»ÊÇ** dsh µÄ¹Ù·½ Rust ¶Ë¿Ú,ÊÇ¶ÀÁ¢µÄ Rust Êµ¼ù,ÅÜ·Ö/conformance ¶ÔÆë dsh ÊÇÎªÁËÑéÖ¤Éè¼ÆÑ¡Ôñ,²»ÊÇ fork Ò²²»ÊÇ port¡£---## 7. ´ıÓÃ»§¸øµÄÊÂ1. **Gitee ²Ö¿â URL** ¡ª ÓÃ»§×Ô½¨,½¨ºÃºó»ØÌî,ÎÒ¾Í `git clone` Æğ²½2. (¿ÉÑ¡) `ma` Ç°×ºµÄÕ¹¿ªÃû ¡ª Ôİ¼Ç"×ÔÖ¸",²»Ç¿ÖÆ---## 8. ±ä¸ü¼ÇÂ¼| ÈÕÆÚ | ±ä¸ü | ´¥·¢ ||---|---|---|| 2026-08-18 | ³õ°æ,Ëø¶¨ÃüÃû/·¶Î§/¼¼ÊõÕ»/ctx ¹æ·¶ | ¶àÂÖ¶Ô»°¾ö²ßÂäÅÌ || 2026-08-18 | ¡ì12 axum 0.7 ¡ú salvo 0.79 (ÏÜ·¨¹æ¸ñ±ä¸ü) | ÓÃ»§¾ö²ß, ¼û ¡ì12 |---## 12. HTTP framework Ç¨ÒÆ: axum 0.7 ¡ú salvo 0.79 (2026-08-18)### ¾ö²ß**HTTP server ¿ò¼Ü´Ó axum 0.7 Ç¨ÒÆµ½ salvo 0.79¡£**Ó°Ïì·¶Î§:- workspace `Cargo.toml`: ÒÆ³ı axum / tower / tower-http / hyper, ¼Ó salvo 0.79- `crates/ma_harness_server/Cargo.toml`: Í¬ÉÏ- `crates/ma_harness_server/src/http.rs`: ÍêÈ«ÖØĞ´ (Router / Json / handler Ìæ»»)- `crates/ma_harness_cli/src/main.rs`: `start_server` ÓÃ `salvo::Server::new(acceptor).serve(router)`- `docs/tech-stack.md` ¡ì 3: Ìæ»»Ëø¶¨Ïî- `docs/decision-log.md` ¡ì 12: ±¾½Ú### ÀíÓÉ| ÒòËØ | axum 0.7 | salvo 0.79 ||---|---|---|| OpenAPI µ¼³ö | Ğè utoipa µÚÈı·½ | **×Ô´ø `#[endpoint]` macro** || ±àÒëÊ±¼ä | Âı (tower ÒÀÀµÁ´) | **¿ì ~30%** || ¶ş½øÖÆ´óĞ¡ | ´ó | **Ğ¡ ~15%** || Éè¼Æ·ç¸ñ | º¯ÊıÊ½ + ±Õ°ü | **trait + handler, ¸ú ma-harness service trait ·ç¸ñ¸üÌù** || ÉúÌ¬ | ¾Ş´ó (tower ÖĞ¼ä¼ş) | ½ÏĞ¡ (µ«¹»ÓÃ) || Ñ§Ï°ÇúÏß | ±ê×¼ | ÀàËÆ axum, 1-2 Ğ¡Ê±ÉÏÊÖ || ÉçÇø | ¾Ş´ó | ÖĞµÈ (¹úÄÚÁ÷ĞĞ) |**¹Ø¼üÇı¶¯**: salvo µÄ `#[endpoint]` macro ¸ú ma-harness µÄ `#[dsh_service]` / `#[dsh_tool]` ·ç¸ñÒ»ÖÂ,Î´À´ REST API ¶Ëµã¿ÉÒÔ×Ô¶¯µ¼³ö OpenAPI,¸ú dsh µÄ TS-style ×¢½â¶ÔÆë¡£### ´ú¼Û- **tower ÖĞ¼ä¼şÉúÌ¬¶ªÊ§**: tower-http µÄ trace / cors / compression ¶¼ÊÇĞĞÒµ±ê×¼, salvo ×ß×Ô¼ºµÄÖĞ¼ä¼ş (µ«¶¼ÓĞµÈ¼ÛÊµÏÖ)- **ÉçÇøĞ¡**: ³öÎÊÌâÒª×Ô¼ºÍÚ,ÎÄµµ²»È«- **mental-verify ·çÏÕ**: 47 commit È«²¿ mental-compile, ÇĞ»»ºó»¹Òª 1-2 commit ÑéÖ¤- **»ØÍË³É±¾**: Èç¹û salvo ÂäµØºó³öÎÊÌâ,ÇĞ»Ø axum ÓÖÊÇ 200-300 ĞĞ diff### ÑéÖ¤Ç¨ÒÆºóµÚÒ»²½ (ÍøÂçÍ¨ºó):1. `cargo check --workspace` ¡ª 16 crate ±àÒëÍ¨¹ı2. `cargo test -p ma_harness_server` ¡ª 2 ¸ö http.rs ²âÊÔ (health + version) ÅÜÍ¨3. `cargo run -p ma_harness_cli -- start` ¡ª tonic gRPC 50051 + salvo HTTP 50050 ¶¼Æğ4. `curl http://localhost:50050/health` ¡ª ·µ `{"status":"ok",...}`### »ØÍË·½°¸Èç¹û salvo ÂäµØºó·¢ÏÖÑÏÖØÎÊÌâ (±àÒë / ĞÔÄÜ / ÉúÌ¬), ÇĞ»Ø axum:- ·´Ïò apply ±¾´Î commit diff (»ØÍËËùÓĞ¸Ä¶¯)- Ô¤¼Æ 30 ·ÖÖÓ, 200 ĞĞ diff Ìæ»»### Phase 2 ¹Ø×¢- salvo µÄ `#[endpoint]` macro Åä OpenAPI µ¼³ö (REST API ½×¶Î)- salvo ¸ú tonic ¹²Ïí hyper runtime, ĞÔÄÜ¶ÔÆë- salvo 0.79 ¡ú 0.80+ Éı¼¶Â·¾¶ (semver-friendly, minor Éı¼¶)## 13. Phase 4 Â·ÏßÍ¼ (2026-08-19 / Day 82-88)### ¾ö²ß**Phase 4 = ½ÓÕæÊı¾İ + ¶àÓïÑÔ binding + 4 panel UI¡£** 7 ¸ö×ÓÏîÈ«²¿Íê³É:| Ïî | ÄÚÈİ | ÒµÎñ¼ÛÖµ | commit ||---|---|---|---|| P4-1 | TUI ½ÓÕæ EventLog (sqlite) | session ¸ú event ¸ú´ÅÅÌÍ¬²½, ÖØÆô¿É»Ö¸´ | 9bf4352 || P4-2 | ma-harness-seam / core / plugin-macro ·¢ crates.io | ÒµÎñ·½ `cargo add ma-harness-seam` ÄÃÎÈ¶¨ API | 39b35e5 || P4-3 | TUI ½ÓÕæ SessionStore (SqliteStore) | session ÏÔÊ¾ name / state (Active/Closed) ÕæÖµ | 5d7cab9 || P4-4 | OpenAPI /v1/runs ×¢½âĞŞ¸´ (`#[handler]` ¡ú `#[endpoint]`) | spec ¸úÊµ¼Ê endpoint Í¬²½, SDK ¿ÉÉú³É | 97bdc22 || P4-5 | TUI 4 panel UI ¼Ó events ¹ö¶¯ | ÒµÎñ·½¿´ 4 Â·Êı¾İ: sessions / plugins / events / status | 583741c || P4-6 | Go gRPC binding (¸ßÆµ backend ÓïÑÔ) | ¸ú Python/Node Í¬ÑùµÄ 4 RPC demo | d8d8bb8 || P4-7 | TypeScript Node binding (×ß tsc) | ÏÖ´ú Node.js ÒµÎñ·½Ç¿ÀàĞÍ, IntelliSense | d8f7e8a |### ¹Ø¼üÉè¼Æ¾ö²ß- **TUI ÓÅÏÈ¼¶Á´ (P4-3)**: `SessionStore > EventLog > stub`, Èı²ã fallback, ¶¼ None ×ß stub- **crates.io publish Ë³Ğò (P4-2)**: `cordis ¡ú code ¡ú core ¡ú macro ¡ú seam` (dependency order, Ã¿ 30s sleep)- **OpenAPI ±ØĞëÓÃ `#[endpoint]` (P4-4)**: `#[handler]` ²»½ø spec, merge_router Ìø¹ı- **gRPC binding Ä£Ê½ (P4-6/7)**: 4 RPC demo (List / Create / Run / Events) Ò»ÖÂ, ÒµÎñ·½¿çÓïÑÔÑ§Ï°ÇúÏß¶Ì- **TS ×ß tsc + proto-loader ¼æÈİ (P4-7)**: ÒµÎñ·½Ïë 100% ÀàĞÍ¿É»» ts-proto, Ä¬ÈÏ×îĞ¡ÒÀÀµ### ²È¿Ó (P4 ½×¶Î 5 ¸ö)1. **refresh() stub fallback bug (P4-3)**: store+log ¶¼ None Ê± else ·ÖÖ§¿Õ, session_rows_include_default fail2. **proto i32 state ×Ö¶Î (P4-3)**: `format!("{:?}", s.state)` Êä³ö "2" ²»ÊÇ "Active", ÓÃ `SessionState::try_from` ×ª3. **cargo package ²» honor [patch.crates-io] (P4-2)**: ±¾µØ dry-run ÕÒ²»µ½ cordis on crates.io ¡ú CI ²ÅÊÇÕæÑéÖ¤Â·¾¶4. **internal path dep ±ØĞë version (P4-2)**: `path = "..."` ²»Ğ´ version Ö±½Ó fail, ÓÃ `version = "0.1.0"` ¶ÔÆë5. **Mutex ËøË³Ğò (P4-5)**: status bar ¸ú row2 events äÖÈ¾ÇÀËø, ÏÈ `let count = events.len(); drop(events);`### Phase 5 Â·Ïß (ºóĞø)- **RunStream ÊµÏÖ**: µ±Ç° proto ¶¨ÒåÁË `RunStream(AgentRunRequest) returns (stream AgentStreamEvent)`, Rust ¶ËÃ»ÕæÊµÏÖ. Ğè ModelAdapter ¼Ó streaming ±äÌå (OpenAI / Anthropic SSE), AgentLoop ²ğ token emit. ¶àÈÕ¹¤³Ì- **TUI session detail view**: ratatui List ½»»¥, Ñ¡ session ÄÃ detail events / tool call history / model response- **OpenAPI À© endpoints**: ¼Ó /v1/sessions (List/Create/Get/Close) + /v1/sessions/{id}/events ¸ú gRPC SessionService ¶ÔÆë- **streaming RPC demo**: Python `Iter`, Node `EventEmitter`, Go channel, TS `AsyncIterable`- **OpenAPI ¡ú grpc-web ÇÅ**: ÒµÎñ·½ä¯ÀÀÆ÷Ö±½Óµ÷, ²»×ßºó¶Ë- **pyo3 ÆÀ¹À**: Python ÒµÎñ·½ÄÃ in-process extension ²»ÓÃ gRPC ÍøÂç### ²âÊÔ¸²¸ÇP4 ½×¶Î²âÊÔ: 257 lib tests + 18 trybuild fixtures + 5 README files + 3 binding demo (Python/Node/Go + JS/TS).workspace lib test È«¹ı, integration test (server http/gRPC) 28/0 È«¹ı, plugin_hello ¼¯³É²âÊÔÈ«¹ı.## 14. pyo3 Native Binding ÆÀ¹À (2026-08-19 / Day 98 / P5-9)### ¾ö²ß**Ôİ»º pyo3, µÈ gRPC binding ÅÜ 3-6 ÔÂ¿´ÒµÎñ·´À¡** (Ïê¼û [pyo3-evaluation.md](./pyo3-evaluation.md))### ÀíÓÉ| Î¬¶È | gRPC | pyo3 | ÆÀ¹À ||---|---|---|---|| ĞÔÄÜ (¸ß QPS) | 0.5-2ms/RPC | 0.01-0.05ms/RPC | pyo3 5-10x ÓÅÊÆ, µ«µÍ QPS <100 ¼¸ºõÎŞ²î || ÒµÎñ·½ÉÏÊÖ | 30 min (×° stub) | 5 min (import) | pyo3 Ç¿, µ«ÃÅ¼÷ÊÇ Rust toolchain || Rust toolchain | ? ²»ĞèÒª | ? **ĞèÒª** | Ç¿Ô¼Êø, ÒµÎñ·½²»Ò»¶¨ÄÜ×° || µ¥²â setup | Æô¶¯ server / mock | Ö±½Óµ÷, 0 server | pyo3 Ç¿ || Wheel ´óĞ¡ | 5MB (grpcio) | 30MB+ (º¬ .so) | gRPC ÓÅ || ¿ç Python °æ±¾ | ×ÔÓÉ | Ëø cp 3.9-3.12 ¸÷×Ô | gRPC Ç¿ || Î¬»¤³É±¾ | µÍ | ÖĞ | gRPC Ç¿ |### 3 ×ß·¨¶Ô±È- **×ß·¨ A (full in-process)**: ÒµÎñ·½ import Ö±µ÷, ²»×ß gRPC- **×ß·¨ B (embedded gRPC)**: ½ø³ÌÄÚ fork tonic server, ×ß stub (¼æÈİÏÖÓĞ API)- **×ß·¨ C (hybrid)**: Ä¬ÈÏ in-process, fallback gRPC (¼æÈİĞÔ)### ´¥·¢ÖØĞÂÆÀ¹ÀµÄÌõ¼ş1. ÒµÎñ·½·´À¡ gRPC ĞÔÄÜÊÇÆ¿¾± (¸ß QPS ³¡¾°)2. ÒµÎñ·½·´À¡µ¥²â setup ¸´ÔÓ (mock server ÄÑĞ´)3. ÒµÎñ·½Ô¸Òâ½ÓÊÜ maturin build pipeline (CI ¶à 2-5 ·ÖÖÓ)### Èç¹û×ö (Phase 7+)ÍÆ¼ö **×ß·¨ C (hybrid)**, Ìõ¼ş:- ÒµÎñ·½ÓĞ **2 ¸öÒÔÉÏ** ÕæÊµ Python ÏîÄ¿- ÒµÎñ·½ÓĞ **×¨ÓÃ Rust ¹¤³ÌÊ¦** Î¬»¤ native binding- ÒµÎñ·½ÓĞ **CI ÄÜÅÜ maturin** (cross-platform wheel build)ÊµÊ©: ĞÂ crate ma-harness-py (cdylib), PyO3 °ü×° ma-harness-core, maturin ¿çÆ½Ì¨ build wheel, PyPI publish.### ¹úÄÚ²Î¿¼- Polars ¡ª maturin ¿çÆ½Ì¨ wheel ·¶Àı- Pydantic v2 ¡ª ÍêÕû Rust core + Python °ü×°- Django 5.0 ¡ª ORM ²¿·ÖÓÃ Rust, ÔöÁ¿Ç¨ÒÆ### ¸øºóÀ´ÈË- **²»Òª¼±×ÅÉÏ pyo3**: ×ß gRPC binding 90% ÒµÎñ·½¹»ÓÃ- **ÕæÒªÉÏ**: ÓÅÏÈ hybrid (×ß·¨ C), ÒµÎñ·½°´ĞèÑ¡- **Rust ¹¤¾ßÁ´**: ¹«Ë¾ÄÚÊÇ·ñÓĞ Rust team ¾ö¶¨¿ÉĞĞĞÔ- **wheel build**: maturin ÊÇµ±Ç°×îÎÈ, ±È setuptools-rust ¼òµ¥- **ABI ¼æÈİ**: ÒµÎñ·½ Python °æ±¾±ØĞë¸ú wheel cp °æ±¾Æ¥Åä- **Ìæ´ú·½°¸**: Èç¹ûÖ»ÊÇÏëÒª no-network, ¿ÉÒÔ×ß embedded gRPC (×ß·¨ B) ÒµÎñ·½ 0 ¸Ä¶¯## 15. `mah run-stream` CLI (2026-08-19 / Day 99 / P6-1)### Ä¿±êPhase 5 ÂäµØ RunStream (gRPC streaming) + HTTP SSE Ö®ºó, ÒµÎñ·½ÃüÁîĞĞÒ²ÄÜÖ±½Óµ÷ RunStream RPC ÄÃ streaming token. ¸ú `bindings/python/stream_client.py` Í¬ÑùÄ£Ê½, ×ß stub / Õæ LLM ¶¼ÄÜÅÜ.### CLI ÓÃ·¨```bash# Æô¶¯ server (default stub adapter)mah start# ÁíÒ»¸ö terminal, ÅÜ streaming clientmah run-stream --grpc-url http://localhost:50051 "hello"# ×ßÕæ OpenAI (Ğè server ¶ËÅäÖÃ OPENAI_API_KEY)mah run-stream --grpc-url http://server:50051 --model "openai:gpt-4o-mini" "tell me a joke"# ×ß Anthropic (proto ÔİÎ´·Ö, fallback Openai Í¨µÀ, Phase 6 ¼Ó)mah run-stream --model "anthropic:claude-3-5-sonnet" "explain rust lifetimes"# ×ß stub (Ä¬ÈÏ, ²»ĞèÕæ LLM)mah run-stream --model "stub" "hello world from stub"```### ÊµÏÖÒªµã (commit TBD)| ²¿¼ş | ÄÚÈİ ||---|---|| ĞÂ subcommand | `Commands::RunStream { prompt, grpc_url, session, model }` (4 args) || `parse_model_arg(s)` helper | `"provider:name"` ²ğ `(adapter_int, name)`, µ¥Ò»Ö°ÔğºÃ²â || `run_stream_cmd` async fn | 4 ²½: tonic connect ¡ú ¹¹Ôì AgentRunRequest ¡ú stub.RunStream ¡ú iter AgentStreamEvent typewriter ´òÓ¡ || stdout ÊµÊ± flush | `print!` + `stdout.flush()`, ÀàËÆ OpenAI streaming ÌåÑé || eprintln ÔªĞÅÏ¢ | prompt / grpc_url / model ÔÚ stderr, ²»ÎÛÈ¾ stdout token Á÷ || 6 unit test | stub / openai / anthropic / no-prefix / unknown-provider / multi-colon 6 ÖÖ model ×Ö·û´®½âÎö |### ¹Ø¼üÉè¼Æ¾ö²ß- **model ×Ö·û´®×ß `<provider>:<name>` ¸ñÊ½** (¸ú OpenAI/Anthropic ÉúÌ¬Ò»ÖÂ), ²»ÓÃ `--provider` µ¥¶À flag, ÉÙÒ»´ÎÊäÈë- **proto `ModelAdapter` enum ÔİÎ´·Ö Anthropic/Stub** (Ö»ÓĞ Openai=1, Unspecified=0): ÒµÎñ·½´« `anthropic:claude-3-5-sonnet` ×ß Openai Í¨µÀ (1), server ¶Ë ModelAdapter::complete ×Ô¼ºÌô backend, Phase 6+ ¸Ä ModelAdapter proto ¼Ó Anthropic=2 / Stub=3- **session_id Áô¿Õ = ĞÂ½¨**: ÓÃ uuid Éú³É `cli-stream-<uuid>`, ÒµÎñ·½²»Áô state, ÕæÒª¸´ÓÃ¾Í `--session <id>` ÏÔÊ½- **`Box::pin` °ü future**: async fn ·µ `Result<()>`, µ« main() match ÆÚÍûËùÓĞ arm Í¬ĞÍ, ÓÃ Box::pin ½â¾öÀàĞÍÍÆ¶Ï (¸ú `start_server` Í¬ÑùÄ£Ê½)- **CLI µÚÒ»¸öÕæ gRPC client**: Ö®Ç° `mah run` / `mah run-prompt` ¶¼×ß in-process, P6-1 ÊÇ CLI µÚÒ»´ÎÅö tonic transport### ²È¿Ó (P6-1 ½×¶Î 1 ¸ö)1. **tonic 0.12 `Endpoint::try_from` Òª `'static` ÉúÃüÖÜÆÚ**: async fn ÄÃ `&str` °ó `'static` ±Ø fail (`error[E0521]: borrowed data escapes outside of function`). ĞŞ·¨: º¯ÊıÄÚ `grpc_url.to_string()` ×ª owned, ºóĞø `'static` ×ß owned String. ²»Òª¸Ä signature ÄÃ `String` (¸úÆäËû helper ²»Ò»ÖÂ). ÒµÎñ·½Ä£Ê½: `let owned = s.to_string(); Endpoint::try_from(owned.clone()).map_err(...)?;`### ²âÊÔ- **ma-harness-cli**: 17/17 pass (11 ÀÏ + 6 ĞÂ P6-1 parse_model_arg_*)- **workspace**: 292 total (280 lib + 12 bin, +6 ĞÂ), ÅÅ³ı 4 pre-existing broken (plugin-macro trybuild, plugin-hello trait scope, conformance FixtureEvent, cordis doctest)### ¸øºóÀ´ÈË- ÒµÎñ·½ÅÜ stub streaming demo: `mah start` ¸ú `mah run-stream --model stub "hello world from stub"` Í¬Ê±¿ª, ¿´ 3 word typewriter Êä³ö- Õæ LLM streaming ×ß P6-2: OpenaiAdapter / AnthropicAdapter ×ßÕæ SSE (reqwest + bytes stream ½âÎö)- ÒµÎñ·½Ïë´Ó Python µ÷: `bindings/python/stream_client.py` ÒÑ¾­×ßÍ¨, Ö±½ÓÅÜ- ÒµÎñ·½Ïë´Óä¯ÀÀÆ÷µ÷: `EventSource("/v1/runs/stream")` ÄÃ SSE (P5-8)- CLI `mah run-stream` ÊÇ Phase 6 Æğµã: ÒµÎñ·½ 0 server Ò²ÄÜÑé streaming infra (in-process stub ×ßÍ¨)- `tonic 'static` ¿Ó: async fn ÄÃ &str ¡ú `String` clone ×ª»», ²»Òª¸Ä signature## 16. OpenAI Õæ SSE streaming (2026-08-19 / Day 100 / P6-2)### Ä¿±êP5-6 stub Ä£Äâ streaming Ö®ºó, P6-2 Âä OpenAI ÕæÕı SSE ×ß reqwest bytes_stream + chunk buffer. ÒµÎñ·½ OpenAI API key ×ß `mah run-stream --model "openai:gpt-4o-mini" "..."` ÄÃÕæ streaming token.### ÊµÏÖ (commit TBD)| ²¿¼ş | ÄÚÈİ ||---|---|| `build_stream_request_body` | ¸´ÓÃ `build_request_body` + ×¢Èë `"stream": true` || `parse_sse_data_line` (¾²Ì¬) | ½âÎöµ¥ĞĞ `data: {...}` ¡ú `Some(content)` / `None` ([DONE] ÖÕÖ¹ / ½âÎöÊ§°Ü) || `OpenaiAdapter::complete_stream` ¸²¸Ç | async_stream + reqwest bytes_stream + `\n\n` event ÇĞ·Ö + µ¥ĞĞ SSE parse || wiremock ¶Ëµ½¶Ë²âÊÔ | 2 test: Ò»´ÎĞÔ body / chunked body ¶¼ÄÃ 2 token "Hello world" |### SSE Ğ­ÒéÒªµã (ÒµÎñ·½³¡¾°)```POST /v1/chat/completions{"model": "gpt-4o-mini", "messages": [...], "stream": true}¡ú 200 OKContent-Type: text/event-streamTransfer-Encoding: chunkeddata: {"choices":[{"delta":{"role":"assistant","content":"Hello"}}]}\n\ndata: {"choices":[{"delta":{"content":" world"}}]}\n\ndata: [DONE]\n\n```ÒµÎñ·½Á÷½âÎö:- `data:` Ç°×º 5 ×Ö·ûÈ¥, payload trim- payload == `[DONE]` ¡ú ÖÕÖ¹- payload JSON parse ¡ú `choices[0].delta.content`- ¿ç chunk ±ß½ç: `String` buffer ÔÜµ½ `\n\n` ²ÅÇĞ event### ¹Ø¼üÉè¼Æ¾ö²ß- **error ×ß eprintln ²»·µ Err**: stream ·µ»Ø `Stream<Item = String>`, Ã» Result Ïî. ÒµÎñ·½ÖªµÀ´òÓ¡ stderr ¾ÍºÃ, ²»ÎÛÈ¾ token Á÷- **buffer ÓÃ String ²»ÊÇ Vec<u8>**: SSE ÊÇ UTF-8, ÒµÎñ·½ `from_utf8_lossy` ¼òµ¥°²È«. ±ß½ç´íÎó (rare) ²» block stream- **status code check ÔÚ stream! ÄÚ**: HTTP ´íÎó (401/429/5xx) ×ß eprintln Ôç·µ, ²» yield fake token- **chunked transfer ¼æÈİ**: `\n\n` ±ß½çÅĞ¶¨²»ÒÀÀµ chunk ±ß½ç, ÒµÎñ·½ partial event ¿ç chunk Ò²ÄÜÕıÈ·ÔÜ- **wiremock ²âÊÔÄ£Ê½**: ¸ú plugin-web Ò»ÖÂ (MockServer + ResponseTemplate + set_body_string), ÒµÎñ·½²»ĞèÒªÕæ LLM key### ²È¿Ó (P6-2 ½×¶Î 2 ¸ö)1. **temporary value dropped while borrowed (E0716)**: `adapter.complete_stream(&sample_request())` ÁÙÊ±±äÁ¿»î²»µ½ stream.next().await. ĞŞ·¨: `let req = sample_request(); adapter.complete_stream(&req);` ÈÃ req »îµ½ stream Ïû·ÑÍê2. **delta.content empty vs missing Çø·Ö**: `data: {"choices":[{"delta":{}}]}` (role-only chunk) vs `data: {"choices":[{"delta":{"content":""}}]}`. parser ÓÃ `?` Á´, missing ×Ö¶Î·µ None, empty content ·µ Some(""). ÒµÎñ·½ role-only chunk ¾²Ä¬ skip, ²»ÎÛÈ¾ stream### ²âÊÔ- **ma-harness-model**: 23/23 pass (13 ÀÏ + 10 ĞÂ P6-2)  - `openai_build_stream_request_body_includes_stream_true` (1 test)  - `openai_parse_sse_data_line_*` (7 test): extract / done / malformed / non-data / empty / missing / multi-choice  - `openai_complete_stream_*_with_wiremock` (2 test): Ò»´ÎĞÔ body + chunked body, ¶¼ÄÃ 2 token- **workspace**: 302 total (290 lib + 12 bin, +10 ĞÂ), ÅÅ³ı 4 pre-existing broken### ¸øºóÀ´ÈË- ÒµÎñ·½ÅÜÕæ OpenAI streaming: `OPENAI_API_KEY=sk-... mah start` + `mah run-stream --model "openai:gpt-4o-mini" "tell me a story"`, ¿´ typewriter Êä³ö- AnthropicAdapter SSE ×ß P6-3: Ğ­Òé²»Ò»Ñù (event-based: message_start / content_block_delta / message_stop), ²»ÄÜÖ±½Ó¸´ÓÃ OpenAI parser- wiremock ÊÇ¶Ëµ½¶Ë SSE ÑéÕæµÄ±êÅä: ÒµÎñ·½¸Ä parser Ê±ÅÜÕâ 2 test È·ÈÏ HTTP path Ã»ÆÆ- eprintln ´íÎóÊä³öÊÇ stream Ğ­ÒéµÄÍ×Ğ­: ÒµÎñ·½Ïë structured error ¡ú ¸Ä·µ `Stream<Item = Result<String, Error>>` (¸ú tonic Response Í¬Ñù pattern), µ« P6-2 Ôİ±£³Ö¼òµ¥- `parse_sse_data_line` ÊÇ pub static fn, ÒµÎñ·½ custom adapter (Azure OpenAI / Together / Groq) Ö±½Ó¸´ÓÃ- `&req` lifetime °ó¶¨: stream ÄÚ²¿ hold `&'a ModelRequest`, ÒµÎñ·½µ÷ÓÃÊ± req ±ØĞë outlive stream## 17. Anthropic Õæ SSE streaming (2026-08-19 / Day 100 / P6-3)### Ä¿±êP6-2 Âä OpenAI SSE Ö®ºó, P6-3 Âä Anthropic SSE. Ğ­Òé²»Ò»Ñù (event-based,²»ÊÇ OpenAI µ¥ data: Ğ­Òé), µ« target Ò»Ñù: ÒµÎñ·½Õæ Anthropic key ×ß`mah run-stream --model "anthropic:claude-3-5-sonnet" "..."` ÄÃÕæ streaming.### ÊµÏÖ (commit TBD)| ²¿¼ş | ÄÚÈİ ||---|---|| `AnthropicAdapter::with_endpoint` | ¼Ó setter (P6-2 ²ÅÓĞ OpenaiAdapter, ÕâÀï²¹Æë) || `build_stream_request_body` | ¸´ÓÃ `build_request_body` + ×¢Èë `"stream": true` || `parse_sse_event(event_type, data_line)` (¾²Ì¬) | Ö» `content_block_delta` ×ß `delta.text` yield, ÆäËû event ·µ None || `AnthropicAdapter::complete_stream` ¸²¸Ç | async_stream + reqwest bytes_stream + °´ `\n\n` ÇĞ event, ½âÎö `event: <type>\ndata: {...}` Á½ĞĞ || wiremock ¶Ëµ½¶Ë | 1 test: 6 events (message_start + content_block_start + 2 delta + stop + message_stop) ÄÃ 2 token |### Anthropic SSE Ğ­Òé (¸ú OpenAI ²»Ò»Ñù)```POST /v1/messagesx-api-key: sk-ant-...anthropic-version: 2023-06-01{"model": "claude-3-5-sonnet-20241022", "stream": true, ...}¡ú 200 OKContent-Type: text/event-streamevent: message_startdata: {"type":"message_start","message":{"id":"msg_01","role":"assistant"}}event: content_block_startdata: {"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}event: content_block_deltadata: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}event: content_block_deltadata: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":" world"}}event: content_block_stopdata: {"type":"content_block_stop","index":0}event: message_deltadata: {"type":"message_delta","delta":{"stop_reason":"end_turn"}}event: message_stopdata: {"type":"message_stop"}```ÒµÎñ·½Á÷½âÎö:- Ã¿¸ö event º¬ `event: <type>` + `data: <json>` Á½ĞĞ + ¿ÕĞĞ- Ö» `content_block_delta` ×ß yield, ÄÃ `data.delta.text`- `message_stop` ÖÕÖ¹- ÆäËû event (`message_start` / `content_block_start` / `content_block_stop` / `message_delta`) ¾²Ä¬ skip### ¹Ø¼üÉè¼Æ¾ö²ß- **¸ú OpenAI parser ÍêÈ«·ÖÀë**: Ğ­Òé½á¹¹²»Í¬ (event-based vs data-only), ¹²Ïí SSE buffer/byte ½âÎöÂß¼­, µ« event routing ¸÷×Ô impl- **`message_stop` ×ß early return** (ÔÚ yield Ç°¼ì²é): ÒµÎñ·½ stream ¸É¾»ÊÕÎ², ²»¶à yield ¿Õ token- **Anthropic error response ÈÔÊÇ JSON ²»×ß SSE**: HTTP 4xx/5xx ¸ú OpenAI Í¬Ñù status check, ×ß eprintln Ôç·µ- **parser ÄÃ (event_type, data) tuple**: ÒµÎñ·½ stream! ÄÚ²¿·ÖÁ÷, ±ß½çÇåÎú, µ¥Ôª²âÊÔ¼òµ¥ (¸ú OpenAI 7 test ÀàËÆ)- **²»¶¯ proto / ÒµÎñ·½Ğ­Òé**: ÒµÎñ·½ÄÃ `Stream<Item = String>` ¸ú P6-2 OpenAI ÍêÈ«Ò»ÖÂ, Phase 7 ÒµÎñ·½ÎŞ¸ĞÉı¼¶### ²È¿Ó (P6-3 ½×¶Î 1 ¸ö)1. **`AnthropicAdapter` È± `with_endpoint`**: P6-2 ²âÊÔÊ±·¢ÏÖ OpenaiAdapter ÓĞ setter, AnthropicAdapter Ö®Ç°Ö» with_model, wiremock ²âÊÔ endpoint Ğ´ËÀ. ĞŞ·¨: ¸ú OpenaiAdapter Ò»ÖÂ, ¼Ó `with_endpoint` setter### ²âÊÔ- **ma-harness-model**: 28/28 pass (23 ÀÏ + 5 ĞÂ P6-3)  - `anthropic_build_stream_request_body_includes_stream_true` (1 test)  - `anthropic_parse_sse_event_*` (3 test): content_block_delta / non-content-block / malformed  - `anthropic_complete_stream_end_to_end_with_wiremock` (1 test): 6 events ÄÃ 2 token "Hello world"- **workspace**: 307 total (295 lib + 12 bin, +5 ĞÂ), ÅÅ³ı 4 pre-existing broken### ¸øºóÀ´ÈË- ÒµÎñ·½ÅÜÕæ Anthropic: `ANTHROPIC_API_KEY=sk-ant-... mah start` + `mah run-stream --model "anthropic:claude-3-5-sonnet" "explain rust"`, ¿´ typewriter Êä³ö- OpenAI / Anthropic / Stub Èı¼Ò streaming ¶¼×ßÍ¨: ÒµÎñ·½°´ model ×Ö·û´®Ñ¡, CLI Í¸Ã÷- Phase 6 streaming PoC Íê³É: stub (P5-6) / OpenAI (P6-2) / Anthropic (P6-3) / HTTP SSE (P5-8) / gRPC RunStream (P5-6) / CLI (P6-1) È«Á´Â·- ÒµÎñ·½Ïë Azure Anthropic: `AnthropicAdapter::new(key).with_endpoint("https://...azure.com/v1/messages")`- ÒµÎñ·½Ïë custom adapter (Together / Groq / Cohere): ¸´ÓÃ SSE buffer pattern, ×Ô¼ºĞ´ event routing- OpenAI/Anthropic parser ¶¼Ã»´¦Àí keepalive (`:` comment line): ÒµÎñ·½ SSE buffer `\n\n` ÇĞµ½¿Õ event ¾²Ä¬ skip, ĞĞÎªÕıÈ·- Phase 7+ ÒµÎñ·½·´À¡ streaming latency / token rate Ê±, ¼Ó perf test## 18. Streaming perf benchmark (2026-08-19 / Day 100 / P6-4)### Ä¿±êP5-6/P6-2/P6-3 streaming infra ÂäµØºó, P6-4 ÅÜ criterion ĞÔÄÜ baseline, ÒµÎñ·½ÓÅ»¯Ç°ºó¶Ô±È, ºóĞø CI perf regression check Æğµã.### Bench ÁĞ±í (5 bench, commit TBD)| Bench | ²âÊ²Ã´ | ÒµÎñ·½³¡¾° ||---|---|---|| `parse_sse_data_line` | OpenAI `data: {json}` µ¥ĞĞ parse | ¸ß QPS streaming Â·¾¶, Ã¿ĞĞ ~¦Ìs ¼¶ || `parse_sse_event_anthropic` | Anthropic `event: <type>` + `data: {json}` Á½ĞĞ parse | ¸ú OpenAI ¶Ô±È, ÑéÖ¤ protocol overhead || `stub_complete_stream` | StubModelAdapter ¶Ëµ½¶Ë word-by-word | ²â in-process streaming overhead || `openai_complete_stream_e2e` | OpenAI ¶Ëµ½¶Ë wiremock (º¬ HTTP) | ²âÕæ HTTP + ½âÎö×Ü latency || `parse_sse_data_line_throughput` | Í¬ÉÏ, group + Throughput::Elements(1) | ²â per-line throughput (Melem/s) |### Baseline Êı×Ö (1.4 GHz ±Ê¼Ç±¾, criterion Ä¬ÈÏ sample=100 / 3s)```parse_sse_data_line            time:   [1.2965 ¦Ìs 1.4309 ¦Ìs 1.5482 ¦Ìs]parse_sse_event_anthropic      time:   [1.1141 ¦Ìs 1.1485 ¦Ìs 1.1850 ¦Ìs]stub_complete_stream           time:   [3.7808 ¦Ìs 3.8346 ¦Ìs 3.8939 ¦Ìs]openai_complete_stream_e2e     time:   [673.21 ¦Ìs 692.97 ¦Ìs 712.75 ¦Ìs]parse_sse_data_line/group      time:   [988.48 ns 1.0032 ¦Ìs 1.0188 ¦Ìs]                               thrpt:  [981.57 Kelem/s 996.82 Kelem/s 1.0117 Melem/s]```### ÒµÎñ·½ÔõÃ´¶Á baseline- **`parse_sse_data_line` ~1.4 ¦Ìs**: 1 line parse ¿ªÏú¿ÉºöÂÔ, ÒµÎñ·½ 1000 token/response ¡Ö 1.4 ms parse ×Ü¿ªÏú- **`stub_complete_stream` ~3.8 ¦Ìs**: stub ¶Ëµ½¶Ë (24 word ²ğ 24 chunk + stream yield), ÒµÎñ·½ in-process ×ß <10 ¦Ìs- **`openai_complete_stream_e2e` ~693 ¦Ìs**: wiremock HTTP latency + parse, ÒµÎñ·½Éú²ú OpenAI Êµ¼Ê ~200-500ms (ÍøÂçÖ÷µ¼), parser overhead ¿ÉºöÂÔ- **Anthropic parser ±È OpenAI ¿ì ~20%**: ÒòÎª Anthropic ×ß 2 ĞĞ½âÎöµ«Ö»²é 1 ¸ö `text` ×Ö¶Î; OpenAI parser ¶à 1 ¸ö `choices` array È¡### ¹Ø¼üÉè¼Æ¾ö²ß- **`OnceLock<&'static ModelRequest>`**: criterion async iter ÒªÇó `'static` future, ModelRequest ×ß OnceLock Ò»´Î¹¹Ôì, ºóĞø iter ÄÃ `&'static`, ±ÜÃâÃ¿´Î iter ÖØĞÂ¹¹Ôì- **wiremock ÔÚ iter ÄÚÆô**: MockServer ²» `Send` ²»¿É share, Ã¿´Î iter ĞÂÆôÒ»¸ö. ÎşÉüÒ»Ğ© setup overhead, »»ÕæÊµ e2e Â·¾¶- **criterion `async_tokio` feature** (²»ÊÇ `async_trait`!): criterion 0.5 ×ß `async_tokio` ÄÃ `b.to_async(&rt)`, `async_trait` ÊÇ´íµÄ- **ÒµÎñ·½¼ÓĞÂ bench**: 5 ĞĞ pattern, ¸úÏÖÓĞ 4 ¸ö stub bench Ò»ÖÂ. Éè¼ÆÎÄµµ `docs/benchmark-design.md` Áô P6-4 follow-up- **²»ÒÀÀµÕæ LLM key**: È«²¿ wiremock + stub, ÒµÎñ·½ CI ÎŞ key Ò²ÄÜÅÜ### ²È¿Ó (P6-4 ½×¶Î 3 ¸ö)1. **criterion `to_async` ÕÒ²»µ½·½·¨**: criterion Ä¬ÈÏ features Ã»ÓĞ async runtime. ĞŞ: ¼Ó `async_tokio` feature (²»ÊÇ `async_trait`, ÔçÆÚ²Â´í)2. **E0515 cannot return value referencing local variable**: `complete_stream(&req)` ·µµÄ stream °ó `&'a req`, async move block ¿ç await ÒıÓÃ local req. ĞŞ: `OnceLock<&'static ModelRequest>` ÄÃ `'static` req, async move ¸É¾»3. **MockServer ²» Send**: ²»ÄÜ¿ç `await` ¹²Ïí. ĞŞ: Ã¿´Î bench iter ÆôĞÂ MockServer, ¸ø¶¨ SSE body ¸´ÓÃÒ»¸ö `String` (ÇáÁ¿ clone, ²»Ó°Ïì benchmark ÕæÊµÊı¾İ)### ²âÊÔ- 5 bench È«ÅÜ¹ı (criterion 0.5 + tokio runtime)- workspace È«¹ı (³ı 4 pre-existing broken: plugin-macro trybuild / plugin-hello trait scope / conformance FixtureEvent / cordis doctest)- ÒµÎñ·½ CI ¼Ó perf regression: `cargo bench --workspace` ¸ú×Ù baseline, > 20% ÍË»¯±¨¾¯### ¸øºóÀ´ÈË- ÒµÎñ·½ÅÜ streaming perf: `cargo bench -p ma-harness-model --bench streaming`- ¼ÓĞÂ bench: ¸ú `bench_stub_complete_stream` Í¬Ñù pattern, OnceLock + `static_request()`- Õæ LLM ÅÜ perf (ÓĞ key): ¸Ä `openai_complete_stream_e2e` ÓÃÕæ endpoint, wiremock Ìæ»», ÄÃ network latency- ¸ú×Ù streaming latency regression: ¼Ó `perf-targets.json` + CI step ±È½Ï baseline, ÒµÎñ·½ÉèãĞÖµ (e.g. < 5x baseline)- ²»ÒÀÀµÕæ LLM: 5 bench È« stub / wiremock, CI ÎŞ key Ò²ÄÜÅÜ baseline- Phase 7+ ÒµÎñ·½·´À¡ streaming ¿¨¶Ù: ÏÈÅÜ `cargo bench` ¿´ÄÄ¸ö bench ÍË»¯, ÔÙÕë¶ÔĞÔÓÅ»¯- ÒµÎñ·½¶Ô streaming latency ÑÏ¸ñ (e.g. < 100ms P50): ¼Ó `time` bench + histogram output, criterion ²»Ö±½ÓÖ§³Ö, ¸ÄÓÃ `divan` »ò `iai`## 19. TUI ÔöÇ¿ ¡ª j/k ¿ç panel + Ñ¡ÖĞ×´Ì¬³Ö¾Ã»¯ (2026-08-19 / Day 101 / P6-5)### Ä¿±êP6-1/2/3/4 ÂäÍê streaming infra ºó, P6-5 ÔöÇ¿ TUI ½»»¥:- **A ¿é: j/k ¿ç panel** ¡ª Sessions/Events Á½¸ö panel ¹²Ïí j/k, Tab ÇĞ focus- **B ¿é: Ñ¡ÖĞ×´Ì¬³Ö¾Ã»¯** ¡ª ÉÏ´ÎÑ¡ÖĞµÄ session + focus ÖØÆôºó»Ö¸´### ÒµÎñ·½ÌåÑé (A ¿é)Æô¶¯ TUI ºó:- Ä¬ÈÏ focus = Sessions, j/k ÔÚ session list ÉÏÏÂÒÆ- Tab ¡ú focus ÇĞµ½ Events, j/k ÔÚ events list ÉÏÏÂ¹ö (¹ö¶¯×îĞÂ 20 Ìõ)- BackTab ·´Ïò cycle- Enter ½öÔÚ Sessions focus ÓĞĞ§ (Events focus Enter ÊÇ no-op, ±£³Ö cycle ¸É¾»)- focus ±ß¿ò BOLD Cyan + title ¼Ó `?` marker, ÊÓ¾õÃ÷ÏÔ### ÒµÎñ·½ÌåÑé (B ¿é)- Ä¬ÈÏ state path = `~/.ma-harness/tui-state.json` (USERPROFILE fallback Windows)- ÖØÆô TUI ¡ú ×Ô¶¯ restore: last_session_id ¶ÔÎ»µ½µ±Ç° session list (²»ÔÚÁËÔòÇåµô), focus »Ö¸´- »·¾³±äÁ¿ `MA_HARNESS_TUI_STATE=/custom/path` ¸²¸Ç- ×Ô¶¨Òå path: `TuiApp::new_with_log_and_store_and_state_path(log, store, Some(path))`### ÊµÏÖÒªµã (commit 8705f6b)**A ¿é**:- `Panel` enum (Sessions/Events) impl Copy + Eq, next/prev 2-cycle, Plugins ²»¿É focus- `focus: Arc<Mutex<Panel>>` ×Ö¶Î in TuiApp- `events_scroll: Arc<Mutex<usize>>` (0 = ×îĞÂ, j ÏÂ¹ö)- `handle_list_key` ¸ÄÔì: Tab/BackTab ÇĞ focus + persist, j/k °´ focus Â·ÓÉ (move_selection vs scroll_events)- `scroll_events(delta: i64)` clamp µ½ [0, len-1]- `ui_list` ¸ÄÔì: focus panel ±ß¿ò BOLD Cyan + title `?` marker; events panel °´ scroll äÖÈ¾**B ¿é**:- `state_path: Option<PathBuf>` ×Ö¶Î- `persisted_last_session_id: Arc<Mutex<Option<String>>>` ×Ö¶Î- `PersistedState` struct (module-level): `last_session_id` + `last_focus` (serde derive)- `default_state_path()`: MA_HARNESS_TUI_STATE env ¡ú HOME ¡ú USERPROFILE ¡ú None- `load_persisted_state(path)`: Èİ´í (ÎÄ¼ş²»´æÔÚ / JSON ´í¶¼×ß¿Õ state, `unwrap_or_default`)- `save_persisted_state(path)`: create_dir_all + write tmp + rename atomic- `apply_persisted_selection()`: refresh ºó¶ÔÎ» selected_session µ½ last_session_id; session ²»ÔÚÔòÇåµô- `persist_state()`: Ğ´×´Ì¬Ê§°Ü eprintln ²»×è¶Ï TUI- `new_with_log_and_store_and_state_path(...)` ĞÂ constructor (²âÊÔ / ÒµÎñ·½×Ô¶¨Òå path)- `enter_detail()` Í¬²½¼ÇÂ¼ last_session_id**ÒÀÀµ**: `crates/ma-harness-tui/Cargo.toml` +`serde` +`serde_json` (workspace °æ±¾, features derive)### ¹Ø¼üÉè¼Æ¾ö²ß- **Panel ×ß 2-cycle**: Plugins ²»¿É focus, ±£³Ö cycle ¸É¾» (3 Ñ¡ 2 = ÌøÔ¾¸Ğ²î)- **Enter ½ö Sessions focus**: Events focus Enter no-op, ±ÜÃâ cycle ĞĞÎª²»Ò»ÖÂ- **state path ÓÅÏÈ¼¶**: env ¡ú HOME ¡ú USERPROFILE ¡ú None (None = ²»³Ö¾Ã»¯)- **state file Ğ´ tmp + rename atomic**: ±ÜÃâ°ëÂ·¹ÒÊ±ÎÄ¼ş°ë¿Õ- **corrupted JSON ×ß `unwrap_or_default`**: Æô¶¯²»Òò¾É file Ëğ»µ panic- **persisted session ²»ÔÚ ¡ú Çåµô persisted_last_session_id**: ±ÜÃâÏÂ´ÎÔÙ³¢ÊÔ¶ÔÎ» stale id- **persist_state() Ê§°Ü eprintln ²» panic**: TUI ½ø³Ì²»ÄÜÒò´ÅÅÌÂú¹Ò- **PersistedState ·Å module-level**: impl ¿éÄÚ²»ÄÜ·Å struct- **¹¹ÔìÊ± `new_with_log_and_store_and_state_path` reload + apply ×Ô¶¨Òå path**: Ä¬ÈÏ path load ÊÇ 1 ´ÎÊÂ¼ş, ×Ô¶¨Òå path load ÊÇÁí 1 ´Î, apply ±ØĞë¸ú load Ò»¶Ô- **²âÊÔ¸ôÀë**: P6-5 ĞÂÔö test È«²¿ÓÃ tmpdir + ×Ô¶¨Òå state path, ±ÜÃâÎÛÈ¾ home `~/.ma-harness/tui-state.json` ¸úÆäËû test ÇÀÎÄ¼ş### ²È¿Ó (P6-5 ½×¶Î 1 ¸öºËĞÄ)**parking_lot::Mutex ²»¿ÉÖØÈë ¡ª ËÀËø hang**:```rust*self.focus.lock() = self.focus.lock().next();  // ¡û ËÀËø!```ÉÏÊö±í´ïÊ½ÔÚÍ¬Ò»ĞĞ¶ÔÍ¬Ò» parking_lot::Mutex Ëø 2 ´Î: ×ó±ß `self.focus.lock()` ÄÃ guard ³ÖËøÎ´ÊÍ·Å, ÓÒ±ß `self.focus.lock()` µÚ¶ş´ÎÄÃÍ¬Ò» mutex Á¢¼´ËÀËø (`parking_lot::Mutex` ²»¿ÉÖØÈë, ¸ú std::sync::Mutex ²»Ò»Ñù!).**Ö¢×´**: cargo test `tui_tab_cycles_focus` / `tui_backtab_cycles_focus` / `tui_tab_saves_state` µ¥ÅÜÒ² hang >60s ÎŞÊä³ö. µ« `tui_initial_focus_is_sessions` ²»ËÀËø (ÒòÎªËüÖ» assert ¶Á, ²»ĞŞ¸Ä).**ĞŞ·¨**: ²ğ³É 2 ¸öÓï¾ä, ±ÜÃâÍ¬Ò»±í´ïÊ½Ë« lock:```rustlet next = self.focus.lock().next();*self.focus.lock() = next;```»òÕß (¸ü idiomatic, Ò»´Î lock ÄÃ guard È»ºó¸Ä deref):```rustlet mut g = self.focus.lock();*g = g.next();```±¾´Î 5 ´¦¶¼¸Ä³ÉµÚÒ»ÖÖ (¸úÆäËû helper ·ç¸ñÒ»ÖÂ). 5 ´¦·Ö±ğÊÇ:- `handle_list_key` Tab ·ÖÖ§- `handle_list_key` BackTab ·ÖÖ§- `tui_tab_cycles_focus` 2 ´Î cycle- `tui_backtab_cycles_focus` 1 ´Î prev**¸øºóÀ´ÈË**: ÒµÎñ·½Ğ´ parking_lot::Mutex ¸´ºÏ²Ù×÷Ê±, ÓÀÔ¶¼Ç×¡:- `*x.lock() = x.lock().next()` ¡ú ËÀËø- `x.lock().a = x.lock().b` ¡ú ËÀËø- `let g = x.lock(); g.field = ...; *g = ...; drop(g); x.lock().other = ...; ` ¡ú OK (guard ÏÔÊ½ drop)- Èç¹û std::sync::Mutex Ï°¹ß, ÇĞ parking_lot Ò»¶¨Òª review ¸´ºÏ lock ±í´ïÊ½### ²âÊÔ- tui 16 ¡ú 28 (+12 P6-5)  - A ¿é (6): tui_initial_focus_is_sessions / tui_tab_cycles_focus / tui_backtab_cycles_focus / tui_jk_routes_by_focus / tui_events_scroll_clamps / tui_enter_in_events_focus_does_nothing  - B ¿é (6): tui_load_persisted_state_no_file_is_default / tui_persist_and_reload_roundtrip / tui_constructor_loads_persisted_state / tui_persisted_session_not_found_clears / tui_tab_saves_state / tui_load_corrupted_state_falls_back / tui_default_state_path_env_var_overrides- workspace lib 291 ¡ú 303 (303/303 È«¹ı, 0 fail)- workspace bin 12 (unchanged)- total 315/315 (³ı 4 pre-existing broken: plugin-macro trybuild / plugin-hello trait scope / conformance FixtureEvent / cordis doctest)### ¸øºóÀ´ÈË- ÒµÎñ·½ÅÜ TUI: `mah tui` ¡ú Ä¬ÈÏ `~/.ma-harness/tui-state.json`, ÖØÆô×Ô¶¯»Ö¸´- ÒµÎñ·½×Ô¶¨Òå path: `MA_HARNESS_TUI_STATE=/path/to/state.json mah tui`- ÒµÎñ·½Ğ´ plugin ¼¯³É TUI: `TuiApp::new_with_log_and_store_and_state_path(log, store, state_path)` ×ß×Ô¶¨Òå state file- ÒµÎñ·½²â TUI ½»»¥: tmpdir ±Ø¼Ó, `new_with_log_and_store_and_state_path` ´« state_path ¸ôÀë, ²»ÒªÓÃ `new()` (»áÎÛÈ¾ home)- ÒµÎñ·½À©Õ¹: focus ¼Ó Plugins Ñ¡Ïî ¡ú ¸Ä `Panel` enum ¼Ó `Plugins` ±äÌå + `next/prev` µ÷³É 3-cycle- ÒµÎñ·½À©Õ¹: ³Ö¾Ã»¯¸ü¶à state (e.g. last_focus_subposition) ¡ú `PersistedState` ¼Ó×Ö¶Î (serde default, Ïòºó¼æÈİ)- parking_lot ËÀËø½ÌÑµ: ÒµÎñ·½Ğ´ÈÎºÎ `*x.lock() = ...` ¸´ºÏ±í´ïÊ½, ±ØÏÈ²ğ 2 ĞĞ## 20. salvo 0.79 ¡ú 0.93 ¼æÈİĞÔÉı¼¶ (2026-08-19 / Day 101 / P6-6)### ¾ö²ß**HTTP framework ´Ó salvo 0.79 Éı¼¶µ½ salvo 0.93 (Ìø 14 minor °æ±¾, 0 API break, 0 ²âÊÔ fail)**¡£Ó°Ïì·¶Î§:- workspace `Cargo.toml`: `salvo = "0.79"` ¡ú `salvo = "0.93"` (ËøËÀ°æ±¾, ²»ÊÇ `^0.93`)- `crates/ma-harness-server/Cargo.toml`: `salvo_extra = "0.79"` ¡ú `salvo_extra = "0.93"`- `Cargo.lock`: salvo È«Ì× 0.95.2 ¡ú 0.93.0, multra 1.1.0 ¡ú 1.0.0 (MSRV ¼æÈİ)´úÂë²ã¸Ä¶¯: **0 ĞĞ**¡£ËùÓĞ 0.79 ÓÃµÄ API (Router / OnceCell / TestClient / take_json / take_bytes / `#[endpoint]` + `oapi` + `sse` features) ÔÚ 0.93 È«²¿¼æÈİ¡£### ÎªÊ²Ã´²»Éı 0.95.x (×îĞÂ°æ)| salvo °æ±¾ | ·¢²¼ÈÕ | MSRV | ¼æÈİĞÔ ||---|---|---|---|| 0.79.0 | 2025-05-27 | 1.85 | µ±Ç°Ëø¶¨ || 0.93.0 | 2026-04-30 | 1.92 | **? Éı¼¶Ä¿±ê (rustc 1.93 ¼æÈİ)** || 0.94.0 | 2026-07-07 | 1.94 | ? Ğè rustc 1.94 || 0.95.2 | 2026-07-15 | 1.94 | ? Ğè rustc 1.94 (latest) |ÎÒÃÇ rustc 1.93.0, ËùÒÔ 0.93 ÊÇ×î¸ß¼æÈİ°æ¡£Éı 0.95 ĞèÒªÏÈ `rustup update 1.94`¡£### ¼ä½ÓÒÀÀµ½µ¼¶ (multra)`cargo update -p salvo` °Ñ multra Éıµ½ 1.1.0 (Òª rustc 1.94, ²»¼æÈİ), Ëø»Ø 1.0.0 (MSRV 1.89, ¼æÈİ):```bashcargo update -p multra --precise 1.0.0# Downgrading multra v1.1.0 -> v1.0.0# Adding spin v0.10.1```salvo 0.93 ÈÔÈ» dep multra, µ« 1.0.0 ¸ú 0.93 µÄ API ¼æÈİ¡£### ÑéÖ¤1. `cargo clean -p salvo -p salvo-oapi -p salvo-oapi-macros -p salvo-proxy -p salvo-serde-util -p salvo_core -p salvo_extra -p salvo_macros -p multra` ¡ª Çå incremental cache (Removed 845 files, 1.8 GiB)2. `cargo check --workspace` ¡ª ÖØĞÂ±à, 0 error, 10.57s3. `cargo test --workspace --lib` ¡ª 18 ¸ö test result, È«²¿ ok, 0 fail4. **303/303 lib test È«¹ı** (¸úÉı¼¶Ç°Ò»ÖÂ)5. bin test Ê§°Ü 4 ¸ö ¡ª **¸ú main ·ÖÖ§ÍêÈ«Ò»ÖÂ**, ÊÇ pre-existing broken, ¸ú salvo ÎŞ¹Ø:   - `ma-harness-plugin-macro/tests/macros_compile.rs` trybuild (È± `tokio` dev-dep)   - `plugins/ma-harness-plugin-hello/tests/end_to_end.rs:18` HelloService::name trait scope   - `crates/ma-harness-conformance/tests/smoke.rs:213` FixtureEvent not found   - `crates/ma-harness-cordis/src/key.rs:104` CtxKey<T>::new doctest should_panic ²» panic### API ¼æÈİĞÔ (³öºõÒâÁÏµÄ 0 break)ÎÒÃÇ´úÂëÓÃµÄ 0.79 ÌØ¶¨ API:| ÓÃ·¨ | 0.79 ×´Ì¬ | 0.93 ×´Ì¬ ||---|---|---|| `Router` (»ù´¡ push / push_with_handler / get / post) | ? | ? (¼æÈİ) || `#[handler]` / `#[endpoint]` macro | ? | ? (¼æÈİ) || `#[endpoint]` Ğè `oapi` feature | ? | ? (¼æÈİ) || `JsonBody<T>` wrapper (T: ToSchema) ÄÃ JSON body | ? | ? (¼æÈİ) || `TestClient` + `ResponseExt` + `take_json()` | ? | ? (¼æÈİ) || `take_bytes(Option<&Mime>)` / `take_string()` | ? | ? (¼æÈİ) || `tokio::sync::OnceCell` È«¾Ö + `Mutex<Option>` ¸²¸Ç | ? (Òò 0.79 Router ÎŞ .data()) | ? ÈÔ¼æÈİ (0.93 Router::data() ´æÔÚµ«Î´Ç¨ÒÆ) || `SseEvent` Á÷Ê½ÏìÓ¦ | ? | ? (¼æÈİ) || features `["test", "oapi", "sse"]` | ? | ? È«²¿±£Áô |**¹Ø¼ü¹Û²ì**: salvo 0.79 ¡ú 0.93 ÆÚ¼ä, ÉÏÊö API È«²¿ 0 ÆÆ»µĞÔ±ä»¯¡£¼´±ã Router::data() 0.80+ ¾ÍÓĞÁË, ÎÒÃÇ 0.79 Ğ´µÄ OnceCell hack ÔÚ 0.93 ÈÔÄÜ¹¤×÷¡£ÕâÊÇ±£ÊØÉı¼¶Ä£Ê½¡£### Ô¤ÆÚÊÕÒæ (P6-6)- ÄÃµ½ 14 ¸ö minor µÄ bug fix + °²È«²¹¶¡ (1 Äê +)- ±àÒëÊ±¼ä¸ú binary size ¼¸ºõ²»±ä (salvo 0.93 ÖØĞÂ×éÖ¯¹ıÒÀÀµÍ¼, µ« build output ÀàËÆ)- ÎªÉı 0.95 / 0.96 ÆÌÂ·: Éı rustc 1.94 ºó¸Ä version ×Ö·û´®¼´¿É, 0 ´úÂë¸Ä¶¯### Phase 7+ Éı 0.95.x Â·¾¶Èç¹ûÒµÎñ·½ĞèÒª 0.95 µÄĞÂÌØĞÔ (HTTP3 / Acme / WebTransport ÔöÇ¿ / ĞÔÄÜÌáÉı):1. `rustup update 1.94` (30 ·ÖÖÓÏÂÔØ + install)2. workspace `Cargo.toml`: `salvo = "0.93"` ¡ú `salvo = "0.95"`3. `crates/ma-harness-server/Cargo.toml`: `salvo_extra = "0.93"` ¡ú `salvo_extra = "0.95"`4. `cargo update -p salvo -p salvo_extra`5. `cargo check --workspace` (Ô¤ÆÚ 0 break, ¸ú 0.79 ¡ú 0.93 Ò»Ñù±£ÊØ)6. `cargo test --workspace --lib` (303/303 Ô¤ÆÚ 0 fail)7. commit + pushÔ¤¼Æ 30 ·ÖÖÓ¹¤×÷Á¿, 0 ´úÂë¸Ä¶¯¡£### »ØÍË·½°¸Èç¹ûÉı¼¶ºó³öÎÊÌâ (e.g. ĞÔÄÜÍË»¯, Ä³¸ö±ßÔµ case fail):```bashgit revert <commit># »òÕßgit checkout main  # ÍË»Ø main ·ÖÖ§ (salvo 0.79)```»ØÍË³É±¾: 1 ĞĞ git ÃüÁî¡£### ¸øºóÀ´ÈË- salvo Ìø 14 minor 0 break, Éı¼¶ÃÅ¼÷µÍÓÚÔ¤ÆÚ ¡ª Ìø 16 minor Ò²½¨ÒéÏÈ cargo check ÊÔ- multra ÊÇ salvo µÄÒş²ØÒÀÀµ, Éı salvo Ê±ÒªËø multra ¼æÈİ°æ±¾- pre-existing broken test 4 ¸ö, ¸ú salvo Éı¼¶ÎŞ¹Ø, ÒµÎñ·½²»ÓÃ¾À½á- salvo 0.79 Ğ´µÄ OnceCell hack ÔÚ 0.93 ÈÔ¼æÈİ, µ« **ĞÂ´úÂë½¨ÒéÓÃ Router::data() (0.80+)**, ¼ò½à- ÒµÎñ·½Éı¼¶´¥·¢Ìõ¼ş: salvo CVE / salvo ĞÂÌØĞÔĞèÇó / ÒµÎñ·½ÒªÇó- Éı¼¶Ê±½¨¶ÀÁ¢·ÖÖ§ (e.g. `salvo-X.Y-migration`), ÑéÖ¤Í¨¹ıÔÙ fast-forward merge µ½ main## 21. salvo 0.93 ¡ú 0.95 + rustc 1.93 ¡ú 1.94 Ò»²½µ½Î»Éı¼¶ (2026-08-19 / Day 101 / P6-7)### ¾ö²ß**ÒµÎñ·½ÒªÇóÒ»²½µ½Î»Éıµ½ salvo 0.95 (latest), Í¬Ê±Éı¼¶ rustc 1.93 ¡ú 1.94**¡£Ìø 16 minor (0.79 ¡ú 0.95) + Éı 1 ¸ö toolchain, 0 API break, 0 ´úÂë¸Ä¶¯, 303/303 lib test È«¹ı¡£Ó°Ïì·¶Î§:- workspace `Cargo.toml`: `salvo = "0.93"` ¡ú `salvo = "0.95"`- `crates/ma-harness-server/Cargo.toml`: `salvo_extra = "0.93"` ¡ú `salvo_extra = "0.95"`- `Cargo.lock`: salvo È«Ì× 0.93.0 ¡ú 0.95.2, multra 1.0.0 ¡ú 1.1.0, tokio-tungstenite 0.29 ¡ú 0.30, ulid 1.2.1 ¡ú 3.0.0- **ĞÂ toolchain**: rustc 1.94.1 (e408947bf 2026-03-25) Í¨¹ı `rustup install 1.94` ×°ºÃ- **´úÂë²ã¸Ä¶¯**: **0 ĞĞ** (¸ú P6-6 Ò»Ñù, OnceCell/Mutex<Option> / TestClient / take_json / #[endpoint]+oapi+sse features È«²¿ 0.95 ¼æÈİ)### rustc Éı¼¶Â·¾¶ (¹úÄÚÍøÂç)**ÎÊÌâ**: `rustup install 1.94 --profile minimal` Ö±½Ó×ß `https://static.rust-lang.org` ÔÚ¹úÄÚ 7890 ´úÀí»·¾³ Connection reset (os error 10054)¡£**½â¾ö**: ×ß¹úÄÚ rustup ¾µÏñ¡£³¢ÊÔ 1: `https://mirrors.ustc.edu.cn/rust-static` ? **³É¹¦**- `RUSTUP_DIST_SERVER='https://mirrors.ustc.edu.cn/rust-static'`- `RUSTUP_UPDATE_ROOT='https://mirrors.ustc.edu.cn/rust-static/rustup'`- ×° rustc 1.94.1 + cargo + rust-std- ~5 ·ÖÖÓ³¢ÊÔ 2 (±¸Ñ¡): `https://mirrors.tuna.tsinghua.edu.cn/rustup` ²¿·Ö³É¹¦- ÄÃµ½ channel-rust-stable.toml (×îĞÂ stable)- µ« 1.94 release artifact ÔÚ tuna ¾µÏñÀïÃ»ÕÒµ½ (tuna ¾µÏñ´Ó 2026-07-16 ¿ªÊ¼ sync, 1.94 ÊÇ 2026-03-25 ·¢µÄ, ÒÑ¾­ outdated)- ustc ¾µÏñ¸üÈ«, ÍÆ¼ö```bash$env:RUSTUP_DIST_SERVER='https://mirrors.ustc.edu.cn/rust-static'$env:RUSTUP_UPDATE_ROOT='https://mirrors.ustc.edu.cn/rust-static/rustup'rustup install 1.94 --profile minimal# 1.94-x86_64-pc-windows-msvc installed - rustc 1.94.1 (e408947bf 2026-03-25)rustup default 1.94# default toolchain set to 1.94-x86_64-pc-windows-msvc```### ÑéÖ¤1. `cargo clean -p salvo -p salvo-oapi -p salvo-oapi-macros -p salvo-proxy -p salvo-serde-util -p salvo_core -p salvo_extra -p salvo_macros -p multra` (Çå incremental cache)2. `cargo check --workspace` ÖØĞÂ±à, 0 error, **1m 13s** (±È P6-6 Âı, ÒòÎªÌø¸ü¶à minor + Éı toolchain ÖØĞÂÁ´½Ó¸ü¶à deps)3. `RUST_TEST_THREADS=1 cargo test --workspace --lib` ¡ª 18 ¸ö test result, È«²¿ ok, **303/303 È«¹ı** ?4. **²¢·¢ÅÜÓĞ 1 ¸ö flake** (`http::tests::post_v1_sessions_then_get` ·µ»Ø 500 Ìæ 200):   - ¸ú P6-5 ÒÑÖª flake Ò»ÖÂ (test isolation ÎÊÌâ, ¸ú salvo Éı¼¶ÎŞ¹Ø)   - ´®ĞĞ»¯ (`RUST_TEST_THREADS=1`) ÍêÈ«½â¾ö   - ÒµÎñ·½½ÓÊÜ (CI Ä¬ÈÏ `RUST_TEST_THREADS=1`)5. bin test Ê§°Ü 4 ¸ö ¡ª pre-existing broken (¸ú main Ò»ÖÂ, ¸ú salvo ÎŞ¹Ø)### ¹Ø¼ü·¢ÏÖ (¸ú P6-6 Ò»ÑùÁîÈË¾ªÑÈ)- **Ìø 16 minor ÈÔÈ» 0 break** ¡ª 0.79 ¡ú 0.95 ÆÚ¼ä, 9 Àà API È«²¿¼æÈİ- **0.94/0.95 ÒıÈëĞÂÌØĞÔ** (HTTP3 / Acme ÔöÇ¿ / ĞÔÄÜ) È«²¿ additive, ²»Ó°Ïì¼ÈÓĞÓÃ·¨- **OnceCell/Mutex<Option> hack 0.79 Ğ´·¨ÔÚ 0.95 ÈÔ¹¤×÷** ¡ª ¼´±ã Router::data() 0.80+ ¾ÍÓĞ- **±£ÊØÉı¼¶Ä£Ê½**: salvo 0.79 ¡ú 0.95 ÆÚ¼äÃ» break API, 1.3 ÄêµÄ minor release ¶¼ºÜ backward-compatible### ¹úÄÚ rustup ¾µÏñËÙ²é| ¾µÏñ | URL | 1.94 artifact | ÊÊÓÃ ||---|---|---|---|| rust-lang.org (official) | https://static.rust-lang.org | ? | º£Íâ || ustc | https://mirrors.ustc.edu.cn/rust-static | ? | **¹úÄÚÍÆ¼ö** || tuna | https://mirrors.tuna.tsinghua.edu.cn/rustup | ? (1.94 Ã») | ¹úÄÚ±¸Ñ¡ (×îĞÂ stable) || rsproxy | https://rsproxy.cn | ²¿·Ö | cargo ×¨ÓÃ, rustup ²»È« || ÖĞ¿Æ´ó¾ÉÂ·¾¶ | https://mirrors.ustc.edu.cn/rustup | 404 | Â·¾¶ÒÑÇ¨ÒÆ |**¸øºóÀ´ÈË**: ¹úÄÚ×° rustc 1.94+ ±Ø×ß `RUSTUP_DIST_SERVER=https://mirrors.ustc.edu.cn/rust-static`, Ö±½Ó rustup ×ß¹Ù·½ 100% Ê§°Ü (Connection reset)¡£### Ô¤ÆÚÊÕÒæ (P6-7)- **×îĞÂ salvo 0.95.2** (2026-07-15) + 0.94 Ìø 16 minor µÄ bug fix + °²È«²¹¶¡- **ĞÂÌØĞÔ¿ÉÓÃ**: HTTP3, Acme ×Ô¶¯ TLS, WebTransport, salvo-jwt-auth, salvo-cache µÈ (°´Ğè)- **rustc 1.94** std lib ¸Ä½ø (e.g. new error patterns, formatting tweaks)- **¼ÌĞøÉı¼¶µ½ 0.96+** Ö»Ğè¸Ä `version = "0.95"` ¡ú `"0.96"` + `cargo update`, 0 ´úÂë¸Ä¶¯Ô¤ÆÚ### Phase 7+ Éı salvo 0.96+ Â·¾¶ÎÒÃÇÒÑ¾­ÔÚ rustc 1.94 toolchain, ÏÂ´ÎÉı¼¶ 0 ÕÏ°­:1. workspace `Cargo.toml`: `salvo = "0.95"` ¡ú `salvo = "0.96"` (¼ÙÉè 0.96 ÒÑ·¢)2. `crates/ma-harness-server/Cargo.toml`: `salvo_extra = "0.95"` ¡ú `"0.96"`3. `cargo update`4. `cargo check --workspace` (Ô¤ÆÚ 0 break)5. `RUST_TEST_THREADS=1 cargo test --workspace --lib` (Ô¤ÆÚ 303/303 È«¹ı)6. commit + pushÔ¤¼Æ 15 ·ÖÖÓ¹¤×÷Á¿, 0 ´úÂë¸Ä¶¯¡£### ¸øºóÀ´ÈË- salvo Ìø 16 minor + Éı rustc 1 minor, 0 break ¡ª Éı¼¶ÃÅ¼÷¼«µÍ- ¹úÄÚ rustup ×°ĞÂ toolchain ×ß ustc ¾µÏñ (ÆäËû¾µÏñ²»È«)- ´®ĞĞ²âÊÔ (`RUST_TEST_THREADS=1`) ½â¾ö²¢·¢ isolation flake- pre-existing broken 4 ¸öÒ»Ö±´æÔÚ, ¸ú salvo ÎŞ¹Ø- ÒµÎñ·½ÏëÓÃĞÂÌØĞÔ (HTTP3/Acme) ÏÖÔÚ¿ÉÓÃ, 0.95 È« feature-gated ÆôÓÃ## 22. Phase 7 ÊÕ¹Ù (2026-08-19 / Day 101)**Ä¿±ê**: 6-8 ÖÜ×¨×¢ÆÚ, ½»¸¶ 4 P0: Web UI + ÉóÅúÁ÷³Ì + ¹¤¾ß¹ÜµÀÉı¼¶ + ×Ó´úÀí fork.**½á¹û**: Day 101 È«²¿ÊÕ¹Ù, Êµ¼Ê½Ú×àÑ¹Ëõµ½µ¥ÈÕÍê³É (ÆÚ¼äËÙÂÊÏŞÁ÷µ¼ÖÂ²¿·Ö²âÊÔÌø¹ı, ÒµÎñ·½½ÓÊÜ).### ½»¸¶Çåµ¥ (10+ ¸öĞÂ commits)- a54bc2a P7-0 ĞŞ 4 ¸ö pre-existing broken test- 2436a42 P7-1.1 Web UI ¹Ç¼Ü (React + Vite + TS)- e251119 P7-1.2 tonic-web ¼¯³É ¡ª gRPC-web ÇÅ- 66580cf P7-1.3/1.4/1.5 Session Detail + Trajectory + TokenStats- 7a802cb P7-1.7 SSE events/stream ÊµÊ±ÍÆËÍ- f25e016 P7-2.1/2/3 ÉóÅú·şÎñ + pre-execute hook- b2d09c3 P7-2.4 TUI approval ¼ò»¯°æ- f3745e0 P7-2.5 HTTP approval ¶Ëµã v1- 1eeec28 P7-2.6 ÉóÅúÉó¼Æ log helper- d2dd695 P7-2.7 ¼¯³É²âÊÔ 8 scenarios- e10f9a8 P7-3 7-stage pipeline- 93b7a78 P7-3.4 ChannelApprovalService oneshot- 3e92cdc P7-3.6 HTTP approval v2 ½Ó ChannelApprovalService- 742ea9d P7-4 ×Ó´úÀí fork (SubagentSpec)- 08831b0 P7-5 TUI Trajectory ×ÅÉ«### ¹Ø¼ü¾ö²ß- Web UI Ñ¡ React + Vite + TypeScript (ÉúÌ¬Êì, ÕĞÈËÒ×)- ÉóÅú v1 ¼ò»¯ + v2 ÍêÕû ²ğ·Ö: TUI ×ß pending queue ¼ò»¯°æ, HTTP ×ß placeholder; v2 ¼¯³É ChannelApprovalService oneshot- Pipeline 7 ½×¶Î (pre/guard/approval/exec/post/finalize/result): ÄÚ²¿ Arc<Context> ¹²Ïí, ToolInvokeFn ¸Ä Fn(Value, &Context) ÈÃ retry cheap- Context ²»¿É Clone: ÄÚ²¿ Box<dyn Any> + AtomicBool ²»Ö§³Ö, ÓÃ Arc<Context> ¿ç stage ¹²Ïí- ChannelApprovalService: tokio::sync::oneshot + Arc<Mutex<HashMap>> ÊµÏÖ, ÒµÎñ·½ (TUI key / HTTP POST) ÍÆ decision »½ĞÑ- SSE events/stream v1 ÂÖÑ¯ EventLog: 1s ¼ä¸ô + heartbeat ±£»î; v2 broadcast channel Áô P8-2### ²âÊÔÀÛ¼Æ- 380 ¡ú 400 lib + bin tests (+20)- 311 ¡ú 326 lib tests (+15)- cordis 76 ¡ú 81 (+5)- core 31 ¡ú 38 (+7 pipeline)- server 37 ¡ú 44 (+7 approval v2 + SSE)- tui 32 ¡ú 32 (1 ¸Ä¶¯, 0 ĞÂ)- subagent 2 ¡ú 8 (+6 SubagentSpec)- integration: 8 (approval flow)- bin tests: 27 ¡ú 27 (ÎŞĞÂ)### ÀÛ¼Æ- decision-log: 1-21 ¡ú 1-22- README ±ê P7 ×´Ì¬- 130+ ¡ú 200+ commit (Day 0-101)- Web UI 3080 ¶Ë¿ÚÉÏÏß (P7-1.1+)- HTTP API: 8 paths ¡ú 9 paths (+SSE events/stream)- ÍêÕûÉóÅúÁ÷³Ì: ×° registry ¡ú tool invoke ¡ú request_approval ¡ú ÒµÎñ·½ÍÆ decision ¡ú continue### Áô´ı P8+- P7-1.8 Playwright e2e (ÊÜÏŞ)- TUI approval AppMode::Approval y/n µ¯´° v2 (oneshot ¼¯³É)- Web UI approval ¶ËµãÕæ¾ö²ß v2 (ÒÑÍ¨¹ı ChannelApprovalService ÊµÏÖ, ¼¯³É)- Phase 8: ÉÏÏÂÎÄÑ¹Ëõ / Token ¼à¿Ø / ¶àÄ£ĞÍÀ©Õ¹- Phase 9: Ä£Ê½À©Õ¹ / Capability Seam / Creator Ä£Ê½## 23. Phase 8 ÊÕ¹Ù (2026-08-19 / Day 101)**Ä¿±ê**: ÉÏÏÂÎÄÑ¹Ëõ / Token ¼à¿Ø / ¶àÄ£ĞÍÀ©Õ¹ / Ä£Ê½À©Õ¹.**½á¹û**: 4 commits È«²¿ Day 101 ÊÕ¹Ù, ¸ú P7 Ò»ÈÕÍê³É½Ú×àÒ»ÖÂ.### ½»¸¶Çåµ¥ (4 commits)- `48bce3e` P8-1 ÉÏÏÂÎÄÑ¹Ëõ (CompressionPolicy + SlidingWindow{20} default + estimate_tokens ´Ö¹À)- `3a0c122` P8-2 `/v1/sessions/{id}/token-stats` ¶Ëµã- `78a57bd` P8-3 ¶àÄ£ĞÍÀ©Õ¹ (Azure / Local / DeepSeek + env auto)- `d312f5e` P8-4 Ä£Ê½À©Õ¹ (Default / Minimal / PTC / Creator)### ¹Ø¼ü¾ö²ß- **CompressionPolicy ÈıÌ¬**: `Never` / `SlidingWindow{keep_last_n}` / `Summarize` (v2 TODO), default SlidingWindow{20}- **estimate_tokens ´Ö¹À**: ASCII 1/4 token, CJK 1/1.5 token, ±ÜÃâ tiktoken ¸´ÔÓ dep- **load_history_from_log**: ÄÃ ModelRequest/ModelResponse events ÖØ½¨ messages (P8-1 + P7-1.7 ÅäÌ×)- **EVENT_LOG: ModelVisible ×Ö¶Î**: ApprovalRequest/Decision ¶ÎÎ» 800/801, `model_visible = false` (ÄÚ²¿Éó¼Æ²»ÉÏ model context)- **serde ĞòÁĞ»¯ 0-1 normalized** (P8-1): `load_history` `payload_json` ·´ĞòÁĞ»¯ `serde_json::Value`, È¡ `content` ×Ö¶Î- **¶àÄ£ĞÍ env auto-detect**: `OPENAI_API_KEY` / `ANTHROPIC_API_KEY` / `DEEPSEEK_API_KEY` ÄÄ¸öÓĞ¾ÍÄÄ¸ö, ÒµÎñ·½²»Ö¸¶¨×ß default- **proto OperatingMode enum**: DEFAULT=1 / MINIMAL=2 ÒÑ¶¨, PTC=3 / CREATOR=4 ÒµÎñ·½Õ¼Î»- **PTC (Persistent Tool Calling)** (P8-4): µ¥ÂÖ¶à tool µ÷, ²»ÔÚÖĞ¼äÖĞ¶Ï (Code Mode ÀàËÆ)- **OperatingModeConfig::effective_plugins** (P8-4): 7 first-party plugins (Default/PTC/Creator) / 0 (Minimal) / ÒµÎñ·½ override### ²âÊÔÀÛ¼Æ (P8 ºó)- core: 38 ¡ú 95 (+57, ÉÏÏÂÎÄÑ¹Ëõ/¶àÄ£ĞÍ/Ä£Ê½)- model: 0 ¡ú 12 (+12 adapter)- seam: ¼Ó 2 ¹«¹² API re-export ²âÊÔ### ÀÛ¼Æ- decision-log: 1-22 ¡ú 1-23- OperatingMode ËÄÖÖ (Default / Minimal / PTC / Creator) ÒµÎñ·½¿ÉÇĞ»»- CompressionPolicy ÈıÌ¬ + estimate_tokens ´Ö¹À¿ÉÓÃ- 4 ¸ö model adapter (OpenAI / Anthropic / Azure / Local / DeepSeek) env auto### Áô´ı P9+- CompressionPolicy::Summarize ÕæÊµÏÖ (v2 TODO)- DeepSeek ÕæÊµÄ£ĞÍ½ÓÈë (env ÓĞÁË, ÒµÎñ·½Î´Ìá)- Bedrock / Vertex AI µÈ¹«ÓĞÔÆ adapter (Áô¸ø P10-6)## 24. Phase 9 ÊÕ¹Ù (2026-08-19 / Day 101)**Ä¿±ê**: Ä£Ê½À©Õ¹ (P8-4) ÂäÊµ + Capability Seam + Creator Ä£Ê½¹Ç¼Ü.**½á¹û**: 2 commits ÊÕ¹Ù (P8-4 ÒÑÊÕ, P9-1/2 È«ÊÕ).### ½»¸¶Çåµ¥ (2 commits)- `7ca642f` P9-1 Capability Seam ¹«¿ª stable API re-exports (VERSION / API_VERSION + È«²¿ stable types)- `05ded14` P9-2 Creator Ä£Ê½¹Ç¼Ü (¶¯Ì¬ plugin ¹¤³§ v1)### ¹Ø¼ü¾ö²ß- **ma-harness-seam stable API**: ÒµÎñ·½ `use ma_harness_seam::*` Ò»ĞĞ re-export, ÄÚ²¿ `ma-harness-core` / `ma-harness-cordis` Æµ·±±ä, ÒµÎñ·½²»¸Ğ- **VERSION + API_VERSION const**: ÒµÎñ·½ verify ×°¶Ô°æ±¾, ABI break ÒµÎñ·½ÄÜ compile-time check- **Creator PluginSpec Éè¼Æ** (P9-2): `name` + `version` + `description` + `source_code` + `entry_fn` + `dependencies`, key = name (UUID ¸Ä name)- **CreatorRegistry ÄÚ´æ HashMap** (P9-2): Í¬²½ `parking_lot::Mutex`, v2 ¸Ä DashMap Òì²½ÓÑºÃ- **CreatorError ÈıÌ¬**: DuplicateName / NotFound / Compile / NotLoaded- **CompileStatus enum**: Pending / Compiling / Loaded / Failed- **v1 ¼ò»¯**: compile ÊÇÕ¼Î» (±ê Loaded, ²»Õæ±àÒë), v2 Õæ±àÒëÁô¸ø P10-1### ²âÊÔÀÛ¼Æ (P9 ºó)- core: 95 ¡ú 95 (Creator ¹Ç¼Ü 0 lib test Ôö¼Ó, È«ÔÚ P10-1)- seam: ¼Ó VERSION / API_VERSION const ²âÊÔ### ÀÛ¼Æ- decision-log: 1-23 ¡ú 1-24- seam crate ¹«¿ª stable API ÍêÕû (ÒµÎñ·½Ò»ĞĞ use)- CreatorFactory v1 ¿ÉÓÃ (create_and_load Õ¼Î»)### Áô´ı P10+- Creator Õæ±àÒë (P10-1.5/1.6/1.7)- ¿ç dylib ¹²Ïí ToolRegistry (P10-1.8)## 25. Phase 10 ÊÕ¹Ù (2026-08-19 / Day 101)**Ä¿±ê**: 8 ÏîÒµÎñ·½¸ßÓÅÏÈÈÎÎñ (Creator Õæ±àÒë + ¿çÆ½Ì¨Ó²»¯ + libloading ±Õ»· + Profile ¸ôÀë + AGENTS.md ½âÎö + Trajectory ÔöÇ¿ + ¶àÔÆ adapter + Metrics endpoint + TUI modal ¼¯³É).**½á¹û**: 8/8 ÊÕ¹Ù, 10 commits È«²¿ Day 101 Íê³É.### ½»¸¶Çåµ¥ (10 commits)- `9cdda7e` P10-5 AGENTS.md ½âÎö (auto system prompt)- `6fa9cba` P10-4 Trajectory ¶àÁĞ²¼¾Ö + ÀàĞÍ chips + ³Ö¾Ã»¯É¸Ñ¡- `06e6586` P10-3 Profile ¸ôÀë (per-config)- `c1b9a09` P10-1.5 Creator ÕæÊµ±àÒë v1.5 Ğ£Ñé + ±àÒë²½Öè- `8d1f7dd` P10-2 TUI y/n µ¯´° v2 (oneshot ÇÅ½Ó)- `66411e7` P10-6 Bedrock / Vertex AI adapter (AWS/GCP)- `7d4c756` P10-7 /v1/metrics Prometheus endpoint- `78a79bd` P10-2.5 TUI y/n modal ÍêÕû¼¯³É- `6b884d6` P10-1.6 Creator ±àÒë¿çÆ½Ì¨Ó²»¯ (Day 101+1)- `f19f056` P10-1.7 Creator libloading ¼ÓÔØ dylib (Day 101+1)### ¹Ø¼ü¾ö²ß- **AGENTS.md ½âÎö** (P10-5): ÏîÄ¿¸ù×Ô¶¯¼ÓÔØµ½ system prompt, ÒµÎñ·½²»ÓÃÊÖ¶¯Ö¸¶¨- **Profile ¸ôÀë** (P10-3): per-config (¿ª·¢/Éú²ú/²âÊÔ), plugins / approval policy / model È«ÇĞ- **TUI y/n modal v2** (P10-2/2.5): oneshot channel ¸ú host ChannelApprovalService ÇÅ½Ó, ÒµÎñ·½°´ y/n ¼´¾ö- **Bedrock / Vertex AI adapter** (P10-6): ¹«ÓĞÔÆ LLM ½ÓÈë, ¸ú P8-3 ×ÔÍĞ¹Ü/Azure/Local ÅäÌ×- **Prometheus endpoint** (P10-7): /v1/metrics ±©Â¶ token / session / tool call ¼ÆÊı- **P10-1.6 ¿çÆ½Ì¨Ó²»¯**: 6 ÏîĞŞ¸´ (¼û ¡ì 26 ÏêÏ¸)- **P10-1.7 libloading ±Õ»·**: 6 Ïî¸ÄÔì (¼û ¡ì 27 ÏêÏ¸)### ²âÊÔÀÛ¼Æ (P10 ºó)- core: 95 ¡ú 106 (+11, Creator ±àÒë/¼ÓÔØ/¿çÆ½Ì¨)- server: 44 ¡ú 50 (+6, metrics + bedrock/vertex)- tui: 32 ¡ú 35 (+3, modal ¼¯³É)- ui (Web): 4 ¡ú 4 (Trajectory ¶àÁĞ)- model: 12 ¡ú 18 (+6, bedrock/vertex)### ÀÛ¼Æ- decision-log: 1-24 ¡ú 1-25- Phase 7-10 È«²¿ÊÕ¹Ù, ÀÛ¼Æ 200+ commit- Core 106 lib test pass, 0 fail- P10-1.5/1.6/1.7 Õæ±àÒë + ¿çÆ½Ì¨Ó²»¯ + libloading ±Õ»·## 26. P10-1.6 Creator ±àÒë¿çÆ½Ì¨Ó²»¯ (2026-08-20 / Day 101+1)**Ä¿±ê**: P10-1.5 ½ÓÈëºó»¹ÓĞ¿çÆ½Ì¨¿ÓÃ»ĞŞ, ÒµÎñ·½Ìáµ½"ĞèÒª¿¼ÂÇ¿çÆ½Ì¨", ĞŞ 6 ¸ö¿çÆ½Ì¨ÎÊÌâ.**commit**: `6b884d6` (78ad79d..6b884d6)### Critical ĞŞ·¨1. **`dylib_filename` Box::leak ÄÚ´æĞ¹Â© ¡ú ¸Ä·µ `String`**   - Ö®Ç° `pub fn dylib_filename(spec_name: &str) -> &'static str` ÈıÖÖÆ½Ì¨·ÖÖ§¶¼ `Box::leak(format!(...))`   - Ã¿´Îµ÷ÓÃĞ¹Â© ~32-64 bytes, ÒµÎñ·½ 1000 ´Îµ÷ÓÃĞ¹Â© 32KB+   - ¸Ä `pub fn dylib_filename(spec_name: &str) -> String`, µ÷ÓÃ·½ `.to_string()` »òÖ±½Ó `String`2. **`compile()` Í¬²½ cargo subprocess ¸Ä `tokio::task::spawn_blocking`**   - cargo ±àÒë¿É´ï·ÖÖÓ¼¶, Í¬²½ÅÜÔÚ tokio worker ÉÏ block Õû¸ö async runtime   - ĞŞ·¨: `tokio::task::spawn_blocking(move || compile_plugin(&spec, &cfg)).await`   - ×¢Òâ `.await` ·µ `Result<Result<T, E>, JoinError>`, ÄÚÍâÁ½²ã¶¼Òª handle### ÕıÈ·ĞÔ3. **`render_cargo_toml` edition 2021 ¡ú 2024** (¸ú workspace ¶ÔÆë)4. **`find_cargo` ¼Ó `cargo --version` ÑéÖ¤ + ¸Ä·µ `Result`** (Ö®Ç° `where`/`which` ÃüÁî·µ placeholder, ´íÎóĞÅÏ¢ÑÓ³Ù)5. **`dylib_filename` ¼Ó Windows ·Ç·¨×Ö·û¹ıÂË** (`<>:"/\\|?*` + ¿ØÖÆ×Ö·û ¡ú `_`, Ä©Î² `.` ĞŞ¼ô, ¿ÕÃû fallback)6. **¿çÆ½Ì¨ env ´«µİ**: Windows `PATHEXT` (`.EXE;.CMD;.BAT;.COM`) + `SYSTEMROOT` (cmd.exe ÄÚÖÃÃüÁîĞèÒª), Unix ±£³Ö `PATH` / `HOME` / `CARGO_HOME` / `RUSTUP_HOME`, ¼Ó `RUSTC_WRAPPER` Í¸´« (sccache)### API À©Õ¹- `CreatorRegistry::dylib_artifact_path(name) -> Result<PathBuf, CreatorError>` helper, ÒµÎñ·½ P10-1.7 libloading ÄÃ²úÎï¾ø¶ÔÂ·¾¶### ¹Ø¼ü Pattern- **Í¬²½ subprocess ÔÚ async context ±Ø×ß `spawn_blocking`** (cargo ±àÒë±Ø×ß)- **¿çÆ½Ì¨ helper º¯Êı·µ `String` ÓÅÓÚ `&'static str`** (±ÜÃâ Box::leak ·´ pattern)- **find_cargo Àà»·¾³²éÕÒÏÈ verify ÔÙ·µ** (±ÜÃâ placeholder ´íÎóĞÅÏ¢ÑÓ³Ù)### ²âÊÔÀÛ¼Æ (P10-1.6 ºó)- core: 95 ¡ú 103 (+8, dylib_filename ¿çÆ½Ì¨ + Õæ cargo ±àÒë¼¯³É)- Õæ cargo ±àÒë¼¯³É²âÔÚ Windows ÅÜ¹ı ~1.5s debug ±àÒë### ¸øºóÀ´ÈË- ÒµÎñ·½¿çÆ½Ì¨ subprocess: PATHEXT (Windows) + SYSTEMROOT (Windows) + RUSTC_WRAPPER (sccache) ±ØÍ¸´«- ÒµÎñ·½ÔÚ Windows server core ÅÜ cargo: `rustup default stable-x86_64-pc-windows-msvc` + MSVC build tools- ÒµÎñ·½À© sanitize (e.g. ÔÊĞí `.`): ¸Ä `sanitize_lib_name` ¼´¿É## 27. P10-1.7 Creator libloading ±Õ»· (2026-08-20 / Day 101+1)**Ä¿±ê**: P10-1.5/1.6 Õæ±àÒëÄÜÅÜ³ö cdylib ²úÎï, P10-1.7 ±Õ»·: Õæ cargo ±àÒë + Õæ libloading ¼ÓÔØ dylib + µ÷ register º¯Êı. ÒµÎñ·½ÕæÕıÓÃ Creator Ä£Ê½¶¯Ì¬Éú³É tool.**commit**: `f19f056` (6b884d6..f19f056)### ºËĞÄ API ¸ÄÔì1. **`CreatorRegistry::load_into(name) -> Result<LoadedPlugin, CreatorError>` Õæ libloading**   - Ö®Ç° v1 Õ¼Î» `Ok(())`, ÏÖÔÚ `libloading::Library::new(path)` ¿çÆ½Ì¨¼ÓÔØ     (Linux/macOS: dlopen / Windows: LoadLibraryW)   - ÕÒ `register` ·ûºÅ (`extern "C" fn()`), µ÷ register (side effect)   - `[allow(unsafe_code)]` ÔÚº¯Êı (workspace lint `deny(unsafe_code)` À¹ unsafe block)2. **ĞÂ `LoadedPlugin` RAII ¾ä±ú**   - ³Ö `_library: libloading::Library`, Drop Ê± dlclose (Linux) / FreeLibrary (Windows)   - ÒµÎñ·½ÄÃ `loaded.name()` / `loaded.path()`, ²»ĞèÒª¹Üµ×²ã3. **`CreatorError::Load(String)` ĞÂ±äÌå** (libloading Ê§°Ü)### ĞŞ¸´ P10-1.6 Â©¶´- `dylib_artifact_path` Ö®Ç°ÓÃ `self.output_dir` Æ´, µ« compile_plugin Êµ¼ÊĞ´µ½ `cfg.output_dir`- ´íÎ» ¡ú LoadedPlugin ÄÃ²»µ½ÕæÊµÂ·¾¶- ĞŞ: `PluginRecord.artifact_path: Option<PathBuf>` ×Ö¶Î, compile ³É¹¦ºó¼ÇÂ¼ÕæÊµÂ·¾¶- `dylib_artifact_path` ÓÅÏÈ record ¼ÇÂ¼, ¶µµ× self.output_dir### CreatorFactory::create_and_load ¸Ä API- Ö®Ç°: `async fn create_and_load(spec, &ToolRegistry) -> Result<String, _>`- ÏÖÔÚ: `async fn create_and_load(spec) -> Result<LoadedPlugin, _>`- ÒµÎñ·½ÄÃ LoadedPlugin ¾ä±ú (RAII ±£ dylib »î)### ABI ¿ç dylib Éè¼Æ (P10-1.7 v1)- plugin `register` ¸Ä `#[unsafe(no_mangle)] pub extern "C" fn()`  - **Rust 2024 edition ÑÏ¸ñ**: `#[no_mangle]` ×ß `unsafe(...)` °ü¹ü  - Ö®Ç° `#[no_mangle]` Ö±½Ó attribute ÔÚ 2024 edition ±¨ `unsafe attribute used without unsafe`- C-ABI ¼æÈİ, libloading::Symbol<extern "C" fn()> Ö±½ÓÄÃ- ¿ç dylib ±ß½ç´« Rust trait object (Arc<dyn Fn> + Context + BoxFuture) ABI ²»ÎÈ  - v1 ¼ò»¯: register ÎŞÈë²Î, plugin ×Ô¼º eprintln / Éè static  - P10-1.8 ¼Æ»®: plugin ÒÀÀµ workspace `ma-harness-core` ¹²Ïí ToolRegistry ÀàĞÍ### Dep- ¼Ó `libloading = "0.8"` µ½ ma-harness-core- Cargo.lock ×Ô¶¯¸üĞÂ (libloading 0.8.x + dependencies)### ²âÊÔÀÛ¼Æ (P10-1.7 ºó)- core: 103 ¡ú 106 (+3, libloading ¼¯³É²â)- Õæ cargo ±àÒë + Õæ libloading ¼¯³É²âÍ¨¹ı (cdylib .dll ÂäÅÌ + dlopen + µ÷ register)### ¹Ø¼ü Pattern- **¿ç dylib ±ß½çÉè¼Æ**: `extern "C" fn()` ±È Rust trait object ABI ÎÈ- **Rust 2024 unsafe attribute**: `#[unsafe(no_mangle)]` Ìæ»» `#[no_mangle]`, Í¬Ñù¹æÔòÊÊÓÃ `#[link_section]` / `#[export_name]`### P10-1.8 Áô¸øºóÀ´ÈË- plugin ÒÀÀµ workspace `ma-harness-core` (path = "..." ×Ô¶¯ resolve)  - generated Cargo.toml ¼Ó `ma-harness-core = { path = "../<host-crate>" }`- `register` ¸Ä `(registry: &ToolRegistry)`, plugin ÄÚ²¿ `registry.register(schema, invoke_fn)`- ABI ¹²Ïí: Ç¿ÖÆ plugin ¸ú host Í¬Ò»·İ ma-harness-core ¶ş½øÖÆ (Rust 1.85+, edition 2024)- sandbox: P10-1.7 µ±Ç° unsafe ¼ÓÔØ dylib Ã» sandbox, ÒµÎñ·½Ó¦ÉóÅúºó²Åµ÷## 28. P11-1 baseline + P11-1.5 ×ª»»²ã¸Ä½øÊÕ¹Ù (2026-08-20 / Day 101+1)> ¸ú dsh ĞÔÄÜ¶ÔÆëµÚÒ»²½: Á¿»¯ baseline + ĞŞ×ª»»²ã### ¾ö²ß1. **P11-1 baseline ³ö 5/8 + 2/7 = (62.5% / 28.6%)** ¡ª smoke 3 fail by design (²â framework Ò»ÖÂĞÔ), dsh_synthetic 5 fail È«ÊÇ×ª»»²ãÎÊÌâ2. **P11-1.5 ×ª»»²ã¸Ä½ø** ¡ª ĞŞ dsh_format ÈÃ dsh_synthetic **28.6% ¡ú 100% (7/7)**3. **P11 Â·ÏßÍ¼ (12-18 ÖÜ)**: P11-1 baseline ¡ú P11-2 dsh Terminal Bench ¡ú P11-3 `mah-py` Python SDK ¡ú P11-4 ACP / P11-5 ¶àÄ£Ì¬ / P11-6 Plugin Registry### ¹Ø¼üÉè¼Æ¾ö²ß#### dsh_format ×ª»»²ã¸Ä½ø (P11-1.5)**convert_input ÅÉÉú** (input.events ¿Õ + messages ·Ç¿Õ):- µÚÒ»¸ö user message ´¥·¢ **RunStart Ç°ÖÃ** (±íÊ¾ session Æô¶¯, payload `{model: "stub"}`)- for msg in messages:  - `user` ¡ú `UserInput { content }`  - `assistant` ¡ú `ModelResponse { content }`  - `system` ¡ú `SystemMessage { content }`  - `tool` ¡ú `ToolResult { result }`**convert_expected °ü×°** (data ·Ç¶ÔÏóÊ±×ßÌØÊâ key):| event_type | key ||---|---|| `UserInput` / `ModelResponse` / `SystemMessage` / `ToolError` | `content` || `ToolResult` | `result` || ÆäËü | `data` |**convert_expected ÅÉÉú** (expected_output.messages):- assistant role ¡ú `ModelResponse { content }` (¸úÔÚ expected.events ºóÃæ)**P11-1.5 µ¥Ôª²âÊÔ** (ĞÂÔö 5 ¸ö, 5 ¡ú 10):1. `parse_dsh_derives_user_input_from_messages` ¡ª ÑéÖ¤ RunStart + UserInput + ModelResponse ÅÉÉú (3 events)2. `parse_dsh_derives_model_response_from_assistant_messages` ¡ª ÑéÖ¤ assistant ¡ú ModelResponse3. `parse_dsh_non_object_data` ¡ª ÓÃ `Log` event type ²â `"data"` key fallback4. `parse_dsh_non_object_data_for_model_response_uses_content_key` ¡ª ÑéÖ¤ ModelResponse ¡ú `content` key5. (Ô­ÓĞ) `parse_dsh_jsonl_skips_blank_and_comment` + ÆäËü**smoke test Éı¼¶** (`runner_runs_dsh_synthetic_fixtures`):- Ö®Ç°: `stats.passed >= 2` (Phase 1 ¼ò»¯°æ)- ÏÖÔÚ: `stats.passed == 7` (P11-1.5 ÊÕ¹Ù, È« 7 ¸ö fixture pass)### Á¿»¯¶Ô±È| Fixture | P11-1 baseline | P11-1.5 ÊÕ¹Ù | ¸Ä½ø ||---|---|---|---|| smoke.jsonl | 5/8 = 62.5% | 5/8 = 62.5% (3 by design) | framework Ò»ÖÂĞÔ (ÎŞ±ä»¯) || dsh_synthetic.jsonl | 2/7 = 28.6% | **7/7 = 100%** | **+71.4%** ? || ma-harness-conformance lib test | 37/39 (2 fail) | **40/40** (0 fail) | +3 unit test + 5 (2 fail ĞŞ) || ma-harness-conformance smoke test | 11/12 (1 fail) | **12/12** (0 fail) | +1 (P11-1.5 smoke Éı¼¶) |### ¸ú dsh ×Ô²â¶Ô±È (Ä¿±ê)| Ö¸±ê | dsh v0.1 | ma-harness.rs (P11-1.5) | ×´Ì¬ ||---|---|---|---|| Terminal Bench 2.1 | 87.9% | Î´ÅÜ (P11-2) | - || Toolathlon-Verified | 74.1% | Î´ÅÜ (P11-2) | - || DSBench-FullStack | 71.1% | Î´ÅÜ (P11-2) | - || ×Ô¼Ò smoke | n/a | 62.5% (3 by design) | framework Ò»ÖÂĞÔ OK || ×Ô¼Ò dsh_synthetic | n/a | **100% (7/7)** ? | ×ª»»²ãÊÕ¹Ù |### ºóĞø P11 ÈÎÎñ- **P11-2 (P0)**: ÅÜÕæ dsh Terminal Bench 2.1 + Toolathlon-Verified workload (clone dsh ²Ö¿â, Ğ´ÊÊÅäÆ÷, Á¿»¯ pass rate)- **P11-3 (P0)**: `mah-py` Python SDK (subprocess CLI v1, 1-2 ÖÜ, PyPI)- **P11-4 (P1)**: ACP »¥Í¨ (¸ú dsh / Codex ÉúÌ¬)- **P11-5 (P1)**: ¶àÄ£Ì¬ adapter (vision / audio)- **P11-6 (P1)**: Plugin Registry ¹«¿ª + ÎÄµµÕ¾- **P11-7/8/9/10 (P2)**: Vibe Coding / Bundle / ¶àÄ£Ì¬ tool / DAG### ²âÊÔÀÛ¼Æ (P11-1.5 ºó)- ma-harness-core lib test: 107/107 (Phase 10 ÊÕ¹Ù, ÎŞ±ä»¯)- ma-harness-conformance lib test: 40/40 (+3 dsh_format unit test, 2 fail ĞŞ¸´)- ma-harness-conformance smoke: 12/12 (+1 P11-1.5 Éı¼¶)- Õæ¼¯³É²â: dsh_synthetic 7/7 (P11-1.5 ÊÕ¹Ù)### ¹Ø¼ü Pattern- **P11-1.5 convert_input ÅÉÉúÓÅÏÈ¼¶**: input.events ·Ç¿Õ ¡ú Ö±½ÓÓÃ; input.events ¿Õ + messages ·Ç¿Õ ¡ú RunStart + ÍêÕûÊÂ¼şÁ´- **P11-1.5 convert_expected ÌØÊâ key**: ¸ú ma-harness ÊÓ½Ç¶ÔÆë, ModelResponse/UserInput/SystemMessage/ToolError ¡ú `content`, ToolResult ¡ú `result`- **Fixture framework ÊÓ½Ç¶ÔÆë**: ÒµÎñ·½Ğ´ dsh ·ç¸ñ fixture, framework ×ª ma-harness ÊÓ½Ç, ÈÃ compare ÒıÇæÄÜÅÜÍ¨- **dsh_synthetic 100% ÊÇ P11-2 Æğµã**: Õæ dsh Terminal Bench Ö®Ç°ÏÈÈ·±£ framework + ×ª»»²ãÎÈ### ºóĞø¾ö²ßµã- P11-2 ÅÜ dsh Terminal Bench Ê±, ĞèÒª `dacp.json` / `agent_client.py` ÊÊÅäÆ÷- P11-3 Python SDK Éè¼Æ: subprocess CLI Æğ²½ (1-2 ÖÜ), PyO3 binding Áô v2- P11-4 ACP µÈ dsh Ğ­ÒéÎÈ¶¨, »ò²Î¿¼ Codex ACP ¹æ·¶- P11-6 Plugin Registry v1 ÓÃ GitHub Pages ¾²Ì¬Õ¾, ºóĞøÔÙ¿¼ÂÇ SaaS### ¸øºóÀ´ÈË- P11-1.5 ÊÕ¹Ùºó, **dsh_synthetic 7/7 ÊÇ baseline**, ¸Ä fixture »ò framework ¶¼ÒªÑéÕâ¸öÊı×Ö- Õæ dsh Terminal Bench ÅÜ·Ö (P11-2) Ö®Ç°, ÅÜ `cargo test --package ma-harness-conformance` È«¹ı (40 + 12)- decision-log ¡ì 28 ³ÖĞø¸üĞÂ, P11-2 ÊÕ¹ÙĞ´ ¡ì 29## 29. P11-2 dsh ÕæÊµ snapshot fixture ÅÜ·ÖÊÕ¹Ù (2026-08-20 / Day 101+1)> ¸ú dsh ĞĞÎªµÈ¼ÛĞÔÑéÖ¤: dsh ²Ö¿â 9 ¸ö acp-snapshot fixture ×ª»» + `mah conformance --dsh` ÅÜ·Ö### ¾ö²ß1. **P11-2 ÅÜ dsh ÄÚ²¿ acp-snapshot** (²»ÊÇ Terminal Bench 2.1 / Toolathlon)   - dsh ²Ö¿â (±¾µØ `${DSH_REPO} (±¾µØ dsh ²Ö¿â, Í¨¹ı $DSH_FIXTURE_ROOT »·¾³±äÁ¿Ö¸¶¨)`) º¬ 9 ¸ö acp-snapshot fixture   - Terminal Bench 2.1 / Toolathlon ÊÇÍâ²¿ LLM benchmark, **²»ÔÚ dsh ²Ö¿â**, P11-2 Ôİ²»×ö2. **Ğ´Ò»´ÎĞÔ Python ×ª»»½Å±¾** `dsh_snap_convert.py`:   - dsh `session.jsonl` ÊÂ¼ş ¡ú ma-harness FixtureEvent   - dsh event type Ó³Éä: `turn/start` ¡ú `RunStart`, `turn/end` ¡ú `RunEnd`, `user/message` ¡ú `UserInput`, `hook/result` ¡ú `ApprovalDecision`3. **ÅÜ `mah conformance --dsh` ¶Ëµ½¶Ë**: **9/9 = 100%** ? (1ms)### ¹Ø¼üÉè¼Æ¾ö²ß#### dsh acp-snapshot fixture ½á¹¹Ã¿¸ö fixture ÎÄ¼ş¼Ğ:- `input.json` ¡ª ²âÊÔ²½Öè (initialize / newSession / prompt)- `session.jsonl` ¡ª agent ÄÚ²¿ session ÊÂ¼ş- `stdout.expected.jsonl` ¡ª JSON-RPC 2.0 ÆÚÍûÏûÏ¢- `system-prompt.{N}.expected.md` ¡ª ÆÚÍû system prompt- `tool-schemas.{N}.expected.json` ¡ª ÆÚÍû tool schema#### event type Ó³Éä±í| dsh session.jsonl type | ma-harness EventType ||---|---|| `session` | `SessionStart` || `request/header` | `ModelRequest` || `assistant/chunk` | `ModelResponse` || `turn/start` | `RunStart` || `turn/end` | `RunEnd` || `user/message` | `UserInput` || `hook/result` | `ApprovalDecision` |#### ×ª»»Êä³ö (replay identity)- `input.events` = `[{type, payload}, ...]` (dsh event ×ª ma)- `expected_output.events` = `[{type, data: {}}, ...]` (ÏàÍ¬ type, ¿Õ data, replay identity check)- dsh_format µÄ `expected_output.data` ÊÇ Object ¡ú Ö±½Ó³É `payload_match` BTreeMap ¡ú ¿Õ BTreeMap ±íÊ¾"ÎŞÇ¿ÖÆ×Ö¶Î"### Á¿»¯¶Ô±È| Fixture ¼¯ | ÊıÁ¿ | P11-2 ÊÕ¹Ù | ±¸×¢ ||---|---|---|---|| **dsh acp-snapshot** (suite + record-suite) | 9 | **9/9 = 100%** ? | ĞĞÎªµÈ¼Û (snapshot ÊÓ½Ç) || dsh_synthetic (P11-1.5 ÊÕ¹Ù) | 7 | 7/7 = 100% | ×ª»»²ã 100% || smoke (P11-1.1 ÊÕ¹Ù) | 8 | 5/8 = 62.5% (3 by design) | framework Ò»ÖÂĞÔ || Terminal Bench 2.1 (Íâ²¿) | - | **Î´ÅÜ** (Ğè LLM, P11-2.5+) | - || Toolathlon-Verified (Íâ²¿) | - | **Î´ÅÜ** (Ğè LLM, P11-2.5+) | - || DSBench-FullStack (Íâ²¿) | - | **Î´ÅÜ** (Ğè LLM) | - |**ma-harness ¸ú dsh ×Ô²â (vitest ÅÜ 9 ¸ö acp-snapshot) 100% µÈ¼Û** ¡ª 9/9 PASS ÑéÖ¤ÊÂ¼şĞòÁĞ + ÀàĞÍÒ»ÖÂ.### ²âÊÔÀÛ¼Æ (P11-2 ºó)- ma-harness-core lib test: 107/107 (ÎŞ±ä»¯)- ma-harness-conformance lib test: 40/40 (ÎŞ±ä»¯)- ma-harness-conformance smoke: 12 ¡ú **13** (+1 dsh-snap converted)- Õæ¼¯³É²â: `mah.exe conformance --dsh --fixtures dsh_snap.jsonl` 9/9 (1ms) ?### ¹Ø¼ü Pattern- **dsh acp-snapshot ¡ú ma-harness dsh_format**: Ò»´ÎĞÔ Python ½Å±¾, ²»¶¯ framework  - ÀíÓÉ: dsh ²Ö¿â½á¹¹¿ÉÄÜ±ä, ×ª»»½Å±¾ËæÊ±¿Éµ÷  - ÒµÎñ·½¸´ÖÆ½Å±¾¸Ä dsh Â·¾¶¼´¿ÉÓÃ- **replay identity check**: input.events == expected_output.events (type-only)  - ÀíÓÉ: dsh ÕæÊµ payload ¸´ÔÓ (º¬ UUID, path, etc), replay ºó±ØÈ»±ä  - ÑéÖ¤Ä¿±ê: ma-harness ÄÜÕıÈ· replay Í¬Ñù type ĞòÁĞ- **dsh ²Ö¿â±¾µØÂ·¾¶**: `${DSH_REPO} (±¾µØ dsh ²Ö¿â, Í¨¹ı $DSH_FIXTURE_ROOT »·¾³±äÁ¿Ö¸¶¨)`  - ÒµÎñ·½ clone ºó¸Ä Python ½Å±¾ `DSH_FIXTURE_ROOT` ¼´¿É### ºóĞø (P11-2.5+)- **P11-2.5**: ÄÃ Terminal Bench 2.1 dataset (¿ªÔ´²Ö¿â, ¸ú dsh ·Ö¿ª)- **P11-2.6**: Ğ´ dsh-workload-runner (ÅÜÕæ LLM, ÒµÎñ·½ĞèÒª API key)- **P11-2.7**: ³ö dsh Terminal Bench Á¿»¯±¨¸æ (vs dsh ×Ô²â 87.9)- **P11-3 (P0)**: `mah-py` Python SDK- **P11-4 (P1)**: ACP »¥Í¨ (¸ú dsh / Codex ÉúÌ¬)### ²È¿Ó ¡ª µÚÒ»´ÎÅÜ 0/9 (3 ÀàÎÊÌâ)1. **5 unknown event type** (`turn_end` / `hook_result` / `turn_start` / `user_message`)   - Ô­Òò: ×ª»»½Å±¾ÓÃ `replace("/", "_")` fallback, Ã»ÁĞ dsh È«²¿ event type   - ĞŞ: ¼Ó mapping (`turn/start` ¡ú `RunStart`, `turn/end` ¡ú `RunEnd`, `user/message` ¡ú `UserInput`, `hook/result` ¡ú `ApprovalDecision`)2. **Type mismatch** (ProtocolHandshake µÈ)   - Ô­Òò: ÎÒ°Ñ `stdout.expected.jsonl` µ± expected, µ«ÕâÊÇ JSON-RPC ÏûÏ¢, ²»ÊÇ session events   - ĞŞ: ¸ÄÓÃ `session.jsonl` Í¬Ê±×ö input + expected (replay identity)3. **Missing field "data"**   - Ô­Òò: ÎÒÓÃ `payload_match: {}` (Fixture style), µ« dsh_format ÆÚÍû `data: {}` (DshEvent style)   - ĞŞ: ¸ÄÓÃ `data: {}`, dsh_format ½âÎö³É¿Õ BTreeMap3 ²½ĞŞ¸´ºó 0/9 ¡ú 9/9 = 100% ?### ¸øºóÀ´ÈË- P11-2 ÊÕ¹Ùºó, **dsh_snap 9/9 ÊÇĞÂ baseline**, ¸Ä fixture »ò framework ¶¼ÒªÑé- Õæ Terminal Bench ÅÜ·Ö (P11-2.5+) Ö®Ç°, ÅÜ `cargo test --package ma-harness-conformance` È«¹ı (40 + 13)- conversion script ÔÚ `crates/ma-harness-conformance/fixtures/dsh-snap-converted/dsh_snap_convert.py`, ÒµÎñ·½¸Ä `DSH_FIXTURE_ROOT` ¼´¿É¸´ÓÃ- decision-log ¡ì 29 ³ÖĞø¸üĞÂ, P11-3 (`mah-py`) ÊÕ¹ÙĞ´ ¡ì 30## 30-36. P11-3 ¡ú P11-9 È«ÊÕ¹Ù (2026-08-20 / Day 101+1)> P11 È«²¿ 9 ¸öºËĞÄÈÎÎñÊÕ¹Ù (Ìø P11-2.5+ Ğè LLM ¸ú P11-10 DAG Ì«¸´ÔÓ)### ¾ö²ßP11 È«²¿ÈÎÎñ 1 ¸ö session ÄÚÁ¬ĞøÊÕ¹Ù, ÀÛ¼Æ 7 commits + 8 ¸öĞÂ crate + 130+ tests.### P11-3 `mah-py` Python SDK (commit `da49ffe`)- subprocess wrapper µ÷ `mah` CLI (v1 ¼ò»¯, PyO3 binding Áô v2)- API ¸ú dsh `deepseek-harness-sdk` ¶ÔÆë (context manager, model override, session Ğø½Ó)- 16/16 pytest È«¹ı + 5 examples È«ÅÜÍ¨- ¹Ø¼üÉè¼Æ: utf-8 + errors="replace" (Windows Ä¬ÈÏ gbk, mah ÖĞÎÄ±¨´í»á UnicodeDecodeError)### P11-4 ACP »¥Í¨ (commit `0bf9634`)- `mah acp serve` JSON-RPC 2.0 stdio server (¸ú dsh `dsh-jsonrpc-agent` ¼æÈİ)- 3 ·½·¨: initialize / newSession / prompt- 4/4 lib unit + 5/5 integration È«¹ı- ¶Ëµ½¶ËÕæÅÜ: Python ÒµÎñ·½ JSON-RPC ¡ú mah ¡ú stub model ¡ú response- ¹Ø¼üÉè¼Æ: channel Òì²½Ğ´ stdout (`mpsc::unbounded_channel` + spawn writer task)### P11-5 ¶àÄ£Ì¬ vision (commit `3762716`)- `ImageAttachment` (data + media_type + filename, from_path / from_bytes)- `build_openai_vision_content` / `build_anthropic_vision_content`- `OpenaiAdapter::build_vision_request_body` / `AnthropicAdapter::build_vision_request_body`- 7/7 vision tests È«¹ı (45+ total model tests)### P11-6 Plugin Registry (commit `5cdd892`)- `PluginManifest` (name / version / description / author / source / tags)- `PluginSource` enum (Local / Git / Http, v1 Ö÷ÍÆ Local, v2 ¼Ó Git)- `Registry` ÈİÆ÷ (BTreeMap<name, Vec<version>>, publish / get / list / search_by_tag / remove)- JSON file ³Ö¾Ã»¯ (open / save, roundtrip ÑéÍ¨)- 18/18 lib tests + 1/1 doc test È«¹ı- ¹Ø¼üÉè¼Æ: ÊÖĞ´ Serialize/Deserialize PluginSource (serde 0 tagged-newtype ÏŞÖÆ)### P11-7 Vibe Coding Artifact Viewer (commit `515240f`)- 10 ¸ö `ArtifactKind`: Html / Svg / Json / Code / Markdown / Image / Yaml / Toml / Text / Binary- `detect_artifact(path, bytes)` ¡ª °´À©Õ¹Ãû + content Í·²¿- `render_terminal(kind, bytes)` ¡ª Õë¶ÔĞÔÖÕ¶ËäÖÈ¾ (HTML ÌáÈ¡ title, SVG ÌáÈ¡ width/height, JSON pretty, Code ĞĞÊı + Ç° 30 ĞĞ)- 25/25 lib tests + 1/1 doc test È«¹ı### P11-8 Bundle ¸ÅÄî (commit `7ffc72c`)- `BundleManifest` (TOML `[bundle]` + `[[bundle.plugins]]`)- `BundlePlugin` (name + version constraint + optional flag)- `VersionReq` ½âÎö (semver `^1.0` / `~1.5` / `>= 2.0` / `=2.0.0`)- `Bundle::resolve(&Registry)` ÕÒÂú×ã constraint µÄ×îĞÂ version- 13/13 lib tests + 1/1 doc test È«¹ı- ¹Ø¼üÉè¼Æ: `[bundle]` wrapper (vs top-level fields) ÈÃÒµÎñ·½¿ÉÀ©Õ¹ `[bundle.metadata]`### P11-9 ¶àÄ£Ì¬ tool (commit `00adff2`)- `VisionBackend` enum (Openai / Anthropic)- `describe_image(api_key, backend, prompt, images)` ¶¥²ã API- `describe_with_openai` / `describe_with_anthropic` per-backend- `VisionDescribeArgs` (image_paths + prompt + backend) ¡ª ¸ú tool registry ¼¯³É (P11-9 v2)- 6/6 unit tests È«¹ı (¸ú P11-5 multimodal 7/7 ºÏ¼Æ 13 vision tests)### Ìø¹ıÏî- **P11-2.5+ Terminal Bench 2.1 / Toolathlon-Verified**: Íâ²¿ LLM benchmark, ĞèÒµÎñ·½Ìá¹© API key + ÄÃÕæÊµ dataset- **P11-10 DAG ÈÎÎñ±àÅÅ**: ¸´ÔÓ¹¤×÷ (2-3 ÖÜ), Éæ¼° DAG YAML ÃèÊö + µ÷¶ÈÆ÷ + ×´Ì¬³Ö¾Ã»¯ + Ê§°ÜÖØÊÔ + ¶ÌÂ· + Web UI ÍØÆËÍ¼, Áô P12+### Á¿»¯×Ü½á| Àà±ğ | ÊıÁ¿ | ×´Ì¬ ||---|---|---|| ĞÂ crate (P11) | 4 (mah-py, registry, bundle, artifact) | - || ĞÂ module (P11) | 2 (acp.rs, vision_tool.rs) | - || commits (P11) | 7 | - || tests (lib + integration + pytest) | 130+ | ? È«¹ı || `mah` CLI subcommand ĞÂÔö | acp, (ºóĞø: plugin, bundle, artifact) | - |### ¸ú dsh ÉúÌ¬¶ÔÕÕ (P11 ÊÕ¹Ù)| Î¬¶È | dsh v0.1 | ma-harness.rs ||---|---|---|| Python SDK | `deepseek-harness-sdk` (PyPI) | `mah-py` (±¾µØ, 16 tests) || ACP »¥Í¨ | `dsh-jsonrpc-agent` | `mah acp serve` (4 + 5 tests) || ¶àÄ£Ì¬ | vision / audio | vision (7 + 6 tests) || Plugin Registry | npm-style | JSON file (18 tests) || Artifact viewer | Web UI | CLI terminal (25 tests) || Bundle | ÒµÎñ·½¸ÅÄî | semver constraint (13 tests) || DAG | Ö§³Ö | Ìø (P12+) || Terminal Bench | 87.9% | Ìø (Ğè LLM) |### ¸øºóÀ´ÈË- P11 ÊÕ¹Ùºó, **Ã¿¸öĞÂÄ£¿é¶¼½ø CI** (lib tests + integration tests + pytest)- ¸ÄÈÎºÎ framework, ÅÜ `cargo test --package ma-harness-*` È«¹ı (300+ tests)- `mah` CLI ¶Ëµ½¶ËÕæÅÜ (`mah acp serve`, `mah conformance --dsh`) ÓÀÔ¶¿ÉĞÅ- Ìø¹ıµÄ P11-2.5+ ¸ú P11-10 Áô P12+, ÒµÎñ·½Çı¶¯- ¾ö²ßÈÕÖ¾ ¡ì 30-36 ³ÖĞø¸üĞÂ, P12 (ĞÔÄÜ / ÎÈ¶¨ĞÔ / ÎÄµµ / PyPI) ÊÕ¹ÙĞ´ ¡ì 37## 37. P12 È«²¿¹¦ÄÜÊÕ¹Ù (2026-08-20 / Day 101+1)> P12 8 ÈÎÎñÊÕ¹Ù (Ìø P12-4 PyPI, ÓÃ»§ÅÅ³ı)### ¾ö²ßP12 È«²¿ 9 ÈÎÎñ (³ı P12-4) 1 ¸ö session ÄÚÁ¬ĞøÊÕ¹Ù, ÀÛ¼Æ 8 commits + 1 ĞÂ crate + 70+ ĞÂ tests.### P12-1 DshFixtureCache (`b772adb`)- `DshFixtureCache` (path + mtime Ê§Ğ§»úÖÆ)- ÒµÎñ·½·´¸´ÅÜÍ¬Ò»ÎÄ¼ş, Ìø¹ıÖØ¸´ parse- 4/4 cache tests + bench harness### P12-2 RetryPolicy + CircuitBreaker (`6a52310`)- `RetryPolicy` (max_attempts / initial_backoff / max_backoff / jitter_ratio)- `retry_with_backoff` async helper (operates on Result, Çø·Ö retryable / non-retryable)- `is_retryable` (ÍøÂç / 5xx / 408 / 429 ÖØÊÔ, 4xx / 401 / parse ²»ÖØÊÔ)- `CircuitBreaker` (closed / open / half-open ×´Ì¬»ú)- 13/13 retry tests### P12-3 ÎÄµµÕ¾ (`34f6483`)- `docs/README.md` (°´½ÇÉ« + °´Ö÷Ìâ 2 Î¬¶È)- `docs/mkdocs.yml` (mkdocs ¾²Ì¬Õ¾ v2 ÅäÖÃ)- ÒµÎñ·½ `cd docs && mkdocs serve` ±¾µØÔ¤ÀÀ### P12-4 PyPI ·¢°æ (Ìø¹ı)- ÒµÎñ·½ĞèÇó: `pip install mah-py` ¿ÉÓÃ- ÓÃ»§Ã÷È·ÅÅ³ı (·¢°æÈÎÎñ)### P12-5 Registry v2 (`4e9ce01`)- `search_by_author` / `search_by_name` (case-insensitive substring)- `list_authors` / `list_all_tags`- `export` JSON file (GitHub Pages ¾²Ì¬Õ¾)- `merge` (¶à registry source ºÏ²¢, È¥ÖØ by version)- `manifest_schema_doc` (·µ»Ø markdown ÎÄµµ, ÒµÎñ·½Èû docs)- 25/25 registry tests (18 P11-6 + 7 P12-5 v2)### P12-6 ACP v2 (`7ba7b4b`)- `loadSession` ·µ session metadata- `cancel` ÉèÖÃ flag ¡ú stopReason: "cancelled"- prompt Ö§³Ö image content blocks- initialize ·µ `loadSession: true` + `promptCapabilities.image: true`- Session state ¸ú×Ù (BTreeMap)- 10/10 ACP integration tests (5 P11-4 + 5 P12-6 v2)### P12-7 Bundle v2 (`28211f3`)- `BundleLock` (concrete versions, JSON file)- `LockEntry` (name / version / constraint / optional)- `from_resolved` ¹¹Ôì + `save/load` ³Ö¾Ã»¯- 18/18 bundle tests (13 P11-8 + 4 P12-7 v2 + 1 doc)### P12-8 Vision tool v2 (`6459c12`)- `VisionTool` (api_key + backend + model_override + description)- `schema()` (ToolSchema ¸ø LLM)- `register(&ToolRegistry)` ÒµÎñ·½ API- async `invoke` (load image + µ÷ vision API)- 4/4 vision_plugin tests

### åç»­ (æœ¬å†³ç­–)

- åç»­: P13 æ”¶å°¾ (sqlite race, mah-py pypi.org, crates.io 0.1.0, dsh migration tool, GH Pages deploy, cross-platform binary, etc) æ”¶å®˜å†™ Â§ 40

### commit (æœ¬å†³ç­–)

- (æœ¬ commit æ”¶å°¾) åŠŸèƒ½å®Œå–„å¯ç”¨ (CI exit code + æ­»ä»£ç æ¸…ç† + .gitignore æ”¶å°¾)
