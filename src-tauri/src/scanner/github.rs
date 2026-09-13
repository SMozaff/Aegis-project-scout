use anyhow::{anyhow, Result};
use base64::Engine;
use octocrab::{models, Octocrab};
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION, USER_AGENT};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use tokio::time::{sleep, Duration};

use crate::models::project::ProjectDiscovery;

const RESULTS_PER_QUERY: usize = 30;
const ENRICHMENT_LIMIT: usize = 20;
const SEARCH_MAX_ATTEMPTS: u32 = 3;
const SEARCH_RETRY_WAIT_SECS: u64 = 65;
const API_BASE: &str = "https://api.github.com";

#[derive(Clone)]
pub struct GithubScanner {
    /// Raw HTTP client — used for code search because we need response headers.
    http: reqwest::Client,
    /// Octocrab client — used for metadata/README/content, where its typed
    /// models are genuinely helpful and no header inspection is required.
    client: Octocrab,
    token: Option<String>,
}

// ---------------------------------------------------------------------------
// Response shapes for GitHub's code search endpoint.
// GitHub's code-search response uses a *reduced* repository shape compared to
// /repos/{owner}/{repo} — fields like stargazers_count are absent. Every field
// is defaulted so we never fail on shape drift again.
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct CodeSearchResponse {
    #[serde(default)]
    total_count: u64,
    #[serde(default)]
    incomplete_results: bool,
    #[serde(default)]
    items: Vec<CodeSearchItem>,
}

#[derive(Deserialize)]
struct CodeSearchItem {
    #[serde(default)]
    path: String,
    #[serde(default)]
    repository: CodeSearchRepo,
}

#[derive(Deserialize, Default)]
struct CodeSearchRepo {
    #[serde(default)]
    id: u64,
    #[serde(default)]
    name: String,
    #[serde(default)]
    full_name: String,
    #[serde(default)]
    html_url: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    language: Option<String>,
    #[serde(default)]
    topics: Vec<String>,
    #[serde(default)]
    stargazers_count: u64,
    #[serde(default)]
    forks_count: u64,
    #[serde(default)]
    owner: CodeSearchOwner,
}

#[derive(Deserialize, Default)]
struct CodeSearchOwner {
    #[serde(default)]
    login: String,
}

#[derive(Debug, Clone)]
pub struct HistoricalFileVersion {
    pub commit_sha: String,
    pub commit_date: Option<chrono::DateTime<chrono::Utc>>,
    pub content: String,
}

// ---------------------------------------------------------------------------

impl GithubScanner {
    pub fn new(token: Option<String>) -> Result<Self> {
        let token = token.filter(|t| !t.is_empty());

        let mut headers = HeaderMap::new();
        headers.insert(USER_AGENT, HeaderValue::from_static("Raven-API-Hunter"));
        headers.insert(
            ACCEPT,
            HeaderValue::from_static("application/vnd.github.v3+json"),
        );
        if let Some(t) = &token {
            headers.insert(
                AUTHORIZATION,
                HeaderValue::from_str(&format!("Bearer {t}"))?,
            );
        }

        let http = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(Duration::from_secs(30))
            .build()?;

        let client = match token.clone() {
            Some(t) => Octocrab::builder().personal_token(t).build()?,
            None => Octocrab::builder().build()?,
        };

        Ok(Self { http, client, token })
    }

    // -----------------------------------------------------------------------
    // Public API (unchanged signatures — callers in scan_headless.rs and
    // commands.rs continue to work as-is).
    // -----------------------------------------------------------------------

    pub async fn search_projects(
        &self,
        query: &str,
        _lookback: u32,
    ) -> Result<Vec<ProjectDiscovery>> {
        let technologies: Vec<&str> = query
            .split(" OR ")
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .collect();
        if technologies.is_empty() {
            return Err(anyhow!("Select at least one technology to search for."));
        }

        let mut repositories: Vec<CodeSearchRepo> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        let mut repo_paths: HashMap<String, Vec<String>> = HashMap::new();

        // Credential-literal queries. These search for the *shape* of the
        // secret directly rather than for a technology name inside a file.
        // Each query is spaced and rate-limit-aware (see search_code_page).
        let mut sub_queries: Vec<String> = vec![
            "\"sk-proj-\" in:file".into(),
            "\"AKIA\" in:file".into(),
            "\"ghp_\" in:file".into(),
        ];

        // Technology-scoped fallbacks (always included)
        for tech in &technologies {
            sub_queries.push(format!("\"{tech}\" in:file filename:.env"));
            sub_queries.push(format!("\"{tech}\" in:file filename:credentials"));
        }

        for search_query in sub_queries {
            let mut collected = 0usize;
            let mut page_num: u32 = 1;

            loop {
                let (items, _total, _incomplete) =
                    self.search_code_page(&search_query, page_num).await?;

                if items.is_empty() {
                    break;
                }

                for item in items {
                    if collected >= RESULTS_PER_QUERY {
                        break;
                    }
                    collected += 1;

                    let key = item.repository.full_name.clone();
                    if key.is_empty() {
                        continue;
                    }

                    if seen.insert(key.clone()) {
                        repositories.push(item.repository.clone());
                    }
                    if !item.path.is_empty() {
                        let paths = repo_paths.entry(key).or_default();
                        if !paths.contains(&item.path) {
                            paths.push(item.path.clone());
                        }
                    }
                }

                if collected >= RESULTS_PER_QUERY || page_num >= 4 {
                    break;
                }
                page_num += 1;
            }
        }

        // Enrich each discovered repository.
        let mut out = Vec::with_capacity(repositories.len());
        for (index, repo) in repositories.into_iter().enumerate() {
            let enrich = index < ENRICHMENT_LIMIT;
            let owner = repo.owner.login.clone();
            let name = repo.name.clone();

            // For enriched repos, fetch full metadata from /repos/{owner}/{repo}
            // so we get stars/forks/language/topics (absent in code-search shape).
            let (stars, forks, language, topics, description, html_url, full_name) =
                if enrich && !owner.is_empty() && !name.is_empty() {
                    match self.client.repos(&owner, &name).get().await {
                        Ok(full) => {
                            let lang = full
                                .language
                                .as_ref()
                                .and_then(|l| l.as_str())
                                .map(String::from);
                            let topics = full
                                .topics
                                .clone()
                                .unwrap_or_default();
                            let html = full
                                .html_url
                                .as_ref()
                                .map(|u| u.to_string())
                                .unwrap_or_else(|| repo.html_url.clone());
                            let full_name = full
                                .full_name
                                .clone()
                                .unwrap_or_else(|| format!("{}/{}", owner, name));
                            (
                                full.stargazers_count.unwrap_or(0) as u32,
                                full.forks_count.unwrap_or(0) as u32,
                                lang,
                                topics,
                                full.description.clone().or(repo.description.clone()),
                                html,
                                full_name,
                            )
                        }
                        Err(_) => (
                            repo.stargazers_count as u32,
                            repo.forks_count as u32,
                            repo.language.clone(),
                            repo.topics.clone(),
                            repo.description.clone(),
                            repo.html_url.clone(),
                            if repo.full_name.is_empty() {
                                format!("{}/{}", owner, name)
                            } else {
                                repo.full_name.clone()
                            },
                        ),
                    }
                } else {
                    (
                        repo.stargazers_count as u32,
                        repo.forks_count as u32,
                        repo.language.clone(),
                        repo.topics.clone(),
                        repo.description.clone(),
                        repo.html_url.clone(),
                        if repo.full_name.is_empty() {
                            format!("{}/{}", owner, name)
                        } else {
                            repo.full_name.clone()
                        },
                    )
                };

            let matched_file_paths = repo_paths
                .get(&full_name)
                .cloned()
                .unwrap_or_default();

            out.push(ProjectDiscovery {
                id: full_name.clone(),
                name: full_name,
                repository_url: html_url,
                description,
                discovered_at: chrono::Utc::now(),
                last_updated: None,
                stars,
                forks,
                tech_stack: language
                    .into_iter()
                    .chain(topics.into_iter())
                    .collect(),
                endpoints: Vec::new(),
                health_status: None,
                confidence_score: 0.5,
                evidence:
                    "Collected from public GitHub code-search results and repository metadata"
                        .into(),
                source_file: None,
                matched_file_paths,
            });
        }

        Ok(out)
    }

    pub async fn fetch_readme(&self, owner: &str, repo: &str) -> Result<String> {
        let readme = self
            .client
            .repos(owner, repo)
            .get_readme()
            .send()
            .await
            .map_err(|e| {
                anyhow!(
                    "GitHub README fetch failed for {}/{}: {}",
                    owner,
                    repo,
                    format_octocrab_error(&e)
                )
            })?;

        match readme.content {
            Some(encoded) => {
                let cleaned: String = encoded.chars().filter(|c| !c.is_whitespace()).collect();
                let bytes = base64::engine::general_purpose::STANDARD.decode(&cleaned)?;
                Ok(String::from_utf8_lossy(&bytes).into_owned())
            }
            None => Ok(String::new()),
        }
    }

    pub async fn fetch_code_file(&self, owner: &str, repo: &str, path: &str) -> Result<String> {
        let content = self
            .client
            .repos(owner, repo)
            .get_content()
            .path(path)
            .send()
            .await
            .map_err(|e| {
                anyhow!(
                    "GitHub content fetch failed for {}/{} at {}: {}",
                    owner,
                    repo,
                    path,
                    format_octocrab_error(&e)
                )
            })?;

        let first = content
            .items
            .into_iter()
            .next()
            .ok_or_else(|| anyhow!("no content for {}", path))?;

        match first.content {
            Some(encoded) => {
                let cleaned: String = encoded.chars().filter(|c| !c.is_whitespace()).collect();
                let bytes = base64::engine::general_purpose::STANDARD.decode(&cleaned)?;
                Ok(String::from_utf8_lossy(&bytes).into_owned())
            }
            None => Ok(String::new()),
        }
    }

    /// Walk git history for a file. Kept from Path B; uses octocrab for the
    /// list_commits call and reqwest for fetching each historical version.
    pub async fn fetch_file_history(
        &self,
        owner: &str,
        repo: &str,
        path: &str,
        max_commits: usize,
    ) -> Result<Vec<HistoricalFileVersion>> {
        let mut out = Vec::new();
        if max_commits == 0 {
            return Ok(out);
        }

        // Use raw REST for listing commits (avoids octocrab version drift).
        let url = format!(
            "{API_BASE}/repos/{owner}/{repo}/commits?path={}&per_page={}",
            urlencoding::encode(path),
            max_commits.min(30)
        );

        let response = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(|e| anyhow!("history list failed for {}: {}", path, e))?;

        if !response.status().is_success() {
            return Err(anyhow!(
                "history list returned {} for {}",
                response.status(),
                path
            ));
        }

        #[derive(Deserialize)]
        struct CommitEntry {
            sha: String,
            commit: CommitMeta,
        }
        #[derive(Deserialize)]
        struct CommitMeta {
            author: Option<CommitAuthor>,
        }
        #[derive(Deserialize)]
        struct CommitAuthor {
            date: Option<String>,
        }

        let commits: Vec<CommitEntry> = response
            .json()
            .await
            .map_err(|e| anyhow!("history list JSON failed for {}: {}", path, e))?;

        for entry in commits.into_iter().take(max_commits) {
            let sha = entry.sha;
            let date = entry
                .commit
                .author
                .and_then(|a| a.date)
                .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
                .map(|dt| dt.with_timezone(&chrono::Utc));

            // Fetch the file at this specific commit via raw REST so we get
            // base64 content without octocrab's typed wrapper in the way.
            let url = format!("{API_BASE}/repos/{owner}/{repo}/contents/{path}?ref={sha}");
            let resp = match self.http.get(&url).send().await {
                Ok(r) => r,
                Err(_) => continue,
            };
            if !resp.status().is_success() {
                continue;
            }

            #[derive(Deserialize)]
            struct ContentResp {
                #[serde(default)]
                content: String,
                #[serde(default)]
                encoding: String,
            }

            let body: ContentResp = match resp.json().await {
                Ok(b) => b,
                Err(_) => continue,
            };
            if body.encoding != "base64" || body.content.is_empty() {
                continue;
            }
            let cleaned: String = body.content.chars().filter(|c| !c.is_whitespace()).collect();
            let bytes = match base64::engine::general_purpose::STANDARD.decode(&cleaned) {
                Ok(b) => b,
                Err(_) => continue,
            };

            out.push(HistoricalFileVersion {
                commit_sha: sha,
                commit_date: date,
                content: String::from_utf8_lossy(&bytes).into_owned(),
            });
        }

        Ok(out)
    }

    // -----------------------------------------------------------------------
    // Raw code-search: reads rate-limit headers, backs off on 403, spaces
    // requests. This is the piece octocrab could not do for us.
    // -----------------------------------------------------------------------

    async fn search_code_page(
        &self,
        query: &str,
        page: u32,
    ) -> Result<(Vec<CodeSearchItem>, u64, bool)> {
        let encoded = urlencoding::encode(query);
        let url = format!(
            "{API_BASE}/search/code?q={}&per_page={}&page={}",
            encoded, RESULTS_PER_QUERY, page
        );

        let mut attempt: u32 = 0;
        loop {
            let response = self
                .http
                .get(&url)
                .send()
                .await
                .map_err(|e| anyhow!("GitHub search network error: {}", e))?;

            let status = response.status();
            let remaining = response
                .headers()
                .get("x-ratelimit-remaining")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u32>().ok())
                .unwrap_or(999);
            let reset = response
                .headers()
                .get("x-ratelimit-reset")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(0);

            // Rate limited? Sleep until reset, then retry.
            if (status.as_u16() == 403 || status.as_u16() == 429) && attempt < SEARCH_MAX_ATTEMPTS {
                let now = chrono::Utc::now().timestamp() as u64;
                let wait = if reset > now {
                    reset.saturating_sub(now).saturating_add(3).min(180)
                } else {
                    SEARCH_RETRY_WAIT_SECS
                };
                eprintln!(
                    "[throttle] code search 403/429 (remaining={}), waiting {}s (attempt {}/{})",
                    remaining,
                    wait,
                    attempt + 1,
                    SEARCH_MAX_ATTEMPTS
                );
                sleep(Duration::from_secs(wait)).await;
                attempt += 1;
                continue;
            }

            if !status.is_success() {
                let text = response.text().await.unwrap_or_default();
                return Err(anyhow!(
                    "GitHub code search failed for query {:?} (HTTP {}): {}",
                    query,
                    status.as_u16(),
                    text.chars().take(300).collect::<String>()
                ));
            }

            // Proactively pause before the next query if we're near the limit.
            if remaining < 3 {
                let now = chrono::Utc::now().timestamp() as u64;
                let wait = if reset > now {
                    reset.saturating_sub(now).saturating_add(2).min(180)
                } else {
                    SEARCH_RETRY_WAIT_SECS
                };
                eprintln!(
                    "[throttle] search quota low ({} left), sleeping {}s before next query",
                    remaining, wait
                );
                sleep(Duration::from_secs(wait)).await;
            } else {
                // Minimum spacing so we never burst past 10/min.
                sleep(Duration::from_secs(3)).await;
            }

            let body: CodeSearchResponse = response
                .json()
                .await
                .map_err(|e| anyhow!("GitHub search JSON parse failed: {}", e))?;

            return Ok((body.items, body.total_count, body.incomplete_results));
        }
    }
}

/// Octocrab's Display for its Error enum prints only the variant name for
/// GitHub API errors. Fall back to Debug so we at least see the payload.
fn format_octocrab_error(e: &octocrab::Error) -> String {
    let display = format!("{}", e);
    if display == "GitHub" || display.is_empty() {
        format!("{:?}", e)
    } else {
        display
    }
}