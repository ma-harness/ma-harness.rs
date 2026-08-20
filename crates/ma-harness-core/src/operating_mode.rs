//! Operating Mode (P8-4 / Day 101)
//!
//! 定义 4 种 session 模式, 跟 dsh design 对齐:
//! - **Default**: 全功能, 装所有 first-party plugins
//! - **Minimal**: 不加载 plugins, 纯 LLM 调 (轻量, 测试用)
//! - **PTC (Persistent Tool Calling)**: 单轮多 tool 调, 不在中间中断 (Code Mode 类似)
//! - **Creator**: 允许 model 创建新 plugin / service / tool (高级用户, Phase 9 完整版)
//!
//! 业务方在 CreateSessionRequest 设 mode, AgentLoop 读 ctx.operating_mode()
//! 决定行为. v1 简化: enum + 行为描述, AgentLoop v2 集成.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

/// Operating mode 枚举 (P8-4)
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperatingMode {
    /// 全功能, 装所有 first-party plugins
    #[default]
    Default,
    /// 不加载 plugins, 纯 LLM 调 (轻量, 测试)
    Minimal,
    /// Persistent Tool Calling: 单轮多 tool 调, 不在中间中断
    /// 跟 Code Mode 类似, model 在一个 turn 内可调多个 tool
    Ptc,
    /// 允许 model 创建新 plugin / service / tool
    Creator,
}

impl OperatingMode {
    /// 从 proto enum 值 (i32) 解析
    ///
    /// proto 已经有 OPERATING_MODE_DEFAULT=1, OPERATING_MODE_MINIMAL=2 (剩余预留给 PTC=3, Creator=4)
    pub fn from_i32(v: i32) -> Self {
        match v {
            1 => OperatingMode::Default,
            2 => OperatingMode::Minimal,
            3 => OperatingMode::Ptc,
            4 => OperatingMode::Creator,
            _ => OperatingMode::Default,
        }
    }

    /// 返 proto enum 值
    pub fn to_i32(self) -> i32 {
        match self {
            OperatingMode::Default => 1,
            OperatingMode::Minimal => 2,
            OperatingMode::Ptc => 3,
            OperatingMode::Creator => 4,
        }
    }

    /// 这种模式是否装 first-party plugins
    pub fn loads_first_party_plugins(self) -> bool {
        match self {
            OperatingMode::Default | OperatingMode::Ptc | OperatingMode::Creator => true,
            OperatingMode::Minimal => false,
        }
    }

    /// 这种模式是否允许 model 在一个 turn 内多次调 tool (PTC / Creator 启用)
    pub fn allows_multi_tool_per_turn(self) -> bool {
        match self {
            OperatingMode::Ptc | OperatingMode::Creator => true,
            OperatingMode::Default | OperatingMode::Minimal => false,
        }
    }

    /// 这种模式是否允许 model 创建新 plugin (Creator 启用)
    pub fn allows_plugin_creation(self) -> bool {
        // clippy 提示: match {x => true, _ => false} 简化为 `matches!`
        matches!(self, OperatingMode::Creator)
    }

    /// 这种模式需要的最小 system prompt 提示 (给 model 解释当前模式)
    pub fn system_prompt_hint(self) -> &'static str {
        match self {
            OperatingMode::Default => "",
            OperatingMode::Minimal => {
                "You are running in Minimal mode. No plugins are loaded; you can only respond with text."
            }
            OperatingMode::Ptc => {
                "You are running in PTC (Persistent Tool Calling) mode. You can call multiple tools in a single turn without intermediate confirmation."
            }
            OperatingMode::Creator => {
                "You are running in Creator mode. You may create new plugins / services / tools as needed."
            }
        }
    }
}

/// CtxKey "operating_mode" (P8-4)
pub static OPERATING_MODE: ma_harness_cordis::CtxKey<OperatingMode> =
    ma_harness_cordis::CtxKey::new_unchecked("operating_mode");

/// Mode config 工厂 (P8-4)
#[derive(Debug, Clone)]
pub struct OperatingModeConfig {
    pub mode: OperatingMode,
    /// 自定义 enabled plugins 列表 (None = use default for mode)
    pub enabled_plugins: Option<Vec<String>>,
    /// 是否启用审批 (Default + Creator = 启用, Minimal = 关闭)
    pub approval_enabled: bool,
    /// max tool calls per turn
    pub max_tool_calls_per_turn: u32,
}

impl OperatingModeConfig {
    /// 构造默认 (Default mode, 全插件, 审批启用)
    pub fn new(mode: OperatingMode) -> Self {
        Self {
            mode,
            enabled_plugins: None,
            approval_enabled: mode.approves_by_default(),
            max_tool_calls_per_turn: match mode {
                OperatingMode::Minimal => 0,
                OperatingMode::Default => 1,
                OperatingMode::Ptc => 10,
                OperatingMode::Creator => 20,
            },
        }
    }

    /// 业务方覆盖 enabled plugins
    pub fn with_plugins(mut self, plugins: Vec<String>) -> Self {
        self.enabled_plugins = Some(plugins);
        self
    }

    /// 拿最终 enabled plugins (default for mode 或 override)
    pub fn effective_plugins(&self) -> HashSet<String> {
        if let Some(p) = &self.enabled_plugins {
            return p.iter().cloned().collect();
        }
        // 默认 first-party plugins (跟 dsh 一致)
        let mut set: HashSet<String> = HashSet::new();
        if self.mode.loads_first_party_plugins() {
            set.insert("hello".to_string());
            set.insert("bash".to_string());
            set.insert("fs".to_string());
            set.insert("web".to_string());
            set.insert("subagent".to_string());
            set.insert("skill".to_string());
            set.insert("cordis".to_string());
        }
        set
    }
}

impl OperatingMode {
    fn approves_by_default(self) -> bool {
        match self {
            OperatingMode::Default | OperatingMode::Ptc | OperatingMode::Creator => true,
            OperatingMode::Minimal => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_default_is_default() {
        assert_eq!(OperatingMode::default(), OperatingMode::Default);
    }

    #[test]
    fn mode_from_to_i32_round_trip() {
        for m in [
            OperatingMode::Default,
            OperatingMode::Minimal,
            OperatingMode::Ptc,
            OperatingMode::Creator,
        ] {
            assert_eq!(OperatingMode::from_i32(m.to_i32()), m);
        }
    }

    #[test]
    fn minimal_no_plugins() {
        assert!(!OperatingMode::Minimal.loads_first_party_plugins());
    }

    #[test]
    fn default_loads_plugins() {
        assert!(OperatingMode::Default.loads_first_party_plugins());
        assert!(OperatingMode::Ptc.loads_first_party_plugins());
        assert!(OperatingMode::Creator.loads_first_party_plugins());
    }

    #[test]
    fn ptc_allows_multi_tool() {
        assert!(OperatingMode::Ptc.allows_multi_tool_per_turn());
        assert!(OperatingMode::Creator.allows_multi_tool_per_turn());
        assert!(!OperatingMode::Default.allows_multi_tool_per_turn());
        assert!(!OperatingMode::Minimal.allows_multi_tool_per_turn());
    }

    #[test]
    fn creator_only_allows_plugin_creation() {
        assert!(OperatingMode::Creator.allows_plugin_creation());
        assert!(!OperatingMode::Default.allows_plugin_creation());
        assert!(!OperatingMode::Minimal.allows_plugin_creation());
        assert!(!OperatingMode::Ptc.allows_plugin_creation());
    }

    #[test]
    fn minimal_disables_approval() {
        let cfg = OperatingModeConfig::new(OperatingMode::Minimal);
        assert!(!cfg.approval_enabled);
    }

    #[test]
    fn default_enables_approval() {
        let cfg = OperatingModeConfig::new(OperatingMode::Default);
        assert!(cfg.approval_enabled);
    }

    #[test]
    fn ptc_max_tool_calls_10() {
        let cfg = OperatingModeConfig::new(OperatingMode::Ptc);
        assert_eq!(cfg.max_tool_calls_per_turn, 10);
    }

    #[test]
    fn minimal_max_tool_calls_0() {
        let cfg = OperatingModeConfig::new(OperatingMode::Minimal);
        assert_eq!(cfg.max_tool_calls_per_turn, 0);
    }

    #[test]
    fn effective_plugins_default_lists_first_party() {
        let cfg = OperatingModeConfig::new(OperatingMode::Default);
        let p = cfg.effective_plugins();
        assert!(p.contains("hello"));
        assert!(p.contains("bash"));
        assert_eq!(p.len(), 7);
    }

    #[test]
    fn effective_plugins_minimal_empty() {
        let cfg = OperatingModeConfig::new(OperatingMode::Minimal);
        let p = cfg.effective_plugins();
        assert!(p.is_empty());
    }

    #[test]
    fn effective_plugins_override() {
        let cfg = OperatingModeConfig::new(OperatingMode::Default)
            .with_plugins(vec!["custom".to_string()]);
        let p = cfg.effective_plugins();
        assert_eq!(p.len(), 1);
        assert!(p.contains("custom"));
    }

    #[test]
    fn system_prompt_hint_per_mode() {
        assert_eq!(
            OperatingMode::Minimal.system_prompt_hint(),
            "You are running in Minimal mode. No plugins are loaded; you can only respond with text."
        );
        assert!(OperatingMode::Ptc.system_prompt_hint().contains("PTC"));
        assert!(
            OperatingMode::Creator
                .system_prompt_hint()
                .contains("Creator")
        );
        assert_eq!(OperatingMode::Default.system_prompt_hint(), "");
    }
}
