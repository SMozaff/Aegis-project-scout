use anyhow::{Context, Result};
use dirs::home_dir;
use keyring::Entry;
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

use crate::models::Pattern;

const SERVICE: &str = "raven-api-hunter";
const TOKEN_KEY: &str = "github-token";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub github_token: Option<String>,
    pub lookback_days: u32,
    pub max_results: u32,
    pub targets: Vec<String>,
    pub patterns: Vec<Pattern>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            github_token: None,
            lookback_days: 30,
            max_results: 100,
            targets: vec!["Rust".into(), "Python".into(), "TypeScript".into()],
            patterns: Vec::new(),
        }
    }
}

fn config_path() -> Result<PathBuf> {
    Ok(home_dir()
        .context("home directory unavailable")?
        .join(".config")
        .join("raven-api-hunter")
        .join("config.json"))
}

pub fn load_config() -> Result<AppConfig> {
    let path = config_path()?;
    if !path.exists() {
        let config = AppConfig {
            patterns: load_patterns().unwrap_or_default(),
            ..Default::default()
        };
        save_config(&config)?;
        return Ok(config);
    }

    let data = fs::read(path)?;
    let mut config: AppConfig = serde_json::from_slice(&data)?;
    config.github_token = get_token();
    Ok(config)
}

pub fn save_config(config: &AppConfig) -> Result<()> {
    let path = config_path()?;
    fs::create_dir_all(path.parent().unwrap())?;

    let mut stored = config.clone();
    stored.github_token = None;

    fs::write(path, serde_json::to_vec_pretty(&stored)?)?;
    Ok(())
}

pub fn load_patterns() -> Result<Vec<Pattern>> {
    let path = PathBuf::from("patterns/default_patterns.json");
    if !path.exists() {
        return Ok(Vec::new());
    }

    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

pub fn save_token(token: &str) -> Result<()> {
    let entry = Entry::new(SERVICE, TOKEN_KEY)?;
    entry.set_password(token)?;
    Ok(())
}

pub fn get_token() -> Option<String> {
    Entry::new(SERVICE, TOKEN_KEY)
        .ok()
        .and_then(|e| e.get_password().ok())
}
