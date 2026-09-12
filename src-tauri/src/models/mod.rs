use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub languages: Vec<String>,
    pub lookback_days: u16,
    pub max_repositories: u16,
    pub health_check: bool,
    pub max_files_per_repository: u16,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            languages: vec![
                "TypeScript".into(),
                "JavaScript".into(),
                "Python".into(),
                "Go".into(),
                "Rust".into(),
            ],
            lookback_days: 30,
            max_repositories: 12,
            health_check: true,
            max_files_per_repository: 25,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanConfig {
    #[serde(default)]
    pub github_token: Option<String>,
    /// Credential verification is enabled for clients that omit this field.
    #[serde(default = "default_true")]
    pub verify_credentials: bool,
    pub languages: Vec<String>,
    pub lookback_days: u16,
    pub max_repositories: u16,
    /// Endpoint health checks are enabled for clients that omit this field.
    #[serde(default = "default_true")]
    pub health_check: bool,
    pub max_files_per_repository: u16,
    /// Walk git history for deleted/modified credential files.
    #[serde(default)]
    pub scan_history: bool,
}

fn default_true() -> bool {
    true
}

impl ScanConfig {
    pub fn settings(&self) -> AppSettings {
        AppSettings {
            languages: self.languages.clone(),
            lookback_days: self.lookback_days,
            max_repositories: self.max_repositories,
            health_check: self.health_check,
            max_files_per_repository: self.max_files_per_repository,
        }
    }
}

pub mod endpoint;
pub mod pattern;
pub mod project;

pub use endpoint::*;
pub use project::*;

pub use pattern::*;
