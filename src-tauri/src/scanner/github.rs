use anyhow::{anyhow, Result};
use base64::Engine;
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION, USER_AGENT};
use serde::{de::DeserializeOwned, Deserialize};
use std::collections::HashSet;
use tokio::time::{sleep, Duration};

use crate::models::project::ProjectDiscovery;

const API: &str = "https://api.github.com";
const RESULTS_PER_QUERY: usize = 30;
const ENRICHMENT_LIMIT: usize = 20;

#[derive(Clone)]
pub struct GithubScanner {
    client: reqwest::Client,
}

#[derive(Deserialize)]
struct CodeSearchResult {
    #[serde(default)]
    total_count: u64,
    #[serde(default)]
    incomplete_results: bool,
    #[serde(default)]
    items: Vec<CodeSearchItem>,
}

#[derive(Deserialize)]
struct CodeSearchItem {
    repository: Repo,
    #[serde(default)]
    score: f64,
}

#[derive(Deserialize)]
struct Repo {
    name: String,
    full_name: String,
    html_url: String,
    description: Option<String>,
    stargazers_count: u64,
    forks_count: u64,
    language: Option<String>,
    #[serde(default)]
    topics: Vec<String>,
    owner: Owner,
}

#[derive(Deserialize)]
struct Owner {
    login: String,
}

#[derive(Deserialize)]
struct Content {
    #[serde(default)]
    content: String,
    #[serde(default)]
    encoding: String,
}

impl GithubScanner {
    pub fn new(token: Option<String>) -> Result<Self> {
        let mut headers = HeaderMap::new();
        headers.insert(USER_AGENT, HeaderValue::from_static("Raven-API-Hunter"));
        headers.insert(
            ACCEPT,
            HeaderValue::from_static("application/vnd.github+json"),
        );
        if let Some(token) = token {
            headers.insert(
                AUTHORIZATION,
                HeaderValue::from_str(&format!("Bearer {token}"))?,
            );
        }
        Ok(Self {
            client: reqwest::Client::builder()
                .default_headers(headers)
                .build()?,
        })
    }

    pub async fn search_projects(
        &self,
        query: &str,
        _lookback: u32,
    ) -> Result<Vec<ProjectDiscovery>> {
        let technologies: Vec<&str> = query
            .split(" OR ")
            .map(str::trim)
            .filter(|technology| !technology.is_empty())
            .collect();
        if technologies.is_empty() {
            return Err(anyhow!("Select at least one technology to search for."));
        }

        let mut repositories = Vec::new();
        let mut seen = HashSet::new();
        for technology in technologies {
            for search_query in [
                format!("\"{technology}\" in:file filename:.env"),
                format!("\"{technology}\" in:file filename:config"),
                format!("\"{technology}\" extension:yml OR extension:yaml OR extension:json token"),
            ] {
                let encoded = url::form_urlencoded::byte_serialize(search_query.as_bytes())
                    .collect::<String>();
                let mut collected = 0usize;

                for page in 1..=10 {
                    if collected >= RESULTS_PER_QUERY {
                        break;
                    }
                    let result: CodeSearchResult = self
                        .request_json(format!(
                            "{API}/search/code?q={encoded}&page={page}&per_page={RESULTS_PER_QUERY}"
                        ))
                        .await?;
                    if result.items.is_empty() {
                        break;
                    }

                    for item in result.items {
                        collected += 1;
                        if seen.insert(item.repository.full_name.clone()) {
                            repositories.push(item.repository);
                        }
                        if collected >= RESULTS_PER_QUERY {
                            break;
                        }
                    }
                }
            }
        }

        let mut out = Vec::with_capacity(repositories.len());
        for (index, repo) in repositories.into_iter().enumerate() {
            out.push(self.to_discovery(repo, index < ENRICHMENT_LIMIT).await?);
        }
        Ok(out)
    }

    async fn to_discovery(&self, repo: Repo, enrich: bool) -> Result<ProjectDiscovery> {
        let readme = if enrich {
            self.fetch_readme(&repo.owner.login, &repo.name).await.ok()
        } else {
            None
        };
        let code_file = if enrich {
            self.fetch_code_file(&repo.owner.login, &repo.name, "README.md")
                .await
                .ok()
        } else {
            None
        };
        let text = readme
            .as_deref()
            .or(code_file.as_deref())
            .unwrap_or_default()
            .to_lowercase();
        let mut terms = Vec::new();
        for term in [
            "api", "rest", "graphql", "openapi", "swagger", "endpoint", "webhook",
        ] {
            if text.contains(term)
                || repo
                    .description
                    .clone()
                    .unwrap_or_default()
                    .to_lowercase()
                    .contains(term)
            {
                terms.push(term.into());
            }
        }
        let mut stack = Vec::new();
        if let Some(language) = &repo.language {
            stack.push(language.clone());
        }
        stack.extend(
            repo.topics
                .iter()
                .filter(|topic| text.contains(topic.as_str()))
                .cloned(),
        );
        Ok(ProjectDiscovery {
            id: repo.full_name.clone(),
            name: repo.full_name,
            repository_url: repo.html_url,
            description: repo.description,
            discovered_at: chrono::Utc::now(),
            last_updated: None,
            stars: repo.stargazers_count as u32,
            forks: repo.forks_count as u32,
            tech_stack: if stack.is_empty() {
                terms.clone()
            } else {
                stack
            },
            endpoints: Vec::new(),
            health_status: None,
            confidence_score: if terms.is_empty() { 0.0 } else { 0.5 },
            evidence: "Collected from public GitHub code-search results and repository metadata"
                .into(),
            source_file: None,
        })
    }

    pub async fn fetch_readme(&self, owner: &str, repo: &str) -> Result<String> {
        let content: Content = self
            .request_json(format!("{API}/repos/{owner}/{repo}/readme"))
            .await?;
        decode(content)
    }

    pub async fn fetch_code_file(&self, owner: &str, repo: &str, path: &str) -> Result<String> {
        let content: Content = self
            .request_json(format!("{API}/repos/{owner}/{repo}/contents/{path}"))
            .await?;
        decode(content)
    }

    async fn request(&self, url: String) -> Result<reqwest::Response> {
        for attempt in 0..5 {
            let response = self.client.get(&url).send().await?;
            if response.status().is_success() {
                return Ok(response);
            }
            let status = response.status();
            let text = response.text().await?;
            if status.as_u16() == 403 || status.as_u16() == 429 {
                if attempt == 4 {
                    return Err(anyhow!(
                        "GitHub API rate limit exceeded ({}). Reduce scan limits or wait and retry. Body: {}",
                        status,
                        truncate(&text, 200)
                    ));
                }
                sleep(Duration::from_secs(2u64.pow(attempt))).await;
                continue;
            }
            return Err(anyhow!(
                "GitHub API error ({}): {}",
                status,
                truncate(&text, 300)
            ));
        }
        Err(anyhow!("GitHub API request failed after retries"))
    }

    async fn request_json<T: DeserializeOwned>(&self, url: String) -> Result<T> {
        let response = self.request(url).await?;
        let status = response.status();
        let text = response.text().await?;
        serde_json::from_str(&text).map_err(|error| {
            anyhow!(
                "Failed to parse GitHub response ({}): {} — body: {}",
                status,
                error,
                truncate(&text, 300)
            )
        })
    }
}

fn truncate(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

fn decode(content: Content) -> Result<String> {
    if content.encoding != "base64" {
        return Err(anyhow!("Unsupported encoding"));
    }
    Ok(String::from_utf8_lossy(
        &base64::engine::general_purpose::STANDARD.decode(content.content.replace('\n', ""))?,
    )
    .to_string())
}
