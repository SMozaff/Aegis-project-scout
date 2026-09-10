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
    fn default() -> Self { Self { languages: vec!["TypeScript".into(), "JavaScript".into(), "Python".into(), "Go".into(), "Rust".into()], lookback_days: 30, max_repositories: 12, health_check: true, max_files_per_repository: 25 } }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanConfig {
    #[serde(default)] pub github_token: Option<String>,
    pub languages: Vec<String>,
    pub lookback_days: u16,
    pub max_repositories: u16,
    pub health_check: bool,
    pub max_files_per_repository: u16,
}
impl ScanConfig { pub fn settings(&self)->AppSettings { AppSettings { languages:self.languages.clone(), lookback_days:self.lookback_days, max_repositories:self.max_repositories, health_check:self.health_check, max_files_per_repository:self.max_files_per_repository } } }

pub mod project;
pub mod endpoint;
pub mod pattern;

pub use project::*;
pub use endpoint::*;

pub use pattern::*;
