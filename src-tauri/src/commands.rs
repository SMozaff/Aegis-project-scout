use std::{
    collections::HashSet,
    sync::atomic::{AtomicBool, Ordering},
};

use chrono::Utc;
use reqwest::header::{ACCEPT, AUTHORIZATION, USER_AGENT};
use serde::Deserialize;
use tauri::{AppHandle, Emitter, State};

use crate::{
    export,
    models::{
        AppSettings, ExportResult, RepositoryFinding, RepositorySummary, ScanConfig, ScanMetrics,
        ScanProgress, ScanReport, TokenValidation, VerifiedCredential,
    },
    scanner::{
        github::GithubScanner,
        health_checker,
        pattern_analyzer::PatternAnalyzer,
        verify::{ProviderVerifier, VerifyOutcome},
    },
    utils::config,
    AppState,
};

const MAX_HEALTH_PER_REPOSITORY: usize = 8;
const MAX_HEALTH_PER_SCAN: usize = 30;
const MAX_BLOB_READS_PER_SCAN: usize = 500;

#[tauri::command]
pub fn load_settings(_app: AppHandle) -> Result<AppSettings, String> {
    let stored = config::load_config().map_err(|error| error.to_string())?;
    Ok(AppSettings {
        languages: stored.targets,
        lookback_days: stored.lookback_days.min(u16::MAX as u32) as u16,
        max_repositories: stored.max_results.min(u16::MAX as u32) as u16,
        health_check: true,
        max_files_per_repository: 25,
    })
}

#[tauri::command]
pub fn save_settings(_app: AppHandle, settings: AppSettings) -> Result<AppSettings, String> {
    let mut stored = config::load_config().map_err(|error| error.to_string())?;
    stored.targets = settings.languages.clone();
    stored.lookback_days = settings.lookback_days as u32;
    stored.max_results = settings.max_repositories as u32;
    config::save_config(&stored).map_err(|error| error.to_string())?;
    Ok(settings)
}

#[tauri::command]
pub async fn validate_github_token(token: String) -> Result<TokenValidation, String> {
    if token.trim().is_empty() {
        return Ok(TokenValidation {
            authenticated: false,
            login: None,
            rate_limit_remaining: None,
            message: "GitHub token is required.".into(),
        });
    }
    if token.len() > 8_192 {
        return Err("GitHub token is unreasonably long.".into());
    }

    #[derive(Deserialize)]
    struct GithubUser {
        login: String,
    }

    let client = reqwest::Client::builder()
        .user_agent("Aegis-Project-Scout")
        .build()
        .map_err(|error| error.to_string())?;
    let response = client
        .get("https://api.github.com/user")
        .header(USER_AGENT, "Aegis-Project-Scout")
        .header(ACCEPT, "application/vnd.github+json")
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .send()
        .await
        .map_err(|error| error.to_string())?;
    let remaining = response
        .headers()
        .get("x-ratelimit-remaining")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok());

    if !response.status().is_success() {
        return Ok(TokenValidation {
            authenticated: false,
            login: None,
            rate_limit_remaining: remaining,
            message: format!("GitHub rejected the token ({})", response.status()),
        });
    }

    let user: GithubUser = response.json().await.map_err(|error| error.to_string())?;
    Ok(TokenValidation {
        authenticated: true,
        login: Some(user.login.clone()),
        rate_limit_remaining: remaining,
        message: format!("Authenticated as {}", user.login),
    })
}

#[tauri::command]
pub async fn verify_credential(
    provider: String,
    token: String,
) -> Result<VerifyOutcome, String> {
    if token.trim().is_empty() {
        return Err("Credential token is required.".into());
    }
    if token.len() > 8_192 {
        return Err("Credential token is unreasonably long.".into());
    }

    let verifier = ProviderVerifier::new();
    let provider_name = provider.trim();
    match provider_name.to_ascii_lowercase().as_str() {
        "openai" => verifier.verify_openai(&token).await,
        "anthropic" => verifier.verify_anthropic(&token).await,
        "github" => verifier.verify_github(&token).await,
        "aws" => verifier.verify_aws(&token, "").await,
        _ if provider_name.starts_with("http://") || provider_name.starts_with("https://") => {
            verifier.verify_generic(&token, provider_name).await
        }
        _ => Err(format!(
            "Unsupported provider: {provider_name}. Use openai, anthropic, github, aws, or an HTTPS URL."
        )),
    }
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
    if settings.languages.is_empty() {
        return Err("Select at least one language or technology.".into());
    }
    if settings.lookback_days == 0
        || settings.max_repositories == 0
        || settings.max_files_per_repository == 0
    {
        return Err("Scan limits must be greater than zero.".into());
    }
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

    let github =
        GithubScanner::new(config.github_token.clone()).map_err(|error| error.to_string())?;
    let analyzer = PatternAnalyzer::load_default()?;
    let query = settings.languages.join(" OR ");
    let mut repositories = github
        .search_projects(&query, settings.lookback_days as u32)
        .await
        .map_err(|error| error.to_string())?;
    repositories.truncate(settings.max_repositories as usize);

    let mut metrics = ScanMetrics {
        repositories_discovered: repositories.len(),
        ..Default::default()
    };
    let mut findings = Vec::with_capacity(repositories.len());
    let mut global_endpoints = HashSet::new();
    let mut verified_credentials = Vec::new();
    let mut health_checked = 0usize;
    let mut blob_read_budget = MAX_BLOB_READS_PER_SCAN;
    let total = repositories.len().max(1);

    for (index, repository) in repositories.into_iter().enumerate() {
        let repository_name = repository.name.clone();
        emit_progress(
            &app,
            "repository",
            &format!("Scanning {repository_name}"),
            index,
            total,
            Some(repository_name.clone()),
        );

        let (owner, repo) = repository
            .repository_url
            .trim_end_matches('/')
            .rsplit_once("/repos/")
            .map(|(owner, repo)| (owner.to_string(), repo.to_string()))
            .or_else(|| {
                repository
                    .repository_url
                    .trim_end_matches('/')
                    .rsplit_once('/')
                    .map(|(owner, repo)| {
                        (
                            owner.rsplit('/').next().unwrap_or(owner).to_string(),
                            repo.to_string(),
                        )
                    })
            })
            .ok_or_else(|| {
                format!(
                    "Unable to determine owner/repository for {}",
                    repository.name
                )
            })?;

        let allocated_file_reads =
            (settings.max_files_per_repository as usize).min(blob_read_budget);
        blob_read_budget = blob_read_budget.saturating_sub(allocated_file_reads);
        let mut matches = Vec::new();
        let mut scanned_files = 0usize;
        let mut warnings = Vec::new();

        if allocated_file_reads > 0 {
            if let Ok(readme) = github.fetch_readme(&owner, &repo).await {
                matches.extend(analyzer.analyze("README.md", &readme));
                scanned_files += 1;
            } else {
                warnings.push("README could not be fetched.".into());
            }
        } else {
            warnings.push("Per-scan GitHub blob-read budget reached; no additional source blobs were fetched.".into());
        }

        if allocated_file_reads > 1 {
            if let Ok(code) = github.fetch_code_file(&owner, &repo, "README.md").await {
                if scanned_files == 0 {
                    matches.extend(analyzer.analyze("README.md", &code));
                    scanned_files += 1;
                }
            }
        }

        let auth_match_indices: Vec<usize> = matches
            .iter()
            .enumerate()
            .filter(|(_, matched)| matched.category == "auth_token")
            .map(|(match_index, _)| match_index)
            .take(10)
            .collect();
        let verifier = ProviderVerifier::new();
        let mut verified_count = 0u32;
        for match_index in auth_match_indices {
            let matched = &matches[match_index];
            let provider = provider_for_pattern(&matched.pattern_name);
            let outcome = match provider.as_deref() {
                Some("openai") => verifier.verify_openai(&matched.captured).await?,
                Some("anthropic") => verifier.verify_anthropic(&matched.captured).await?,
                Some("github") => verifier.verify_github(&matched.captured).await?,
                Some("aws") => verifier.verify_aws(&matched.captured, "").await?,
                Some(provider) => VerifyOutcome::Unverifiable {
                    reason: format!("Verification is not implemented for {provider} credentials."),
                },
                None => VerifyOutcome::Unverifiable {
                    reason: "The credential provider could not be determined.".into(),
                },
            };
            if matches!(outcome, VerifyOutcome::Valid { .. }) {
                verified_count += 1;
                if let Some(provider) = provider.clone() {
                    verified_credentials.push(VerifiedCredential {
                        provider,
                        source_repo: repository.name.clone(),
                        source_file: matches[match_index].file_path.clone(),
                        line_number: matches[match_index].line_number as u32,
                        pattern_name: matches[match_index].pattern_name.clone(),
                        outcome: outcome.clone(),
                    });
                }
            }
            matches[match_index].verification = Some(outcome);
        }

        let endpoints: Vec<String> = matches
            .iter()
            .filter_map(|matched| matched.absolute_endpoint.clone())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        global_endpoints.extend(endpoints.iter().cloned());
        let mut health = Vec::new();
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
                if let Some(result) = health_checker::check_health(&endpoint).await {
                    if result.is_healthy {
                        metrics.reachable_endpoints += 1;
                    }
                    health.push(crate::models::endpoint::EndpointHealth {
                        endpoint: result.endpoint,
                        reachable: result.is_healthy,
                        blocked: false,
                        status_code: result.status_code,
                        latency_ms: result.response_time_ms,
                        reason: if result.is_healthy {
                            None
                        } else {
                            Some("Endpoint did not return a successful response.".into())
                        },
                    });
                }
                health_checked += 1;
            }
        }

        let finding = RepositoryFinding {
            repository: RepositorySummary {
                full_name: repository.name.clone(),
                html_url: repository.repository_url,
                description: repository.description,
                language: repository.tech_stack.first().cloned(),
                default_branch: "main".into(),
                stars: repository.stars as u64,
                forks: repository.forks as u64,
                pushed_at: repository.last_updated.map(|date| date.to_rfc3339()),
            },
            scanned_files,
            matches,
            health,
            warnings,
            verified_count,
        };
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
        verified_credentials,
    };
    emit_progress(&app, "complete", "Scan complete.", total, total, None);
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

fn provider_for_pattern(pattern_name: &str) -> Option<String> {
    let name = pattern_name.to_ascii_lowercase();
    let provider = if name.starts_with("openai") {
        "openai"
    } else if name.starts_with("anthropic") {
        "anthropic"
    } else if name.starts_with("github") {
        "github"
    } else if name.starts_with("aws") {
        "aws"
    } else if name.starts_with("google") {
        "google"
    } else if name.starts_with("slack") {
        "slack"
    } else if name.starts_with("stripe") {
        "stripe"
    } else if name.starts_with("sendgrid") {
        "sendgrid"
    } else if name.starts_with("twilio") {
        "twilio"
    } else if name.starts_with("huggingface") {
        "huggingface"
    } else if name.starts_with("deepseek") {
        "deepseek"
    } else {
        return None;
    };
    Some(provider.into())
}

struct ScanReset<'a>(&'a AtomicBool);
impl Drop for ScanReset<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}
