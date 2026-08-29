use std::path::PathBuf;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::cli::Cli;
use crate::collectors::inference::InferenceConfig;

/// Every struct here carries `#[serde(default)]`.
///
/// Without it a `config.toml` that predates any newly added key fails to parse
/// as a whole — and `load_from_file` used to swallow that error with `.ok()`,
/// so the user silently got defaults with no indication why.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub refresh_ms: u64,
    pub ollama: OllamaConfig,
    pub vllm: Vec<VllmConfig>,
    pub inference: InferenceSettings,
    pub ui: UiConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct OllamaConfig {
    pub enabled: bool,
    pub url: String,
    /// Fetch `/api/show` for the selected model when the detail overlay opens.
    pub show_details: bool,
    /// Also list installed-but-not-loaded models from `/api/tags`.
    pub include_installed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct VllmConfig {
    pub name: String,
    pub url: String,
}

/// Engine polling, deliberately separate from the render cadence: `ollama ps`
/// changes on the order of minutes, so polling it twice a second is waste.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct InferenceSettings {
    pub refresh_ms: u64,
    pub tags_refresh_ms: u64,
    pub timeout_ms: u64,
    pub connect_timeout_ms: u64,
    pub show_timeout_ms: u64,
    pub stale_after_ms: u64,
    pub drop_after_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UiConfig {
    pub theme: String,
    pub graphs: bool,
    pub mouse: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            refresh_ms: 500,
            ollama: OllamaConfig::default(),
            vllm: vec![VllmConfig::default()],
            inference: InferenceSettings::default(),
            ui: UiConfig::default(),
        }
    }
}

impl Default for OllamaConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            url: "http://localhost:11434".to_string(),
            show_details: true,
            include_installed: true,
        }
    }
}

impl Default for VllmConfig {
    fn default() -> Self {
        Self {
            name: "local".to_string(),
            url: "http://localhost:8888".to_string(),
        }
    }
}

impl Default for InferenceSettings {
    fn default() -> Self {
        let d = InferenceConfig::default();
        Self {
            refresh_ms: d.refresh_ms,
            tags_refresh_ms: d.tags_refresh_ms,
            timeout_ms: d.timeout_ms,
            connect_timeout_ms: d.connect_timeout_ms,
            show_timeout_ms: d.show_timeout_ms,
            stale_after_ms: d.stale_after_ms,
            drop_after_ms: d.drop_after_ms,
        }
    }
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            theme: "default".to_string(),
            graphs: true,
            mouse: false,
        }
    }
}

impl Config {
    pub fn load(cli: Cli) -> Result<Self> {
        let mut config = Self::load_from_file().unwrap_or_default();

        // CLI overrides. `refresh` is an Option rather than carrying a clap
        // default, so an unset flag no longer clobbers the config file value.
        if let Some(refresh) = cli.refresh {
            config.refresh_ms = refresh;
        }

        if cli.no_ollama {
            config.ollama.enabled = false;
        }
        if let Some(url) = cli.ollama {
            config.ollama.enabled = true;
            config.ollama.url = url;
        }

        if cli.no_vllm {
            config.vllm.clear();
        }
        if let Some(url) = cli.vllm {
            config.vllm.push(VllmConfig {
                name: "cli".to_string(),
                url,
            });
        }

        if let Some(theme) = cli.theme {
            config.ui.theme = theme;
        }

        Ok(config)
    }

    /// Assembles the poller settings from the two config sections.
    pub fn inference_config(&self) -> InferenceConfig {
        InferenceConfig {
            refresh_ms: self.inference.refresh_ms,
            tags_refresh_ms: self.inference.tags_refresh_ms,
            timeout_ms: self.inference.timeout_ms,
            connect_timeout_ms: self.inference.connect_timeout_ms,
            show_timeout_ms: self.inference.show_timeout_ms,
            stale_after_ms: self.inference.stale_after_ms,
            drop_after_ms: self.inference.drop_after_ms,
            show_details: self.ollama.show_details,
            include_installed: self.ollama.include_installed,
        }
    }

    fn load_from_file() -> Option<Self> {
        let path = config_path()?;
        let content = std::fs::read_to_string(&path).ok()?;
        match toml::from_str(&content) {
            Ok(cfg) => Some(cfg),
            Err(e) => {
                // Previously `.ok()` — a user whose config stopped working got
                // no signal at all.
                tracing::warn!(
                    target: "pgxtop::config",
                    "ignoring {}: {e}", path.display()
                );
                None
            }
        }
    }
}

fn config_path() -> Option<PathBuf> {
    Some(dirs::config_dir()?.join("pgxtop").join("config.toml"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A config predating every key added by this feature must still load.
    #[test]
    fn a_partial_config_file_still_parses() {
        let toml_src = r#"
refresh_ms = 250

[ollama]
url = "http://box:11434"
"#;
        let cfg: Config = toml::from_str(toml_src).expect("partial config must parse");
        assert_eq!(cfg.refresh_ms, 250);
        assert_eq!(cfg.ollama.url, "http://box:11434");
        // Untouched keys fall back to defaults rather than failing the parse.
        assert!(cfg.ollama.enabled);
        assert!(cfg.ollama.show_details);
        assert_eq!(cfg.inference.refresh_ms, 2000);
        assert_eq!(cfg.ui.theme, "default");
    }

    #[test]
    fn an_empty_config_file_yields_defaults() {
        let cfg: Config = toml::from_str("").expect("empty config must parse");
        assert_eq!(cfg.refresh_ms, 500);
        assert_eq!(cfg.vllm.len(), 1);
    }

    #[test]
    fn config_round_trips_through_toml() {
        let cfg = Config::default();
        let s = toml::to_string(&cfg).expect("serialize");
        let back: Config = toml::from_str(&s).expect("deserialize");
        assert_eq!(back.refresh_ms, cfg.refresh_ms);
        assert_eq!(back.ollama.url, cfg.ollama.url);
        assert_eq!(back.inference.timeout_ms, cfg.inference.timeout_ms);
    }

    #[test]
    fn inference_config_merges_both_sections() {
        let mut cfg = Config::default();
        cfg.inference.timeout_ms = 900;
        cfg.ollama.show_details = false;
        let ic = cfg.inference_config();
        assert_eq!(ic.timeout_ms, 900);
        assert!(!ic.show_details);
        assert!(ic.include_installed);
    }
}
