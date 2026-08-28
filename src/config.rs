use anyhow::Result;
use clap::Parser;
use crate::cli::Cli;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub refresh_ms: u64,
    pub ollama: OllamaConfig,
    pub vllm: Vec<VllmConfig>,
    pub ui: UiConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaConfig {
    pub enabled: bool,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VllmConfig {
    pub name: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiConfig {
    pub theme: String,
    pub graphs: bool,
    pub mouse: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            refresh_ms: 500,
            ollama: OllamaConfig {
                enabled: true,
                url: "http://localhost:11434".to_string(),
            },
            vllm: vec![VllmConfig {
                name: "local".to_string(),
                url: "http://localhost:8888".to_string(),
            }],
            ui: UiConfig {
                theme: "default".to_string(),
                graphs: true,
                mouse: true,
            },
        }
    }
}

impl Config {
    pub fn load(cli: Cli) -> Result<Self> {
        let mut config = Self::load_from_file().unwrap_or_default();

        // CLI overrides
        config.refresh_ms = cli.refresh;

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

    fn load_from_file() -> Option<Self> {
        let path = config_path()?;
        let content = std::fs::read_to_string(path).ok()?;
        toml::from_str(&content).ok()
    }
}

fn config_path() -> Option<PathBuf> {
    let config_dir = dirs::config_dir()?;
    Some(config_dir.join("pgxtop").join("config.toml"))
}