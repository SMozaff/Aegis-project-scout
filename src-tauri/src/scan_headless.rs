use std::collections::HashSet;

use chrono::Utc;

use crate::{
    models::{
        RepositoryFinding, RepositorySummary, ScanConfig, ScanMetrics, ScanProgress, ScanReport,
        VerifiedCredential,
    },
    scanner::{
        github::GithubScanner,
        health_checker,
        pattern_analyzer::PatternAnalyzer,
        verify::{ProviderVerifier, VerifyOutcome},
    },
};

const MAX_HEALTH_PER_REPOSITORY: usize = 8;
const MAX_HEALTH_PER_SCAN: usize = 30;
const MAX_BLOB_READS_PER_SCAN: usize = 500;
const MAX_HISTORY_READS_PER_REPO: usize = 5;

pub async fn run_scan_headless(
    config: ScanConfig,
    progress_callback: Option<Box<dyn Fn(ScanProgress) + Send + Sync>>,
    finding_callback: Option<Box<dyn Fn(RepositoryFinding) + Send + Sync>>,
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

    let started_at = Utc::now();

    emit_progress(
        progress_callback.as_deref(),
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
            progress_callback.as_deref(),
            "repository",
            &format!("Scanning {repository_name}"),
            index,
            total,
            Some(repository_name.clone()),
        );

        let (owner, repo) = split_owner_repo(&repository.repository_url, &repository.name)
            .ok_or_else(|| {
                format!(
                    "Unable to determine owner/repository for {}",
                    repository.name
                )
            })?;

        let allocated_file_reads =
            (settings.max_files_per_repository as usize).min(blob_read_budget);
        blob_read_budget = blob_read_budget.saturating_sub(allocated_file_reads);
        let mut raw_matches = Vec::new();
        let mut scanned_files = 0usize;
        let mut warnings = Vec::new();

        if allocated_file_reads > 0 {
            if let Ok(readme) = github.fetch_readme(&owner, &repo).await {
                raw_matches.extend(analyzer.analyze("README.md", &readme));
                scanned_files += 1;
            } else {
                warnings.push("README could not be fetched.".into());
            }
        } else {
            warnings.push("Per-scan GitHub blob-read budget reached; no additional source blobs were fetched.".into());
        }

        if allocated_file_reads > 0 {
            for file_path in &repository.matched_file_paths {
                if scanned_files >= allocated_file_reads {
                    break;
                }
                if file_path == "README.md" {
                    continue;
                }
                if let Ok(content) = github.fetch_code_file(&owner, &repo, file_path).await {
                    raw_matches.extend(analyzer.analyze(file_path, &content));
                    scanned_files += 1;
                }
            }
        }

        if config.scan_history {
            let history_budget: usize = 5; // files per repo, keep small
            for file_path in repository.matched_file_paths.iter().take(history_budget) {
                if scanned_files >= allocated_file_reads + history_budget {
                    break;
                }
                match github.fetch_file_history(&owner, &repo, file_path, 5).await {
                    Ok(versions) => {
                        for version in versions {
                            let label = format!("{}@{}", file_path, &version.commit_sha[..8.min(version.commit_sha.len())]);
                            raw_matches.extend(analyzer.analyze(&label, &version.content));
                            scanned_files += 1;
                        }
                    }
                    Err(_) => {
                        warnings.push(format!("History fetch failed for {}", file_path));
                    }
                }
            }
        }

        raw_matches.sort_by_key(|matched| (matched.pattern.category != "auth_token") as u8);

        let mut verified_count = 0u32;
        if config.verify_credentials {
            let auth_match_indices: Vec<usize> = raw_matches
                .iter()
                .enumerate()
                .filter(|(_, matched)| matched.pattern.category == "auth_token")
                .map(|(match_index, _)| match_index)
                .take(10)
                .collect();
            let verifier = ProviderVerifier::new();
            for match_index in auth_match_indices {
                let token = raw_matches[match_index].captured.clone();
                let pattern_name = raw_matches[match_index].pattern.pattern_name.clone();
                let provider = provider_for_pattern(&pattern_name);
                let outcome = match provider.as_deref() {
                    Some("openai") => verifier.verify_openai(&token).await?,
                    Some("anthropic") => verifier.verify_anthropic(&token).await?,
                    Some("github") => verifier.verify_github(&token).await?,
                    Some("google") => verifier.verify_google(&token).await?,
                    Some("stripe") => {
                        verifier
                            .verify_generic(&token, "https://api.stripe.com/v1/charges?limit=1")
                            .await?
                    }
                    Some("slack") => {
                        verifier
                            .verify_generic(&token, "https://slack.com/api/auth.test")
                            .await?
                    }
                    Some("sendgrid") => {
                        verifier
                            .verify_generic(&token, "https://api.sendgrid.com/v3/scopes")
                            .await?
                    }
                    Some("twilio") => {
                        verifier
                            .verify_generic(
                                &token,
                                "https://api.twilio.com/2010-04-01/Accounts.json",
                            )
                            .await?
                    }
                    Some("huggingface") => {
                        verifier
                            .verify_generic(&token, "https://huggingface.co/api/whoami-v2")
                            .await?
                    }
                    Some("deepseek") => {
                        verifier
                            .verify_generic(&token, "https://api.deepseek.com/v1/models")
                            .await?
                    }
                    Some("aws") => VerifyOutcome::Unverifiable {
                        reason: "AWS requires paired keys; single-key verification not supported"
                            .into(),
                    },
                    None => VerifyOutcome::Unverifiable {
                        reason: "The credential provider could not be determined.".into(),
                    },
                    Some(provider) => VerifyOutcome::Unverifiable {
                        reason: format!(
                            "No verification endpoint is known for {provider} credentials."
                        ),
                    },
                };
                if matches!(outcome, VerifyOutcome::Valid { .. }) {
                    verified_count += 1;
                    if let Some(provider) = provider.clone() {
                        verified_credentials.push(VerifiedCredential {
                            provider,
                            source_repo: repository.name.clone(),
                            source_file: raw_matches[match_index].pattern.file_path.clone(),
                            line_number: raw_matches[match_index].pattern.line_number as u32,
                            pattern_name,
                            outcome: outcome.clone(),
                        });
                    }
                }
                raw_matches[match_index].pattern.verification = Some(outcome);
            }
        }
        let matches = raw_matches
            .into_iter()
            .map(|raw| raw.pattern)
            .collect::<Vec<_>>();

        let endpoints: Vec<String> = matches
            .iter()
            .filter(|matched| matched.category == "endpoint")
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
                    progress_callback.as_deref(),
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
        if let Some(callback) = finding_callback.as_ref() {
            callback(finding.clone());
        }
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
    emit_progress(
        progress_callback.as_deref(),
        "complete",
        "Scan complete.",
        total,
        total,
        None,
    );
    Ok(report)
}

fn emit_progress(
    callback: Option<&(dyn Fn(ScanProgress) + Send + Sync)>,
    stage: &str,
    message: &str,
    completed: usize,
    total: usize,
    repository: Option<String>,
) {
    if let Some(callback) = callback {
        callback(ScanProgress {
            stage: stage.to_string(),
            message: message.to_string(),
            completed,
            total,
            repository,
        });
    }
}

/// Extract `(owner, repo)` from a GitHub URL or a `owner/repo` fallback.
///
/// Handles:
/// - `https://github.com/{owner}/{repo}` (web URL, produced by octocrab's `html_url`)
/// - `https://api.github.com/repos/{owner}/{repo}` (API URL, if ever encountered)
/// - `{owner}/{repo}` (bare full name from the discovery record)
fn split_owner_repo(repository_url: &str, fallback_full_name: &str) -> Option<(String, String)> {
    let trimmed = repository_url.trim_end_matches('/');

    // API URL: https://api.github.com/repos/{owner}/{repo}
    if let Some((_, tail)) = trimmed.rsplit_once("/repos/") {
        let mut parts = tail.splitn(2, '/');
        let owner = parts.next().unwrap_or("");
        let repo = parts.next().unwrap_or("");
        if !owner.is_empty() && !repo.is_empty() {
            return Some((owner.to_string(), repo.to_string()));
        }
    }

    // Web URL: https://github.com/{owner}/{repo}
    let mut segments = trimmed.rsplit('/');
    let repo = segments.next().unwrap_or("");
    let owner = segments.next().unwrap_or("");
    if !owner.is_empty() && !repo.is_empty() && !repo.contains(':') {
        return Some((owner.to_string(), repo.to_string()));
    }

    // Fallback: use the discovery record's full name (owner/repo)
    if let Some((owner, repo)) = fallback_full_name.split_once('/') {
        return Some((owner.to_string(), repo.to_string()));
    }

    None
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

#[cfg(test)]
mod tests {
    use super::split_owner_repo;

    #[test]
    fn parses_web_url() {
        let result = split_owner_repo("https://github.com/rust-lang/rust", "fallback/repo");
        assert_eq!(
            result,
            Some(("rust-lang".to_string(), "rust".to_string()))
        );
    }

    #[test]
    fn parses_api_url() {
        let result = split_owner_repo(
            "https://api.github.com/repos/rust-lang/rust",
            "fallback/repo",
        );
        assert_eq!(
            result,
            Some(("rust-lang".to_string(), "rust".to_string()))
        );
    }

    #[test]
    fn falls_back_to_full_name() {
        let result = split_owner_repo("", "owner/repo");
        assert_eq!(result, Some(("owner".to_string(), "repo".to_string())));
    }

    #[test]
    fn returns_none_on_garbage() {
        let result = split_owner_repo("", "");
        assert!(result.is_none());
    }
}
