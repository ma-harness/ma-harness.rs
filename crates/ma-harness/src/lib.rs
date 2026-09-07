//! # ma-harness umbrella crate
//!
//! **One `use ma_harness::*` for the full ma-harness SDK.**
//!
//! This crate re-exports the public API of every first-party ma-harness
//! sub-crate under feature flags. The goal is zero-friction onboarding:
//! instead of pulling in three or more crates manually, a library user
//! adds a single `ma-harness` dependency and turns on the features they
//! need.
//!
//! ## Quick start
//!
//! ```toml
//! [dependencies]
//! # Default: core types + LLM adapters (the most common SDK use case).
//! ma-harness = "0.1"
//!
//! # Or pick features explicitly:
//! # ma-harness = { version = "0.1", default-features = false, features = ["core", "server", "plugin"] }
//! ```
//!
//! ```ignore
//! use ma_harness::*;
//!
//! // Cordis DI
//! let ctx = Context::new();
//!
//! // LLM
//! let adapter = OpenaiAdapter::new("sk-...");
//! let req = ModelRequest::new(vec![ModelMessage::user("hi")]);
//! let resp = adapter.complete(&req).await?;
//! ```
//!
//! ## Feature matrix
//!
//! | Feature     | Re-exports                                             | Notes                       |
//! |-------------|--------------------------------------------------------|-----------------------------|
//! | `core`      | `ma-harness-cordis` + `ma-harness-core` + `ma-harness-seam` + `ma-harness-plugin-macro` | Foundation (types, DI, plugin facade, proc-macro) |
//! | `model`     | `ma-harness-model`                                     | OpenAI / Anthropic adapters + retry + vision. Depends on `core`. |
//! | `plugin`    | `ma-harness-registry`                                  | Plugin registry (npm-style). Depends on `core`. |
//! | `bundle`    | `ma-harness-bundle`                                    | Lockfile install. Depends on `plugin`. |
//! | `sandbox`   | `ma-harness-sandbox`                                   | Landlock / Seatbelt / Stub enforcer. |
//! | `code`      | `ma-harness-code`                                      | Wasmtime Code Mode. |
//! | `dag`       | `ma-harness-dag`                                       | DAG orchestration. |
//! | `artifact`  | `ma-harness-artifact`                                  | Vibe coding artifact viewer. |
//! | `server`    | `ma-harness-server`                                    | Salvo HTTP server. |
//! | `tui`       | `ma-harness-tui`                                       | Ratatui TUI dashboard. |
//! | `p14`       | 11 P14 sub-crates (subprocess, shell, skill, compaction, lsp, web, todo, session, profile, context, guard) | P14 ctx.* seams in one feature. |
//! | `p15`       | 6 P15 sub-crates (workflow, webhook, settings, credentials, hooks, self-modification) | P15 features in one feature. |
//! | `web-ui`    | `ma-harness-web-ui`                                    | P15.1 future work. |
//! | `pty`       | `ma-harness-terminal`                                  | P15.2 future work. |
//! | `full`      | All of the above                                       | Equivalent to the full workspace SDK. |
//!
//! **Default features**: `core` + `model`. This matches the most common
//! library use case: types and an LLM adapter. Add `server` / `tui` /
//! `p14` / `p15` etc. as needed.
//!
//! ## Design notes
//!
//! - The umbrella crate is **additive** — every existing first-party
//!   crate continues to be published independently. Users who prefer
//!   granular dependencies keep their current setup.
//! - Feature names mirror the dsh concept groups (`core` = types / DI,
//!   `p14` / `p15` = phase batches). The `full` feature is for
//!   parity testing only; production setups should pick what they
//!   actually need.
//! - Re-exports use `pub use` (not `pub mod`), so the umbrella
//!   crate's own version is what shows in semver-compatible
//!   resolution. Sub-crates stay version-locked via the workspace
//!   `version.workspace = true`.

#![deny(unsafe_code)]
#![warn(missing_docs)]
#![allow(missing_docs)] // sub-crates already document their items; re-exports inherit
#![doc(html_root_url = "https://docs.rs/ma-harness/0.1.1")]

// ============================================================================
// Re-exports: feature-gated
// ============================================================================

// ----------------------------------------------------------------------------
// `core` feature — foundation: types, DI, plugin facade, proc-macro
// ----------------------------------------------------------------------------

/// Bring the `ma-harness-cordis` crate into scope as a module.
///
/// Lets callers do `ma_harness::cordis::approval::ApprovalService` if
/// they want the un-aliased path.
#[cfg(feature = "core")]
pub use ma_harness_cordis as cordis;
#[cfg(feature = "core")]
pub use ma_harness_core as core_crate;
#[cfg(feature = "core")]
pub use ma_harness_seam as seam_crate;
#[cfg(feature = "core")]
pub use ma_harness_plugin_macro as plugin_macro;

// Cordis public surface (DI framework)
#[cfg(feature = "core")]
pub use ma_harness_cordis::{
    is_snake_case, ApprovalDecision, ApprovalPolicy, ApprovalRegistry, ApprovalRequest,
    ApprovalService, AsyncDisposable, BoxedError, ChannelApprovalService, Context, CordisError,
    CtxKey, Disposable, Listener, ListenerEvent, Plugin, RiskLevel, Scope, Service, TypeId,
};
#[cfg(feature = "core")]
pub use ma_harness_cordis::CordisEvent;
#[cfg(feature = "core")]
pub use ma_harness_cordis::EventSeverity;

// Core public surface (types + agent loop + EventLog)
#[cfg(feature = "core")]
pub use ma_harness_core::{
    AgentLoop, AgentRunRequest, AgentRunResponse, CompressionPolicy, EventLog, EventPage,
    EventQuery, EventType, FinishReason, ModelAdapter, ModelMessage, ModelRequest, ModelResponse,
    OperatingMode, OperatingModeConfig, Profile, ProfileStore, SessionEvent, Severity, StoredEvent,
    StubModelAdapter, ToolEntry, ToolRegistry, ToolSchema, OPERATING_MODE, default_profile_dir,
};
#[cfg(feature = "core")]
pub use ma_harness_core::agent_compress::{
    compress, estimate_messages_tokens, estimate_tokens, load_history_from_log, should_compress,
};
#[cfg(feature = "core")]
pub use ma_harness_core::tool_pipeline::{
    invoke_with_pipeline, InvokeContext, PipelineConfig, PipelineStage, PostHookFn, PreHookFn,
    RetryPolicy as PipelineRetryPolicy, ToolConfig,
};
#[cfg(feature = "core")]
pub use ma_harness_core::agents_md::{load_agents_md, AgentsMdConfig, AgentsMdResult};

// Seam (plugin facade) — re-exported as a sibling module for clarity.
#[cfg(feature = "core")]
pub use ma_harness_seam::{
    DshListener, DshService, dsh_command, dsh_handler, dsh_listener_on, dsh_listener_priority,
    dsh_plugin_dual, dsh_service_dual, dsh_tool,
};
#[cfg(feature = "core")]
pub use ma_harness_seam::{API_VERSION as SEAM_API_VERSION, VERSION as SEAM_VERSION};

// ----------------------------------------------------------------------------
// `model` feature — LLM adapters
// ----------------------------------------------------------------------------

/// Bring the `ma-harness-model` crate into scope as a module.
#[cfg(feature = "model")]
pub use ma_harness_model as model;

#[cfg(feature = "model")]
pub use ma_harness_model::{AdapterError, AdapterRegistry, AnthropicAdapter, OpenaiAdapter};

#[cfg(feature = "model")]
pub use ma_harness_model::retry::{
    backoff_for, retry_with_backoff, CircuitBreaker, CircuitState, RetryError, RetryPolicy,
};

#[cfg(feature = "model")]
pub use ma_harness_model::multimodal::{
    build_anthropic_vision_content, build_openai_vision_content, ImageAttachment,
};

#[cfg(feature = "model")]
pub use ma_harness_model::vision_tool::{
    describe_image, describe_with_anthropic, describe_with_openai, VisionBackend,
    VisionDescribeArgs, VisionError, VisionResult, VISION_TOOL_DESCRIPTION, VISION_TOOL_NAME,
};

#[cfg(feature = "model")]
pub use ma_harness_model::vision_plugin::VisionTool;

// ----------------------------------------------------------------------------
// `plugin` feature — Plugin registry (npm-style)
// ----------------------------------------------------------------------------

#[cfg(feature = "plugin")]
pub use ma_harness_registry as registry;
#[cfg(feature = "plugin")]
pub use ma_harness_registry::{PluginManifest, PluginSource, Registry, RegistryError};

// ----------------------------------------------------------------------------
// `bundle` feature — Lockfile-based plugin install
// ----------------------------------------------------------------------------

#[cfg(feature = "bundle")]
pub use ma_harness_bundle as bundle;
#[cfg(feature = "bundle")]
pub use ma_harness_bundle::{
    bundle_summary, load_bundle_from_file, load_bundle_from_str, Bundle, BundleError,
    BundleManifest, BundlePlugin, ResolvedPlugin,
};

// ----------------------------------------------------------------------------
// `sandbox` feature — Landlock / Seatbelt / Stub
// ----------------------------------------------------------------------------

#[cfg(feature = "sandbox")]
pub use ma_harness_sandbox as sandbox;
#[cfg(feature = "sandbox")]
pub use ma_harness_sandbox::{EnforceError, Enforcer, PathRule, Policy, StubEnforcer};
#[cfg(all(feature = "sandbox", target_os = "linux"))]
pub use ma_harness_sandbox::LinuxLandlockEnforcer;
#[cfg(all(feature = "sandbox", target_os = "macos"))]
pub use ma_harness_sandbox::MacosSeatbeltEnforcer;

// ----------------------------------------------------------------------------
// `code` feature — Wasmtime Code Mode
// ----------------------------------------------------------------------------

#[cfg(feature = "code")]
pub use ma_harness_code as code;
#[cfg(feature = "code")]
pub use ma_harness_code::{CodeOutput, CodeRunner, SandboxConfig};

// ----------------------------------------------------------------------------
// `dag` feature — DAG orchestration
// ----------------------------------------------------------------------------

#[cfg(feature = "dag")]
pub use ma_harness_dag as dag;
#[cfg(feature = "dag")]
pub use ma_harness_dag::{
    load_dag_from_file, Dag, DagError, DagRun, DagScheduler, Task, TaskRun, TaskStatus,
};

// ----------------------------------------------------------------------------
// `artifact` feature — Vibe coding artifact viewer
// ----------------------------------------------------------------------------

#[cfg(feature = "artifact")]
pub use ma_harness_artifact as artifact;
#[cfg(feature = "artifact")]
pub use ma_harness_artifact::{detect_artifact, render_terminal, ArtifactError, ArtifactKind};

// ----------------------------------------------------------------------------
// `server` feature — Salvo HTTP server
// ----------------------------------------------------------------------------

#[cfg(feature = "server")]
pub use ma_harness_server as server;
#[cfg(feature = "server")]
pub use ma_harness_server::{AgentServiceImpl, ServerBuilder, SessionServiceImpl};

// ----------------------------------------------------------------------------
// `tui` feature — Ratatui TUI dashboard
// ----------------------------------------------------------------------------

#[cfg(feature = "tui")]
pub use ma_harness_tui as tui;
#[cfg(feature = "tui")]
pub use ma_harness_tui::TuiApp;

// ----------------------------------------------------------------------------
// `p14` feature — P14 ctx.* sub-crates
// ----------------------------------------------------------------------------

#[cfg(feature = "p14")]
pub mod p14 {
    //! P14 batch: 11 ctx.* seam sub-crates.

    // Sub-crate namespaces (for `use ma_harness::p14::subprocess::*;` access)
    pub use ma_harness_subprocess as subprocess;
    pub use ma_harness_shell as shell;
    pub use ma_harness_skill as skill;
    pub use ma_harness_compaction as compaction;
    pub use ma_harness_lsp as lsp;
    pub use ma_harness_web as web;
    pub use ma_harness_todo as todo;
    pub use ma_harness_session as session;
    pub use ma_harness_profile as profile;
    pub use ma_harness_context as context;
    pub use ma_harness_guard as guard;

    // Subprocess (P14.1)
    pub use ma_harness_subprocess::{
        ChildHandle, CommandSpec, ExitStatus, StdioConfig, SubprocessError,
    };

    // Shell (P14.2)
    pub use ma_harness_shell::{
        LocalShellProvider, ShellError, ShellKind, ShellResult, ShellService, ShellSpec,
    };

    // Skill (P14.3)
    pub use ma_harness_skill::{parse_skill_md, SkillCatalog, SkillError, SkillManifest, SkillMetadata};

    // Compaction (P14.4)
    pub use ma_harness_compaction::{
        default_token_estimator, BasicCompactionProvider, CompactionContext, CompactionError,
        CompactionStats, CompactionStrategy, CompactionSummary, DefaultCompactionProvider,
        LlmCompactionProvider, TokenEstimator, COMPACTION_STRATEGY,
    };

    // LSP (P14.5)
    pub use ma_harness_lsp::{next_id, LspError, LspResponse, LspServerError, LspSpec};

    // Web (P14.6)
    pub use ma_harness_web::{WebError, WebFetchQuery, WebFetchResult, WebSearchQuery, WebSearchResult};

    // Todo (P14.7)
    pub use ma_harness_todo::{TodoError, TodoItem, TodoList, TodoStatus, TodoStore};

    // Session (P14.8) — fork / goals / title
    pub use ma_harness_session::{
        BasicTitleProvider, EventForker, Goal, GoalStatus, GoalStore, InMemoryGoalStore,
        SessionError, TitleProvider,
    };

    // Profile (P14.9)
    pub use ma_harness_profile::{
        builtin_profiles, Profile as P14Profile, ProfileError, ProfileLoader, ProfileRegistry,
    };

    // Context (P14.10)
    pub use ma_harness_context::{
        ContextChain, ContextError, ContextMiddleware, LoggingMiddleware, RequestContext,
    };

    // Guard (P14.11)
    pub use ma_harness_guard::{
        GuardChain, GuardDecision, GuardError, LoopEvent, LoopGuard, MaxStepsGuard,
        RepeatedArgsGuard,
    };
}

// ----------------------------------------------------------------------------
// `p15` feature — P15 feature sub-crates
// ----------------------------------------------------------------------------

#[cfg(feature = "p15")]
pub mod p15 {
    //! P15 batch: 6 feature sub-crates.

    // Sub-crate namespaces
    pub use ma_harness_workflow as workflow;
    pub use ma_harness_webhook as webhook;
    pub use ma_harness_settings as settings;
    pub use ma_harness_credentials as credentials;
    pub use ma_harness_hooks as hooks;
    pub use ma_harness_self_modification as self_modification;

    // Workflow (P15.4)
    pub use ma_harness_workflow::{RunResult, Step, StepResult, StepStatus, WorkflowError};

    // Webhook (P15.3)
    pub use ma_harness_webhook::{
        RateLimiter, SignatureAlgorithm, WebhookError, WebhookEvent, WebhookVerifier,
    };

    // Settings (P15.5)
    pub use ma_harness_settings::{
        EnvSettingsStore, FileSettingsStore, Schema, SchemaRule, SchemaType, Settings,
        SettingsError, SettingsStore, ValidationError, ValidationErrorReason,
    };

    // Credentials (P15.5)
    pub use ma_harness_credentials::{
        Credential, CredentialsError, CredentialsStore, DotenvCredentialsStore, EnvCredentialsStore,
    };

    // Hooks (P15.7)
    pub use ma_harness_hooks::{
        ClaudeCodeAdapter, Hook, HookDecision, HookError, HookEvent, HookEventKind, HookResponse,
        NoopHook,
    };

    // Self-modification (P15.6)
    pub use ma_harness_self_modification::{
        AuditAction, AuditEntry, CordisConfig, LocalSelfMod, MountedPlugin, SelfMod, SelfModError,
    };
}

// ----------------------------------------------------------------------------
// `web-ui` and `pty` features — P15.1 / P15.2 future work
// ----------------------------------------------------------------------------

#[cfg(feature = "web-ui")]
pub use ma_harness_web_ui as web_ui;
#[cfg(feature = "pty")]
pub use ma_harness_terminal as pty_backend;

// ============================================================================
// Crate metadata
// ============================================================================

/// Crate version (matches `Cargo.toml`).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Crate name (`ma-harness`).
pub const NAME: &str = "ma-harness";

/// Returns a list of enabled feature names. Useful for diagnostics.
///
/// Source of truth is the [features] table in `Cargo.toml`; this is a
/// hand-maintained mirror kept in sync with `#[cfg(feature = "...")]`
/// gates above.
///
/// # Example
///
/// ```ignore
/// use ma_harness::enabled_features;
/// println!("enabled: {:?}", enabled_features());
/// ```
pub fn enabled_features() -> &'static [&'static str] {
    &[
        // always-on metadata
        "name",
        "version",
        // foundation
        #[cfg(feature = "core")]
        "core",
        #[cfg(feature = "model")]
        "model",
        // P11-P12
        #[cfg(feature = "plugin")]
        "plugin",
        #[cfg(feature = "bundle")]
        "bundle",
        #[cfg(feature = "sandbox")]
        "sandbox",
        #[cfg(feature = "code")]
        "code",
        #[cfg(feature = "dag")]
        "dag",
        #[cfg(feature = "artifact")]
        "artifact",
        #[cfg(feature = "server")]
        "server",
        #[cfg(feature = "tui")]
        "tui",
        // P14 / P15
        #[cfg(feature = "p14")]
        "p14",
        #[cfg(feature = "p15")]
        "p15",
        // future
        #[cfg(feature = "web-ui")]
        "web-ui",
        #[cfg(feature = "pty")]
        "pty",
        #[cfg(feature = "full")]
        "full",
    ]
}
