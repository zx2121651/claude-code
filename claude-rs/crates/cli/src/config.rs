use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::fs;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// E.g., "auto", "default", "plan"
    pub permission_mode: Option<String>,
    /// E.g., "claude-3-7-sonnet-20250219"
    pub model: Option<String>,
    /// System prompt overrides
    pub custom_system_prompt: Option<String>,
    /// List of tools to completely disable in this project
    pub disabled_tools: Option<Vec<String>>,
}

impl Settings {
    /// Merge another Settings object into this one. Fields in `other` overwrite `self`.
    pub fn merge(&mut self, other: Settings) {
        if other.permission_mode.is_some() {
            self.permission_mode = other.permission_mode;
        }
        if other.model.is_some() {
            self.model = other.model;
        }
        if other.custom_system_prompt.is_some() {
            self.custom_system_prompt = other.custom_system_prompt;
        }
        if let Some(mut dt) = other.disabled_tools {
            if let Some(ref mut self_dt) = self.disabled_tools {
                self_dt.append(&mut dt);
            } else {
                self.disabled_tools = Some(dt);
            }
        }
    }
}

pub struct ConfigManager {
    pub global_config_path: PathBuf,
    pub project_config_path: PathBuf,
    pub active_settings: Settings,
}

impl ConfigManager {
    pub async fn new() -> Result<Self> {
        let global_dir = dirs::home_dir().unwrap_or_else(|| PathBuf::from("~")).join(".claude.json");
        let project_dir = std::env::current_dir()?.join(".claude/settings.json");

        let mut manager = Self {
            global_config_path: global_dir,
            project_config_path: project_dir,
            active_settings: Settings::default(),
        };

        manager.load_cascading().await?;
        Ok(manager)
    }

    /// Read a JSON settings file if it exists.
    async fn read_settings_file(path: &Path) -> Result<Option<Settings>> {
        if path.exists() {
            let content = fs::read_to_string(path).await?;
            let settings: Settings = serde_json::from_str(&content)?;
            Ok(Some(settings))
        } else {
            Ok(None)
        }
    }

    /// Load global, then project, then local settings.
    pub async fn load_cascading(&mut self) -> Result<()> {
        let mut final_settings = Settings::default();

        // 1. Load Global Config (~/.claude.json)
        if let Some(global) = Self::read_settings_file(&self.global_config_path).await? {
            final_settings.merge(global);
        }

        // 2. Load Project Config (.claude/settings.json)
        if let Some(project) = Self::read_settings_file(&self.project_config_path).await? {
            final_settings.merge(project);
        }

        // TODO: Load .claude/settings.local.json if it exists

        self.active_settings = final_settings;
        Ok(())
    }
}
