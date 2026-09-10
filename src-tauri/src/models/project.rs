
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::time::Duration;

use super::endpoint::ApiEndpoint;
use super::pattern::Pattern;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectDiscovery {
    pub id: String,
    pub name: String,
    pub repository_url: String,
    pub description: Option<String>,
    pub discovered_at: DateTime<Utc>,
    pub last_updated: Option<DateTime<Utc>>,
    pub stars: u32,
    pub forks: u32,
    pub tech_stack: Vec<String>,
    pub endpoints: Vec<ApiEndpoint>,
    pub health_status: Option<String>,
    pub confidence_score: f64,
    pub evidence: String,
    pub source_file: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStatus {
    pub endpoint: String,
    pub status_code: Option<u16>,
    pub response_time_ms: Option<u128>,
    pub is_healthy: bool,
    pub checked_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanRequest {
    pub targets: Vec<String>,
    pub lookback_days: u32,
    pub max_results: u32,
    pub source: Vec<String>,
    pub web_search: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanResult {
    pub discoveries: Vec<ProjectDiscovery>,
    pub errors: Vec<String>,
    pub total_found: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepositorySummary {
    pub full_name: String,
    pub html_url: String,
    pub description: Option<String>,
    pub language: Option<String>,
    pub default_branch: String,
    pub stars: u64,
    pub forks: u64,
    pub pushed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepositoryFinding {
    pub repository: RepositorySummary,
    pub scanned_files: usize,
    pub matches: Vec<crate::models::endpoint::PatternMatch>,
    pub health: Vec<crate::models::endpoint::EndpointHealth>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScanMetrics {
    pub repositories_discovered: usize,
    pub repositories_scanned: usize,
    pub files_scanned: usize,
    pub total_matches: usize,
    pub unique_absolute_endpoints: usize,
    pub reachable_endpoints: usize,
    pub blocked_health_checks: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanReport {
    pub generated_at: String,
    pub started_at: String,
    pub completed_at: String,
    pub settings: crate::models::AppSettings,
    pub metrics: ScanMetrics,
    pub findings: Vec<RepositoryFinding>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanProgress {
    pub stage: String,
    pub message: String,
    pub completed: usize,
    pub total: usize,
    pub repository: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenValidation {
    pub authenticated: bool,
    pub login: Option<String>,
    pub rate_limit_remaining: Option<u64>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportResult {
    pub path: String,
    pub format: String,
}
