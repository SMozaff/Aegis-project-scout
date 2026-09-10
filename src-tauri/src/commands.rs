use std::{
    collections::HashSet,
    sync::atomic::{AtomicBool, Ordering},
};

use chrono::{Duration, Utc};
use tauri::{AppHandle, Emitter, State};

use crate::{
    utils::config,
    export,
    scanner::github::GithubScanner,
    health,
    models::{
        AppSettings, ExportResult, RepositoryFinding, ScanConfig, ScanMetrics, ScanProgress,
        ScanReport, TokenValidation,
    },
    scanner::pattern_analyzer::PatternAnalyzer,
    AppState,
};

const MAX_HEALTH_PER_REPOSITORY: usize = 8;
const MAX_HEALTH_PER_SCAN: usize = 30;
const MAX_BLOB_READS_PER_SCAN: usize = 500;

#[tauri::command]
pub fn load_settings(app: AppHandle) -> Result<AppSettings, String> {
    config::load(&app)
}

#[tauri::command]
pub fn save_settings(app: AppHandle, settings: AppSettings) -> Result<AppSettings, String> {
    config::save(&app, &settings)
}

#[tauri::command]
pub async fn validate_github_token(token: String) -> Result<TokenValidation, String> {
    Ok(GithubClient::validate_token(token).await)
}

#[tauri::command]
pub fn export_report(
    app: AppHandle,
    report: ScanReport,
    format: String,
) -> Result<ExportResult, String> {
    export::export(&app, &report, &format)
}

#[tauri::command]
pub async fn run_scan(
    app: AppHandle,
    state: State<'_, AppState>,
    config: ScanConfig,
) -> Result<ScanReport, String> {
    let settings = config.settings();
    config::validate_settings(&settings)?;
    if config
        .github_token
        .as_ref()
        .is_some_and(|token| token.len() > 8_192)
    {
        return Err("GitHub token is unreasonably long.".into());
    }

    if state.scanning.swap(true, Ordering::SeqCst) {
        return Err("A scan is already running.".into());
    }
    let _reset = ScanReset(&state.scanning);

    let started_at = Utc::now();
    emit_progress(
        &app,
        "discovering",
        "Discovering recently active public repositories…",
        0,
        1,
        None,
    );

    let github = GithubClient::new(config.github_token.clone())?;
    let analyzer = PatternAnalyzer::load_default()?;
    let since = Utc::now() - Duration::days(settings.lookback_days as i64);
    let repositories = github
        .discover_repositories(
            &settings.languages,
            since,
            settings.max_repositories as usize,
        )
        .await?;

    let mut metrics = ScanMetrics {
        repositories_discovered: repositories.len(),
        ..Default::default()
    };
    let mut findings: Vec<RepositoryFinding> = Vec::with_capacity(repositories.len());
    let mut global_endpoints = HashSet::new();
    let mut health_checked = 0usize;
    let mut blob_read_budget = MAX_BLOB_READS_PER_SCAN;
    let total = repositories.len().max(1);

    for (index, repository) in repositories.into_iter().enumerate() {
        let repository_name = repository.full_name.clone();
        emit_progress(
            &app,
            "repository",
            &format!("Scanning {repository_name}"),
            index,
            total,
            Some(repository_name.clone()),
        );

        let allocated_file_reads = (settings.max_files_per_repository as usize).min(blob_read_budget);
        blob_read_budget = blob_read_budget.saturating_sub(allocated_file_reads);

        let mut finding = github
            .scan_repository(repository, &analyzer, allocated_file_reads)
            .await;
        if allocated_file_reads == 0 {
            finding.warnings.push(
                "Per-scan GitHub blob-read budget reached; no additional source blobs were fetched."
                    .into(),
            );
        }

        let endpoints: Vec<String> = finding
            .matches
            .iter()
            .filter_map(|matched| matched.absolute_endpoint.clone())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();

        for endpoint in &endpoints {
            global_endpoints.insert(endpoint.clone());
        }

        if settings.health_check && health_checked < MAX_HEALTH_PER_SCAN {
            for endpoint in endpoints
                .into_iter()
                .take(MAX_HEALTH_PER_REPOSITORY)
                .take(MAX_HEALTH_PER_SCAN - health_checked)
            {
                emit_progress(
                    &app,
                    "health",
                    &format!("Checking public endpoint from {repository_name}"),
                    index,
                    total,
                    Some(repository_name.clone()),
                );
                let result = health::check_endpoint(&endpoint).await;
                if result.reachable {
                    metrics.reachable_endpoints += 1;
                }
                if result.blocked {
                    metrics.blocked_health_checks += 1;
                }
                finding.health.push(result);
                health_checked += 1;
            }
        }

        metrics.repositories_scanned += 1;
        metrics.files_scanned += finding.scanned_files;
        metrics.total_matches += finding.matches.len();

        let _ = app.emit("scan://finding", &finding);
        findings.push(finding);
    }

    metrics.unique_absolute_endpoints = global_endpoints.len();
    let completed_at = Utc::now();
    let report = ScanReport {
        generated_at: completed_at.to_rfc3339(),
        started_at: started_at.to_rfc3339(),
        completed_at: completed_at.to_rfc3339(),
        settings,
        metrics,
        findings,
    };

    emit_progress(
        &app,
        "complete",
        "Scan complete.",
        total,
        total,
        None,
    );
    Ok(report)
}

fn emit_progress(
    app: &AppHandle,
    stage: &str,
    message: &str,
    completed: usize,
    total: usize,
    repository: Option<String>,
) {
    let _ = app.emit(
        "scan://progress",
        ScanProgress {
            stage: stage.to_string(),
            message: message.to_string(),
            completed,
            total,
            repository,
        },
    );
}

struct ScanReset<'a>(&'a AtomicBool);

impl Drop for ScanReset<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}
