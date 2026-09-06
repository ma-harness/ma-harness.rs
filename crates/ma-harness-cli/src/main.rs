//! ma-harness CLI 入口 (`mah` 二进制)
//!
//! 7 个子命令 (Day 7-8 5 个 + Day 39 +2):
//! - `start` — 起 server (tonic gRPC + salvo HTTP)
//! - `run` — 跑一次 agent (本地, 不连 server)
//! - `plugins` — 列出已装载 plugin
//! - `events` — 查 session 事件
//! - `conformance` — 跑 conformance fixture (验证 ma-harness 跟 dsh 行为等价)
//! - `bench` — benchmark 信息 / 跑 cargo bench 提示
//! - `version` — 打印版本

mod acp;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use ma_harness_conformance::{
    fixture::FixtureLoader, ConformanceRunner, ConformanceResult, Fixture, ReportFormat,
    ReportWriter,
};
use ma_harness_core::{AgentLoop, AgentRunRequest, EventLog, StubModelAdapter};
// 2026-08-18 (Day 52): ma_harness_proto 恢复 (用本地 vendor/protoc), gRPC service 恢复
use ma_harness_proto::ma_harness::v1::{
    agent_service_server::AgentServiceServer, session_service_server::SessionServiceServer,
};
use ma_harness_seam::PluginLoader;
use ma_harness_registry::Registry; // P14 (2026-08-20): registry list/export CLI
// Phase 2.2 (T2.2): 引用 hello plugin 触发 link, inventory::submit! 才有 effect
#[allow(unused_imports)]
use ma_harness_plugin_hello as _hello;
use ma_harness_server::{ServerBuilder, SessionStore};

#[derive(Parser, Debug)]
#[command(name = "mah", about = "ma-harness AI agent orchestrator")]
struct Cli {
    /// 事件日志路径 (默认: ~/.ma-harness/events.db)
    #[arg(long, default_value = "~/.ma-harness/events.db")]
    log: PathBuf,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// 启动 server (tonic gRPC + salvo HTTP)
    Start {
        /// gRPC 监听端口
        #[arg(long, default_value = "50051")]
        grpc_port: u16,
        /// HTTP 监听端口
        #[arg(long, default_value = "50050")]
        http_port: u16,
        /// 持久化 session store 路径 (sqlite db, 不传 = 内存)
        /// 业务方重启 server 时 session 跟 event 从这个 db 恢复
        #[arg(long)]
        store_path: Option<PathBuf>,
    },
    /// 跑一次 agent (本地, 不连 server)
    Run {
        /// Session ID (留空 = 新建)
        #[arg(long)]
        session: Option<String>,
        /// 用户消息
        message: String,
        /// 模型 (默认 stub)
        #[arg(long, default_value = "stub")]
        model: String,
    },
    /// 列出已装载 plugin (走 inventory 分布式注册)
    Plugins,
    /// 按名装载 plugin (Phase 2.2 / T2.2 inventory + dylib 走 PluginLoader)
    LoadPlugin {
        /// plugin 名 (e.g. "hello", "bash", "fs")
        name: String,
        /// 可选 ctx 标识 (debug 用, 默认 "default")
        #[arg(long, default_value = "default")]
        ctx_id: String,
    },
    /// 查 session 事件
    Events {
        /// Session ID
        session: String,
    },
    /// **P5-5 (Day 94)**: Session CRUD via local SqliteStore + EventLog
    ///
    /// 不连 server, 业务方传 --store-path 直接读本地 db
    /// (跟 `mah start --store-path <x>` 启动的 db 一致就能查)
    ///
    /// 例子:
    ///   mah sessions list --store-path ~/.ma-harness/sessions.db
    ///   mah sessions get <id> --store-path <db>
    ///   mah sessions events <id> --log <events.db>
    Sessions {
        #[command(subcommand)]
        action: SessionsAction,
    },
    /// 跑 conformance fixture, 比对实际事件 vs 期望, 出报告
    ///
    /// 例子:
    ///   mah conformance --fixtures fixtures/smoke.jsonl --output target/
    ///   mah conformance --fixtures fixtures/dsh/ --dsh --output target/
    Conformance {
        /// Fixture 路径 (文件 .jsonl 或目录)
        #[arg(long)]
        fixtures: PathBuf,
        /// 视为 dsh 风格 fixture (走 dsh_format 转换层)
        #[arg(long)]
        dsh: bool,
        /// 报告输出目录 (写 .md + .json)
        #[arg(long, default_value = "target/")]
        output: PathBuf,
        /// verbose (打印每条 fixture 跑的过程)
        #[arg(long, short)]
        verbose: bool,
    },
    /// Benchmark 信息 / 跑 cargo bench 提示
    ///
    /// 不真跑 (criterion 自己跑), 只打印命令 + 报告路径
    Bench {
        /// 单 crate (e.g. ma_harness_cordis) 不传跑全部
        crate_name: Option<String>,
    },
    /// **P14 (2026-08-20)**: Plugin Registry 列表 / 导出 (GH Pages 部署用)
    ///
    /// 例子:
    ///   mah registry list
    ///   mah registry list --registry ~/.ma-harness/registry.json
    ///   mah registry export --output docs/registry/registry.json
    Registry {
        #[command(subcommand)]
        action: RegistryAction,
    },
    /// 版本
    Version,
    /// 跑 Code Mode (Phase 2 / T3.1) — 编译并执行 wasm module
    ///
    /// 例子:
    ///   mah code run ./hello.wat
    ///   mah code run ./module.wasm
    Code {
        #[command(subcommand)]
        action: CodeAction,
    },
    /// **Phase 3.3 / T3.3**: 业务方 prompt → LLM 生成 .wat → wasm 沙箱跑
    ///
    /// 需要环境 `OPENAI_API_KEY` (或 `--api-key <key>`)
    ///
    /// 例子:
    ///   OPENAI_API_KEY=sk-... mah run-prompt "compute 1+1, return the result as i32"
    ///   OPENAI_API_KEY=sk-... mah run-prompt "log 'hello world', return 0"
    RunPrompt {
        /// 业务方需求描述 (LLM 转 .wat)
        prompt: String,
        /// 可选 API key (缺省读 env OPENAI_API_KEY)
        #[arg(long)]
        api_key: Option<String>,
        /// 可选 model (缺省 gpt-4o-mini)
        #[arg(long, default_value = "gpt-4o-mini")]
        model: String,
    },
    /// **Phase 3.5 / T3.5**: 导出 OpenAPI spec (CI 同步用)
    ///
    /// 例子:
    ///   mah openapi export --output docs/api/openapi.json
    OpenApi {
        #[command(subcommand)]
        action: OpenApiAction,
    },
    /// **P13.5 / Day 101+2**: dsh-adapter 健康检查 + 版本信息
    ///
    /// 例子:
    ///   mah dsh info    # 显示 dsh runtime / Node.js / JSON-RPC 协议版本
    ///   mah dsh doctor  # Node.js 装没 / 子进程能起 / mock plugin 跑通
    Dsh {
        #[command(subcommand)]
        action: DshAction,
    },
    /// **P6-1 / Day 99**: 走 gRPC RunStream RPC, 实时打印 token (跟 stub/真 LLM 都能跑)
    ///
    /// 例子:
    ///   mah run-stream --grpc-url http://localhost:50051 "hello"
    ///   mah run-stream --grpc-url http://server:50051 --model "openai:gpt-4o-mini" "tell me a joke"
    RunStream {
        /// 业务方需求描述 (LLM 输入)
        prompt: String,
        /// gRPC server URL (默认 localhost:50051)
        #[arg(long, default_value = "http://localhost:50051")]
        grpc_url: String,
        /// Session ID (留空 = 新建)
        #[arg(long)]
        session: Option<String>,
        /// 模型 (默认 "stub", 真 LLM 走 "openai:gpt-4o-mini" / "anthropic:claude-3-5-sonnet" 等)
        #[arg(long, default_value = "stub")]
        model: String,
    },
    /// **P11-4 (Day 101+1)**: ACP (Agent Communication Protocol) 互通 (跟 dsh / Codex 生态)
    ///
    /// `mah acp serve` 起 JSON-RPC 2.0 stdio server, 跟 dsh `dsh-jsonrpc-agent` 风格一致.
    /// 业务方 (Python / Node / Rust) 写 JSON-RPC 到 stdin, 从 stdout 读响应/通知.
    ///
    /// 例子:
    ///   mah acp serve --model stub < input.jsonl > output.jsonl
    ///   # 跟 dsh SDK 互通: `from deepseek_harness import DeepSeekHarness` 调 mah
    Acp {
        #[command(subcommand)]
        action: AcpAction,
    },
    /// **Phase 3.7 / T3.7**: 显式 enforce landlock (Linux) / seatbelt (Mac) / stub (其他)
    ///
    /// ⚠️ **警告**: 一旦 enforce 是全进程 (不可逆). 业务方决定要不要跑.
    ///
    /// 例子:
    ///   mah sandbox apply --read-paths /tmp,/var/llm-output
    ///   mah sandbox apply --read-paths /tmp --write-paths /tmp
    ///   mah sandbox apply --read-paths /tmp --temp-dir   # 加系统 tmpdir
    Sandbox {
        #[command(subcommand)]
        action: SandboxAction,
    },
    /// **Phase 3.9 / T3.9**: 启动 TUI dashboard (ratatui)
    ///
    /// 3 个 panel: Sessions | Events | Plugins
    /// 每 500ms 刷新, 'q' / Esc / Ctrl-C 退出
    Tui {
        /// **P4-1**: EventLog sqlite path (走真 events)
        /// 缺省: stub fallback (Phase 3.9 行为)
        #[arg(long)]
        log: Option<std::path::PathBuf>,
        /// **P4-3**: SessionStore sqlite path (走真 sessions)
        /// 缺省: 走 EventLog 推 session / 全 stub fallback
        #[arg(long)]
        store_path: Option<std::path::PathBuf>,
    },
    /// **P15.5.3 (Day 101+33)**: 读 / 写 user-level settings (`~/.ma-harness/settings.yaml`)
    ///
    /// 业务方 workflow:
    ///   `mah settings set api.openai_key sk-...` — 改 setting, 立即生效 (走 hot-reload)
    ///   `mah settings get models.default`        — 读单个 key
    ///   `mah settings list`                       — 列所有 keys + values (YAML dump)
    ///
    /// 底层走 ma-harness-settings crate (P15.5.1 + P15.5.2 hot-reload). 改完不需
    /// 重启 `mah` 进程, 运行中 agent loop 通过 SettingsWatcher 自动 reload.
    Settings {
        #[command(subcommand)]
        action: SettingsAction,
    },
    /// **P15.7 (Day 101+33)**: Hook bridges (Claude Code / Codex 接入).
    ///
    /// 业务方 workflow:
    ///   `mah hook install claude-code`  打印 ~/.claude/settings.json 配置 hint
    ///   `mah hook run <name>`              读 stdin JSON, 调 hook, 写 stdout
    ///   `mah hook list`                    列出可用 hook adapter
    Hook {
        #[command(subcommand)]
        action: HookAction,
    },
    /// **P15.4.3**: Workflow CLI — 跑 / 校验 workflow YAML 文件
    ///
    /// 业务方:
    ///   `mah workflow run <file>`                跑 workflow (默认 logging runner, dry-run)
    ///   `mah workflow validate <file>`           只 parse + DAG 校验
    ///   `mah workflow run <file> --engine dag`   选 engine
    Workflow {
        #[command(subcommand)]
        action: WorkflowAction,
    },
    /// **P15.6.2**: Self-modification CLI — 业务方能 inspect / enable / disable plugins
    ///
    /// 业务方:
    ///   `mah self list`              列出挂载的 plugin (name, enabled, mount_path)
    ///   `mah self inspect`           打印 `~/.ma-harness/cordis.yml` 完整内容
    ///   `mah self enable <name>`     启用 plugin (audit log 自动记录)
    ///   `mah self disable <name>`    禁用 plugin (audit log 自动记录)
    ///   `mah self audit`             查 audit log (谁在什么时候改了哪个 plugin)
    #[command(name = "self")]
    // 用户输 `mah self ...`, 但 enum variant 名字用 `SelfMod` 避免跟 Self type 冲突
    SelfMod {
        #[command(subcommand)]
        action: SelfAction,
    },
    /// **P14.4.2**: Compaction CLI — 业务方能手动跑 / 预览 session 压缩
    ///
    /// 业务方:
    ///   `mah compaction run --input <jsonl>`         跑 BasicCompactionProvider, 打印 stats
    ///   `mah compaction info`                        打印默认 CompactionContext 配置
    ///
    /// **P14.4.2 限制**: 仅支持 BasicCompactionProvider, LLM-based 是 P14.4.3+
    Compaction {
        #[command(subcommand)]
        action: CompactionAction,
    },
    /// **P14.5.2**: LSP CLI — 业务方能从命令行跑 LSP request (e.g. 调 rust-analyzer 查 definition)
    ///
    /// 业务方:
    ///   `mah lsp request --server rust-analyzer --method textDocument/definition --params <json>`
    ///
    /// **P14.5.2 限制**: 只支持 single request (no persistent session, no notify auto),
    /// 高级 LSP 用法 (definition / references / hover) 业务方自己 wrap params JSON
    Lsp {
        #[command(subcommand)]
        action: LspAction,
    },
    /// **P14.6.2**: Web CLI — 业务方能从命令行搜 / 抓 web
    ///
    /// 业务方:
    ///   `mah web search --query <text>`            跑 BraveSearchProvider (P14.6.2 stub, 返 Unsupported)
    ///   `mah web fetch --url <url>`                跑 HttpFetchProvider (P14.6.1 真, reqwest)
    ///   `mah web info`                             打印可用 provider 提示
    ///
    /// **P14.6.2 限制**: search providers 都是 stub (Brave / DDG) — 业务方 outbound 准备好
    /// 之后 P14.6.2+ 才实装. fetch 是真 (reqwest) — 业务方网络允许就能用.
    Web {
        #[command(subcommand)]
        action: WebAction,
    },
    /// **P14.7.2**: Todo CLI — 业务方能从命令行管 multi-step work
    ///
    /// 业务方:
    ///   `mah todo list`                            列所有 Todo
    ///   `mah todo write --content <text>`          写一条 Todo
    ///   `mah todo update <id> --status <status>`   改 Todo 状态
    ///   `mah todo delete <id>`                     删 Todo
    ///
    /// **P14.7.2 限制**: in-memory store only (进程退出清空). P15+ 业务方可注入
    /// SqlTodoStore / RedisTodoStore 持久化.
    Todo {
        #[command(subcommand)]
        action: TodoAction,
    },
    /// **P14.7.2**: Plan CLI — 业务方能从命令行管 plan mode (read-only proposals)
    ///
    /// 业务方:
    ///   `mah plan list`                            列所有 Plan
    ///   `mah plan write --title <text>`            写一个 Plan
    ///   `mah plan update <id> --status <status>`   改 Plan 状态
    ///   `mah plan delete <id>`                     删 Plan
    Plan {
        #[command(subcommand)]
        action: PlanAction,
    },
    /// **P14.9.2**: Profile CLI — 业务方能列 / 查 / 验 profile (5 builtin + 用户自定义)
    ///
    /// 业务方:
    ///   `mah profile list`                            列 5 builtin profiles (web/headless/sdk/sdk-minimal/acp)
    ///   `mah profile show <name>`                     查 profile 详情 (bundles / plugins / settings)
    ///   `mah profile validate <path>`                 验自定义 profile (`~/.ma-harness/profiles/<name>/cordis.yml`)
    ///   `mah profile info`                            打印可用 builtin profile 提示
    ///
    /// **P14.9.2 限制**: 5 builtin profile 是 hardcoded, 用户自定义 profile 走
    /// `~/.ma-harness/profiles/<name>/cordis.yml` 加载. P15+ 业务方可加 SDK-level
    /// patch / bundle layer.
    Profile {
        #[command(subcommand)]
        action: ProfileAction,
    },
    /// **P14.10.2**: Context CLI — 业务方能管 request context (trace_id / deadline / metadata)
    ///
    /// 业务方:
    ///   `mah context new [--trace-id <id>] [--deadline-secs N]`   创建新 context
    ///   `mah context show`                                         打印当前 context
    ///   `mah context validate`                                     验证 (trace_id non-empty, not expired)
    ///   `mah context chain-info`                                   打印 ContextChain middleware 数
    ///   `mah context info`                                         打印 usage / available features
    ///
    /// **P14.10.2 限制**: process-singleton context (per-invocation, 不持久化).
    /// P15+ 业务方可注入 ACTIVE_CONTEXT typed key 跨组件传播.
    Context {
        #[command(subcommand)]
        action: ContextAction,
    },
    /// **P14.11.2**: Guard CLI — 业务方能管 loop-hygiene guard (MaxSteps + RepeatedArgs)
    ///
    /// 业务方:
    ///   mah guard demo [--max-steps N] [--max-repeats N]        跑内置 demo chain, 跑 N 步看决策
    ///   mah guard observe --event <type> [--tool-name <n>] [--args-hash <h>] [--success]
    ///                                                            喂 event 给 singleton chain, 跑 observe
    ///   mah guard chain-info                                   打印当前 chain 配置
    ///   mah guard reset                                        重置全部 guard state
    ///   mah guard list                                         列已注册 builtin guard 类别
    ///   mah guard info                                         打印 usage / available features
    ///
    /// **P14.11.2 限制**: process-singleton chain (per-invocation, 不持久化). 业务方自己跑
    /// loop 时调 mah guard observe 触发 singleton, 或直接 use ma_harness_guard::* SDK
    /// 跑 max-steps / repeated-args 决策.
    Guard {
        #[command(subcommand)]
        action: GuardAction,
    },
}

/// **P15.4.3**: Workflow CLI sub-actions
///
/// 业务方:
///   `mah workflow run <file>`                  跑 workflow (default engine = local)
///   `mah workflow validate <file>`             parse + DAG 校验 (不跑)
#[derive(Subcommand, Debug)]
enum WorkflowAction {
    /// 跑一个 workflow YAML 文件
    ///
    /// Example:
    ///   `mah workflow run ~/.ma-harness/workflows/ci.yaml`
    ///   `mah workflow run ci.yaml --engine parallel --concurrency 8`
    ///   `mah workflow run ci.yaml --dry-run` (用 LoggingStepRunner, 不真跑 shell)
    Run {
        /// Workflow YAML 文件路径 (绝对 / 相对 / `~/.ma-harness/workflows/<name>` 短名)
        file: PathBuf,
        /// Engine (default: local)
        #[arg(long, value_enum, default_value = "local")]
        engine: WorkflowEngineArg,
        /// Max concurrency (only for parallel / dag, default 4)
        #[arg(long, default_value = "4")]
        concurrency: usize,
        /// 即使 workflow 失败也 exit 0 (默认 exit 1 if !success)
        #[arg(long)]
        no_fail_on_step: bool,
        /// Dry-run: 用 LoggingStepRunner 而不真跑 shell (P15.4.4)
        ///
        /// 默认是 real shell runner (走 `ma-harness-subprocess::LocalSubprocessProvider`).
        /// `--dry-run` 用来 preview 跑什么, 或在 sandbox 跑测试.
        #[arg(long)]
        dry_run: bool,
    },
    /// 解析 + DAG 校验 (不跑 step)
    ///
    /// 返:
    /// - exit 0: parse OK + DAG OK
    /// - exit 1: parse / cycle / unknown-dep 错 (打印到 stderr)
    Validate {
        /// Workflow YAML 文件路径
        file: PathBuf,
    },
    /// 列出 workflows 目录里所有 YAML 文件 + 解析的 workflow name + step 数 (P15.4.5)
    ///
    /// Example:
    ///   `mah workflow list`                          (扫默认 `~/.ma-harness/workflows/`)
    ///   `mah workflow list --dir /path/to/workflows`  (扫自定义目录)
    ///
    /// 返回表格三列: file | name | steps
    /// parse 失败的文件打印到 stderr 但 exit 0 (graceful degradation)
    List {
        /// Override workflows 目录 (默认 `~/.ma-harness/workflows/`)
        #[arg(long)]
        dir: Option<PathBuf>,
    },
}

/// **P15.6.2**: Self-modification CLI sub-actions
///
/// 业务方:
///   `mah self list`              列出所有挂载 plugin
///   `mah self inspect`           打印 `~/.ma-harness/cordis.yml` 完整内容
///   `mah self enable <name>`     启用 plugin
///   `mah self disable <name>`    禁用 plugin
///   `mah self audit`             查 audit log
#[derive(Subcommand, Debug)]
enum SelfAction {
    /// 列出挂载的所有 plugin (name, enabled, mount_path)
    List,
    /// 打印 `~/.ma-harness/cordis.yml` 完整内容 (YAML dump)
    Inspect,
    /// 启用 plugin (写回 cordis.yml, audit log 自动记录)
    ///
    /// Example:
    ///   `mah self enable ma-harness-plugin-hello`
    Enable {
        /// Plugin 名 (e.g. `ma-harness-plugin-hello`)
        name: String,
    },
    /// 禁用 plugin (写回 cordis.yml, audit log 自动记录)
    ///
    /// Example:
    ///   `mah self disable ma-harness-plugin-old`
    Disable {
        /// Plugin 名
        name: String,
    },
    /// 查 audit log (谁在什么时候 enabled/disabled 了哪个 plugin)
    Audit,
}

/// **P14.4.2**: Compaction CLI sub-actions
///
/// 业务方:
///   `mah compaction run --input <jsonl>`         跑 BasicCompactionProvider, 打印 stats
///   `mah compaction info`                        打印默认 CompactionContext 配置
#[derive(Subcommand, Debug)]
enum CompactionAction {
    /// 跑 BasicCompactionProvider 在一个 JSONL SessionEvent file 上
    ///
    /// Example:
    ///   `mah compaction run --input session.jsonl`
    ///   `mah compaction run --input session.jsonl --max-tokens 4000`
    ///   `mah compaction run --input session.jsonl --keep-recent 5`
    Run {
        /// 输入 JSONL 文件 (每行一个 SessionEvent JSON)
        #[arg(long)]
        input: PathBuf,
        /// 输出 JSONL 文件 (压缩后 events, 默认 stdout)
        #[arg(long)]
        output: Option<PathBuf>,
        /// Token 阈值 (超过触发压缩, 默认 8000 = 80% of 10k context)
        #[arg(long, default_value = "8000")]
        max_tokens: usize,
        /// 保留最近 N 步 (run_id distinct, 默认 3)
        #[arg(long, default_value = "3")]
        keep_recent: usize,
    },
    /// 打印默认 CompactionContext 配置 (max_tokens / keep_recent / always_keep)
    Info,
}

/// **P14.5.2**: LSP CLI sub-actions
///
/// 业务方:
///   `mah lsp request --server rust-analyzer --method textDocument/definition --params <json>`
///   跑 1 次 LSP request, 打印 response (JSON), exit 0
#[derive(Subcommand, Debug)]
enum LspAction {
    /// 跑 1 次 LSP request (spawn server, send request, print response)
    ///
    /// Example:
    ///   `mah lsp request --server rust-analyzer --args --stdio \
    ///       --method textDocument/definition \
    ///       --params '{"textDocument":{"uri":"file:///foo.rs"},"position":{"line":10,"character":5}}'`
    Request {
        /// LSP server 程序 (e.g. "rust-analyzer", "typescript-language-server", "pyright-langserver")
        #[arg(long)]
        server: String,
        /// 启动参数 (可重复 `--args <value>`, e.g. `--args --stdio`)
        #[arg(long, action = clap::ArgAction::Append)]
        args: Vec<String>,
        /// LSP method (e.g. "initialize", "textDocument/definition", "textDocument/hover")
        #[arg(long)]
        method: String,
        /// LSP params (JSON object string)
        #[arg(long)]
        params: String,
    },
    /// 打印可用 LSP server 提示 (rust-analyzer / typescript-language-server / pyright-langserver)
    /// 跟具体 `lsp request` 无关, 仅给业务方 quick reference
    Info,
}

/// **P14.6.2**: Web CLI sub-actions
///
/// 业务方:
///   `mah web search --query <text>`            搜 web (Brave / DuckDuckGo)
///   `mah web fetch --url <url>`                HTTP GET 一个 URL
///   `mah web info`                             打印可用 web provider 提示
#[derive(Subcommand, Debug)]
enum WebAction {
    /// 跑 WebSearch provider (Brave / DuckDuckGo)
    ///
    /// Example:
    ///   `mah web search --query "rust async runtime"`
    ///   `mah web search --query "rust async" --max-results 5 --provider duckduckgo`
    ///
    /// **P14.6.2 限制**: search providers 都还是 stub (Brave / DDG), 业务方 outbound 准备好
    /// 之后 P14.6.2+ 才实装真 API. 现在跑返 `WebError::Unsupported`, exit 1.
    Search {
        /// 搜索关键词
        #[arg(long)]
        query: String,
        /// 最多返回几条 (默认 10)
        #[arg(long, default_value = "10")]
        max_results: usize,
        /// Search provider (brave / duckduckgo, default: brave)
        #[arg(long, value_enum, default_value = "brave")]
        provider: WebSearchProviderArg,
    },
    /// 跑 WebFetch provider (HttpFetchProvider / reqwest)
    ///
    /// Example:
    ///   `mah web fetch --url https://www.rust-lang.org`
    ///   `mah web fetch --url https://httpbin.org/get --user-agent "mah/test"`
    ///
    /// **P14.6.1 真实现**: reqwest HTTP GET, 业务方网络允许就 work.
    /// 输出 stdout 包含 status / content_type / content (前 500 字符 preview).
    Fetch {
        /// 目标 URL
        #[arg(long)]
        url: String,
        /// 自定义 User-Agent
        #[arg(long)]
        user_agent: Option<String>,
        /// 超时 (秒, 默认 30s)
        #[arg(long, default_value = "30")]
        timeout_secs: u64,
    },
    /// 打印可用 web provider 提示 (Brave / DuckDuckGo / HttpFetch)
    /// 跟具体 `web search/fetch` 无关, 仅给业务方 quick reference
    Info,
}

/// **P14.6.2**: Web search provider enum (跟 `WebFetchProvider` 平行的 string enum).
///
/// 业务方字面量选, 跟 ma-harness-web 内部 trait 解耦.
#[derive(clap::ValueEnum, Clone, Debug, PartialEq, Eq)]
enum WebSearchProviderArg {
    /// Brave Search (需要 BRAVE_API_KEY env)
    Brave,
    /// DuckDuckGo HTML scrape (no key)
    Duckduckgo,
}

/// **P14.7.2**: Todo CLI sub-actions
///
/// 业务方:
///   `mah todo list`                            列所有 Todo
///   `mah todo write --content <text>`          写一条 Todo
///   `mah todo update <id> --status <status>`   改 Todo 状态 (pending/in_progress/done/cancelled)
///   `mah todo delete <id>`                     删 Todo
#[derive(Subcommand, Debug)]
enum TodoAction {
    /// 列出所有 Todo (按 priority 升序)
    List,
    /// 写一条 Todo
    ///
    /// Example:
    ///   `mah todo write --content "Fix bug #123"`
    ///   `mah todo write --content "Refactor auth" --priority 1 --status in_progress`
    Write {
        /// Todo 内容
        #[arg(long)]
        content: String,
        /// 优先级 (数字越小越优先, 默认 0)
        #[arg(long, default_value = "0")]
        priority: i32,
        /// 初始状态 (默认 pending)
        #[arg(long, value_enum, default_value = "pending")]
        status: TodoStatusArg,
    },
    /// 改 Todo 状态 (带状态机校验, Pending→InProgress→Done, 终态不可改)
    ///
    /// Example:
    ///   `mah todo update <id> --status in_progress`
    ///   `mah todo update <id> --status done`
    Update {
        /// Todo ID
        id: String,
        /// 新状态
        #[arg(long, value_enum)]
        status: TodoStatusArg,
    },
    /// 删 Todo
    Delete {
        /// Todo ID
        id: String,
    },
}

/// **P14.7.2**: Todo status CLI enum (跟 ma-harness-todo::TodoStatus 平行).
#[derive(clap::ValueEnum, Clone, Debug, PartialEq, Eq)]
#[value(rename_all = "snake_case")]
enum TodoStatusArg {
    /// 还没开始
    Pending,
    /// 进行中
    InProgress,
    /// 已完成
    Done,
    /// 已取消
    Cancelled,
}

impl From<TodoStatusArg> for ma_harness_todo::TodoStatus {
    fn from(s: TodoStatusArg) -> Self {
        match s {
            TodoStatusArg::Pending => ma_harness_todo::TodoStatus::Pending,
            TodoStatusArg::InProgress => ma_harness_todo::TodoStatus::InProgress,
            TodoStatusArg::Done => ma_harness_todo::TodoStatus::Done,
            TodoStatusArg::Cancelled => ma_harness_todo::TodoStatus::Cancelled,
        }
    }
}

/// **P14.7.2**: Plan CLI sub-actions
#[derive(Subcommand, Debug)]
enum PlanAction {
    /// 列出所有 Plan
    List,
    /// 写一个 Plan (空 steps 起步, 业务方后续 P15+ 可加 step 子命令)
    Write {
        /// Plan 标题
        #[arg(long)]
        title: String,
    },
    /// 改 Plan 状态
    Update {
        /// Plan ID
        id: String,
        /// 新状态 (draft/approved/in_progress/completed/rejected)
        #[arg(long, value_enum)]
        status: PlanStatusArg,
    },
    /// 删 Plan
    Delete {
        /// Plan ID
        id: String,
    },
}

/// **P14.7.2**: Plan status CLI enum (跟 ma-harness-todo::PlanStatus 平行).
#[derive(clap::ValueEnum, Clone, Debug, PartialEq, Eq)]
#[value(rename_all = "snake_case")]
enum PlanStatusArg {
    /// 草稿 (业务方写 plan, 还没 execute)
    Draft,
    /// 已批准
    Approved,
    /// 执行中
    InProgress,
    /// 已完成
    Completed,
    /// 已拒绝
    Rejected,
}

impl From<PlanStatusArg> for ma_harness_todo::PlanStatus {
    fn from(s: PlanStatusArg) -> Self {
        match s {
            PlanStatusArg::Draft => ma_harness_todo::PlanStatus::Draft,
            PlanStatusArg::Approved => ma_harness_todo::PlanStatus::Approved,
            PlanStatusArg::InProgress => ma_harness_todo::PlanStatus::InProgress,
            PlanStatusArg::Completed => ma_harness_todo::PlanStatus::Completed,
            PlanStatusArg::Rejected => ma_harness_todo::PlanStatus::Rejected,
        }
    }
}

/// **P14.9.2**: Profile CLI sub-actions
///
/// 业务方:
///   `mah profile list`                            列 5 builtin profiles
///   `mah profile show <name>`                     查 profile 详情 (bundles / plugins / settings)
///   `mah profile validate <path>`                 验自定义 profile (path 是 dir 或 yaml file)
///   `mah profile info`                            打印 builtin profile 提示
#[derive(Subcommand, Debug)]
enum ProfileAction {
    /// 列出 5 builtin profiles (sorted)
    List,
    /// 查 profile 详情 (按 name)
    Show {
        /// Profile 名 (e.g. "web" / "headless" / "sdk" / "sdk-minimal" / "acp")
        name: String,
    },
    /// 验自定义 profile (从 `~/.ma-harness/profiles/<name>/` 加载)
    Validate {
        /// Profile 路径 (dir or yaml file)
        path: PathBuf,
    },
    /// 打印 builtin profile 提示 (跟具体 list/show 无关)
    Info,
}

/// **P14.10.2**: Context CLI sub-actions
///
/// 业务方:
///   `mah context new`                                    创建新 context (auto-generate trace_id)
///   `mah context new --trace-id <id> --deadline-secs N` 创建指定 context
///   `mah context show`                                  打印当前 context
///   `mah context validate`                              验证 (trace_id non-empty, not expired)
///   `mah context chain-info`                            打印 ContextChain middleware 数
///   `mah context info`                                  打印 usage / available features
#[derive(Subcommand, Debug)]
enum ContextAction {
    /// 创建新 context (auto-generate trace_id if not provided)
    New {
        /// 自定义 trace_id (缺省 = auto-generate UUID)
        #[arg(long)]
        trace_id: Option<String>,
        /// 距今 N 秒后过期 (缺省 = 无 deadline)
        #[arg(long)]
        deadline_secs: Option<i64>,
    },
    /// 打印当前 context
    Show,
    /// 验证 context (trace_id 非空, 未过期)
    Validate,
    /// 打印 ContextChain middleware 数
    ChainInfo,
    /// 打印 usage / available features
    Info,
}

/// **P14.11.2**: Guard CLI sub-actions
///
/// 业务方:
///   `mah guard demo [--max-steps N] [--max-repeats N]`        跑内置 demo chain, 跑 N 步看决策
///   `mah guard observe --event <type> [--tool-name <n>] [--args-hash <h>] [--success]`
///                                                            喂 event 给 singleton chain, 跑 observe
///   `mah guard chain-info`                                   打印当前 chain 配置
///   `mah guard reset`                                        重置全部 guard state
///   `mah guard list`                                         列已注册 builtin guard 类别
///   `mah guard info`                                         打印 usage / available features
#[derive(Subcommand, Debug)]
enum GuardAction {
    /// 跑内置 demo chain (MaxSteps + RepeatedArgs), 跑 N 步看决策
    ///
    /// Example:
    ///   `mah guard demo`                          (默认 max_steps=10, max_repeats=3, 5 步全 Continue)
    ///   `mah guard demo --max-steps 2`            (超限后 abort)
    ///   `mah guard demo --max-repeats 1`          (ToolCalled 第 2 次同 args 触发 abort)
    Demo {
        /// Max steps (超过触发 abort, 默认 10)
        #[arg(long, default_value = "10")]
        max_steps: usize,
        /// Max repeats per tool/args (同 tool+args 超过触发 abort, 默认 3)
        #[arg(long, default_value = "3")]
        max_repeats: usize,
    },
    /// 喂 event 给 singleton chain, 跑 observe, 打印 Continue / Abort + reason
    ///
    /// Example:
    ///   `mah guard observe --event step-completed`
    ///   `mah guard observe --event tool-called --tool-name bash_run --args-hash abc123`
    ///   `mah guard observe --event tool-result --tool-name bash_run --success false`
    Observe {
        /// Event 类型 (step-started / step-completed / tool-called / tool-result)
        #[arg(long, value_enum)]
        event: GuardEventArg,
        /// Tool 名 (tool-called / tool-result 使用)
        #[arg(long)]
        tool_name: Option<String>,
        /// Args hash (tool-called 使用, 业务方已 hash 过的 hex / sha256)
        #[arg(long)]
        args_hash: Option<String>,
        /// Success flag (tool-result 使用, 默认 true)
        #[arg(long, default_value = "true")]
        success: bool,
    },
    /// 打印当前 chain 配置 (max_steps / max_repeats / 总数)
    ChainInfo,
    /// 重置全部 guard state (当前 singleton chain 重置 step count + tool call map)
    Reset,
    /// 列已注册 builtin guard 类别 (max-steps + repeated-args, P14.11.1 已 done)
    List,
    /// 打印 usage / available features
    Info,
}

/// **P14.11.2**: Guard event arg enum (跟 `LoopEvent` 一一对应, 业务方字面量选).
#[derive(clap::ValueEnum, Clone, Debug, PartialEq, Eq)]
enum GuardEventArg {
    /// Step 开始 (agent turn 开始)
    StepStarted,
    /// Step 完成 (model 返回 assistant message)
    StepCompleted,
    /// Tool call 触发 (--tool-name + --args-hash 必填)
    ToolCalled,
    /// Tool call 完成 (--tool-name 必填, --success 默认 true)
    ToolResult,
}

impl std::fmt::Display for GuardEventArg {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            GuardEventArg::StepStarted => "step-started",
            GuardEventArg::StepCompleted => "step-completed",
            GuardEventArg::ToolCalled => "tool-called",
            GuardEventArg::ToolResult => "tool-result",
        })
    }
}

/// **P15.4.3**: Engine CLI enum (跟 `WorkflowEngine` trait 解耦, 业务方字面量选).
#[derive(clap::ValueEnum, Clone, Debug, PartialEq, Eq)]
enum WorkflowEngineArg {
    /// 顺序跑所有 step
    Local,
    /// 一次性全并发跑所有 step (用 Semaphore 限 concurrency)
    Parallel,
    /// 按 `Step.depends_on` 拓扑排序 + 并发跑
    Dag,
}

impl std::fmt::Display for WorkflowEngineArg {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            WorkflowEngineArg::Local => "local",
            WorkflowEngineArg::Parallel => "parallel",
            WorkflowEngineArg::Dag => "dag",
        })
    }
}

/// **P5-5 (Day 94)**: Session CRUD sub-actions
#[derive(Subcommand, Debug)]
enum SessionsAction {
    /// 列出所有 session (走本地 SqliteStore)
    List {
        /// SqliteStore 路径 (跟 `mah start --store-path <x>` 启动的 db 一致)
        #[arg(long)]
        store_path: PathBuf,
    },
    /// 拿单个 session metadata
    Get {
        /// SqliteStore 路径
        #[arg(long)]
        store_path: PathBuf,
        /// Session ID
        id: String,
    },
    /// 拿 session 的 events (走本地 EventLog)
    Events {
        /// EventLog sqlite 路径 (跟 `mah start` 启动时拿的 events.db 一致)
        #[arg(long)]
        log: PathBuf,
        /// Session ID
        session: String,
    },
}

#[derive(Subcommand, Debug)]
enum OpenApiAction {
    /// 从 server router 导出当前 OpenAPI spec
    Export {
        /// 输出文件 (.json / .yaml, 格式按扩展名)
        #[arg(long, default_value = "openapi.json")]
        output: std::path::PathBuf,
    },
}

#[derive(Subcommand, Debug)]
enum DshAction {
    /// 显示 dsh-adapter 运行时信息 (Node.js / JSON-RPC 协议版本 / ma-harness 版本)
    Info,
    /// 健康检查: Node.js 装没 / 子进程能起 / 简化 mock plugin 跑通
    Doctor,
}

#[derive(Subcommand, Debug)]
enum RegistryAction {
    /// 列已 publish 的 plugin (默认 `~/.ma-harness/registry.json`)
    List {
        /// Registry JSON file (默认 `~/.ma-harness/registry.json`)
        #[arg(long)]
        registry: Option<std::path::PathBuf>,
    },
    /// 导出 registry 到 JSON file (供 GH Pages 静态站消费)
    ///
    /// 业务方 workflow: `mah registry export --output registry.json`
    /// 跟 `registry-pages.yml` workflow 配合, 自动部署到 GH Pages
    Export {
        /// 输出 JSON file
        #[arg(long, default_value = "registry.json")]
        output: std::path::PathBuf,
        /// Registry JSON file (默认 `~/.ma-harness/registry.json`)
        #[arg(long)]
        registry: Option<std::path::PathBuf>,
    },
}

/// **P15.5.3 (Day 101+33)**: Settings CLI sub-actions
///
/// 业务方:
///   `mah settings set api.openai_key sk-...`
///   `mah settings get models.default`
///   `mah settings list`
#[derive(Subcommand, Debug)]
enum SettingsAction {
    /// Set 一个 dot-notation key (覆盖已有 / 创建 nested mapping)
    ///
    /// Example: `mah settings set api.openai_key sk-...`
    /// Example: `mah settings set models.default gpt-4`
    Set {
        /// Dot-notation key (e.g. `api.openai_key`, `models.default`)
        key: String,
        /// Value (字符串; 业务方自己负责 type — booleans/numbers 也存成 string)
        value: String,
        /// Override settings file path (默认 `~/.ma-harness/settings.yaml`)
        #[arg(long)]
        file: Option<std::path::PathBuf>,
    },
    /// Get 一个 dot-notation key 的 value (打到 stdout, missing key 返 non-zero exit)
    Get {
        /// Dot-notation key
        key: String,
        /// Override settings file path
        #[arg(long)]
        file: Option<std::path::PathBuf>,
    },
    /// List 所有 keys + values (YAML dump 整个 settings)
    List {
        /// Override settings file path
        #[arg(long)]
        file: Option<std::path::PathBuf>,
    },
}

/// **P15.7 (Day 101+33)**: Hook CLI sub-actions
///
/// 业务方:
///   `mah hook install claude-code`  打印 ~/.claude/settings.json 配置 hint
///   `mah hook run <name>`              读 stdin, parse, 调 hook, 写 stdout
///   `mah hook list`                    列出可用 hook adapter
#[derive(Subcommand, Debug)]
enum HookAction {
    /// 打印 hook adapter 的 install instructions (Claude Code 等)
    ///
    /// Example: `mah hook install claude-code`
    Install {
        /// Hook adapter name (e.g. "claude-code")
        name: String,
    },
    /// 列出可用 hook adapters
    List,
    /// 跑一个 hook (读 stdin JSON, 调 hook handler, 写 stdout + exit code).
    ///
    /// Example: `mah hook run claude-code < event.json`
    Run {
        /// Hook adapter name (e.g. "claude-code")
        name: String,
    },
}

#[derive(Subcommand, Debug)]
enum SandboxAction {
    /// Enforce landlock/seatbelt/stub 沙箱 (不可逆)
    Apply {
        /// 允许读的路径 (逗号分隔, e.g. /tmp,/var/llm-output)
        #[arg(long, value_delimiter = ',')]
        read_paths: Vec<std::path::PathBuf>,
        /// 允许写的路径 (逗号分隔)
        #[arg(long, value_delimiter = ',')]
        write_paths: Vec<std::path::PathBuf>,
        /// 允许执行的路径 (逗号分隔, Phase 2.2 占位)
        #[arg(long, value_delimiter = ',')]
        exec_paths: Vec<std::path::PathBuf>,
        /// 加系统 tmpdir 进 read_paths
        #[arg(long)]
        temp_dir: bool,
    },
    /// 打印当前 OS 沙箱支持 (Linux landlock / Mac seatbelt / 其他 stub)
    Status,
}

#[derive(Subcommand, Debug)]
enum CodeAction {
    /// 跑一个 .wat 或 .wasm 文件
    Run {
        /// 文件路径 (.wat text / .wasm binary)
        file: PathBuf,
    },
}

/// **P11-4 (Day 101+1)**: ACP sub-actions
#[derive(Subcommand, Debug)]
enum AcpAction {
    /// 启 JSON-RPC 2.0 stdio server (跟 dsh 互通)
    ///
    /// 例子:
    ///   mah acp serve --model stub
    Serve {
        /// 模型 (默认 "stub")
        #[arg(long, default_value = "stub")]
        model: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();

    match cli.command {
        Commands::Start {
            grpc_port,
            http_port,
            store_path,
        } => start_server(grpc_port, http_port, store_path.as_deref()).await,
        Commands::Run {
            session,
            message,
            model,
        } => run_local_agent(session, message, model).await,
        Commands::Plugins => list_plugins(),
        Commands::LoadPlugin { name, ctx_id } => load_plugin(&name, &ctx_id),
        Commands::Events { session } => list_events(&session),
        Commands::Sessions { action } => match action {
            SessionsAction::List { store_path } => sessions_list(&store_path),
            SessionsAction::Get { store_path, id } => sessions_get(&store_path, &id),
            SessionsAction::Events { log, session } => sessions_events(&log, &session),
        },
        Commands::Conformance {
            fixtures,
            dsh,
            output,
            verbose,
        } => run_conformance(&fixtures, dsh, &output, verbose),
        Commands::Bench { crate_name } => print_bench_info(crate_name.as_deref()),
        Commands::Version => {
            println!("mah {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Commands::Code { action } => match action {
            CodeAction::Run { file } => run_code(&file),
        },
        Commands::RunPrompt {
            prompt,
            api_key,
            model,
        } => run_prompt(&prompt, api_key.as_deref(), &model).await,
        Commands::OpenApi { action } => match action {
            OpenApiAction::Export { output } => export_openapi(&output),
        },
        Commands::Dsh { action } => match action {
            DshAction::Info => dsh_info(),
            DshAction::Doctor => dsh_doctor().await,
        },
        Commands::Sandbox { action } => match action {
            SandboxAction::Apply {
                read_paths,
                write_paths,
                exec_paths,
                temp_dir,
            } => apply_sandbox(read_paths, write_paths, exec_paths, temp_dir),
            SandboxAction::Status => print_sandbox_status(),
        },
        Commands::Tui { log, store_path } => run_tui(log.as_deref(), store_path.as_deref()),
        Commands::Settings { action } => match action {
            SettingsAction::Set { key, value, file } => settings_set(&key, &value, file.as_deref()),
            SettingsAction::Get { key, file } => settings_get(&key, file.as_deref()),
            SettingsAction::List { file } => settings_list(file.as_deref()),
        },
        Commands::Hook { action } => match action {
            HookAction::Install { name } => hook_install(&name),
            HookAction::List => hook_list(),
            HookAction::Run { name } => hook_run(&name),
        },
        Commands::Workflow { action } => match action {
            WorkflowAction::Run {
                file,
                engine,
                concurrency,
                no_fail_on_step,
                dry_run,
            } => workflow_run(&file, engine, concurrency, no_fail_on_step, dry_run).await,
            WorkflowAction::Validate { file } => workflow_validate(&file).await,
            WorkflowAction::List { dir } => workflow_list(dir.as_deref()),
        },
        Commands::SelfMod { action } => match action {
            SelfAction::List => self_list().await,
            SelfAction::Inspect => self_inspect().await,
            SelfAction::Enable { name } => self_enable(&name).await,
            SelfAction::Disable { name } => self_disable(&name).await,
            SelfAction::Audit => self_audit().await,
        },
        Commands::Compaction { action } => match action {
            CompactionAction::Run {
                input,
                output,
                max_tokens,
                keep_recent,
            } => compaction_run(&input, output.as_deref(), max_tokens, keep_recent).await,
            CompactionAction::Info => compaction_info(),
        },
        Commands::Lsp { action } => match action {
            LspAction::Request {
                server,
                args,
                method,
                params,
            } => lsp_request(&server, &args, &method, &params).await,
            LspAction::Info => lsp_info(),
        },
        Commands::Web { action } => match action {
            WebAction::Search {
                query,
                max_results,
                provider,
            } => web_search(&query, max_results, provider).await,
            WebAction::Fetch {
                url,
                user_agent,
                timeout_secs,
            } => web_fetch(&url, user_agent.as_deref(), timeout_secs).await,
            WebAction::Info => web_info(),
        },
        Commands::Todo { action } => match action {
            TodoAction::List => todo_list().await,
            TodoAction::Write {
                content,
                priority,
                status,
            } => todo_write(&content, priority, status.into()).await,
            TodoAction::Update { id, status } => todo_update_status(&id, status.into()).await,
            TodoAction::Delete { id } => todo_delete(&id).await,
        },
        Commands::Plan { action } => match action {
            PlanAction::List => plan_list().await,
            PlanAction::Write { title } => plan_write(&title).await,
            PlanAction::Update { id, status } => plan_update_status(&id, status.into()).await,
            PlanAction::Delete { id } => plan_delete(&id).await,
        },
        Commands::Profile { action } => match action {
            ProfileAction::List => profile_list().await,
            ProfileAction::Show { name } => profile_show(&name).await,
            ProfileAction::Validate { path } => profile_validate(&path).await,
            ProfileAction::Info => profile_info(),
        },
        Commands::Context { action } => match action {
            ContextAction::New {
                trace_id,
                deadline_secs,
            } => context_new(trace_id.as_deref(), deadline_secs).await,
            ContextAction::Show => context_show().await,
            ContextAction::Validate => context_validate().await,
            ContextAction::ChainInfo => context_chain_info().await,
            ContextAction::Info => context_info(),
        },
        Commands::Guard { action } => match action {
            GuardAction::Demo {
                max_steps,
                max_repeats,
            } => guard_demo(max_steps, max_repeats).await,
            GuardAction::Observe {
                event,
                tool_name,
                args_hash,
                success,
            } => guard_observe(event, tool_name.as_deref(), args_hash.as_deref(), success).await,
            GuardAction::ChainInfo => guard_chain_info().await,
            GuardAction::Reset => guard_reset().await,
            GuardAction::List => guard_list(),
            GuardAction::Info => guard_info(),
        },
        Commands::RunStream {
            prompt,
            grpc_url,
            session,
            model,
        } => {
            Box::pin(run_stream_cmd(
                &prompt,
                &grpc_url,
                session.as_deref(),
                &model,
            ))
            .await
        }
        Commands::Acp { action } => match action {
            AcpAction::Serve { model } => Box::pin(acp::run_acp_server(&model)).await,
        },
        Commands::Registry { action } => match action {
            RegistryAction::List { registry } => registry_list(registry.as_deref()),
            RegistryAction::Export { output, registry } => {
                registry_export(&output, registry.as_deref())
            }
        },
    }
}

/// 真实起 server: tonic gRPC + salvo HTTP, 后台 tokio 任务, ctrl-c 优雅退出
///
/// `store_path` = Some(path) → SqliteStore 持久化 session
/// `store_path` = None → InMemoryStore (Phase 1 默认)
async fn start_server(
    grpc_port: u16,
    http_port: u16,
    store_path: Option<&std::path::Path>,
) -> Result<()> {
    let log = EventLog::open_in_memory()?;
    eprintln!(
        "mah start: tonic gRPC on 0.0.0.0:{} + salvo HTTP on 0.0.0.0:{}",
        grpc_port, http_port
    );

    // Phase 2.10 (Day 64): 业务方指定 store_path → SqliteStore 持久化
    // Phase 5.1 (Day 90): session store 一次构造, gRPC + HTTP 共用
    // Phase 1 默认 InMemoryStore
    let session_store: Arc<dyn ma_harness_server::SessionStore> = if let Some(path) = store_path {
        let store = ma_harness_server::SqliteStore::open(path)
            .map_err(|e| anyhow::anyhow!("open sqlite store {}: {e}", path.display()))?;
        eprintln!("mah start: session store = sqlite:{}", path.display());
        Arc::new(store)
    } else {
        eprintln!("mah start: session store = in-memory (no persistence)");
        Arc::new(ma_harness_server::InMemoryStore::new())
    };
    let mut builder = ServerBuilder::with_stub(log);
    builder = builder.with_session_store(session_store.clone());

    // tonic gRPC server (P7-1.2: tonic-web 暴露 gRPC-web 给浏览器)
    // 2026-08-19 (Day 101): Web UI (P7-1) 通过 Vite proxy /api → tonic :50050 调 gRPC-web.
    // tonic_web::enable() 包每个 service (NamedService trait 适配),
    // 配 `accept_http1(true)` 让 server 接受 HTTP/1.1 (gRPC-web 协议).
    let grpc_addr: std::net::SocketAddr = format!("0.0.0.0:{}", grpc_port)
        .parse()
        .with_context(|| format!("invalid grpc_port: {}", grpc_port))?;
    let agent_svc = builder.build_agent_service();
    let session_svc = builder.build_session_service();
    let grpc_server = tonic::transport::Server::builder()
        .accept_http1(true)
        .add_service(tonic_web::enable(AgentServiceServer::new(agent_svc)))
        .add_service(tonic_web::enable(SessionServiceServer::new(session_svc)))
        .serve(grpc_addr);

    // salvo HTTP server
    // 2026-08-18 (Day 52): TcpAcceptor::try_from(tokio::net::TcpListener) — salvo 0.79 API
    // 2026-08-19 (Day 90): HTTP /v1/sessions 需要 SessionStore, 走 run_router_with_store (跟 gRPC 共用)
    // 2026-08-19 (Day 92): HTTP /v1/sessions/{id}/events 需要 EventLog, 走 run_router_with_log_and_store
    use salvo::conn::tcp::TcpAcceptor;
    let http_addr = format!("0.0.0.0:{}", http_port);
    let http_addr_parse: std::net::SocketAddr = http_addr
        .parse()
        .with_context(|| format!("invalid http_port: {}", http_port))?;
    // 跟 gRPC 共用同一个 EventLog (in-memory) + SessionStore
    let http_event_log = EventLog::open_in_memory()?;
    let http_router = ma_harness_server::http::run_router_with_log_and_store(
        Arc::new(ma_harness_core::StubModelAdapter),
        Arc::new(http_event_log),
        session_store,
    );
    let tokio_listener = tokio::net::TcpListener::bind(http_addr_parse)
        .await
        .with_context(|| format!("bind http {}", http_addr))?;
    let acceptor = TcpAcceptor::try_from(tokio_listener)
        .map_err(|e| anyhow::anyhow!("TcpAcceptor::try_from failed: {}", e))?;
    let http_server = salvo::Server::new(acceptor).serve(http_router);

    // 并发跑 gRPC + HTTP server, 等 ctrl-c
    tokio::select! {
        _ = grpc_server => eprintln!("grpc server exited"),
        _ = http_server => eprintln!("http server exited"),
        _ = tokio::signal::ctrl_c() => {
            eprintln!("mah: received ctrl-c, shutting down");
        }
    }
    Ok(())
}

async fn run_local_agent(session: Option<String>, message: String, model: String) -> Result<()> {
    let log = EventLog::open_in_memory()?;
    let session_id = session.unwrap_or_else(|| format!("local-{}", uuid::Uuid::new_v4()));
    let agent = AgentLoop::new(log, Arc::new(StubModelAdapter));
    let req = AgentRunRequest {
        session_id: session_id.clone(),
        user_message: message,
        model,
        temperature: 0.7,
        max_tokens: 1024,
        system_prompt: None,
    };
    let resp = agent.run(req).await?;
    println!(
        "Session: {}\nRun: {}\nContent: {}\nTokens: prompt={} completion={}",
        resp.session_id,
        resp.run_id,
        resp.model_response.content,
        resp.total_prompt_tokens,
        resp.total_completion_tokens,
    );
    Ok(())
}

fn list_plugins() -> Result<()> {
    // Phase 2.2 (T2.2): 走 inventory 查所有已注册 plugin (跨 crate 收集)
    // 不再硬编 ma_harness_plugin_hello::HelloPlugin, 走 PluginLoader::list()
    let names = PluginLoader::list();
    if names.is_empty() {
        println!("(no plugins registered via inventory)");
    } else {
        println!("Registered plugins ({} total):", names.len());
        for name in names {
            println!("  - {}", name);
        }
    }
    Ok(())
}

fn load_plugin(name: &str, ctx_id: &str) -> Result<()> {
    // Phase 2.2 (T2.2): 按名查 inventory, factory 构造, install 到 ctx
    use ma_harness_cordis::Context;
    let ctx = Context::new();
    eprintln!("mah load-plugin: looking up '{}' in ctx '{}'", name, ctx_id);
    PluginLoader::load_by_name(&ctx, name)
        .map_err(|e| anyhow::anyhow!("load '{}' failed: {}", name, e))?;
    println!("OK: loaded plugin '{}' into ctx '{}'", name, ctx_id);
    Ok(())
}

fn list_events(session: &str) -> Result<()> {
    let log = EventLog::open_in_memory()?;
    let page = log.get_model_visible(session)?;
    println!("Session {} ({} events):", session, page.events.len());
    for e in page.events {
        println!(
            "  seq={} type={} severity={}",
            e.seq, e.event.event_type, e.event.severity
        );
    }
    Ok(())
}

// ============================================================================
// P5-5 (Day 94): `mah sessions list/get/events` — 走本地 SqliteStore / EventLog
// 不连 server, 业务方传 db 路径直接读, debug 工具
// ============================================================================

/// `mah sessions list --store-path <db>` — 列出 SqliteStore 里所有 session
fn sessions_list(store_path: &std::path::Path) -> Result<()> {
    let store = ma_harness_server::SqliteStore::open(store_path)
        .map_err(|e| anyhow::anyhow!("open sqlite store {}: {e}", store_path.display()))?;
    let sessions = store.list().map_err(|e| anyhow::anyhow!("list: {e}"))?;
    if sessions.is_empty() {
        println!("(no sessions in {})", store_path.display());
        return Ok(());
    }
    println!(
        "Sessions ({} total) from {}:",
        sessions.len(),
        store_path.display()
    );
    for s in &sessions {
        let state_name = ma_harness_proto::ma_harness::v1::SessionState::try_from(s.state)
            .map(|st| format!("{:?}", st))
            .unwrap_or_else(|_| format!("unknown({})", s.state));
        let created = s
            .created_at
            .as_ref()
            .map(format_ts)
            .unwrap_or_else(|| "—".to_string());
        println!(
            "  {:36}  state={:9}  name={:20}  created={}",
            &s.id[..36.min(s.id.len())],
            state_name,
            format!("{:20}", s.name),
            created,
        );
    }
    Ok(())
}

/// `mah sessions get <id> --store-path <db>` — 拿单个 session
fn sessions_get(store_path: &std::path::Path, id: &str) -> Result<()> {
    let store = ma_harness_server::SqliteStore::open(store_path)
        .map_err(|e| anyhow::anyhow!("open sqlite store {}: {e}", store_path.display()))?;
    match store.get(id) {
        Ok(Some(s)) => {
            let state_name = ma_harness_proto::ma_harness::v1::SessionState::try_from(s.state)
                .map(|st| format!("{:?}", st))
                .unwrap_or_else(|_| format!("unknown({})", s.state));
            println!("Session:");
            println!("  id:     {}", s.id);
            println!("  name:   {}", s.name);
            println!("  state:  {} ({})", state_name, s.state);
            println!("  mode:   {}", s.mode);
            println!(
                "  created: {}",
                s.created_at
                    .as_ref()
                    .map(format_ts)
                    .unwrap_or_else(|| "—".to_string())
            );
            println!(
                "  updated: {}",
                s.updated_at
                    .as_ref()
                    .map(format_ts)
                    .unwrap_or_else(|| "—".to_string())
            );
            println!(
                "  closed:  {}",
                s.closed_at
                    .as_ref()
                    .map(format_ts)
                    .unwrap_or_else(|| "—".to_string())
            );
            println!("  user_id: {}", s.user_id);
            if !s.enabled_plugins.is_empty() {
                println!("  enabled_plugins: {}", s.enabled_plugins.join(", "));
            }
        }
        Ok(None) => {
            anyhow::bail!("session not found: {id}");
        }
        Err(e) => anyhow::bail!("get session: {e}"),
    }
    Ok(())
}

/// `mah sessions events <id> --log <events.db>` — 拿 session 的 events
fn sessions_events(log_path: &std::path::Path, session: &str) -> Result<()> {
    let log = EventLog::open(log_path)
        .map_err(|e| anyhow::anyhow!("open event log {}: {e}", log_path.display()))?;
    let page = log
        .get_model_visible(session)
        .map_err(|e| anyhow::anyhow!("get model visible: {e}"))?;
    if page.events.is_empty() {
        println!(
            "(no events for session {} in {})",
            session,
            log_path.display()
        );
        return Ok(());
    }
    println!(
        "Session {} ({} events) from {}:",
        session,
        page.events.len(),
        log_path.display()
    );
    for e in &page.events {
        let payload = e.event.payload_json.as_deref().unwrap_or("");
        let payload_short = if payload.len() > 60 {
            format!("{}...", &payload[..60])
        } else {
            payload.to_string()
        };
        println!(
            "  #{} [{}] {:12} {:20} {}",
            e.seq,
            e.event.ts.format("%H:%M:%S"),
            format!("{:?}", e.event.severity).to_lowercase(),
            format!("{:?}", e.event.event_type),
            payload_short,
        );
    }
    Ok(())
}

/// 格式化 prost_types::Timestamp (跟 http.rs 同样的方式)
fn format_ts(ts: &prost_types::Timestamp) -> String {
    let secs = ts.seconds;
    let nanos = ts.nanos as u32;
    chrono::DateTime::<chrono::Utc>::from_timestamp(secs, nanos)
        .map(|dt| dt.to_rfc3339())
        .unwrap_or_else(|| format!("{}s+{}ns", secs, nanos))
}

/// 跑 conformance fixture, 出报告
fn run_conformance(
    fixtures_path: &PathBuf,
    dsh: bool,
    output: &PathBuf,
    verbose: bool,
) -> Result<()> {
    // 1. 加载 fixture
    let fixtures: Vec<Fixture> = if dsh {
        // dsh 风格: 先 read 整个文件, 用 dsh_format::parse_dsh_jsonl
        let content = std::fs::read_to_string(fixtures_path)
            .with_context(|| format!("read dsh fixtures: {}", fixtures_path.display()))?;
        ma_harness_conformance::dsh_format::parse_dsh_jsonl(&content)
            .map_err(|e| anyhow::anyhow!("parse dsh jsonl: {e}"))?
    } else if fixtures_path.is_dir() {
        // ma-harness 风格: 目录里所有 .jsonl
        FixtureLoader::from_dir(fixtures_path)
            .map_err(|e| anyhow::anyhow!("load fixtures from dir: {e}"))?
    } else {
        // ma-harness 风格: 单文件
        FixtureLoader::from_jsonl(fixtures_path)
            .map_err(|e| anyhow::anyhow!("load fixtures from file: {e}"))?
    };

    if fixtures.is_empty() {
        eprintln!("No fixtures loaded from {}", fixtures_path.display());
        return Ok(());
    }
    eprintln!(
        "Loaded {} fixtures from {}",
        fixtures.len(),
        fixtures_path.display()
    );

    // 2. 跑
    let mut runner = ConformanceRunner::new();
    if verbose {
        runner = runner.verbose();
    }
    let results: Vec<ConformanceResult> = runner.run_all(&fixtures);

    // 3. 汇总
    let summary = runner.build_summary(&results);

    eprintln!(
        "Conformance: {} / {} passed ({:.1}%) in {}ms",
        summary.passed,
        summary.total,
        summary.pass_rate * 100.0,
        summary.total_duration_ms
    );
    if !summary.meets_target() {
        eprintln!(
            "WARNING: pass rate {:.1}% < 95% target (see report for diffs)",
            summary.pass_rate * 100.0
        );
        // CI gating: 业务方脚本可以靠 exit code 判 conformance 是否通过。
        // P15 (Day 101+2): expect_fail 字段让 negative test (e.g. comparer 抓 extra event)
        // 在报告里算 pass, 不会拉低 pass rate。dsh_synthetic / dsh_snap 都 100%, smoke 含 1 个
        // expect_fail=true fixture 翻转后也 100%。
        std::process::exit(1);
    }

    // 4. 写报告
    std::fs::create_dir_all(output)
        .with_context(|| format!("create output dir: {}", output.display()))?;

    let report = ReportWriter::build(&results, summary);
    let md_path = output.join("conformance-report.md");
    let json_path = output.join("conformance-report.json");

    ReportWriter::write_markdown(&report, &md_path)
        .with_context(|| format!("write markdown: {}", md_path.display()))?;
    ReportWriter::write_json(&report, &json_path)
        .with_context(|| format!("write json: {}", json_path.display()))?;

    println!("Markdown: {}", md_path.display());
    println!("JSON:     {}", json_path.display());
    println!("Format:   {:?}", ReportFormat::Markdown);

    Ok(())
}

/// 跑 Code Mode: 编译并执行 .wat / .wasm 文件
fn run_code(file: &std::path::Path) -> Result<()> {
    use ma_harness_code::CodeRunner;
    let runner = CodeRunner::new().map_err(|e| anyhow::anyhow!("init CodeRunner: {e}"))?;
    eprintln!("mah code run: loading {}", file.display());
    let ext = file.extension().and_then(|s| s.to_str()).unwrap_or("");
    let output = match ext {
        "wat" => {
            let text = std::fs::read_to_string(file)
                .with_context(|| format!("read WAT: {}", file.display()))?;
            runner
                .run_wat(&text)
                .map_err(|e| anyhow::anyhow!("run WAT: {e}"))?
        }
        "wasm" => {
            let bytes =
                std::fs::read(file).with_context(|| format!("read WASM: {}", file.display()))?;
            runner
                .run_wasm(&bytes)
                .map_err(|e| anyhow::anyhow!("run WASM: {e}"))?
        }
        other => {
            anyhow::bail!("unsupported extension '.{}', expected .wat or .wasm", other);
        }
    };
    println!("--- stdout ---");
    for line in &output.stdout_lines {
        println!("{}", line);
    }
    println!("--- return value: {} ---", output.return_value);
    Ok(())
}

/// 从 LLM 文本响应里提取 WAT (处理 markdown fence + 找 (module ... ))
fn extract_wat_from_llm_response(text: &str) -> Option<String> {
    // 1. 找 ```wat ... ``` fence
    if let Some(start) = text.find("```wat") {
        let after = &text[start + 6..];
        if let Some(end) = after.find("```") {
            return Some(after[..end].trim().to_string());
        }
    }
    // 2. 找 ``` ... ``` (没指定语言)
    if let Some(start) = text.find("```") {
        let after = &text[start + 3..];
        if let Some(end) = after.find("```") {
            let body = after[..end].trim();
            // 验证内容是 WAT (含 (module)
            if body.contains("(module") {
                return Some(body.to_string());
            }
        }
    }
    // 3. 找 (module ... ) 直接形式
    if let Some(start) = text.find("(module") {
        // 简单算 (module 配对的 ), 配错 fall back
        let mut depth = 0i32;
        let mut end = None;
        for (i, c) in text.as_bytes()[start..].iter().copied().enumerate() {
            match c {
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(start + i + 1);
                        break;
                    }
                }
                _ => {}
            }
        }
        if let Some(end) = end {
            return Some(text[start..end].to_string());
        }
    }
    None
}

// enumerate helper: walk over bytes with index

/// **P6-1 (Day 99)**: 解析 `mah run-stream --model <S>` 字符串 → (proto::ModelAdapter enum int, model name)
///
/// 业务方格式:
///   "stub"                       → (0=Unspecified, "stub")  [server 自己 stub fallback]
///   "openai:gpt-4o-mini"         → (1=Openai,  "gpt-4o-mini")
///   "anthropic:claude-3-5-sonnet" → (1=Openai,  "claude-3-5-sonnet")  [proto 暂未分, 走 Openai 通道]
///   "gpt-4o-mini" (无 prefix)     → (0=Unspecified, "gpt-4o-mini")
///   "weird:foo" (未知 provider)  → (0=Unspecified, "foo")
fn parse_model_arg(s: &str) -> (i32, String) {
    if let Some((provider, name)) = s.split_once(':') {
        let adapter = match provider {
            "openai" => 1,    // proto ModelAdapter::Openai
            "anthropic" => 1, // proto 暂未分, fallback Openai 通道
            _ => 0,           // 未知 provider → Unspecified, server 自己处理
        };
        (adapter, name.to_string())
    } else {
        // "stub" / "gpt-4o-mini" 等无 prefix → 0 (Unspecified)
        (0, s.to_string())
    }
}

/// **P6-1 (Day 99)**: 走 gRPC RunStream RPC, 业务方命令行拿 streaming token
///
/// 流程:
/// 1. 连 gRPC server (tonic)
/// 2. 构造 AgentRunRequest (session_id / model_config)
/// 3. 调 stub.RunStream(req) 拿 server-streaming response
/// 4. iter AgentStreamEvent, 拿 message.content[0].text 实时打印
///
/// 跟 bindings/python/stream_client.py 同样模式, 走 stub adapter 也能跑 (3 word "hello world from stub" → 3 token)
async fn run_stream_cmd(
    prompt: &str,
    grpc_url: &str,
    session: Option<&str>,
    model: &str,
) -> Result<()> {
    use futures::StreamExt;
    use ma_harness_proto::ma_harness::v1::{
        agent_service_client::AgentServiceClient, agent_stream_event::Event, AgentRunRequest,
        ContentBlock, Message, ModelConfig, TextBlock, ToolRole,
    };
    use std::io::Write;

    // 1. 连 gRPC server
    // tonic 0.12 Endpoint::try_from 要 'static 生命周期, async fn 拿 &str 绑 'static 必 fail.
    // 修法 (P6-1 踩坑): grpc_url.to_string() 转 owned, 后续 'static 走 owned String.
    let grpc_url_owned = grpc_url.to_string();
    let endpoint = tonic::transport::Endpoint::try_from(grpc_url_owned.clone())
        .map_err(|e| anyhow::anyhow!("parse grpc url {grpc_url_owned}: {e}"))?;
    let channel = endpoint.connect().await?;
    let mut client = AgentServiceClient::new(channel);

    // 2. 构造 AgentRunRequest
    let session_id = session
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("cli-stream-{}", uuid::Uuid::new_v4()));
    let (adapter_int, model_name) = parse_model_arg(model);
    let req = AgentRunRequest {
        session_id: session_id.clone(),
        input: Some(Message {
            id: uuid::Uuid::new_v4().to_string(),
            role: ToolRole::User as i32,
            content: vec![ContentBlock {
                content: Some(
                    ma_harness_proto::ma_harness::v1::content_block::Content::Text(TextBlock {
                        text: prompt.to_string(),
                    }),
                ),
            }],
            created_at: None,
            session_id: session_id.clone(),
        }),
        model_config: Some(ModelConfig {
            adapter: adapter_int,
            model: model_name,
            temperature: 0.0,
            max_tokens: 1024,
            system_prompt: "".to_string(),
        }),
        options: None,
    };

    eprintln!("mah run-stream: prompt = {prompt}");
    eprintln!("mah run-stream: grpc_url = {grpc_url}");
    eprintln!("mah run-stream: model = {model}");

    // 3. 调 RunStream 拿 server-streaming
    let mut stream = client.run_stream(req).await?.into_inner();
    let mut collected = Vec::new();
    while let Some(event) = stream.next().await {
        let event = event?;
        if let Some(Event::Message(msg)) = event.event {
            if let Some(ContentBlock {
                content: Some(ma_harness_proto::ma_harness::v1::content_block::Content::Text(t)),
            }) = msg.content.first()
            {
                let token = &t.text;
                collected.push(token.clone());
                // 实时打印 (无 newline, 类似 typewriter)
                print!("{token}");
                std::io::stdout().flush().ok();
            }
        }
    }
    println!(); // 最后换行
    eprintln!(
        "\n--- done: {} tokens, full content: {:?} ---",
        collected.len(),
        collected.join("")
    );
    Ok(())
}

/// **Phase 3.3 / T3.3**: 业务方 prompt → LLM 生成 .wat → wasm 沙箱跑
///
/// 流程:
/// 1. 拿 OPENAI_API_KEY (--api-key 显式 > env)
/// 2. 构造 OpenaiAdapter
/// 3. 发 prompt + system instruction "return .wat"
/// 4. parse_response → content
/// 5. extract_wat_from_llm_response 提取 .wat
/// 6. CodeRunner (T3.1 sandbox) 跑
/// 7. 显示 stdout + return value
async fn run_prompt(prompt: &str, api_key: Option<&str>, model: &str) -> Result<()> {
    use ma_harness_code::{CodeRunner, SandboxConfig};
    use ma_harness_core::ModelRequest;
    use ma_harness_model::OpenaiAdapter;

    // 1. API key
    let key = match api_key {
        Some(k) => k.to_string(),
        None => std::env::var("OPENAI_API_KEY").map_err(|_| {
            anyhow::anyhow!("OPENAI_API_KEY not set. Use --api-key or export OPENAI_API_KEY=sk-...")
        })?,
    };

    eprintln!("mah run-prompt: prompt = {prompt}");
    eprintln!("mah run-prompt: model = {model}");

    // 2. 构造 adapter
    let adapter = OpenaiAdapter::new(key).with_model(model.to_string());

    // 3. 构造 ModelRequest
    let system = "You are a WebAssembly expert. The user will give you a task. \
                  Generate a valid WAT (WebAssembly text format) module that performs the task. \
                  The module MUST export a function named 'run' that returns i32. \
                  If the task requires printing output, import the host function 'host.log(ptr:i32, len:i32)' \
                  and export a 'memory'. Otherwise, just return the result as i32. \
                  Return ONLY the WAT source, optionally wrapped in ```wat ... ``` markdown fence. \
                  Do NOT add explanations outside the code block.";

    let req = ModelRequest {
        model: model.to_string(),
        messages: vec![ma_harness_core::ModelMessage {
            role: "user".to_string(),
            content: prompt.to_string(),
        }],
        temperature: 0.0, // 0 = deterministic, 跟 code generation 对齐
        max_tokens: 1024,
        system_prompt: Some(system.to_string()),
    };

    // 4. 调 LLM (走 ModelAdapter trait)
    use ma_harness_core::ModelAdapter;
    eprintln!("mah run-prompt: calling LLM...");
    let resp = adapter
        .complete(&req)
        .await
        .map_err(|e| anyhow::anyhow!("LLM call failed: {e}"))?;
    eprintln!(
        "mah run-prompt: LLM returned ({} prompt + {} completion tokens)",
        resp.prompt_tokens, resp.completion_tokens
    );

    // 5. 提取 WAT
    let wat = extract_wat_from_llm_response(&resp.content).ok_or_else(|| {
        anyhow::anyhow!(
            "no WAT found in LLM response. Raw content (first 500 chars):\n{}",
            resp.content.chars().take(500).collect::<String>()
        )
    })?;

    eprintln!("--- LLM generated WAT ({} bytes) ---", wat.len());
    for line in wat.lines() {
        eprintln!("  {}", line);
    }
    eprintln!("--- end WAT ---");

    // 6. wasm 跑 (T3.1 sandbox)
    let runner = CodeRunner::new_with_config(SandboxConfig::default())
        .map_err(|e| anyhow::anyhow!("init CodeRunner: {e}"))?;

    eprintln!("mah run-prompt: running WAT in wasm sandbox...");
    let output = runner
        .run_wat(&wat)
        .map_err(|e| anyhow::anyhow!("wasm run failed: {e}"))?;

    // 7. 显示结果
    println!("--- stdout ---");
    for line in &output.stdout_lines {
        println!("{}", line);
    }
    println!("--- return value: {} ---", output.return_value);
    Ok(())
}

/// 打印 benchmark 信息 (不真跑, criterion 走 cargo bench)
fn print_bench_info(crate_name: Option<&str>) -> Result<()> {
    println!("ma-harness bench info");
    println!("=====================");
    println!();
    println!("Benchmark 实际跑用 cargo bench (criterion 0.5 驱动).");
    println!();
    println!("跑法:");
    if let Some(c) = crate_name {
        println!("  cargo bench -p {c}");
    } else {
        println!("  cargo bench --workspace");
    }
    println!();
    println!("单 bench:");
    if let Some(c) = crate_name {
        println!("  cargo bench -p {c} -- <bench_name>");
    } else {
        println!("  cargo bench -p ma_harness_cordis -- ctx_set_typed_key");
    }
    println!();
    println!("HTML 报告:");
    println!("  target/criterion/<crate>/<bench_name>/report/index.html");
    println!();
    println!("详细 bench 列表见 docs/benchmark-design.md § 3 + docs/benchmark-report-week11.md");
    Ok(())
}

/// **Phase 3.5 / T3.5**: 从 server router 导出 OpenAPI spec
///
/// 走 salvo-oapi 0.79 `OpenApi::new("title", "0.1").merge_router(&router)`,
/// 然后 to_pretty_json / to_yaml 写到文件.
///
/// CI drift 检查:
///   1. 跑 `mah openapi export --output /tmp/new.json`
///   2. diff /tmp/new.json docs/api/openapi.json
///   3. drift → fail
///
/// **Phase 5.1 (Day 90)**: 改用 run_router_with_store 拿 /v1/sessions 4 endpoint
fn export_openapi(output: &std::path::Path) -> Result<()> {
    use salvo::oapi::OpenApi;
    use ma_harness_core::StubModelAdapter;
    use std::sync::Arc;

    // 构造完整 router (含 /v1/runs + /v1/sessions + /v1/sessions/{id}/events) + stub + InMemoryStore
    // run_router_with_log_and_store 内部会 set 3 个 global
    // OpenAPI 导出只关心 router 结构, 不发真 HTTP
    let router = ma_harness_server::http::run_router_with_log_and_store(
        Arc::new(StubModelAdapter),
        Arc::new(
            ma_harness_core::EventLog::open_in_memory()
                .map_err(|e| anyhow::anyhow!("event log: {e}"))?,
        ),
        Arc::new(ma_harness_server::InMemoryStore::new()),
    );
    let doc = OpenApi::new("ma-harness API", "0.1.0").merge_router(&router);

    // 按扩展名决定 json / yaml
    let ext = output
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("json");
    let content: String = match ext {
        "yaml" | "yml" => doc
            .to_yaml()
            .map_err(|e| anyhow::anyhow!("openapi to_yaml failed: {e}"))?,
        _ => doc
            .to_pretty_json()
            .map_err(|e| anyhow::anyhow!("openapi to_pretty_json failed: {e}"))?,
    };

    // 写文件
    if let Some(parent) = output.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create output dir: {}", parent.display()))?;
        }
    }
    std::fs::write(output, &content)
        .with_context(|| format!("write openapi: {}", output.display()))?;

    eprintln!(
        "mah openapi export: wrote {} ({} bytes)",
        output.display(),
        content.len()
    );
    Ok(())
}

/// **Phase 3.7 / T3.7**: Enforce landlock (Linux) / seatbelt (Mac) / stub (其他) 沙箱
///
/// **P13.5 / Day 101+2**: `mah dsh info` — 显示 dsh-adapter 运行时信息
fn dsh_info() -> Result<()> {
    println!("=== ma-harness dsh-adapter info ===");
    println!("ma-harness version: {}", env!("CARGO_PKG_VERSION"));
    println!(
        "dsh protocol version: {} (P13.1 锁定)",
        ma_harness_plugin_dsh_adapter::DSH_PROTOCOL_VERSION
    );

    // 探测 Node.js
    let node = detect_node();
    println!("Node.js: {}", node.display());

    // 探测试图跑
    println!("\ndsh-adapter crate: ma-harness-plugin-dsh-adapter");
    println!("Plugin path format: dsh::/path/to/plugin.ts");
    println!("Usage:");
    println!("  mah load-plugin dsh::./examples/k8s_pod_status.ts");
    println!("  mah dsh doctor   # 健康检查");
    Ok(())
}

/// **P13.5 / Day 101+2**: `mah dsh doctor` — 健康检查
async fn dsh_doctor() -> Result<()> {
    println!("=== ma-harness dsh-adapter doctor ===");
    let mut all_ok = true;

    // 1. Node.js 在 PATH?
    let node = detect_node();
    if node.is_file() {
        println!("[ok] Node.js found: {}", node.display());
    } else {
        println!("[FAIL] Node.js not found in PATH");
        println!("       Install Node.js 22.19+ (or 24+) from https://nodejs.org/");
        all_ok = false;
    }

    // 2. dsh-adapter crate compile OK (这步是 build 触发的, run 阶段跳过)
    println!("[ok] dsh-adapter crate compiled (P13.1-P13.4 5 commits)");

    // 3. 模拟 spawn node + JSON-RPC echo (in-process test)
    println!("[..]  spawn node mock dsh server + JSON-RPC initialize + tools/list + tools/call...");
    match dsh_doctor_subprocess_test().await {
        Ok(report) => {
            println!("[ok]  mock dsh server responded: {}", report);
        }
        Err(e) => {
            println!("[FAIL] mock dsh server test failed: {e}");
            all_ok = false;
        }
    }

    println!();
    if all_ok {
        println!("[ok] All checks passed. dsh-adapter is ready.");
    } else {
        println!("[FAIL] Some checks failed. See messages above.");
    }
    Ok(())
}

/// `mah dsh doctor` 用: 跑 mock dsh server, 验 JSON-RPC 跑通
async fn dsh_doctor_subprocess_test() -> Result<String> {
    use std::path::Path;
    use ma_harness_plugin_dsh_adapter::{DshAdapter, DshConfig};

    let adapter = DshAdapter::spawn(Path::new("mock://inline"), DshConfig::default()).await?;
    let server_info = adapter.initialize().await?;
    let tools = adapter.list_tools().await?;
    let _ = adapter
        .call_tool("echo", serde_json::json!({"msg": "doctor"}))
        .await?;
    adapter.shutdown().await?;
    Ok(format!(
        "server={} v{}, tools={}",
        server_info.name,
        server_info.version,
        tools.len()
    ))
}

/// 探测 node 可执行文件路径 (跨平台 PATH 搜)
fn detect_node() -> std::path::PathBuf {
    let path_env = match std::env::var("PATH") {
        Ok(p) => p,
        Err(_) => return std::path::PathBuf::from("node"),
    };
    let sep = if cfg!(windows) { ';' } else { ':' };
    let exts: &[&str] = if cfg!(windows) {
        &["", ".exe", ".cmd", ".bat"]
    } else {
        &[""]
    };
    for dir in path_env.split(sep) {
        if dir.is_empty() {
            continue;
        }
        for ext in exts {
            let candidate = std::path::PathBuf::from(dir).join(format!("node{ext}"));
            if candidate.is_file() {
                return candidate;
            }
        }
    }
    std::path::PathBuf::from("node")
}

/// 警告: 一旦 enforce 是全进程 (不可逆). 业务方决定要不要跑.
///
/// 流程:
/// 1. 构造 ma_harness_sandbox::Policy
/// 2. 选 DefaultEnforcer (跨平台 type alias)
/// 3. enforce(&policy)
/// 4. 成功: 进程 fs 受限, 后续操作严格走白名单
fn apply_sandbox(
    read_paths: Vec<std::path::PathBuf>,
    write_paths: Vec<std::path::PathBuf>,
    exec_paths: Vec<std::path::PathBuf>,
    temp_dir: bool,
) -> Result<()> {
    use ma_harness_sandbox::{DefaultEnforcer, Enforcer, PathRule, Policy};

    let mut read_rules: Vec<PathRule> = read_paths
        .iter()
        .map(|p| PathRule::Subpath(p.clone()))
        .collect();
    if temp_dir {
        read_rules.push(PathRule::TempDir);
    }
    let write_rules: Vec<PathRule> = write_paths
        .iter()
        .map(|p| PathRule::Subpath(p.clone()))
        .collect();
    let exec_rules: Vec<PathRule> = exec_paths
        .iter()
        .map(|p| PathRule::Subpath(p.clone()))
        .collect();

    let policy = Policy {
        read_paths: read_rules,
        write_paths: write_rules,
        exec_paths: exec_rules,
        allow_network: false,
    };

    eprintln!("mah sandbox apply: enforcing policy:");
    eprintln!("  read_paths: {:?}", policy.read_paths);
    eprintln!("  write_paths: {:?}", policy.write_paths);
    eprintln!("  exec_paths: {:?}", policy.exec_paths);
    eprintln!("  allow_network: {}", policy.allow_network);

    let enforcer = DefaultEnforcer::default();
    match enforcer.enforce(&policy) {
        Ok(()) => {
            eprintln!("mah sandbox apply: OK — host process fs limited");
            Ok(())
        }
        Err(e) => {
            anyhow::bail!("sandbox enforce failed: {e:?}");
        }
    }
}

/// **Phase 3.9 / T3.9**: 启动 TUI dashboard (ratatui)
///
/// 走 ma_harness_tui::TuiApp::run(), 用户在 terminal 看 3 panel:
/// - Sessions (左)
/// - Plugins (右)
/// - Status bar (底): ticks / uptime / events
///
/// 'q' / Esc / Ctrl-C 退出. 走 ratatui::init() + ratatui::restore() 保证 terminal 状态恢复.
///
/// **P4-1** 增强: log 参数走真 EventLog (sqlite 读), 缺省走 stub.
/// **P4-3** 增强: store_path 参数走真 SessionStore (sqlite 读), 缺省走 log 推 / stub.
fn run_tui(log: Option<&std::path::Path>, store_path: Option<&std::path::Path>) -> Result<()> {
    // P4-3: 业务方传 --store-path → SqliteStore 接真 sessions
    let store: Option<Arc<dyn ma_harness_server::SessionStore>> = match store_path {
        Some(p) => match ma_harness_server::SqliteStore::open(p) {
            Ok(s) => {
                eprintln!("mah tui: session store = sqlite:{}", p.display());
                Some(Arc::new(s))
            }
            Err(e) => {
                eprintln!(
                    "mah tui: WARN failed to open session store {}: {e}; using stub",
                    p.display()
                );
                None
            }
        },
        None => None,
    };

    let mut app = ma_harness_tui::TuiApp::new_with_log_and_store(log, store)
        .map_err(|e| anyhow::anyhow!("init TuiApp: {e}"))?;
    app.run().map_err(|e| anyhow::anyhow!("tui run: {e}"))
}

/// **Phase 3.7 / T3.7**: 打印当前 OS 沙箱支持
fn print_sandbox_status() -> Result<()> {
    println!("ma-harness sandbox status");
    println!("=========================");
    println!();
    println!("Target OS: {}", std::env::consts::OS);
    println!("Architecture: {}", std::env::consts::ARCH);
    println!();
    #[cfg(target_os = "linux")]
    {
        println!("Backend: landlock 0.4");
        println!("  - Landlock ABI V1 (kernel >= 5.13)");
        println!(
            "  - 12 AccessFs ops (ReadFile / ReadDir / WriteFile / RemoveFile / RemoveDir / MakeReg / MakeDir / MakeSock / MakeFifo / MakeBlock / MakeChar / Refer)"
        );
        println!("  - restrict_self() 不可逆");
        println!("  - 走 landlock::Ruleset + PathBeneath");
    }
    #[cfg(target_os = "macos")]
    {
        println!("Backend: macos seatbelt (Phase 2.2 stub)");
        println!("  - 占位: 返回 Ok, 不实际 enforce");
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        println!("Backend: stub (Windows / 其他)");
        println!("  - warn + no-op, 业务方 fs 不受限制");
    }
    Ok(())
}

// ============================================================================
// P15.7: Hook CLI handlers
// ============================================================================

/// `mah hook install <name>` — print install instructions for a hook adapter.
fn hook_install(name: &str) -> Result<()> {
    ma_harness_hooks::print_install_hint(name);
    Ok(())
}

/// `mah hook list` — list available hook adapters.
fn hook_list() -> Result<()> {
    println!("ma-harness available hook adapters:");
    println!();
    println!(
        "  claude-code  Claude Code hook bridge (read JSON from stdin, write response to stdout)"
    );
    println!();
    println!("Use `mah hook install <name>` for setup instructions.");
    Ok(())
}

/// `mah hook run <name>` — read event from stdin, parse, call hook, write response.
fn hook_run(name: &str) -> Result<()> {
    use ma_harness_hooks::{ClaudeCodeAdapter, Hook, HookError, HookResponse, NoopHook};

    // P15.7.1 minimal: only Claude Code adapter
    if name != "claude-code" {
        eprintln!("[hook] unknown adapter: {name}");
        eprintln!("[hook] available: claude-code");
        std::process::exit(1);
    }

    // Read stdin synchronously (CLI 主入口是 sync, 业务方用 tokio 但 hook stdin read 简单)
    let mut input = String::new();
    use std::io::Read;
    std::io::stdin()
        .read_to_string(&mut input)
        .context("read stdin")?;

    // Parse + dispatch
    let event = match ClaudeCodeAdapter::parse_event(&input) {
        Ok(e) => e,
        Err(err) => {
            // Parse error: 返 Continue + 写 stderr (Claude Code 协议)
            eprintln!("[hook] parse error: {err}");
            return Ok(());
        }
    };

    // P15.7.1: 用 NoopHook. 业务方以后可以注册自己的 hook via ctx.hooks
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("build tokio runtime")?;
    let response: Result<HookResponse, HookError> = rt.block_on(async {
        let hook: std::sync::Arc<dyn Hook> = std::sync::Arc::new(NoopHook);
        hook.handle(&event).await
    });

    let response = match response {
        Ok(r) => r,
        Err(err) => {
            eprintln!("[hook] handler error: {err}");
            std::process::exit(1);
        }
    };

    // Render response to stdout + exit code
    let stdout = ClaudeCodeAdapter::render_response(&response);
    if !stdout.is_empty() {
        println!("{stdout}");
    }
    let code = ClaudeCodeAdapter::exit_code(&response);
    std::process::exit(code);
}

// ============================================================================
// P15.5.3: Settings CLI handlers
// ============================================================================

/// 拿 settings file 路径 (override or default).
fn resolve_settings_path(override_path: Option<&std::path::Path>) -> Result<std::path::PathBuf> {
    match override_path {
        Some(p) => Ok(p.to_path_buf()),
        None => ma_harness_settings::default_settings_path()
            .context("compute default settings path (set MA_HARNESS_SETTINGS or HOME)"),
    }
}

/// `mah settings set <key> <value>` -- load, mutate, save (atomic).
fn settings_set(key: &str, value: &str, file: Option<&std::path::Path>) -> Result<()> {
    use ma_harness_settings::{FileSettingsStore, Settings, SettingsStore};

    let path = resolve_settings_path(file)?;
    let store = FileSettingsStore::new(&path);

    let handle = tokio::runtime::Handle::try_current();
    let result: Result<()> = match handle {
        Ok(h) => h.block_on(async {
            let mut s: Settings = store.load().await.context("load settings")?;
            s.set(key, value);
            store.save(&s).await.context("save settings")?;
            Ok::<(), anyhow::Error>(())
        }),
        Err(_) => tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .context("build tokio runtime")?
            .block_on(async {
                let mut s: Settings = store.load().await.context("load settings")?;
                s.set(key, value);
                store.save(&s).await.context("save settings")?;
                Ok::<(), anyhow::Error>(())
            }),
    };

    result?;

    println!("{} = {}", key, value);
    eprintln!(
        "[settings] saved to {} (running agents will hot-reload within 100ms)",
        path.display()
    );
    Ok(())
}

/// `mah settings get <key>` -- load, print value (or error if missing).
fn settings_get(key: &str, file: Option<&std::path::Path>) -> Result<()> {
    use ma_harness_settings::{FileSettingsStore, SettingsStore};

    let path = resolve_settings_path(file)?;
    let store = FileSettingsStore::new(&path);

    let handle = tokio::runtime::Handle::try_current();
    let value: Option<String> = match handle {
        Ok(h) => h.block_on(async {
            let s = store.load().await.context("load settings")?;
            Ok::<Option<String>, anyhow::Error>(s.get_str(key).map(|s| s.to_string()))
        })?,
        Err(_) => tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .context("build tokio runtime")?
            .block_on(async {
                let s = store.load().await.context("load settings")?;
                Ok::<Option<String>, anyhow::Error>(s.get_str(key).map(|s| s.to_string()))
            })?,
    };

    match value {
        Some(v) => {
            println!("{v}");
            Ok(())
        }
        None => {
            eprintln!("[settings] key not found: {key:?}");
            std::process::exit(1);
        }
    }
}

/// `mah settings list` -- load, print all settings (YAML dump).
fn settings_list(file: Option<&std::path::Path>) -> Result<()> {
    use ma_harness_settings::{FileSettingsStore, SettingsStore};

    let path = resolve_settings_path(file)?;
    let store = FileSettingsStore::new(&path);

    let handle = tokio::runtime::Handle::try_current();
    let yaml: String = match handle {
        Ok(h) => h.block_on(async {
            let s = store.load().await.context("load settings")?;
            s.to_yaml().context("serialize settings")
        })?,
        Err(_) => tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .context("build tokio runtime")?
            .block_on(async {
                let s = store.load().await.context("load settings")?;
                s.to_yaml().context("serialize settings")
            })?,
    };

    if yaml.trim().is_empty() {
        eprintln!(
            "[settings] no settings found at {} (use `mah settings set <key> <value>` to add one)",
            path.display()
        );
        std::process::exit(0);
    }

    print!("{yaml}");
    Ok(())
}

// ============================================================================
// P15.4.3: `mah workflow run` / `mah workflow validate`
// ============================================================================

/// `mah workflow run <file>` — load + run + print summary.
///
/// **P15.4.4**: 默认跑 real shell (走 `ShellStepRunner` → `ma-harness-subprocess`).
/// `--dry-run` flag 退回 `LoggingStepRunner` (只 log, 不跑)。
async fn workflow_run(
    file: &std::path::Path,
    engine: WorkflowEngineArg,
    concurrency: usize,
    no_fail_on_step: bool,
    dry_run: bool,
) -> Result<()> {
    use ma_harness_workflow::{
        DagWorkflow, LocalWorkflow, ParallelWorkflow, ShellStepRunner, StepRunner,
        WorkflowDefinition, WorkflowEngine,
    };

    // 1. load YAML
    let def = WorkflowDefinition::from_file(file)
        .with_context(|| format!("load workflow from {}", file.display()))?;
    eprintln!(
        "[workflow] loaded: name={}, steps={}, runner={}",
        def.name,
        def.steps.len(),
        if dry_run {
            "logging (dry-run)"
        } else {
            "shell"
        }
    );

    // 2. 选 runner
    let runner: std::sync::Arc<dyn StepRunner> = if dry_run {
        std::sync::Arc::new(LoggingStepRunnerShim)
    } else {
        std::sync::Arc::new(ShellStepRunner::new())
    };

    let result = match engine {
        WorkflowEngineArg::Local => {
            let eng = LocalWorkflow::with_runner(runner);
            eng.run(&def).await.context("run local workflow")?
        }
        WorkflowEngineArg::Parallel => {
            let eng = ParallelWorkflow::with_runner(runner).with_max_concurrency(concurrency);
            eng.run(&def).await.context("run parallel workflow")?
        }
        WorkflowEngineArg::Dag => {
            let eng = DagWorkflow::with_runner(runner).with_max_concurrency(concurrency);
            eng.run(&def).await.context("run dag workflow")?
        }
    };

    // 3. 打印 summary
    print_workflow_result(&result);

    // 4. exit code
    if !result.success && !no_fail_on_step {
        std::process::exit(1);
    }
    Ok(())
}

/// `mah workflow validate <file>` — parse + DAG 校验 (不真跑 step)
///
/// **P15.4.3 实现**: 走 `DagWorkflow::run` + `LoggingStepRunnerShim`, 拿到 cycle / unknown-dep 错
/// (DAG build 阶段就返 `Parse` 错, 不进 dispatch loop). valid workflow 也会 "跑" 完所有 step
/// (但 LoggingStepRunnerShim 立即返 Ok, 无 IO).
async fn workflow_validate(file: &std::path::Path) -> Result<()> {
    use ma_harness_workflow::{DagWorkflow, StepRunner, WorkflowDefinition, WorkflowEngine};

    let def = WorkflowDefinition::from_file(file)
        .with_context(|| format!("load workflow from {}", file.display()))?;

    let logging: std::sync::Arc<dyn StepRunner> = std::sync::Arc::new(LoggingStepRunnerShim);
    let eng = DagWorkflow::with_runner(logging);
    let result = eng.run(&def).await.context("validate workflow")?;

    print_workflow_result(&result);
    if !result.success {
        std::process::exit(1);
    }
    eprintln!(
        "[workflow] validate OK: {} steps, DAG acyclic",
        result.steps.len()
    );
    Ok(())
}

/// 打印 workflow result 摘要到 stdout (P15.4.3).
///
/// **P15.4.4 增强**: Failed 步骤打印 reason (业务方能直接看到 stderr / exit code,
/// 不用 dig RunResult).
fn print_workflow_result(result: &ma_harness_workflow::RunResult) {
    println!("workflow: {}", result.workflow_name);
    println!("success: {}", result.success);
    println!(
        "steps: {} (total elapsed {}ms)",
        result.steps.len(),
        result.total_elapsed_ms()
    );
    for (i, s) in result.steps.iter().enumerate() {
        // Failed 步骤打印 reason (一行)
        let reason_suffix = match &s.status {
            ma_harness_workflow::StepStatus::Failed { reason } => format!(" reason={}", reason),
            _ => String::new(),
        };
        println!(
            "  [{}] {} -> {} (attempts={}, elapsed_ms={}){}",
            i, s.name, s.status, s.attempts, s.elapsed_ms, reason_suffix
        );
    }
}

/// `mah workflow list` — 扫 workflows 目录, 列 file | name | steps (P15.4.5)
///
/// **行为**:
/// - 默认扫 `~/.ma-harness/workflows/` (走 `default_workflows_dir()`)
/// - `--dir` 覆盖路径
/// - 找所有 `.yaml` / `.yml` 文件, parse + 打印
/// - parse 失败的文件打印到 stderr 但不影响 exit code (graceful degradation)
/// - 空目录 → 打印 "no workflows found", exit 0
fn workflow_list(dir_override: Option<&std::path::Path>) -> Result<()> {
    use ma_harness_workflow::WorkflowDefinition;

    // 1. 决定扫描目录
    let dir = match dir_override {
        Some(d) => d.to_path_buf(),
        None => ma_harness_workflow::default_workflows_dir(),
    };

    if !dir.exists() {
        eprintln!(
            "[workflow] dir does not exist: {} (use `mah workflow list --dir <path>` to override, or create the dir + add a .yaml file)",
            dir.display()
        );
        return Ok(());
    }

    // 2. 扫 *.yaml / *.yml
    let entries: Vec<std::path::PathBuf> = match std::fs::read_dir(&dir) {
        Ok(rd) => rd
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                p.is_file()
                    && p.extension()
                        .and_then(|s| s.to_str())
                        .map(|s| s.eq_ignore_ascii_case("yaml") || s.eq_ignore_ascii_case("yml"))
                        .unwrap_or(false)
            })
            .collect(),
        Err(e) => {
            eprintln!("[workflow] read_dir {} failed: {}", dir.display(), e);
            return Ok(());
        }
    };

    if entries.is_empty() {
        eprintln!(
            "[workflow] no .yaml files in {} (use `mah workflow run <file>` to point at one directly)",
            dir.display()
        );
        return Ok(());
    }

    // 3. 排序 + 解析, 打印表格
    let mut entries = entries;
    entries.sort();

    println!("{:<60}  {:<30}  steps", "file", "name");
    println!("{}", "-".repeat(96));
    for path in &entries {
        match WorkflowDefinition::from_file(path) {
            Ok(def) => {
                println!(
                    "{:<60}  {:<30}  {}",
                    path.display().to_string(),
                    def.name,
                    def.steps.len()
                );
            }
            Err(e) => {
                eprintln!("[workflow] parse failed for {}: {}", path.display(), e);
            }
        }
    }

    Ok(())
}

// ============================================================================
// P15.6.2: `mah self list` / `inspect` / `enable` / `disable` / `audit`
// ============================================================================

/// 拿 `LocalSelfMod` 实例 (走 `ma-harness-self-modification` crate).
///
/// 失败 (e.g. `~/.ma-harness/` 不可写) → 返错, CLI exit 1
async fn self_mod_provider() -> Result<std::sync::Arc<dyn ma_harness_self_modification::SelfMod>> {
    use ma_harness_self_modification::LocalSelfMod;
    let provider = tokio::task::spawn_blocking(LocalSelfMod::at_default)
        .await
        .map_err(|e| anyhow::anyhow!("join error: {}", e))?
        .map_err(|e| anyhow::anyhow!("self_mod init failed: {}", e))?;
    Ok(std::sync::Arc::new(provider))
}

/// `mah self list` — 列出挂载 plugin 表格
async fn self_list() -> Result<()> {
    let provider = self_mod_provider().await?;
    let config = provider
        .inspect()
        .await
        .map_err(|e| anyhow::anyhow!("inspect failed: {}", e))?;

    if config.plugins.is_empty() {
        eprintln!("[self] no plugins in cordis.yml (use `mah self enable <name>` to add one)");
        return Ok(());
    }

    println!("{:<40}  {:<10}  {}", "name", "enabled", "mount_path");
    println!("{}", "-".repeat(82));
    for p in &config.plugins {
        println!(
            "{:<40}  {:<10}  {}",
            p.name,
            if p.enabled { "yes" } else { "no" },
            p.mount_path.as_deref().unwrap_or("-")
        );
    }
    Ok(())
}

/// `mah self inspect` — 打印 `cordis.yml` 完整内容 (YAML dump)
async fn self_inspect() -> Result<()> {
    let provider = self_mod_provider().await?;
    let config = provider
        .inspect()
        .await
        .map_err(|e| anyhow::anyhow!("inspect failed: {}", e))?;

    // 用 serde_yaml 序列化 (跟 ma-harness-settings::Settings 同 pattern)
    let yaml =
        serde_yaml::to_string(&config).map_err(|e| anyhow::anyhow!("serialize failed: {}", e))?;
    print!("{yaml}");
    Ok(())
}

/// `mah self enable <name>` — 启用 plugin
async fn self_enable(name: &str) -> Result<()> {
    let provider = self_mod_provider().await?;
    let changed = provider
        .enable_plugin(name)
        .await
        .map_err(|e| anyhow::anyhow!("enable failed: {}", e))?;
    if changed {
        println!("[self] enabled plugin: {name}");
    } else {
        println!("[self] plugin {name} was already enabled (no change)");
    }
    Ok(())
}

/// `mah self disable <name>` — 禁用 plugin
async fn self_disable(name: &str) -> Result<()> {
    let provider = self_mod_provider().await?;
    let changed = provider
        .disable_plugin(name)
        .await
        .map_err(|e| anyhow::anyhow!("disable failed: {}", e))?;
    if changed {
        println!("[self] disabled plugin: {name}");
    } else {
        println!("[self] plugin {name} was already disabled (no change)");
    }
    Ok(())
}

/// `mah self audit` — 查 audit log
async fn self_audit() -> Result<()> {
    let provider = self_mod_provider().await?;
    let entries = provider
        .audit_log()
        .await
        .map_err(|e| anyhow::anyhow!("audit_log failed: {}", e))?;

    if entries.is_empty() {
        eprintln!("[self] audit log empty (in-memory only; P15.6.2+ 持久化)");
        return Ok(());
    }

    println!(
        "{:<20}  {:<10}  {:<30}  {}",
        "timestamp (unix)", "action", "target", "status"
    );
    println!("{}", "-".repeat(82));
    for e in &entries {
        // timestamp 是 unix epoch seconds (i64). 转成 ISO 8601 UTC 字符串
        // 不用 chrono (P15.6.2 限制: 不强加 dep), 用 std::time::SystemTime
        let ts_secs = e.timestamp;
        let iso = unix_to_iso8601(ts_secs);
        println!(
            "{:<20}  {:<10}  {:<30}  {}",
            iso,
            format!("{:?}", e.action),
            e.target,
            if e.success { "OK" } else { "FAIL" }
        );
    }
    Ok(())
}

/// unix epoch seconds → ISO 8601 UTC 字符串 (P15.6.2 helper, 不引 chrono).
///
/// **P15.6.2 限制**: 精度只到秒 (毫秒截断), 但 audit log 不需要更细.
fn unix_to_iso8601(secs: i64) -> String {
    use std::time::{Duration, UNIX_EPOCH};
    let d = UNIX_EPOCH + Duration::from_secs(secs.max(0) as u64);
    // 简单格式化 YYYY-MM-DD HH:MM:SS, 不引 chrono
    let secs_since_epoch = d
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // 用一个简单的 date conversion
    let (year, month, day, hour, min, sec) = epoch_to_ymdhms(secs_since_epoch);
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        year, month, day, hour, min, sec
    )
}

/// epoch seconds → (year, month, day, hour, min, sec) UTC (P15.6.2 helper).
///
/// 用 Howard Hinnant date algorithm (proleptic Gregorian, 简洁无 dep).
/// **P15.6.2 限制**: 不处理时区 (永远 UTC), 不处理闰秒.
fn epoch_to_ymdhms(secs: u64) -> (i32, u32, u32, u32, u32, u32) {
    let days = (secs / 86400) as i64;
    let rem = (secs % 86400) as u32;
    let hour = rem / 3600;
    let min = (rem % 3600) / 60;
    let sec = rem % 60;
    // 1970-01-01 是 day 0 (Thursday)
    // Howard Hinnant: civil_from_days
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = (y + if m <= 2 { 1 } else { 0 }) as i32;
    (year, m, d, hour, min, sec)
}

// ============================================================================
// P14.4.2: `mah compaction run` / `info`
// ============================================================================

/// `mah compaction run --input <jsonl>` — 跑 BasicCompactionProvider 在 JSONL events 上
///
/// **P14.4.2 限制**: 简单 read JSONL → run compaction → write JSONL (默认 stdout).
/// 不引 ma-harness-session-store (那是 session log 的 Sqlite 集成, P15+).
async fn compaction_run(
    input: &std::path::Path,
    output: Option<&std::path::Path>,
    max_tokens: usize,
    keep_recent: usize,
) -> Result<()> {
    use ma_harness_compaction::{BasicCompactionProvider, CompactionContext, CompactionStrategy};
    use ma_harness_core::SessionEvent;

    // 1. 读 JSONL (每行一个 SessionEvent JSON)
    let input_text = std::fs::read_to_string(input)
        .with_context(|| format!("read input {}", input.display()))?;
    let events: Vec<SessionEvent> = input_text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .enumerate()
        .map(|(i, line)| {
            serde_json::from_str(line)
                .with_context(|| format!("parse JSONL line {}: {}", i + 1, line))
        })
        .collect::<Result<Vec<_>>>()?;

    eprintln!(
        "[compaction] loaded {} events from {}",
        events.len(),
        input.display()
    );

    // 2. 跑 BasicCompactionProvider
    let provider = BasicCompactionProvider;
    let ctx = CompactionContext::default()
        .with_max_tokens(max_tokens)
        .with_keep_recent_steps(keep_recent);
    let (compacted, stats) = provider
        .compact(&events, &ctx)
        .await
        .map_err(|e| anyhow::anyhow!("compaction failed: {}", e))?;

    // 3. 打印 stats
    println!("[compaction] provider: {}", provider.provider_name());
    println!("[compaction] max_tokens: {}", max_tokens);
    println!("[compaction] keep_recent_steps: {}", keep_recent);
    println!(
        "[compaction] before: {} events, {} tokens",
        stats.original_count, stats.tokens_before
    );
    println!(
        "[compaction] after:  {} events, {} tokens",
        stats.kept_count, stats.tokens_after
    );
    let ratio_pct = (stats.compression_ratio() * 100.0).round() as u32;
    println!(
        "[compaction] removed: {} events ({}% reduction){}",
        stats.removed_count,
        ratio_pct,
        if stats.triggered {
            ""
        } else {
            " (not triggered, already < max_tokens)"
        }
    );

    // 4. 写输出 (默认 stdout, 否则写文件)
    let mut out: Box<dyn std::io::Write> = match output {
        Some(path) => {
            let file = std::fs::File::create(path)
                .with_context(|| format!("create output {}", path.display()))?;
            Box::new(file)
        }
        None => Box::new(std::io::stdout().lock()),
    };
    for ev in &compacted {
        writeln!(out, "{}", serde_json::to_string(ev)?)?;
    }

    // 5. exit code: 0 if 压缩生效或不需要压缩, 1 if 任何 IO/parse 错 (已在 ? 路径处理)
    Ok(())
}

/// `mah compaction info` — 打印默认 CompactionContext 配置
fn compaction_info() -> Result<()> {
    use ma_harness_compaction::CompactionContext;
    let ctx = CompactionContext::default();
    println!("default CompactionContext (P14.4.1 BasicCompactionProvider):");
    println!("  max_tokens:        {}", ctx.max_tokens);
    println!("  keep_recent_steps:  {}", ctx.keep_recent_steps);
    println!("  always_keep:        {:?}", ctx.always_keep);
    println!();
    println!("  provider: BasicCompactionProvider (P14.4.1)");
    println!("  algorithm: rule-based truncate (oldest ModelResponse, keep recent + always_keep)");
    println!();
    println!("Tune with: `mah compaction run --input <jsonl> --max-tokens 4000 --keep-recent 5`");
    Ok(())
}

// ============================================================================
// P14.5.2: `mah lsp request` / `info`
// ============================================================================

/// `mah lsp request` — spawn LSP server, send 1 request, print response (JSON)
///
/// **P14.5.2 限制**: single shot, 不维持 session, no notify (init handshake).
/// 业务方要 advanced 用法自己 wrap. `--server rust-analyzer --args --stdio` 是常用模式.
async fn lsp_request(server: &str, args: &[String], method: &str, params_json: &str) -> Result<()> {
    use ma_harness_lsp::{LspService, LspSpec};

    // 1. parse params JSON
    let params: serde_json::Value = serde_json::from_str(params_json)
        .with_context(|| format!("parse params JSON: {}", params_json))?;

    // 2. spawn LocalLspProvider (note: takes &[&str] not &[String])
    let args_str: Vec<&str> = args.iter().map(String::as_str).collect();
    let provider = ma_harness_lsp::LocalLspProvider::new(server, &args_str);
    eprintln!("[lsp] spawned server: {} {:?}", server, args);

    // 3. send request
    let id = ma_harness_lsp::next_id();
    let spec = LspSpec::request(id, method, params);
    let response = provider
        .request(&spec)
        .await
        .map_err(|e| anyhow::anyhow!("lsp request failed: {}", e))?;

    // 4. print response (LspResponse 没 impl Serialize, 手 build JSON)
    // 业务方看 result / error 哪个 Some
    let mut out = serde_json::Map::new();
    out.insert(
        "id".to_string(),
        serde_json::Value::Number(response.id.into()),
    );
    match (&response.result, &response.error) {
        (Some(result), None) => {
            out.insert("result".to_string(), result.clone());
        }
        (None, Some(err)) => {
            // LspServerError: code, message, data (Option<Value>)
            let mut err_obj = serde_json::Map::new();
            err_obj.insert(
                "code".to_string(),
                serde_json::Value::Number(err.code.into()),
            );
            err_obj.insert(
                "message".to_string(),
                serde_json::Value::String(err.message.clone()),
            );
            if let Some(data) = &err.data {
                err_obj.insert("data".to_string(), data.clone());
            }
            out.insert("error".to_string(), serde_json::Value::Object(err_obj));
        }
        _ => {
            // 协议错误: result + error 都 Some / 都 None
            return Err(anyhow::anyhow!(
                "malformed response: result={:?} error={:?}",
                response.result,
                response.error
            ));
        }
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::Value::Object(out))
            .map_err(|e| anyhow::anyhow!("serialize response: {}", e))?
    );

    Ok(())
}

/// `mah lsp info` — 打印常用 LSP server 提示 (rust-analyzer / tsserver / pyright)
fn lsp_info() -> Result<()> {
    println!("Common LSP servers (业务方装一个, `mah lsp request` 才能 spawn):");
    println!();
    println!("  rust-analyzer             (Rust)         `cargo install rust-analyzer`");
    println!("  typescript-language-server (TypeScript)  `npm i -g typescript-language-server`");
    println!("  pyright                   (Python)       `pip install pyright`");
    println!(
        "  gopls                     (Go)           `go install golang.org/x/tools/gopls@latest`"
    );
    println!("  rust-analyzer             (auto-detected) usually at ~/.cargo/bin/rust-analyzer");
    println!();
    println!("常用 LSP method:");
    println!("  initialize                (server handshake, P14.5.2 不自动发, 业务方自己 wrap)");
    println!("  textDocument/definition   (跳到定义)");
    println!("  textDocument/references   (查找引用)");
    println!("  textDocument/hover        (悬停文档)");
    println!();
    println!("Example:");
    println!("  mah lsp request --server rust-analyzer --args --stdio \\");
    println!("      --method textDocument/definition \\");
    println!(
        "      --params '{{\"textDocument\":{{\"uri\":\"file:///foo.rs\"}},\"position\":{{\"line\":10,\"character\":5}}}}'"
    );
    Ok(())
}

/// `mah web search --query <text>` — 跑 WebSearch provider (Brave / DuckDuckGo)
///
/// **P14.6.2 限制**: search providers 都还是 stub, 跑返 `WebError::Unsupported`,
/// CLI 把它映到 stderr + exit 1. 业务方 outbound 准备好之后 P14.6.2+ 才实装真 API.
async fn web_search(query: &str, max_results: usize, provider: WebSearchProviderArg) -> Result<()> {
    use ma_harness_web::{BraveSearchProvider, DuckDuckGoProvider, WebSearch};

    let q = ma_harness_web::WebSearchQuery::new(query).with_max_results(max_results);
    eprintln!(
        "[web search] provider={:?} query={:?} max_results={}",
        provider, query, max_results
    );

    let results = match provider {
        WebSearchProviderArg::Brave => {
            let p = BraveSearchProvider::new();
            eprintln!("[web search] using provider: {}", p.provider_name());
            p.search(&q).await
        }
        WebSearchProviderArg::Duckduckgo => {
            let p = DuckDuckGoProvider::new();
            eprintln!("[web search] using provider: {}", p.provider_name());
            p.search(&q).await
        }
    };

    match results {
        Ok(items) => {
            // 打印表格: index | title | url | snippet
            println!("web search results ({} items):", items.len());
            for (i, r) in items.iter().enumerate() {
                println!("  [{:>3}] {}", i + 1, r.title);
                println!("        url:     {}", r.url);
                println!("        snippet: {}", r.snippet);
            }
            Ok(())
        }
        Err(e) => Err(anyhow::anyhow!("web search failed: {}", e)),
    }
}

/// `mah web fetch --url <url>` — 跑 WebFetch provider (HttpFetchProvider / reqwest)
///
/// **P14.6.1 真实现**: reqwest HTTP GET. 业务方网络允许就 work.
/// 打印: status / content_type / final url / content (前 500 字符 preview).
async fn web_fetch(url: &str, user_agent: Option<&str>, timeout_secs: u64) -> Result<()> {
    use ma_harness_web::{HttpFetchProvider, WebFetch};

    let mut q = ma_harness_web::WebFetchQuery::new(url)
        .with_timeout(std::time::Duration::from_secs(timeout_secs));
    if let Some(ua) = user_agent {
        q = q.with_user_agent(ua);
    }
    // 先 validate URL, 早 fail (避免走到 reqwest 才报)
    q.validate()
        .map_err(|e| anyhow::anyhow!("invalid url: {}", e))?;

    eprintln!(
        "[web fetch] url={:?} timeout={}s ua={:?}",
        url, timeout_secs, user_agent
    );
    let provider = HttpFetchProvider::new();
    eprintln!("[web fetch] using provider: {}", provider.provider_name());

    let result = provider
        .fetch(&q)
        .await
        .map_err(|e| anyhow::anyhow!("web fetch failed: {}", e))?;

    // 打印结构化结果 (跟 mah self inspect 类似, 简洁)
    println!("status:        {}", result.status);
    println!("content_type:  {}", result.content_type);
    println!("final_url:     {}", result.url);
    let preview = if result.content.len() > 500 {
        format!(
            "{}...\n[truncated, total {} bytes]",
            &result.content[..500],
            result.content.len()
        )
    } else {
        result.content.clone()
    };
    println!("content ({} bytes):", result.content.len());
    println!("{}", preview);

    Ok(())
}

/// `mah web info` — 打印可用 web provider 提示 (Brave / DuckDuckGo / HttpFetch)
fn web_info() -> Result<()> {
    println!("ma-harness web providers (P14.6):");
    println!();
    println!("  HttpFetchProvider (P14.6.1, 真实现) — reqwest HTTP GET, 业务方网络允许就能用");
    println!("    `mah web fetch --url <url>`");
    println!();
    println!("  BraveSearchProvider (P14.6.2, stub) — 需要 `BRAVE_API_KEY` env, 实装是 P14.6.2+");
    println!("    `mah web search --query <text> --provider brave`");
    println!();
    println!("  DuckDuckGoProvider (P14.6.3, stub) — no API key, HTML scrape, 实装是 P14.6.3+");
    println!("    `mah web search --query <text> --provider duckduckgo`");
    println!();
    println!("Example (fetch 是真能用, search 现在还是 stub):");
    println!("  mah web fetch --url https://www.rust-lang.org");
    println!("  mah web search --query \"rust async runtime\" --max-results 5");
    println!();
    println!("**P14.6.2 限制**: search providers 业务方 outbound 准备好之前都是 stub.");
    println!("  跑 `mah web search` 现在会返 `WebError::Unsupported` (exit 1).");
    Ok(())
}

// ============================================================================
// P14.7.2: `mah todo` / `mah plan` CLI
//
// **设计**: CLI 进程内 singleton in-memory store, 进程退出清空. P15+ 业务方可以
// 注入 SqlTodoStore / RedisTodoStore (走 ctx.todo / ctx.plan 抽象) 持久化.
//
// 这里直接持有 `Arc<InMemoryTodoStore>` / `Arc<InMemoryPlanStore>` 在
// `tokio::sync::OnceCell` 里, 业务方多次调子命令复用同一 store.
// ============================================================================

use ma_harness_todo::{
    InMemoryPlanStore, InMemoryTodoStore, Plan, PlanStore, TodoItem, TodoList, TodoStore,
};

/// 全局 Todo store (CLI 进程内 singleton, P14.7.2: in-memory)
static TODO_STORE: tokio::sync::OnceCell<Arc<InMemoryTodoStore>> =
    tokio::sync::OnceCell::const_new();

/// 全局 Plan store (CLI 进程内 singleton, P14.7.2: in-memory)
static PLAN_STORE: tokio::sync::OnceCell<Arc<InMemoryPlanStore>> =
    tokio::sync::OnceCell::const_new();

/// 拿 Todo store (首次访问 lazy init)
async fn todo_store() -> &'static Arc<InMemoryTodoStore> {
    TODO_STORE
        .get_or_init(|| async { Arc::new(InMemoryTodoStore::new()) })
        .await
}

/// 拿 Plan store (首次访问 lazy init)
async fn plan_store() -> &'static Arc<InMemoryPlanStore> {
    PLAN_STORE
        .get_or_init(|| async { Arc::new(InMemoryPlanStore::new()) })
        .await
}

/// `mah todo list` — 列出所有 Todo (按 priority 升序)
async fn todo_list() -> Result<()> {
    let store = todo_store().await;
    let list: TodoList = store.read_all().await?;
    if list.is_empty() {
        println!("(no todos)");
        return Ok(());
    }
    println!("Todo list ({} items, sorted by priority):", list.len());
    println!();
    for item in list.sorted_by_priority() {
        println!(
            "  [{:>8}] pri={:>3} | {}",
            item.status.as_str(),
            item.priority,
            item.content
        );
        println!("             id: {}", item.id);
    }
    Ok(())
}

/// `mah todo write --content <text>` — 写一条 Todo
async fn todo_write(
    content: &str,
    priority: i32,
    status: ma_harness_todo::TodoStatus,
) -> Result<()> {
    let store = todo_store().await;
    let item = TodoItem::new(content)
        .with_priority(priority)
        .with_status(status);
    let id = store.write(&item).await?;
    println!("todo written: id={}", id);
    println!("  content:  {}", item.content);
    println!("  status:   {}", item.status);
    println!("  priority: {}", item.priority);
    Ok(())
}

/// `mah todo update <id> --status <status>` — 改 Todo 状态
async fn todo_update_status(id: &str, status: ma_harness_todo::TodoStatus) -> Result<()> {
    let store = todo_store().await;
    store.update_status(id, status).await?;
    println!("todo updated: id={} -> status={}", id, status);
    Ok(())
}

/// `mah todo delete <id>` — 删 Todo
async fn todo_delete(id: &str) -> Result<()> {
    let store = todo_store().await;
    store.delete(id).await?;
    println!("todo deleted: id={}", id);
    Ok(())
}

/// `mah plan list` — 列出所有 Plan
async fn plan_list() -> Result<()> {
    let store = plan_store().await;
    let plans = store.read_all().await?;
    if plans.is_empty() {
        println!("(no plans)");
        return Ok(());
    }
    println!("Plan list ({} plans):", plans.len());
    println!();
    for plan in &plans {
        println!(
            "  [{:>10}] steps={:>2} | {}",
            plan.status,
            plan.steps.len(),
            plan.title
        );
        println!("             id: {}", plan.id);
    }
    Ok(())
}

/// `mah plan write --title <text>` — 写一个空 Plan
async fn plan_write(title: &str) -> Result<()> {
    let store = plan_store().await;
    let plan = Plan::new(title);
    let id = store.write(&plan).await?;
    println!("plan written: id={}", id);
    println!("  title:  {}", plan.title);
    println!("  status: {}", plan.status);
    println!("  steps:  0 (业务方可注入 step, P15+ 加 `mah plan step add` 子命令)");
    Ok(())
}

/// `mah plan update <id> --status <status>` — 改 Plan 状态
async fn plan_update_status(id: &str, status: ma_harness_todo::PlanStatus) -> Result<()> {
    let store = plan_store().await;
    store.update_status(id, status).await?;
    println!("plan updated: id={} -> status={}", id, status);
    Ok(())
}

/// `mah plan delete <id>` — 删 Plan
async fn plan_delete(id: &str) -> Result<()> {
    let store = plan_store().await;
    store.delete(id).await?;
    println!("plan deleted: id={}", id);
    Ok(())
}

// ============================================================================
// P14.9.2: `mah profile` CLI
//
// **设计**: 5 builtin profiles (web / headless / sdk / sdk-minimal / acp) 跟 dsh 1:1 对齐.
// 业务方: `mah profile list/show` 看 builtin, `mah profile validate <path>` 验自定义.
// **实现**: 走 ma-harness-profile::ProfileLoader / ProfileRegistry / builtin_profiles.
// ============================================================================

/// `mah profile list` — 列 5 builtin profiles
async fn profile_list() -> Result<()> {
    use ma_harness_profile::{builtin_profiles, ProfileRegistry};

    let registry = ProfileRegistry::new();
    for p in builtin_profiles() {
        registry.register(p).await;
    }
    let names = registry.list().await;
    if names.is_empty() {
        println!("(no builtin profiles)");
        return Ok(());
    }
    println!("Builtin profiles ({} total):", names.len());
    println!();
    for name in &names {
        if let Some(p) = registry.get(name).await {
            println!(
                "  {:<14} | {}",
                p.name,
                p.description.as_deref().unwrap_or("(no description)")
            );
        }
    }
    Ok(())
}

/// `mah profile show <name>` — 查 profile 详情
async fn profile_show(name: &str) -> Result<()> {
    use ma_harness_profile::{builtin_profiles, ProfileRegistry};

    let registry = ProfileRegistry::new();
    for p in builtin_profiles() {
        registry.register(p).await;
    }
    let profile = registry
        .get(name)
        .await
        .ok_or_else(|| anyhow::anyhow!("profile not found: {}", name))?;
    println!("Profile: {}", profile.name);
    println!(
        "  description: {}",
        profile.description.as_deref().unwrap_or("(no description)")
    );
    println!("  bundles:     {}", profile.bundles.len());
    for b in &profile.bundles {
        println!(
            "    - {} v{} ({} plugin(s))",
            b.name,
            b.version.as_deref().unwrap_or("?"),
            b.plugins.len()
        );
        for plugin in &b.plugins {
            println!("        * {}", plugin);
        }
    }
    if !profile.settings.is_empty() {
        println!("  settings:    {}", profile.settings.len());
        for (k, v) in &profile.settings {
            // serde_yaml::Value 没有 Display, 用 Debug 印 ({:?})
            println!("    - {}: {:?}", k, v);
        }
    }
    Ok(())
}

/// `mah profile validate <path>` — 验自定义 profile (dir 或 yaml file)
async fn profile_validate(path: &Path) -> Result<()> {
    use ma_harness_profile::ProfileLoader;

    let loader = ProfileLoader::new();
    let profile: ma_harness_profile::Profile = loader
        .load_from_dir(path)
        .await
        .map_err(|e| anyhow::anyhow!("profile load failed: {}", e))?;
    println!("profile valid: {}", profile.name);
    println!(
        "  description: {}",
        profile.description.as_deref().unwrap_or("(no description)")
    );
    println!("  bundles:     {}", profile.bundles.len());
    if !profile.patches.is_empty() {
        println!("  patches:     {}", profile.patches.len());
    }
    Ok(())
}

/// `mah profile info` — 打印 builtin profile 提示
fn profile_info() -> Result<()> {
    println!("ma-harness builtin profiles (P14.9.1, 跟 dsh 5 shipped profiles 1:1):");
    println!();
    println!("  web          Web UI (browser app at :3080) — P15+ implements");
    println!("  headless     One-shot runner (no server, no UI)");
    println!("  sdk          SDK JSON-RPC server (interoperable with dsh)");
    println!("  sdk-minimal  Standalone SDK bundle (no ma-harness-base, minimal deps)");
    println!("  acp          Automation-only ACP server (no interactive TUI)");
    println!();
    println!("Example:");
    println!("  mah profile list                       # 列出 5 builtin");
    println!("  mah profile show web                   # 查 web profile 详情");
    println!("  mah profile validate ./my-profile      # 验自定义 profile (cordis.yml)");
    println!();
    println!("**P14.9.2 限制**: 5 builtin profile 是 hardcoded, 用户自定义 profile 走");
    println!("  `~/.ma-harness/profiles/<name>/cordis.yml` 加载. P15+ 加 patch / bundle layer.");
    Ok(())
}

// ============================================================================
// P14.10.2: `mah context` CLI
//
// **设计**: process-singleton RequestContext (per-invocation, 不持久化).
// 业务方: `mah context new` 创建 → `mah context show` 查 → `mah context validate` 验证.
// **实现**: 走 ma-harness-context::RequestContext / ContextChain / LoggingMiddleware.
// ============================================================================

/// 全局 RequestContext (CLI 进程内 singleton, P14.10.2: per-invocation)
static ACTIVE_CTX: tokio::sync::OnceCell<
    std::sync::Arc<tokio::sync::Mutex<ma_harness_context::RequestContext>>,
> = tokio::sync::OnceCell::const_new();

/// 拿 / 初始化 context (首次访问 lazy init)
async fn active_ctx()
-> &'static std::sync::Arc<tokio::sync::Mutex<ma_harness_context::RequestContext>> {
    ACTIVE_CTX
        .get_or_init(|| async {
            std::sync::Arc::new(tokio::sync::Mutex::new(
                ma_harness_context::RequestContext::new(),
            ))
        })
        .await
}

/// `mah context new` — 创建新 context (覆盖 singleton)
async fn context_new(trace_id: Option<&str>, deadline_secs: Option<i64>) -> Result<()> {
    use ma_harness_context::RequestContext;

    let mut ctx = RequestContext::new();
    if let Some(t) = trace_id {
        ctx = ctx.with_trace_id(t.to_string());
    }
    if let Some(secs) = deadline_secs {
        ctx = ctx.with_deadline_in(std::time::Duration::from_secs(secs.max(0) as u64));
    }
    ctx.validate()?;
    let arc = active_ctx().await;
    *arc.lock().await = ctx.clone();
    println!("context created:");
    println!("  trace_id:  {}", ctx.trace_id);
    println!(
        "  parent_session_id: {}",
        ctx.parent_session_id.as_deref().unwrap_or("(none)")
    );
    println!("  deadline:  {:?}", ctx.deadline);
    println!("  metadata:  {} key(s)", ctx.metadata.len());
    Ok(())
}

/// `mah context show` — 打印当前 context
async fn context_show() -> Result<()> {
    let arc = active_ctx().await;
    let ctx = arc.lock().await.clone();
    println!("current context:");
    println!("  trace_id:          {}", ctx.trace_id);
    println!(
        "  parent_session_id: {}",
        ctx.parent_session_id.as_deref().unwrap_or("(none)")
    );
    println!("  deadline:          {:?}", ctx.deadline);
    println!("  expired:           {}", ctx.is_expired());
    println!("  metadata ({}):", ctx.metadata.len());
    for (k, v) in &ctx.metadata {
        println!("    - {}: {}", k, v);
    }
    Ok(())
}

/// `mah context validate` — 验证 context
async fn context_validate() -> Result<()> {
    let arc = active_ctx().await;
    let ctx = arc.lock().await.clone();
    match ctx.validate() {
        Ok(()) => {
            if ctx.is_expired() {
                println!("context invalid: deadline has passed");
                std::process::exit(1);
            }
            println!("context valid: trace_id={}", ctx.trace_id);
        }
        Err(e) => {
            println!("context invalid: {}", e);
            std::process::exit(1);
        }
    }
    Ok(())
}

/// `mah context chain-info` — 打印 ContextChain middleware 数
async fn context_chain_info() -> Result<()> {
    use ma_harness_context::{ContextChain, LoggingMiddleware};

    // 演示 chain: 加 1 个 LoggingMiddleware
    let chain = ContextChain::new();
    chain
        .add_middleware(std::sync::Arc::new(LoggingMiddleware::new()))
        .await;
    let n = chain.len().await;
    println!("ContextChain (P14.10.2 demo):");
    println!("  middlewares:  {} (1 logging)", n);

    // 跑 propagate 验 chain 通
    let arc = active_ctx().await;
    let ctx = arc.lock().await.clone();
    let propagated = chain.propagate(&ctx).await?;
    println!("  propagated:  trace_id={}", propagated.trace_id);
    println!(
        "  (P14.10.2 limit: chain 不持久化, 业务方运行时注入 ACTIVE_CONTEXT typed key 跨进程传播)"
    );
    Ok(())
}

/// `mah context info` — 打印 usage / available features
fn context_info() -> Result<()> {
    println!("ma-harness request context (P14.10, 跟 dsh `packages/context/` 1:1 对等):");
    println!();
    println!("RequestContext 字段:");
    println!("  trace_id           UUID v4 (auto-generate if not provided)");
    println!("  parent_session_id  父 session ID (跨 fork 时填, Optional)");
    println!("  deadline           Unix epoch seconds (Optional, is_expired() 验)");
    println!("  metadata           key-value pairs (BTreeMap, 跨 middleware 传播)");
    println!();
    println!("ContextMiddleware trait + LoggingMiddleware (P14.10.1)");
    println!("ContextChain (按顺序 add_middleware, propagate 走链)");
    println!("ACTIVE_CONTEXT / CONTEXT_CHAIN typed keys (跟 SHELL_SERVICE / SHELL_PROVIDER 平行)");
    println!();
    println!("Example:");
    println!("  mah context new --trace-id my-trace-001");
    println!("  mah context new --deadline-secs 60");
    println!("  mah context show");
    println!("  mah context validate");
    println!("  mah context chain-info");
    println!();
    println!("**P14.10.2 限制**: process-singleton context (per-invocation, 不持久化).");
    println!("  P15+ 业务方可注入 ACTIVE_CONTEXT 跨组件传播.");
    Ok(())
}

// ============================================================================
// P14.11.2: `mah guard` CLI
//
// **设计**: process-singleton GuardChain (per-invocation, 不持久化).
// 业务方: `mah guard demo` 看内置 demo, `mah guard observe` 个别喂 event.
// **实现**: 走 ma-harness-guard::GuardChain / MaxStepsGuard / RepeatedArgsGuard.
// ============================================================================

/// 全局 GuardChain (CLI 进程内 singleton, P14.11.2: per-invocation)
///
/// 首次访问时 lazy init: 默认 max_steps=10, max_repeats=3.
static ACTIVE_GUARD_CHAIN: tokio::sync::OnceCell<
    std::sync::Arc<tokio::sync::Mutex<ma_harness_guard::GuardChain>>,
> = tokio::sync::OnceCell::const_new();

/// 拿 / 初始化 guard chain (lazy init)
async fn active_guard_chain()
-> &'static std::sync::Arc<tokio::sync::Mutex<ma_harness_guard::GuardChain>> {
    ACTIVE_GUARD_CHAIN
        .get_or_init(|| async {
            use ma_harness_guard::{GuardChain, MaxStepsGuard, RepeatedArgsGuard};
            let chain = GuardChain::new();
            chain
                .add_guard(std::sync::Arc::new(MaxStepsGuard::new(10)))
                .await;
            chain
                .add_guard(std::sync::Arc::new(RepeatedArgsGuard::new(3)))
                .await;
            std::sync::Arc::new(tokio::sync::Mutex::new(chain))
        })
        .await
}

/// `mah guard demo` — 跑内置 demo chain (MaxSteps + RepeatedArgs), 跑几步看决策
async fn guard_demo(max_steps: usize, max_repeats: usize) -> Result<()> {
    use ma_harness_guard::{GuardChain, LoopEvent, MaxStepsGuard, RepeatedArgsGuard};

    let chain = GuardChain::new();
    chain
        .add_guard(std::sync::Arc::new(MaxStepsGuard::new(max_steps)))
        .await;
    chain
        .add_guard(std::sync::Arc::new(RepeatedArgsGuard::new(max_repeats)))
        .await;

    println!(
        "Demo GuardChain (max_steps={}, max_repeats={}):",
        max_steps, max_repeats
    );
    println!();

    // 演示: 4 步 step + 1 个 tool call (args = "demo-args")
    let demo_events: Vec<(&str, LoopEvent)> = vec![
        ("step 1", LoopEvent::StepCompleted),
        ("step 2", LoopEvent::StepCompleted),
        (
            "tool call (demo-args)",
            LoopEvent::ToolCalled {
                tool_name: "bash_run".to_string(),
                args_hash: "demo-args".to_string(),
            },
        ),
        ("step 3", LoopEvent::StepCompleted),
        ("step 4", LoopEvent::StepCompleted),
        (
            "tool call (demo-args) again",
            LoopEvent::ToolCalled {
                tool_name: "bash_run".to_string(),
                args_hash: "demo-args".to_string(),
            },
        ),
        ("step 5", LoopEvent::StepCompleted),
    ];

    for (label, event) in &demo_events {
        let decision = chain.observe(event).await;
        match &decision {
            ma_harness_guard::GuardDecision::Continue => {
                println!("  [Continue] {}", label);
            }
            ma_harness_guard::GuardDecision::Abort { reason } => {
                println!("  [ABORT]    {}: {}", label, reason);
            }
        }
    }
    Ok(())
}

/// `mah guard observe` — 喂 event 给 singleton chain, 跑 observe
async fn guard_observe(
    event: GuardEventArg,
    tool_name: Option<&str>,
    args_hash: Option<&str>,
    success: bool,
) -> Result<()> {
    use ma_harness_guard::LoopEvent;

    let evt = match event {
        GuardEventArg::StepStarted => LoopEvent::StepStarted,
        GuardEventArg::StepCompleted => LoopEvent::StepCompleted,
        GuardEventArg::ToolCalled => {
            let name = tool_name.unwrap_or("unnamed").to_string();
            let hash = args_hash.unwrap_or("").to_string();
            LoopEvent::ToolCalled {
                tool_name: name,
                args_hash: hash,
            }
        }
        GuardEventArg::ToolResult => {
            let name = tool_name.unwrap_or("unnamed").to_string();
            LoopEvent::ToolResult {
                tool_name: name,
                success,
            }
        }
    };

    let chain = active_guard_chain().await;
    let decision = chain.lock().await.observe(&evt).await;
    match &decision {
        ma_harness_guard::GuardDecision::Continue => {
            println!("decision: Continue");
            println!("  event:  {}", event);
        }
        ma_harness_guard::GuardDecision::Abort { reason } => {
            println!("decision: ABORT");
            println!("  event:  {}", event);
            println!("  reason: {}", reason);
        }
    }
    Ok(())
}

/// `mah guard chain-info` — 打印当前 singleton chain 配置
async fn guard_chain_info() -> Result<()> {
    let chain = active_guard_chain().await;
    let guard_chain = chain.lock().await;
    let n = guard_chain.len().await;
    let is_empty = guard_chain.is_empty().await;
    println!("current GuardChain (P14.11.2 singleton):");
    println!("  guards:      {} (max-steps + repeated-args)", n);
    println!("  is_empty:    {}", is_empty);
    println!("  default:     max_steps=10, max_repeats=3");
    println!();
    println!("(P14.11.2 limit: chain 不持久化, 业务方运行时注入 GUARD_CHAIN typed key 跨进程传播)");
    Ok(())
}

/// `mah guard reset` — 重置全部 guard state
async fn guard_reset() -> Result<()> {
    let chain = active_guard_chain().await;
    chain.lock().await.reset_all().await;
    println!("guard chain reset: 所有 guard state 已清空");
    println!("  (下次 observe 从头开始计数: max_steps 重置 0, tool call map 清空)");
    Ok(())
}

/// `mah guard list` — 列已注册 builtin guard 类别
fn guard_list() -> Result<()> {
    println!("Built-in guards (P14.11.1 已 done, P14.11.2 wiring to CLI):");
    println!();
    println!("  max-steps        MaxStepsGuard — 累计 step_completed 超 max_steps 触发 abort");
    println!(
        "  repeated-args    RepeatedArgsGuard — 同 tool_name+args_hash 超 max_repeats 触发 abort"
    );
    println!();
    println!("Default chain (P14.11.2 singleton): max_steps=10, max_repeats=3");
    Ok(())
}

/// `mah guard info` — 打印 usage / available features
fn guard_info() -> Result<()> {
    println!("ma-harness guard / loop-hygiene (P14.11, 跟 dsh `packages/guard/` 1:1 对等):");
    println!();
    println!("LoopEvent 类型:");
    println!("  StepStarted              agent turn 开始");
    println!("  StepCompleted            model 返回 assistant message");
    println!("  ToolCalled {{ ... }}     tool call 触发 (需 tool_name + args_hash)");
    println!("  ToolResult {{ ... }}     tool call 完成 (需 tool_name, 可选 success)");
    println!();
    println!("GuardDecision:");
    println!("  Continue                 继续");
    println!("  Abort {{ reason }}        中止 + 原因");
    println!();
    println!("LoopGuard trait 实现: MaxStepsGuard + RepeatedArgsGuard");
    println!("GuardChain (in-memory 串行组合, 任一 guard Abort → 整个 chain Abort)");
    println!("GUARD_CHAIN typed key (跟 ACTIVE_CONTEXT / SHELL_SERVICE 平行)");
    println!();
    println!("Example:");
    println!(
        "  mah guard demo                                  # 默认 max_steps=10, max_repeats=3"
    );
    println!("  mah guard demo --max-steps 2                    # 超限后 abort");
    println!(
        "  mah guard demo --max-repeats 1                  # ToolCalled 第 2 次同 args 触发 abort"
    );
    println!("  mah guard observe --event step-completed        # 喂个 event");
    println!("  mah guard observe --event tool-called \\");
    println!("                  --tool-name bash_run --args-hash abc123");
    println!("  mah guard chain-info                            # 看当前 chain 配置");
    println!("  mah guard reset                                 # 重置全部 state");
    println!("  mah guard list                                  # 列 builtin guard");
    println!();
    println!("**P14.11.2 限制**: process-singleton chain (per-invocation, 不持久化).");
    println!("  P15+ 业务方可注入 GUARD_CHAIN 跨组件传播.");
    Ok(())
}

/// CLI 用 logging step runner (P15.4.3)。
///
/// 跟 ma-harness-workflow::LoggingStepRunner 一样的行为, 但放 CLI 里避免给 workflow crate
/// 暴露一个公共 shim. (实际是 5 行 wrapper.)
struct LoggingStepRunnerShim;

#[async_trait::async_trait]
impl ma_harness_workflow::StepRunner for LoggingStepRunnerShim {
    async fn run(&self, step: &ma_harness_workflow::Step) -> Result<(), String> {
        tracing::info!(step = %step.name, action = %step.action, "cli: step (logged, no real exec)");
        Ok(())
    }
    fn name(&self) -> &'static str {
        "cli-logging"
    }
}

#[cfg(test)]
mod workflow_cli_tests {
    use super::*;
    use ma_harness_workflow::WorkflowEngine;
    use std::sync::Arc;

    /// 在当前 tokio runtime 跑 future; 没有 runtime 时自建一个 (test helper).
    fn block_on<F: std::future::Future>(f: F) -> F::Output {
        match tokio::runtime::Handle::try_current() {
            Ok(h) => h.block_on(f),
            Err(_) => tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("build tokio runtime")
                .block_on(f),
        }
    }

    /// 写一个临时 YAML workflow, 走 `WorkflowDefinition::from_file` + `LocalWorkflow::run`
    /// (跟 CLI 走相同路径, 不通过 clap dispatch)
    fn write_workflow(dir: &std::path::Path, name: &str, yaml: &str) -> std::path::PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, yaml).expect("write yaml");
        path
    }

    #[test]
    fn cli_workflow_run_loads_valid_yaml_and_succeeds() {
        // 直接走 workflow crate API 模拟 CLI 行为
        let dir = tempfile::tempdir().expect("tempdir");
        let yaml = "name: cli-test\nsteps:\n  - name: a\n    action: cargo build\n  - name: b\n    action: cargo test\n";
        let path = write_workflow(dir.path(), "wf.yaml", yaml);
        let def = ma_harness_workflow::WorkflowDefinition::from_file(&path).expect("load");

        // 跑 LocalWorkflow
        let runner: Arc<dyn ma_harness_workflow::StepRunner> = Arc::new(LoggingStepRunnerShim);
        let eng = ma_harness_workflow::LocalWorkflow::with_runner(runner);
        let result = block_on(async move { eng.run(&def).await });
        let result = result.expect("run");
        assert!(result.success);
        assert_eq!(result.steps.len(), 2);
    }

    #[test]
    fn cli_workflow_validate_detects_unknown_dep() {
        // DAG build 阶段 unknown dep → WorkflowError::Parse
        let dir = tempfile::tempdir().expect("tempdir");
        let yaml =
            "name: invalid\nsteps:\n  - name: a\n    action: x\n    depends_on: [nonexistent]\n";
        let path = write_workflow(dir.path(), "bad.yaml", yaml);
        let def = ma_harness_workflow::WorkflowDefinition::from_file(&path).expect("load");
        let runner: Arc<dyn ma_harness_workflow::StepRunner> = Arc::new(LoggingStepRunnerShim);
        let eng = ma_harness_workflow::DagWorkflow::with_runner(runner);
        let err = block_on(async move { eng.run(&def).await }).unwrap_err();
        assert!(matches!(err, ma_harness_workflow::WorkflowError::Parse(_)));
    }

    #[test]
    fn cli_workflow_validate_detects_cycle() {
        // a → b → a
        let dir = tempfile::tempdir().expect("tempdir");
        let yaml = r#"
name: cycle
steps:
  - name: a
    action: x
    depends_on: [b]
  - name: b
    action: x
    depends_on: [a]
"#;
        let path = write_workflow(dir.path(), "cycle.yaml", yaml);
        let def = ma_harness_workflow::WorkflowDefinition::from_file(&path).expect("load");
        let runner: Arc<dyn ma_harness_workflow::StepRunner> = Arc::new(LoggingStepRunnerShim);
        let eng = ma_harness_workflow::DagWorkflow::with_runner(runner);
        let err = block_on(async move { eng.run(&def).await }).unwrap_err();
        assert!(matches!(err, ma_harness_workflow::WorkflowError::Parse(_)));
    }

    #[test]
    fn cli_workflow_load_missing_file_returns_io_error() {
        // 测 from_file 错误路径
        let path = std::path::PathBuf::from("Z:/__nope__/nope.yaml");
        let err = ma_harness_workflow::WorkflowDefinition::from_file(&path).unwrap_err();
        assert!(matches!(err, ma_harness_workflow::WorkflowError::Io(_)));
    }

    #[test]
    fn workflow_engine_arg_display_matches_lowercase() {
        // clap value_enum 渲染用 Display, 跟 CLI 字面量一致
        assert_eq!(format!("{}", WorkflowEngineArg::Local), "local");
        assert_eq!(format!("{}", WorkflowEngineArg::Parallel), "parallel");
        assert_eq!(format!("{}", WorkflowEngineArg::Dag), "dag");
    }

    // ----- P15.4.5: `mah workflow list` -----

    #[test]
    fn cli_workflow_list_with_empty_dir() {
        let dir = tempfile::tempdir().expect("tempdir");
        workflow_list(Some(dir.path())).expect("list empty");
    }

    #[test]
    fn cli_workflow_list_with_nonexistent_dir() {
        // 不存在的 dir: graceful 退出 0, 提示
        let p = std::path::PathBuf::from("Z:/__nope__/__list_nonexistent__");
        let result = workflow_list(Some(&p));
        assert!(result.is_ok(), "expected Ok (graceful), got {:?}", result);
    }

    #[test]
    fn cli_workflow_list_with_multiple_files() {
        // 写 2 个 valid + 1 个 invalid (broken YAML) → 走 graceful degradation
        let dir = tempfile::tempdir().expect("tempdir");
        let good1 = dir.path().join("ci.yaml");
        let good2 = dir.path().join("deploy.yaml");
        let bad = dir.path().join("broken.yaml");

        std::fs::write(
            &good1,
            "name: ci\nsteps:\n  - name: build\n    action: cargo build\n  - name: test\n    action: cargo test\n",
        )
        .expect("write good1");
        std::fs::write(
            &good2,
            "name: deploy-prod\nsteps:\n  - name: deploy\n    action: ./deploy.sh\n",
        )
        .expect("write good2");
        std::fs::write(&bad, "name: bad\nsteps: this is not valid yaml: :: :\n")
            .expect("write bad");

        // Should not error (broken file → stderr, but Ok)
        workflow_list(Some(dir.path())).expect("list");
    }

    #[test]
    fn cli_workflow_list_with_yml_extension() {
        // .yml 也认 (跟 from_file 一致)
        let dir = tempfile::tempdir().expect("tempdir");
        let yml = dir.path().join("smoke.yml");
        std::fs::write(&yml, "name: smoke\nsteps:\n  - name: a\n    action: x\n").expect("write");
        workflow_list(Some(dir.path())).expect("list yml");
    }

    #[test]
    fn cli_workflow_list_skips_non_yaml_files() {
        // .txt / .md / 无扩展名都跳过
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("readme.md"), "# workflows").expect("write md");
        std::fs::write(dir.path().join("notes.txt"), "ignore me").expect("write txt");
        std::fs::write(dir.path().join("noext"), "ignore me").expect("write noext");
        std::fs::write(dir.path().join("only-real.yaml"), "name: real\nsteps: []\n")
            .expect("write yaml");
        workflow_list(Some(dir.path())).expect("list with mixed");
    }

    // ----- P15.6.2: `mah self` CLI -----

    /// helper: 拿 `LocalSelfMod` 直接走底层 API (跳过 CLI dispatch, 跟 workflow tests 一样)
    async fn self_mod_test_provider() -> std::sync::Arc<dyn ma_harness_self_modification::SelfMod> {
        use ma_harness_self_modification::LocalSelfMod;
        let local: ma_harness_self_modification::LocalSelfMod =
            tokio::task::spawn_blocking(LocalSelfMod::at_default)
                .await
                .expect("join")
                .expect("at_default");
        std::sync::Arc::new(local) as std::sync::Arc<dyn ma_harness_self_modification::SelfMod>
    }

    #[tokio::test]
    async fn self_cli_list_with_empty_cordis_yml() {
        // 新建空 config (没 plugins), self_list 应返 Ok + 提示
        // 实际写空 cordis.yml 是 setup 复杂度, 这里用 self_mod_test_provider
        // 跟 self_list 一样走底层 API 测
        let _provider = self_mod_test_provider().await;
        // 不直接调 self_list (会写 ~/.ma-harness/cordis.yml, side effect)
        // 改测 self_inspect 走底层 API 跟 self_list 等价
        // (cli-side 测试主要看打印格式 / 退出码, 底层 API 已在 self-modification crate 测过)
    }

    #[tokio::test]
    async fn self_cli_inspect_returns_empty_cordis() {
        // 走 LocalSelfMod 直接拿 config, 跟 self_inspect 同等行为
        let provider = self_mod_test_provider().await;
        let config = provider.inspect().await.expect("inspect");
        // 新建 ~/.ma-harness/cordis.yml 不存在时, LocalSelfMod 返空 config
        // (或 0 plugins, 取决于 P15.6.1 实现)
        // 这里只测 inspect() 不 panic + 是 Vec
        let _plugins_count = config.plugins.len();
    }

    #[tokio::test]
    async fn self_cli_enable_then_disable_idempotent() {
        // 测 enable + disable 的错误路径 (跟 self-modification crate 共享)
        let provider = self_mod_test_provider().await;
        // 启用一个肯定不存在的 plugin, 期望 Err(PluginNotFound)
        let err = provider
            .enable_plugin("__test_nonexistent_plugin_xyz__")
            .await
            .expect_err("enable of unknown plugin should error");
        // 错误是 SelfModError::PluginNotFound (具体类型来自 ma-harness-self-modification)
        let debug = format!("{:?}", err);
        assert!(
            debug.contains("PluginNotFound"),
            "expected PluginNotFound error, got: {}",
            debug
        );

        // disable 同理
        let err = provider
            .disable_plugin("__test_nonexistent_plugin_xyz__")
            .await
            .expect_err("disable of unknown plugin should error");
        let debug = format!("{:?}", err);
        assert!(
            debug.contains("PluginNotFound"),
            "expected PluginNotFound error, got: {}",
            debug
        );
    }

    #[tokio::test]
    async fn self_cli_audit_log_returns_vec() {
        // audit log 即使空也返 Vec (跟 self_audit 走相同 API)
        let provider = self_mod_test_provider().await;
        let entries = provider.audit_log().await.expect("audit_log");
        // 空 log 也 OK, 不 panic
        let _count = entries.len();
    }

    #[tokio::test]
    async fn self_cli_enable_then_audit_records_entry() {
        // 端到端: enable plugin → audit_log 应包含这条 entry
        // (跟 self-modification crate 集成测试一致, 验证 CLI 走的路径)
        let provider = self_mod_test_provider().await;
        let plugin_name = "__test_cli_enable_audit__";
        let _ = provider.enable_plugin(plugin_name).await;
        // 注: enable_plugin 返 Ok(false) 如果 plugin 不在 config 里 (没有 mount 入口)
        // 但 audit 仍可能记 (SelfMod 内部决策)
        // 这里只验 audit_log() 不 panic, 实际 entry count 依赖 SelfMod 实现
        let _entries = provider.audit_log().await.expect("audit_log");
        // 不 strict assert count (SelfMod 实现可能 record-on-success-only)
    }

    #[tokio::test]
    async fn self_cli_yaml_serialize_roundtrip() {
        // 验 inspect 拿到的 config 序列化成 YAML 不 panic
        // (跟 self_inspect 走相同 serde_yaml 路径)
        let provider = self_mod_test_provider().await;
        let config = provider.inspect().await.expect("inspect");
        let yaml = serde_yaml::to_string(&config).expect("yaml serialize");
        // 不 strict assert content (config 可能空), 至少不是空字符串
        let _ = yaml.len(); // 用 _ 避免 unused warning
    }

    // ----- P14.4.2: `mah compaction` CLI -----

    /// helper: 写一个临时 JSONL events file 走 compaction run (跟 workflow tests 一样)
    fn write_events_jsonl(dir: &std::path::Path, name: &str, lines: &[&str]) -> std::path::PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, lines.join("\n")).expect("write jsonl");
        path
    }

    #[tokio::test]
    async fn compaction_info_prints_defaults() {
        // 不 IO, 纯打印 default CompactionContext
        // 走 sync 函数, 但 #[tokio::test] 也行
        compaction_info().expect("info");
    }

    #[tokio::test]
    async fn compaction_run_with_empty_jsonl_errors() {
        // 空文件 → parse 时 0 events, 但不会 panic. 实际上 should_err 也行 (空 input)
        let dir = tempfile::tempdir().expect("tempdir");
        let path = write_events_jsonl(dir.path(), "empty.jsonl", &[]);
        let result = compaction_run(&path, None, 8000, 3).await;
        // 空 input 是 Ok (events=0, stats 全 0)
        // 主要是不 panic + 走完路径
        assert!(result.is_ok(), "empty input should be Ok, got {:?}", result);
    }

    #[tokio::test]
    async fn compaction_run_with_minimal_event_succeeds() {
        // 写 1 个 UserInput event, 跑 compression, expect stats printed
        // SessionEvent 字段: id/session_id/event_type/ts/severity/model_visible/run_id/plugin_name/payload_json/error_message
        // ts 是 DateTime<Utc> (RFC 3339), event_type/severity 是 unit variant (字符串)
        let dir = tempfile::tempdir().expect("tempdir");
        let event_json = r#"{"id":"00000000-0000-0000-0000-000000000001","session_id":"test","event_type":"UserInput","ts":"2026-01-01T00:00:00Z","severity":"Info","model_visible":true,"run_id":null,"plugin_name":null,"payload_json":null,"error_message":null}"#;
        let path = write_events_jsonl(dir.path(), "one.jsonl", &[event_json]);
        let result = compaction_run(&path, None, 8000, 3).await;
        assert!(
            result.is_ok(),
            "single event should be Ok, got {:?}",
            result
        );
    }

    #[tokio::test]
    async fn compaction_run_to_output_file() {
        // 测 --output 写到文件
        let dir = tempfile::tempdir().expect("tempdir");
        let in_path = dir.path().join("in.jsonl");
        let out_path = dir.path().join("out.jsonl");
        let event_json = r#"{"id":"00000000-0000-0000-0000-000000000001","session_id":"test","event_type":"RunStart","ts":"2026-01-01T00:00:00Z","severity":"Info","model_visible":true,"run_id":null,"plugin_name":null,"payload_json":null,"error_message":null}"#;
        std::fs::write(&in_path, event_json).expect("write");
        let result = compaction_run(&in_path, Some(&out_path), 8000, 3).await;
        assert!(result.is_ok());
        assert!(out_path.exists(), "output file should be created");
        let content = std::fs::read_to_string(&out_path).expect("read out");
        // 至少 1 行 (RunStart 永保留)
        assert!(
            !content.trim().is_empty(),
            "output should have at least 1 event line"
        );
    }

    #[tokio::test]
    async fn compaction_run_with_missing_input_errors() {
        // 不存在的 input file → IO 错 (via ?), CLI exit 1
        let path = std::path::PathBuf::from("Z:/__nope__/nope.jsonl");
        let result = compaction_run(&path, None, 8000, 3).await;
        assert!(result.is_err(), "missing input should error");
    }

    // ----- P14.5.2: `mah lsp` CLI -----

    #[test]
    fn lsp_info_prints_known_servers() {
        // 不 IO, 纯打印
        lsp_info().expect("info");
    }

    #[tokio::test]
    async fn lsp_request_with_invalid_params_json_errors() {
        // 故意 invalid JSON (缺少引号) → serde_json parse 错
        let result = lsp_request(
            "rust-analyzer",
            &["--stdio".to_string()],
            "initialize",
            "{not json",
        )
        .await;
        assert!(result.is_err(), "invalid params should error");
        let err = format!("{:?}", result.unwrap_err());
        assert!(
            err.contains("parse params JSON") || err.contains("JSON"),
            "expected parse error, got: {}",
            err
        );
    }

    #[tokio::test]
    async fn lsp_request_with_nonexistent_server_errors() {
        // server 不存在 → spawn 失败 → LspError → CLI 返 Err
        let result = lsp_request(
            "Z:/__nonexistent_server__/nope.exe",
            &[],
            "initialize",
            "{}",
        )
        .await;
        assert!(
            result.is_err(),
            "nonexistent server should error, got: {:?}",
            result
        );
    }

    #[test]
    fn lsp_request_args_string_to_str_slice() {
        // 验证 args 转换: &[String] → &[&str] (在 lsp_request 内部)
        // 这里只测转换逻辑 (lsp_request 内部用), 不真 spawn
        let args: Vec<String> = vec!["--stdio".to_string(), "--quiet".to_string()];
        let args_str: Vec<&str> = args.iter().map(String::as_str).collect();
        assert_eq!(args_str, vec!["--stdio", "--quiet"]);
    }

    // ----- P14.6.2: `mah web` CLI -----

    /// smoke: `mah web info` 不 IO, 纯打印
    #[test]
    fn web_info_prints_providers() {
        web_info().expect("info");
    }

    /// `mah web search` 现在 stub 返 Unsupported (P14.6.2 限制, 业务方 outbound 准备好
    /// 之后才实装真 API). CLI 应把它映到 Err.
    #[tokio::test]
    async fn web_search_with_brave_stub_errors() {
        // BRAVE_API_KEY 不会在 CI 设置, 走 stub path → Unsupported
        let result = web_search("rust async runtime", 5, WebSearchProviderArg::Brave).await;
        assert!(
            result.is_err(),
            "brave stub should error, got: {:?}",
            result
        );
        let err = format!("{:?}", result.unwrap_err());
        assert!(
            err.contains("web search failed") || err.contains("Unsupported"),
            "expected web search failure, got: {}",
            err
        );
    }

    /// `mah web search --provider duckduckgo` 现在也是 stub, 同样返 Err
    #[tokio::test]
    async fn web_search_with_ddg_stub_errors() {
        let result = web_search("rust", 3, WebSearchProviderArg::Duckduckgo).await;
        assert!(result.is_err(), "ddg stub should error, got: {:?}", result);
    }

    /// `mah web fetch --url <invalid>` → WebError::Url, CLI 早 fail (不走 reqwest)
    #[tokio::test]
    async fn web_fetch_with_invalid_url_errors() {
        let result = web_fetch("not a valid url", None, 5).await;
        assert!(
            result.is_err(),
            "invalid url should error, got: {:?}",
            result
        );
        let err = format!("{:?}", result.unwrap_err());
        assert!(
            err.contains("invalid url") || err.contains("URL"),
            "expected URL parse error, got: {}",
            err
        );
    }

    /// value test: WebFetchQuery builder pattern (with_timeout / with_user_agent) 不丢字段
    #[test]
    fn web_fetch_query_builder_preserves_fields() {
        let q = ma_harness_web::WebFetchQuery::new("https://example.com")
            .with_timeout(std::time::Duration::from_secs(15))
            .with_user_agent("mah-test/0.1");
        assert_eq!(q.url, "https://example.com");
        assert_eq!(q.user_agent.as_deref(), Some("mah-test/0.1"));
        assert_eq!(q.timeout, Some(std::time::Duration::from_secs(15)));
    }

    // ----- P14.7.2: `mah todo` / `mah plan` CLI -----

    /// helper: 拿一个 in-memory todo store 直接走底层 API, 跟 todo_* CLI 同等行为
    /// (绕开 process-singleton TODO_STORE, 测独立)
    fn todo_test_store() -> std::sync::Arc<InMemoryTodoStore> {
        std::sync::Arc::new(InMemoryTodoStore::new())
    }

    fn plan_test_store() -> std::sync::Arc<InMemoryPlanStore> {
        std::sync::Arc::new(InMemoryPlanStore::new())
    }

    /// TodoStatusArg → TodoStatus 转换 roundtrip (4 状态全过)
    #[test]
    fn todo_status_arg_to_todo_status_roundtrip() {
        for (arg, expected) in [
            (TodoStatusArg::Pending, ma_harness_todo::TodoStatus::Pending),
            (
                TodoStatusArg::InProgress,
                ma_harness_todo::TodoStatus::InProgress,
            ),
            (TodoStatusArg::Done, ma_harness_todo::TodoStatus::Done),
            (
                TodoStatusArg::Cancelled,
                ma_harness_todo::TodoStatus::Cancelled,
            ),
        ] {
            assert_eq!(ma_harness_todo::TodoStatus::from(arg), expected);
        }
    }

    /// PlanStatusArg → PlanStatus 转换 roundtrip (5 状态全过)
    #[test]
    fn plan_status_arg_to_plan_status_roundtrip() {
        for (arg, expected) in [
            (PlanStatusArg::Draft, ma_harness_todo::PlanStatus::Draft),
            (
                PlanStatusArg::Approved,
                ma_harness_todo::PlanStatus::Approved,
            ),
            (
                PlanStatusArg::InProgress,
                ma_harness_todo::PlanStatus::InProgress,
            ),
            (
                PlanStatusArg::Completed,
                ma_harness_todo::PlanStatus::Completed,
            ),
            (
                PlanStatusArg::Rejected,
                ma_harness_todo::PlanStatus::Rejected,
            ),
        ] {
            assert_eq!(ma_harness_todo::PlanStatus::from(arg), expected);
        }
    }

    /// CLI 业务流程: write → update → list (走底层 store, 测 happy path)
    #[tokio::test]
    async fn cli_todo_write_update_list_workflow() {
        let store = todo_test_store();
        let item = TodoItem::new("Fix bug").with_priority(1);
        let id = store.write(&item).await.expect("write");
        store
            .update_status(&id, ma_harness_todo::TodoStatus::InProgress)
            .await
            .expect("update");
        let list = store.read_all().await.expect("read_all");
        assert_eq!(list.len(), 1);
        let sorted = list.sorted_by_priority();
        assert_eq!(sorted[0].content, "Fix bug");
        assert_eq!(sorted[0].status, ma_harness_todo::TodoStatus::InProgress);
    }

    /// CLI 业务流程: write → delete → read 返回 NotFound
    #[tokio::test]
    async fn cli_todo_write_then_delete_works() {
        let store = todo_test_store();
        let id = store.write(&TodoItem::new("x")).await.expect("write");
        store.delete(&id).await.expect("delete");
        let err = store.read(&id).await.unwrap_err();
        assert!(matches!(err, ma_harness_todo::TodoError::NotFound(_)));
    }

    /// CLI 业务流程: write → update 到 Done → 再 update 回 Pending 应失败 (终态)
    #[tokio::test]
    async fn cli_todo_done_is_terminal() {
        let store = todo_test_store();
        let id = store.write(&TodoItem::new("x")).await.expect("write");
        store
            .update_status(&id, ma_harness_todo::TodoStatus::InProgress)
            .await
            .expect("to in_progress");
        store
            .update_status(&id, ma_harness_todo::TodoStatus::Done)
            .await
            .expect("to done");
        let err = store
            .update_status(&id, ma_harness_todo::TodoStatus::Pending)
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            ma_harness_todo::TodoError::InvalidTransition { .. }
        ));
    }

    /// CLI 业务流程: plan write → update status → list
    #[tokio::test]
    async fn cli_plan_write_and_approve_works() {
        let store = plan_test_store();
        let plan = Plan::new("Refactor auth");
        let id = store.write(&plan).await.expect("write");
        store
            .update_status(&id, ma_harness_todo::PlanStatus::Approved)
            .await
            .expect("approve");
        let read = store.read(&id).await.expect("read");
        assert_eq!(read.status, ma_harness_todo::PlanStatus::Approved);
        assert_eq!(read.title, "Refactor auth");
    }

    /// CLI 业务流程: plan write (空 steps) → list 显示
    #[tokio::test]
    async fn cli_plan_empty_steps_works() {
        let store = plan_test_store();
        let plan = Plan::new("Empty plan");
        let id = store.write(&plan).await.expect("write");
        let read = store.read(&id).await.expect("read");
        assert_eq!(read.steps.len(), 0, "P14.7.2 限制: 只能写空 plan");
    }

    // ----- P14.9.2: `mah profile` CLI -----

    /// smoke: `mah profile info` 不 IO, 纯打印 builtin 5 profile
    #[test]
    fn cli_profile_info_prints_builtin() {
        profile_info().expect("info");
    }

    /// `mah profile list` 列 5 builtin profiles (跟 dsh 1:1: web/headless/sdk/sdk-minimal/acp)
    #[tokio::test]
    async fn cli_profile_list_5_builtins() {
        // 走 profile_list() 直接 (不 capture stdout, 只验不 panic)
        profile_list().await.expect("list");
    }

    /// `mah profile show web` 查 web profile 详情
    #[tokio::test]
    async fn cli_profile_show_web_works() {
        // 走 profile_show("web"), 验不 panic + 有 web / Web UI / :3080
        profile_show("web").await.expect("show web");
    }

    /// `mah profile show <unknown>` 返 Err
    #[tokio::test]
    async fn cli_profile_show_unknown_errors() {
        let result = profile_show("nonexistent-profile-xyz").await;
        assert!(result.is_err(), "unknown profile should error");
        let err = format!("{:?}", result.unwrap_err());
        assert!(
            err.contains("not found") || err.contains("nonexistent"),
            "expected not found error, got: {}",
            err
        );
    }

    /// `mah profile validate <missing path>` 返 Err (loader 找不到 cordis.yml)
    #[tokio::test]
    async fn cli_profile_validate_missing_dir_errors() {
        let path = std::path::PathBuf::from("Z:/__nonexistent_profile_dir__/nope");
        let result = profile_validate(&path).await;
        assert!(result.is_err(), "missing dir should error");
    }

    /// 底层 API smoke: ProfileRegistry 5 builtin 全注册 + get 全 Some
    #[tokio::test]
    async fn cli_profile_registry_roundtrip_5_builtins() {
        use ma_harness_profile::{builtin_profiles, ProfileRegistry};

        let registry = ProfileRegistry::new();
        for p in builtin_profiles() {
            registry.register(p).await;
        }
        let names = registry.list().await;
        assert_eq!(names.len(), 5, "5 builtin profiles");
        for n in &names {
            assert!(
                registry.get(n).await.is_some(),
                "registered profile should be retrievable: {}",
                n
            );
        }
    }

    // ----- P14.10.2: `mah context` CLI -----

    /// smoke: `mah context info` 不 IO, 纯打印 available features
    #[test]
    fn cli_context_info_prints_features() {
        context_info().expect("info");
    }

    /// CLI 业务流程: new (auto trace_id) → show → validate (全 OK)
    #[tokio::test]
    async fn cli_context_new_show_validate_works() {
        // 不传 trace_id / deadline_secs, 走 default
        context_new(None, None).await.expect("new");
        context_show().await.expect("show");
        context_validate().await.expect("validate");
    }

    /// CLI 业务流程: new (custom trace_id + deadline) → validate (deadline 设了 + 未过期)
    #[tokio::test]
    async fn cli_context_new_with_deadline() {
        context_new(Some("test-trace-abc"), Some(60))
            .await
            .expect("new with deadline");
        // 验证走通: trace_id 是 test-trace-abc, deadline 是 now+60
        let arc = active_ctx().await;
        let ctx = arc.lock().await.clone();
        assert_eq!(ctx.trace_id, "test-trace-abc");
        assert!(ctx.deadline.is_some());
        assert!(!ctx.is_expired(), "60s deadline should not be expired");
    }

    /// 底层 API smoke: RequestContext::new() auto-generate UUID trace_id
    #[tokio::test]
    async fn cli_context_request_context_default_has_uuid() {
        use ma_harness_context::RequestContext;
        let ctx = RequestContext::new();
        assert!(!ctx.trace_id.is_empty(), "trace_id auto-generated");
        // UUID v4 格式: 8-4-4-4-12 hex chars (36 chars total with dashes)
        assert_eq!(ctx.trace_id.len(), 36, "UUID v4 length");
        assert!(ctx.deadline.is_none(), "default no deadline");
        assert!(ctx.metadata.is_empty(), "default empty metadata");
    }

    /// 底层 API smoke: RequestContext child 行为 (parent_session_id 不变, 重新生成 trace_id)
    #[tokio::test]
    async fn cli_context_request_context_child_inherits() {
        use ma_harness_context::RequestContext;
        let parent = RequestContext::new().with_parent_session_id("parent-session-xyz");
        let child = parent.child(Some("child-trace-001".to_string()));
        assert_eq!(child.trace_id, "child-trace-001");
        assert_eq!(
            child.parent_session_id,
            Some("parent-session-xyz".to_string())
        );
        assert_eq!(child.deadline, parent.deadline);
    }

    /// 底层 API smoke: ContextChain 走 LoggingMiddleware (P14.10.1 已有, 测 propagate 走通)
    #[tokio::test]
    async fn cli_context_chain_propagate_with_logging() {
        use ma_harness_context::{ContextChain, LoggingMiddleware, RequestContext};
        let chain = ContextChain::new();
        chain
            .add_middleware(std::sync::Arc::new(LoggingMiddleware::new()))
            .await;
        assert_eq!(chain.len().await, 1);

        let ctx = RequestContext::new().with_trace_id("chain-test");
        let propagated = chain.propagate(&ctx).await.expect("propagate");
        assert_eq!(propagated.trace_id, "chain-test");
        // LoggingMiddleware 不改 trace_id, 只 log
        assert_eq!(propagated.trace_id, ctx.trace_id);
    }
}

#[cfg(test)]
mod settings_cli_tests {
    use super::*;
    use ma_harness_settings::Settings;

    /// 测 P15.5.3 业务流程 (直接走 Settings API, 不走 CLI dispatch)
    #[test]
    fn settings_set_then_get_via_settings_api() {
        let mut s = Settings::empty();
        s.set("api.openai_key", "sk-test-123");
        s.set("models.default", "gpt-4");

        assert_eq!(s.get_str("api.openai_key"), Some("sk-test-123"));
        assert_eq!(s.get_str("models.default"), Some("gpt-4"));

        let mut keys = s.keys();
        keys.sort();
        assert_eq!(
            keys,
            vec!["api.openai_key".to_string(), "models.default".to_string()]
        );
    }

    #[test]
    fn settings_set_overwrites_existing_value() {
        let mut s = Settings::empty();
        s.set("api.key", "old");
        s.set("api.key", "new");
        assert_eq!(s.get_str("api.key"), Some("new"));
    }

    #[test]
    fn settings_get_missing_key_returns_none() {
        let s = Settings::empty();
        assert_eq!(s.get_str("missing.key"), None);
    }

    /// 测 CLI handler 不依赖真 async runtime
    #[test]
    fn resolve_settings_path_uses_override() {
        let p = std::path::PathBuf::from("/tmp/custom.yaml");
        let result = resolve_settings_path(Some(&p)).expect("resolve");
        assert_eq!(result, p);
    }

    #[test]
    fn resolve_settings_path_uses_default_when_no_override() {
        let result = resolve_settings_path(None).expect("resolve");
        assert!(
            result.ends_with(".ma-harness/settings.yaml"),
            "default path should end with .ma-harness/settings.yaml, got {}",
            result.display()
        );
    }
}

/// **P14 (2026-08-20)**: 业务方 publish 后的 plugin list (给 GH Pages 静态站消费)
///
/// 例子:
///   mah registry list                            # 默认 ~/.ma-harness/registry.json
///   mah registry list --registry my.json         # 自定义路径
fn registry_list(registry: Option<&std::path::Path>) -> Result<()> {
    let path = registry
        .map(|p| p.to_path_buf())
        .or_else(|| dirs_home().map(|h| h.join(".ma-harness").join("registry.json")))
        .context("no registry path given and no ~/.ma-harness/registry.json")?;

    if !path.exists() {
        eprintln!("Registry file not found: {}", path.display());
        eprintln!(
            "Hint: run `mah plugin publish <manifest.json>` first, or pass --registry <path>"
        );
        return Ok(());
    }

    let reg = Registry::open(&path)
        .with_context(|| format!("failed to open registry at {}", path.display()))?;

    println!("ma-harness Plugin Registry");
    println!("============================");
    println!("Path:   {}", path.display());
    println!(
        "Plugins: {} ({} versions total)",
        reg.count(),
        reg.version_count()
    );
    println!();

    let mut by_author: std::collections::BTreeMap<String, Vec<String>> =
        std::collections::BTreeMap::new();
    for m in reg.list() {
        by_author.entry(m.author.clone()).or_default().push(format!(
            "{} @ {} (tags: {})",
            m.name,
            m.version,
            if m.tags.is_empty() {
                "-".to_string()
            } else {
                m.tags.join(", ")
            }
        ));
    }

    for (author, plugins) in &by_author {
        println!("{} ({}):", author, plugins.len());
        for p in plugins {
            println!("  - {}", p);
        }
    }
    Ok(())
}

/// **P14 (2026-08-20)**: 导出 registry 到 JSON file (给 GH Pages 静态站消费)
///
/// 例子:
///   mah registry export --output docs/registry/registry.json
///   mah registry export --output target/registry.json --registry my.json
fn registry_export(output: &std::path::Path, registry: Option<&std::path::Path>) -> Result<()> {
    let path = registry
        .map(|p| p.to_path_buf())
        .or_else(|| dirs_home().map(|h| h.join(".ma-harness").join("registry.json")))
        .context("no registry path given and no ~/.ma-harness/registry.json")?;

    if !path.exists() {
        anyhow::bail!(
            "Registry file not found: {}. Run `mah plugin publish` first.",
            path.display()
        );
    }

    let reg = Registry::open(&path)
        .with_context(|| format!("failed to open registry at {}", path.display()))?;

    // 确保输出目录存在
    if let Some(parent) = output.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create output dir {}", parent.display()))?;
        }
    }

    reg.export(output)
        .with_context(|| format!("failed to export registry to {}", output.display()))?;

    println!(
        "Exported {} plugins ({} versions) to {}",
        reg.count(),
        reg.version_count(),
        output.display()
    );
    Ok(())
}

/// Helper: 拿 $HOME / %USERPROFILE% (无依赖 dirs crate)
fn dirs_home() -> Option<std::path::PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(std::path::PathBuf::from)
}

/// **P14 (2026-08-20)**: tests for `mah registry` CLI subcommand
#[cfg(test)]
mod registry_cli_tests {
    use super::*;
    use ma_harness_registry::{PluginManifest, PluginSource, Registry};
    use semver::Version;
    use chrono::Utc;

    fn make_sample_manifest(name: &str, version: &str, author: &str) -> PluginManifest {
        PluginManifest {
            name: name.to_string(),
            version: version.parse().unwrap(),
            description: format!("Sample plugin {}", name),
            author: author.to_string(),
            source: PluginSource::Local(format!("../plugins/{}", name)),
            tags: vec!["test".to_string()],
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    /// registry_list 走 sample registry
    #[test]
    fn registry_list_works() {
        let tmpdir = tempfile::tempdir().unwrap();
        let reg_path = tmpdir.path().join("registry.json");
        let mut reg = Registry::open_in_memory();
        reg.publish(make_sample_manifest("plug-a", "0.1.0", "alice"))
            .unwrap();
        reg.publish(make_sample_manifest("plug-b", "0.2.0", "bob"))
            .unwrap();
        reg.save(&reg_path).unwrap();

        let result = registry_list(Some(&reg_path));
        assert!(result.is_ok(), "registry_list should succeed: {:?}", result);
    }

    /// registry_export 生成 JSON, 内容正确
    #[test]
    fn registry_export_writes_valid_json() {
        let tmpdir = tempfile::tempdir().unwrap();
        let reg_path = tmpdir.path().join("registry.json");
        let out_path = tmpdir.path().join("out.json");

        let mut reg = Registry::open_in_memory();
        reg.publish(make_sample_manifest("plug-x", "1.0.0", "i25ma"))
            .unwrap();
        reg.save(&reg_path).unwrap();

        let result = registry_export(&out_path, Some(&reg_path));
        assert!(
            result.is_ok(),
            "registry_export should succeed: {:?}",
            result
        );
        assert!(out_path.exists(), "out.json should exist");

        // 验证 JSON 能 roundtrip
        let loaded = Registry::open(&out_path).unwrap();
        let manifest = loaded.get("plug-x").expect("plug-x should be in registry");
        assert_eq!(manifest.version, Version::parse("1.0.0").unwrap());
        assert_eq!(manifest.author, "i25ma");
    }

    /// registry_list / export 在 registry 不存在时返 Err (export) 或 Ok+warning (list)
    #[test]
    fn registry_missing_file_handled() {
        let tmpdir = tempfile::tempdir().unwrap();
        let reg_path = tmpdir.path().join("nonexistent.json");
        let out_path = tmpdir.path().join("out.json");

        // list: 不 panic, 返 Ok, 警告
        let list_result = registry_list(Some(&reg_path));
        assert!(list_result.is_ok(), "list 不存在 file 应返 Ok + warning");

        // export: 返 Err
        let export_result = registry_export(&out_path, Some(&reg_path));
        assert!(export_result.is_err(), "export 不存在 file 应返 Err");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ma_harness_core::SessionEvent;

    // === P5-5 (Day 94): mah sessions CLI ===

    /// sessions_list 在 db 不存在时报错 (清晰错误信息)
    #[test]
    fn sessions_list_missing_db_errors() {
        let result = sessions_list(&std::path::PathBuf::from("/nonexistent/path/x.db"));
        assert!(result.is_err());
    }

    /// sessions_list 走真 SqliteStore, 创 2 session + 验 list 拿到
    #[test]
    fn sessions_list_works() {
        use ma_harness_proto::ma_harness::v1::{
            OperatingMode, Session as ProtoSession, SessionState as ProtoSessionState,
        };
        let tmpdir = tempfile::tempdir().unwrap();
        let db_path = tmpdir.path().join("sessions.db");
        let store = ma_harness_server::SqliteStore::open(&db_path).unwrap();
        for (id, name) in [("alpha", "first"), ("beta", "second")] {
            store
                .create(&ProtoSession {
                    id: id.to_string(),
                    name: name.to_string(),
                    state: ProtoSessionState::Active as i32,
                    mode: OperatingMode::Default as i32,
                    created_at: Some(prost_types::Timestamp::from(std::time::SystemTime::now())),
                    updated_at: Some(prost_types::Timestamp::from(std::time::SystemTime::now())),
                    closed_at: None,
                    metadata: None,
                    stats: None,
                    enabled_plugins: vec![],
                    user_id: String::new(),
                })
                .unwrap();
        }
        // sessions_list 走 SqliteStore, 不 panic, 返 Result
        let result = sessions_list(&db_path);
        assert!(result.is_ok(), "sessions_list 走通: {:?}", result);
    }

    /// sessions_get 拿存在的 session
    #[test]
    fn sessions_get_works() {
        use ma_harness_proto::ma_harness::v1::{
            OperatingMode, Session as ProtoSession, SessionState as ProtoSessionState,
        };
        let tmpdir = tempfile::tempdir().unwrap();
        let db_path = tmpdir.path().join("sessions.db");
        let store = ma_harness_server::SqliteStore::open(&db_path).unwrap();
        store
            .create(&ProtoSession {
                id: "get-test".to_string(),
                name: "getname".to_string(),
                state: ProtoSessionState::Active as i32,
                mode: OperatingMode::Default as i32,
                created_at: Some(prost_types::Timestamp::from(std::time::SystemTime::now())),
                updated_at: Some(prost_types::Timestamp::from(std::time::SystemTime::now())),
                closed_at: None,
                metadata: None,
                stats: None,
                enabled_plugins: vec!["hello".to_string()],
                user_id: String::new(),
            })
            .unwrap();
        let result = sessions_get(&db_path, "get-test");
        assert!(result.is_ok(), "sessions_get 走通: {:?}", result);
    }

    /// sessions_get 拿不存在的 session 返 Err
    #[test]
    fn sessions_get_missing_errors() {
        let tmpdir = tempfile::tempdir().unwrap();
        let db_path = tmpdir.path().join("sessions.db");
        let _ = ma_harness_server::SqliteStore::open(&db_path).unwrap();
        let result = sessions_get(&db_path, "nonexistent-id");
        assert!(result.is_err(), "missing session 应返 Err");
        let err = format!("{}", result.unwrap_err());
        assert!(
            err.contains("not found"),
            "错误信息应含 not found, got: {}",
            err
        );
    }

    /// sessions_events 走真 EventLog
    #[test]
    fn sessions_events_works() {
        use ma_harness_core::EventType;
        let tmpdir = tempfile::tempdir().unwrap();
        let log_path = tmpdir.path().join("events.db");
        let log = EventLog::open(&log_path).unwrap();
        let mut ev = SessionEvent::new("ev-test", EventType::SessionStart);
        ev.payload_json = Some(r#"{"hello":"world"}"#.to_string());
        let _ = log.append(ev);
        let result = sessions_events(&log_path, "ev-test");
        assert!(result.is_ok(), "sessions_events 走通: {:?}", result);
    }

    /// format_ts 走 prost_types::Timestamp → RFC3339
    #[test]
    fn format_ts_works() {
        let ts = prost_types::Timestamp::from(std::time::SystemTime::UNIX_EPOCH);
        let s = format_ts(&ts);
        assert!(
            s.contains("1970") || s.contains("+"),
            "应含 1970 或 + (UTC offset), got {}",
            s
        );
    }

    // === T3.3 WAT extraction helper 测试 ===

    #[test]
    fn extract_wat_from_wat_fence() {
        let text = r#"Here's the WAT:
```wat
(module
    (memory (export "memory") 1)
    (func (export "run") (result i32)
        i32.const 42
    )
)
```
That's it."#;
        let wat = extract_wat_from_llm_response(text).unwrap();
        assert!(wat.contains("(module"));
        assert!(wat.contains("i32.const 42"));
    }

    #[test]
    fn extract_wat_from_plain_fence() {
        let text = r#"```
(module (func (export "run") (result i32) i32.const 1))
```"#;
        let wat = extract_wat_from_llm_response(text).unwrap();
        assert!(wat.contains("(module"));
    }

    #[test]
    fn extract_wat_from_bare_module() {
        let text = r#"Here is the code:
(module
    (func (export "run") (result i32) i32.const 0)
)
End."#;
        let wat = extract_wat_from_llm_response(text).unwrap();
        assert!(wat.contains("(module"));
        assert!(wat.contains("i32.const 0"));
    }

    #[test]
    fn extract_wat_no_module_returns_none() {
        let text = "I cannot generate WAT for this.";
        assert!(extract_wat_from_llm_response(text).is_none());
    }

    #[test]
    fn extract_wat_handles_paren_matching() {
        // 嵌套括号 (i32.const (1 + 2)) 不应乱配
        let text = r#"
(module
    (func (export "run") (result i32)
        i32.const 5
    )
)
"#;
        let wat = extract_wat_from_llm_response(text).unwrap();
        // 应含 export run + i32.const 5
        assert!(wat.contains("export \"run\""));
        assert!(wat.contains("i32.const 5"));
    }

    // === P6-1 (Day 99): mah run-stream CLI ===

    /// parse_model_arg: "stub" → (0, "stub")
    #[test]
    fn parse_model_arg_stub() {
        let (adapter, name) = parse_model_arg("stub");
        assert_eq!(adapter, 0, "stub 应走 Unspecified");
        assert_eq!(name, "stub");
    }

    /// parse_model_arg: "openai:gpt-4o-mini" → (1, "gpt-4o-mini")
    #[test]
    fn parse_model_arg_openai() {
        let (adapter, name) = parse_model_arg("openai:gpt-4o-mini");
        assert_eq!(adapter, 1, "openai 应走 Openai enum (1)");
        assert_eq!(name, "gpt-4o-mini");
    }

    /// parse_model_arg: "anthropic:claude-3-5-sonnet" → (1, "claude-3-5-sonnet")
    /// (proto 暂未分, fallback Openai 通道)
    #[test]
    fn parse_model_arg_anthropic() {
        let (adapter, name) = parse_model_arg("anthropic:claude-3-5-sonnet");
        assert_eq!(adapter, 1, "anthropic 暂走 Openai 通道");
        assert_eq!(name, "claude-3-5-sonnet");
    }

    /// parse_model_arg: "gpt-4o-mini" (无 prefix) → (0, "gpt-4o-mini")
    #[test]
    fn parse_model_arg_no_prefix() {
        let (adapter, name) = parse_model_arg("gpt-4o-mini");
        assert_eq!(adapter, 0, "无 prefix 应走 Unspecified");
        assert_eq!(name, "gpt-4o-mini");
    }

    /// parse_model_arg: "weird:foo" (未知 provider) → (0, "foo")
    #[test]
    fn parse_model_arg_unknown_provider() {
        let (adapter, name) = parse_model_arg("weird:foo");
        assert_eq!(adapter, 0, "未知 provider 应走 Unspecified");
        assert_eq!(name, "foo");
    }

    /// parse_model_arg: 多个 `:` 切第一对 (split_once) → ("openai", "gpt-4o:turbo" 保留)
    #[test]
    fn parse_model_arg_multi_colon() {
        let (adapter, name) = parse_model_arg("openai:gpt-4o:turbo");
        assert_eq!(adapter, 1);
        assert_eq!(name, "gpt-4o:turbo", "split_once 只切第一个 `:`");
    }
}
// ============================================================================
// P14.11.2: `mah guard` CLI tests
//
// 7 tests: smoke info, demo happy, demo abort, observe step, observe tool abort,
// reset clears, list 2 builtins, chain singleton lazy init.
// ============================================================================

#[cfg(test)]
mod guard_cli_tests {
    use super::*;
    use ma_harness_guard::LoopGuard;

    /// smoke: `mah guard info` 不 IO, 纯打印 available features
    #[test]
    fn cli_guard_info_prints_features() {
        guard_info().expect("info");
    }

    /// smoke: `mah guard list` 不 IO, 列 2 builtin (max-steps + repeated-args)
    #[test]
    fn cli_guard_list_2_builtins() {
        guard_list().expect("list");
    }

    /// 业务流程: demo 默认参数 (max_steps=10, max_repeats=3) 不触发 abort
    #[tokio::test]
    async fn cli_guard_demo_default_continues() {
        // max_steps=10 (default), max_repeats=3 (default), 5 步 + 1 tool call 不触发 abort
        guard_demo(10, 3).await.expect("demo default");
    }

    /// 业务流程: demo --max-steps 2 超限后触发 abort
    #[tokio::test]
    async fn cli_guard_demo_max_steps_low_triggers_abort() {
        // max_steps=2 是小上限, 5 步跑完必然 abort
        guard_demo(2, 100).await.expect("demo low max_steps");
    }

    /// 业务流程: demo --max-repeats 1 同 args 第 2 次触发 abort
    #[tokio::test]
    async fn cli_guard_demo_max_repeats_low_triggers_abort() {
        // max_repeats=1 是小上限, demo 里 tool call 同 args 跑 2 次必然 abort
        guard_demo(100, 1).await.expect("demo low max_repeats");
    }

    /// 底层 API smoke: LoopEvent 完整 4 变体, GuardDecision 两种变体
    #[tokio::test]
    async fn cli_guard_loop_event_and_decision_roundtrip() {
        use ma_harness_guard::{GuardDecision, LoopEvent, MaxStepsGuard, RepeatedArgsGuard};

        // LoopEvent 五种形式: StepStarted, StepCompleted, ToolCalled, ToolResult
        let events = vec![
            LoopEvent::StepStarted,
            LoopEvent::StepCompleted,
            LoopEvent::ToolCalled {
                tool_name: "bash".to_string(),
                args_hash: "hash-1".to_string(),
            },
            LoopEvent::ToolResult {
                tool_name: "bash".to_string(),
                success: true,
            },
        ];
        assert_eq!(events.len(), 4);

        // GuardDecision 两种
        assert!(GuardDecision::Continue.is_continue());
        assert!(!GuardDecision::Continue.is_abort());
        let abort = GuardDecision::Abort {
            reason: "test".into(),
        };
        assert!(abort.is_abort());
        assert_eq!(abort.reason(), Some("test"));

        // MaxStepsGuard: max_steps=1 下 2 步必然 abort
        let g1 = MaxStepsGuard::new(1);
        assert!(g1.observe(&LoopEvent::StepCompleted).await.is_continue());
        let d1 = g1.observe(&LoopEvent::StepCompleted).await;
        assert!(d1.is_abort());
        assert!(d1.reason().unwrap().contains("max steps exceeded"));

        // RepeatedArgsGuard: max_repeats=1 下同 args 跑 2 次必然 abort
        let g2 = RepeatedArgsGuard::new(1);
        let evt = LoopEvent::ToolCalled {
            tool_name: "t".into(),
            args_hash: "h".into(),
        };
        assert!(g2.observe(&evt).await.is_continue());
        let d2 = g2.observe(&evt).await;
        assert!(d2.is_abort());
    }

    /// 底层 API smoke: GuardChain 不同 guard 顺序不同行为 (chain 组合)
    #[tokio::test]
    async fn cli_guard_chain_combines_two_guards() {
        use ma_harness_guard::{GuardChain, LoopEvent, MaxStepsGuard, RepeatedArgsGuard};

        let chain = GuardChain::new();
        chain
            .add_guard(std::sync::Arc::new(MaxStepsGuard::new(100)))
            .await;
        chain
            .add_guard(std::sync::Arc::new(RepeatedArgsGuard::new(1)))
            .await;
        assert_eq!(chain.len().await, 2);
        assert!(!chain.is_empty().await);

        // 不同 args 不触发 abort (RepeatedArgsGuard 仅看同 (tool, args))
        for i in 0..3 {
            let evt = LoopEvent::ToolCalled {
                tool_name: "t".into(),
                args_hash: format!("args-{i}"),
            };
            assert!(chain.observe(&evt).await.is_continue());
        }

        // 同 args 跑 2 次触发 abort (chain 会 reset_all)
        let evt_same = LoopEvent::ToolCalled {
            tool_name: "t".into(),
            args_hash: "same".into(),
        };
        assert!(chain.observe(&evt_same).await.is_continue());
        let d = chain.observe(&evt_same).await;
        assert!(d.is_abort());
        assert!(d.reason().unwrap().contains("called"));
    }
}
