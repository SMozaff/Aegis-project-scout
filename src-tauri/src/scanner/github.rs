use anyhow::{anyhow, Result};
use base64::Engine;
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION, USER_AGENT};
use serde::Deserialize;
use tokio::time::{sleep, Duration};

use crate::models::project::ProjectDiscovery;

const API: &str = "https://api.github.com";

#[derive(Clone)]
pub struct GithubScanner { client: reqwest::Client }

#[derive(Deserialize)]
struct SearchResult { items: Vec<Repo> }

#[derive(Deserialize)]
struct Repo {
    name: String,
    full_name: String,
    html_url: String,
    description: Option<String>,
    stargazers_count: u64,
    forks_count: u64,
    language: Option<String>,
    #[serde(default)] topics: Vec<String>,
    owner: Owner,
}

#[derive(Deserialize)]
struct Owner { login: String }

#[derive(Deserialize)]
struct Content { content: String, encoding: String }

impl GithubScanner {
    pub fn new(token: Option<String>) -> Result<Self> {
        let mut headers = HeaderMap::new();
        headers.insert(USER_AGENT, HeaderValue::from_static("Aegis-Project-Scout"));
        headers.insert(ACCEPT, HeaderValue::from_static("application/vnd.github+json"));
        if let Some(token) = token {
            headers.insert(AUTHORIZATION, HeaderValue::from_str(&format!("Bearer {token}"))?);
        }
        Ok(Self { client: reqwest::Client::builder().default_headers(headers).build()? })
    }

    pub async fn search_projects(&self, query: &str, lookback: u32) -> Result<Vec<ProjectDiscovery>> {
        let mut out = Vec::new();
        let date = chrono::Utc::now().date_naive() - chrono::Duration::days(lookback as i64);
        let encoded = url::form_urlencoded::byte_serialize(format!("{query} created:>{date}").as_bytes()).collect::<String>();
        for page in 1..=10 {
            let result: SearchResult = self.request(format!("{API}/search/repositories?q={encoded}&page={page}&per_page=30")).await?.json().await?;
            if result.items.is_empty() { break; }
            for repo in result.items { out.push(self.to_discovery(repo).await?); }
        }
        Ok(out)
    }

    async fn to_discovery(&self, repo: Repo) -> Result<ProjectDiscovery> {
        let readme = self.fetch_readme(&repo.owner.login, &repo.name).await.ok();
        let text = readme.clone().unwrap_or_default().to_lowercase();
        let mut terms = Vec::new();
        for term in ["api", "rest", "graphql", "openapi", "swagger", "endpoint", "webhook"] {
            if text.contains(term) || repo.description.clone().unwrap_or_default().to_lowercase().contains(term) { terms.push(term.into()); }
        }
        let mut stack = Vec::new();
        if let Some(language) = &repo.language { stack.push(language.clone()); }
        stack.extend(repo.topics.iter().filter(|topic| text.contains(topic.as_str())).cloned());
        Ok(ProjectDiscovery {
            id: repo.full_name.clone(),
            name: repo.full_name,
            repository_url: repo.html_url,
            description: repo.description,
            discovered_at: chrono::Utc::now(),
            last_updated: None,
            stars: repo.stargazers_count as u32,
            forks: repo.forks_count as u32,
            tech_stack: if stack.is_empty() { terms.clone() } else { stack },
            endpoints: Vec::new(),
            health_status: None,
            confidence_score: if terms.is_empty() { 0.0 } else { 0.5 },
            evidence: "Collected from public GitHub repository metadata".into(),
            source_file: None,
        })
    }

    pub async fn fetch_readme(&self, owner: &str, repo: &str) -> Result<String> {
        let content: Content = self.request(format!("{API}/repos/{owner}/{repo}/readme")).await?.json().await?;
        decode(content)
    }

    pub async fn fetch_code_file(&self, owner: &str, repo: &str, path: &str) -> Result<String> {
        let content: Content = self.request(format!("{API}/repos/{owner}/{repo}/contents/{path}")).await?.json().await?;
        decode(content)
    }

    async fn request(&self, url: String) -> Result<reqwest::Response> {
        for attempt in 0..5 {
            let response = self.client.get(&url).send().await?;
            if response.status().is_success() { return Ok(response); }
            if response.status().as_u16() == 403 || response.status().as_u16() == 429 {
                sleep(Duration::from_secs(2u64.pow(attempt))).await;
                continue;
            }
            return Err(anyhow!("GitHub API error {}", response.status()));
        }
        Err(anyhow!("GitHub rate limit exceeded"))
    }
}

fn decode(content: Content) -> Result<String> {
    if content.encoding != "base64" { return Err(anyhow!("Unsupported encoding")); }
    Ok(String::from_utf8_lossy(&base64::engine::general_purpose::STANDARD.decode(content.content.replace('\n', ""))?).to_string())
}
