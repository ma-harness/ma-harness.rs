//! Smoke tests for the `ma-harness` umbrella crate.
//!
//! These tests verify the feature-gated re-exports compile and resolve
//! to real types under each feature combination. They do not exercise
//! the underlying functionality -- that is the responsibility of the
//! per-crate test suites. The umbrella is purely a re-export layer.
//!
//! Strategies:
//! - For types: reference them in a position that requires them to
//!   resolve (PhantomData, function arg, return type).
//! - For functions: take their address (`let _: fn(...) = path;`).
//! - For static items: reference their type via a `let _: &T = path;`.
//!
//! Run with:
//!     cargo test -p ma-harness --all-features
//!     cargo test -p ma-harness                       (default: core + model)
//!     cargo test -p ma-harness --no-default-features (metadata only)
// ============================================================================
// Always-on metadata
// ============================================================================
#[test]
fn metadata_constants_resolve() {
    assert_eq!(ma_harness::NAME, "ma-harness");
    assert!(!ma_harness::VERSION.is_empty(), "VERSION must be set");
    assert!(
        ma_harness::VERSION.starts_with("0.1"),
        "expected 0.1.x, got {}",
        ma_harness::VERSION
    );
}
#[test]
fn enabled_features_reports_active_set() {
    let features = ma_harness::enabled_features();
    assert!(features.contains(&"name"));
    assert!(features.contains(&"version"));
    let mut sorted = features.to_vec();
    sorted.sort();
    let original_len = sorted.len();
    sorted.dedup();
    assert_eq!(
        sorted.len(),
        original_len,
        "enabled_features() returned duplicates: {features:?}"
    );
}
/// Helper: assert a type `T` exists in the `ma_harness` namespace.
/// Works for both sized types (structs, enums) and unsized types
/// (traits via `dyn Trait`) by using `TypeId::of` which accepts
/// `?Sized` types.
#[track_caller]
fn _phantom<T: ?Sized + 'static>() {
    let _ = std::any::TypeId::of::<T>();
}
// ============================================================================
// `core` feature 鈥?foundation types
// ============================================================================
#[cfg(feature = "core")]
#[test]
fn core_cordis_types_resolve() {
    _phantom::<ma_harness::Context>();
    _phantom::<dyn ma_harness::Disposable>();
    _phantom::<dyn ma_harness::AsyncDisposable>();
    _phantom::<ma_harness::Scope>();
    _phantom::<ma_harness::CtxKey<u32>>();
    _phantom::<ma_harness::BoxedError>();
    _phantom::<ma_harness::CordisError>();
    _phantom::<ma_harness::CordisEvent>();
    _phantom::<ma_harness::EventSeverity>();
    // Approval types
    _phantom::<dyn ma_harness::ApprovalService>();
    _phantom::<ma_harness::ApprovalDecision>();
    _phantom::<ma_harness::ApprovalRequest>();
    _phantom::<ma_harness::ApprovalRegistry>();
    _phantom::<ma_harness::ApprovalPolicy>();
    _phantom::<ma_harness::ChannelApprovalService>();
    _phantom::<ma_harness::RiskLevel>();
}
#[cfg(feature = "core")]
#[test]
fn core_session_and_agent_types_resolve() {
    _phantom::<ma_harness::SessionEvent>();
    _phantom::<ma_harness::EventType>();
    _phantom::<ma_harness::Severity>();
    _phantom::<ma_harness::EventLog>();
    _phantom::<ma_harness::EventQuery>();
    _phantom::<ma_harness::EventPage>();
    _phantom::<ma_harness::StoredEvent>();
    _phantom::<dyn ma_harness::ModelAdapter>();
    _phantom::<ma_harness::ModelRequest>();
    _phantom::<ma_harness::ModelResponse>();
    _phantom::<ma_harness::ModelMessage>();
    _phantom::<ma_harness::FinishReason>();
    _phantom::<ma_harness::AgentLoop>();
    _phantom::<ma_harness::AgentRunRequest>();
    _phantom::<ma_harness::AgentRunResponse>();
    _phantom::<ma_harness::StubModelAdapter>();
    _phantom::<ma_harness::ToolEntry>();
    _phantom::<ma_harness::ToolRegistry>();
    _phantom::<ma_harness::ToolSchema>();
    _phantom::<ma_harness::Profile>();
    _phantom::<ma_harness::ProfileStore>();
    _phantom::<ma_harness::OperatingMode>();
    _phantom::<ma_harness::OperatingModeConfig>();
    _phantom::<ma_harness::CompressionPolicy>();
    // Function re-exports (not types 鈥?check by reference).
    // Just bind the function value; the compiler will reject the
    // re-export if the path doesn't resolve.
    let _ = ma_harness::invoke_with_pipeline;
    let _ = ma_harness::compress;
    // load_agents_md is generic; existence is verified by lib.rs re-export.
}
#[cfg(feature = "core")]
#[test]
fn core_seam_reexports_resolve() {
    // Derive macros are attribute-shaped; the compiler accepts a
    // "value" position only if you write them as attribute uses.
    // For existence check, just touch the constant.
    assert!(!ma_harness::SEAM_VERSION.is_empty());
    assert!(!ma_harness::SEAM_API_VERSION.is_empty());
}
// ============================================================================
// `model` feature 鈥?LLM adapters
// ============================================================================
#[cfg(feature = "model")]
#[test]
fn model_adapters_resolve() {
    _phantom::<ma_harness::OpenaiAdapter>();
    _phantom::<ma_harness::AnthropicAdapter>();
    _phantom::<ma_harness::AdapterRegistry>();
    _phantom::<ma_harness::AdapterError>();
}
#[cfg(feature = "model")]
#[test]
fn model_retry_types_resolve() {
    _phantom::<ma_harness::RetryPolicy>();
    _phantom::<ma_harness::CircuitBreaker>();
    _phantom::<ma_harness::CircuitState>();
    // Functions
    let _ = ma_harness::backoff_for;
    // retry_with_backoff is generic; existence verified by lib.rs re-export.
}
#[cfg(feature = "model")]
#[test]
fn model_vision_types_resolve() {
    _phantom::<ma_harness::ImageAttachment>();
    _phantom::<ma_harness::VisionTool>();
    _phantom::<ma_harness::VisionBackend>();
    _phantom::<ma_harness::VisionError>();
    let _ = ma_harness::describe_image;
    assert!(!ma_harness::VISION_TOOL_NAME.is_empty());
    assert!(!ma_harness::VISION_TOOL_DESCRIPTION.is_empty());
}
// ============================================================================
// `plugin` / `bundle` features
// ============================================================================
#[cfg(feature = "plugin")]
#[test]
fn plugin_registry_types_resolve() {
    _phantom::<ma_harness::PluginSource>();
    _phantom::<ma_harness::PluginManifest>();
    _phantom::<ma_harness::Registry>();
    _phantom::<ma_harness::RegistryError>();
}
#[cfg(feature = "bundle")]
#[test]
fn bundle_types_resolve() {
    _phantom::<ma_harness::Bundle>();
    _phantom::<ma_harness::BundleError>();
    _phantom::<ma_harness::BundleManifest>();
    _phantom::<ma_harness::BundlePlugin>();
    _phantom::<ma_harness::ResolvedPlugin>();
    // load_bundle_from_file is generic; existence verified by lib.rs re-export.
    let _ = ma_harness::load_bundle_from_str;
    let _ = ma_harness::bundle_summary;
}
// ============================================================================
// `sandbox` / `code` / `dag` / `artifact` features
// ============================================================================
#[cfg(feature = "sandbox")]
#[test]
fn sandbox_types_resolve() {
    _phantom::<ma_harness::Policy>();
    _phantom::<ma_harness::PathRule>();
    _phantom::<ma_harness::EnforceError>();
    _phantom::<ma_harness::StubEnforcer>();
}
#[cfg(all(feature = "sandbox", target_os = "linux"))]
#[test]
fn sandbox_linux_enforcer_resolves() {
    _phantom::<ma_harness::LinuxLandlockEnforcer>();
}
#[cfg(all(feature = "sandbox", target_os = "macos"))]
#[test]
fn sandbox_macos_enforcer_resolves() {
    _phantom::<ma_harness::MacosSeatbeltEnforcer>();
}
#[cfg(feature = "code")]
#[test]
fn code_runner_types_resolve() {
    _phantom::<ma_harness::CodeRunner>();
    _phantom::<ma_harness::CodeOutput>();
    _phantom::<ma_harness::SandboxConfig>();
}
#[cfg(feature = "dag")]
#[test]
fn dag_types_resolve() {
    _phantom::<ma_harness::Dag>();
    _phantom::<ma_harness::Task>();
    _phantom::<ma_harness::TaskRun>();
    _phantom::<ma_harness::DagRun>();
    _phantom::<ma_harness::TaskStatus>();
    _phantom::<ma_harness::DagError>();
    _phantom::<ma_harness::DagScheduler>();
    // load_dag_from_file is generic; existence verified by lib.rs re-export.
}
#[cfg(feature = "artifact")]
#[test]
fn artifact_types_resolve() {
    _phantom::<ma_harness::ArtifactKind>();
    _phantom::<ma_harness::ArtifactError>();
    // detect_artifact is generic; existence verified by lib.rs re-export.
    let _ = ma_harness::render_terminal;
}
// ============================================================================
// `server` / `tui` features
// ============================================================================
#[cfg(feature = "server")]
#[test]
fn server_types_resolve() {
    _phantom::<ma_harness::ServerBuilder>();
    _phantom::<ma_harness::AgentServiceImpl>();
    _phantom::<ma_harness::SessionServiceImpl>();
}
#[cfg(feature = "tui")]
#[test]
fn tui_types_resolve() {
    _phantom::<ma_harness::TuiApp>();
}
// ============================================================================
// `p14` module 鈥?11 ctx.* sub-crates
// ============================================================================
#[cfg(feature = "p14")]
#[test]
fn p14_subprocess_resolves() {
    _phantom::<ma_harness::p14::CommandSpec>();
    _phantom::<ma_harness::p14::ChildHandle>();
    _phantom::<ma_harness::p14::ExitStatus>();
    _phantom::<ma_harness::p14::StdioConfig>();
    _phantom::<ma_harness::p14::SubprocessError>();
}
#[cfg(feature = "p14")]
#[test]
fn p14_shell_resolves() {
    _phantom::<ma_harness::p14::ShellError>();
    _phantom::<ma_harness::p14::ShellKind>();
    _phantom::<ma_harness::p14::ShellSpec>();
    _phantom::<ma_harness::p14::ShellResult>();
    _phantom::<ma_harness::p14::LocalShellProvider>();
    _phantom::<dyn ma_harness::p14::ShellService>();
}
#[cfg(feature = "p14")]
#[test]
fn p14_skill_resolves() {
    _phantom::<ma_harness::p14::SkillError>();
    _phantom::<ma_harness::p14::SkillMetadata>();
    _phantom::<ma_harness::p14::SkillManifest>();
    _phantom::<ma_harness::p14::SkillCatalog>();
    let _ = ma_harness::p14::parse_skill_md;
}
#[cfg(feature = "p14")]
#[test]
fn p14_compaction_resolves() {
    _phantom::<ma_harness::p14::CompactionError>();
    _phantom::<ma_harness::p14::CompactionStats>();
    _phantom::<ma_harness::p14::CompactionContext>();
    _phantom::<ma_harness::p14::CompactionSummary>();
    _phantom::<dyn ma_harness::p14::CompactionStrategy>();
    _phantom::<ma_harness::p14::BasicCompactionProvider>();
    _phantom::<ma_harness::p14::LlmCompactionProvider>();
    _phantom::<ma_harness::p14::DefaultCompactionProvider>();
    // The COMPACTION_STRATEGY static is a CtxKey, not the trait
    // itself — type-checked via the original ma_harness_compaction::COMPACTION_STRATEGY
    // declaration. The umbrella re-export is verified by the
    // module-level visibility of ma_harness::p14::COMPACTION_STRATEGY.
    let _ = ma_harness::p14::COMPACTION_STRATEGY;
    let _ = ma_harness::p14::default_token_estimator;
}
#[cfg(feature = "p14")]
#[test]
fn p14_lsp_resolves() {
    _phantom::<ma_harness::p14::LspError>();
    _phantom::<ma_harness::p14::LspSpec>();
    _phantom::<ma_harness::p14::LspResponse>();
    _phantom::<ma_harness::p14::LspServerError>();
    let _ = ma_harness::p14::next_id;
}
#[cfg(feature = "p14")]
#[test]
fn p14_web_resolves() {
    _phantom::<ma_harness::p14::WebError>();
    _phantom::<ma_harness::p14::WebFetchQuery>();
    _phantom::<ma_harness::p14::WebFetchResult>();
    _phantom::<ma_harness::p14::WebSearchQuery>();
    _phantom::<ma_harness::p14::WebSearchResult>();
}
#[cfg(feature = "p14")]
#[test]
fn p14_todo_resolves() {
    _phantom::<ma_harness::p14::TodoError>();
    _phantom::<ma_harness::p14::TodoStatus>();
    _phantom::<ma_harness::p14::TodoItem>();
    _phantom::<ma_harness::p14::TodoList>();
    _phantom::<dyn ma_harness::p14::TodoStore>();
}
#[cfg(feature = "p14")]
#[test]
fn p14_session_resolves() {
    _phantom::<ma_harness::p14::SessionError>();
    _phantom::<ma_harness::p14::EventForker>();
    _phantom::<ma_harness::p14::GoalStatus>();
    _phantom::<ma_harness::p14::Goal>();
    _phantom::<dyn ma_harness::p14::GoalStore>();
    _phantom::<ma_harness::p14::InMemoryGoalStore>();
    _phantom::<dyn ma_harness::p14::TitleProvider>();
    _phantom::<ma_harness::p14::BasicTitleProvider>();
}
#[cfg(feature = "p14")]
#[test]
fn p14_profile_resolves() {
    _phantom::<ma_harness::p14::ProfileError>();
    _phantom::<ma_harness::p14::ProfileLoader>();
    _phantom::<ma_harness::p14::ProfileRegistry>();
    // P14Profile is the P14 profile re-export (renamed to avoid
    // colliding with the core::Profile re-export).
    _phantom::<ma_harness::p14::P14Profile>();
    let _ = ma_harness::p14::builtin_profiles;
}
#[cfg(feature = "p14")]
#[test]
fn p14_context_resolves() {
    _phantom::<ma_harness::p14::ContextError>();
    _phantom::<ma_harness::p14::RequestContext>();
    _phantom::<dyn ma_harness::p14::ContextMiddleware>();
    _phantom::<ma_harness::p14::ContextChain>();
    _phantom::<ma_harness::p14::LoggingMiddleware>();
}
#[cfg(feature = "p14")]
#[test]
fn p14_guard_resolves() {
    _phantom::<ma_harness::p14::LoopEvent>();
    _phantom::<ma_harness::p14::GuardDecision>();
    _phantom::<ma_harness::p14::GuardError>();
    _phantom::<ma_harness::p14::MaxStepsGuard>();
    _phantom::<ma_harness::p14::RepeatedArgsGuard>();
    _phantom::<ma_harness::p14::GuardChain>();
}
// ============================================================================
// `p15` module 鈥?6 feature sub-crates
// ============================================================================
#[cfg(feature = "p15")]
#[test]
fn p15_workflow_resolves() {
    _phantom::<ma_harness::p15::WorkflowError>();
    _phantom::<ma_harness::p15::Step>();
    _phantom::<ma_harness::p15::StepStatus>();
    _phantom::<ma_harness::p15::StepResult>();
    _phantom::<ma_harness::p15::RunResult>();
}
#[cfg(feature = "p15")]
#[test]
fn p15_webhook_resolves() {
    _phantom::<ma_harness::p15::WebhookError>();
    _phantom::<ma_harness::p15::WebhookEvent>();
    _phantom::<ma_harness::p15::SignatureAlgorithm>();
    _phantom::<ma_harness::p15::RateLimiter>();
    _phantom::<dyn ma_harness::p15::WebhookVerifier>();
}
#[cfg(feature = "p15")]
#[test]
fn p15_settings_resolves() {
    _phantom::<ma_harness::p15::Settings>();
    _phantom::<ma_harness::p15::SettingsError>();
    _phantom::<dyn ma_harness::p15::SettingsStore>();
    _phantom::<ma_harness::p15::FileSettingsStore>();
    _phantom::<ma_harness::p15::EnvSettingsStore>();
    _phantom::<ma_harness::p15::Schema>();
    _phantom::<ma_harness::p15::SchemaRule>();
    _phantom::<ma_harness::p15::SchemaType>();
    _phantom::<ma_harness::p15::ValidationError>();
    _phantom::<ma_harness::p15::ValidationErrorReason>();
}
#[cfg(feature = "p15")]
#[test]
fn p15_credentials_resolves() {
    _phantom::<ma_harness::p15::Credential>();
    _phantom::<ma_harness::p15::CredentialsError>();
    _phantom::<dyn ma_harness::p15::CredentialsStore>();
    _phantom::<ma_harness::p15::EnvCredentialsStore>();
    _phantom::<ma_harness::p15::DotenvCredentialsStore>();
}
#[cfg(feature = "p15")]
#[test]
fn p15_hooks_resolves() {
    _phantom::<ma_harness::p15::HookError>();
    _phantom::<ma_harness::p15::HookEventKind>();
    _phantom::<ma_harness::p15::HookEvent>();
    _phantom::<ma_harness::p15::HookDecision>();
    _phantom::<ma_harness::p15::HookResponse>();
    _phantom::<ma_harness::p15::NoopHook>();
    _phantom::<ma_harness::p15::ClaudeCodeAdapter>();
}
#[cfg(feature = "p15")]
#[test]
fn p15_self_modification_resolves() {
    _phantom::<ma_harness::p15::SelfModError>();
    _phantom::<ma_harness::p15::MountedPlugin>();
    _phantom::<ma_harness::p15::CordisConfig>();
    _phantom::<ma_harness::p15::AuditEntry>();
    _phantom::<ma_harness::p15::AuditAction>();
    _phantom::<ma_harness::p15::LocalSelfMod>();
}
// ============================================================================
// Future-work features
// ============================================================================
#[cfg(feature = "web-ui")]
#[test]
fn web_ui_namespace_resolves() {
    // The `web_ui` alias is `pub use ma_harness_web_ui as web_ui;`.
    _phantom::<ma_harness::web_ui::LocalWebUiServer>();
    _phantom::<ma_harness::web_ui::SseEvent>();
    let _ = ma_harness::web_ui::html_shell;
}
#[cfg(feature = "pty")]
#[test]
fn pty_namespace_resolves() {
    _phantom::<ma_harness::pty_backend::LocalPtyProvider>();
    _phantom::<ma_harness::pty_backend::TerminalSpec>();
    _phantom::<ma_harness::pty_backend::TerminalHandle>();
}
