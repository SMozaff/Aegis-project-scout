use anyhow::{anyhow, Result};
use base64::Engine;
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION, USER_AGENT};
use serde::Deserialize;
use tokio::time::{sleep, Duration};

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
        if let Some(t) = token {
            headers.insert(AUTHORIZATION, HeaderValue::from_str(&format!("Bearer {t}"))?);
        }
        Ok(Self { client: reqwest::Client::builder().default_headers(headers).build()? })
    }

    pub async fn search_projects(&self, query: &str, lookback: u32) -> Result<Vec<ProjectDiscovery>> {
        let mut out = Vec::new();
        let q = format!("{} created:>{}", query, chrono::Utc::now().date_naive() - chrono::Duration::days(lookback as i64));
        for page in 1..=10 {
            let url = format!("{API}/search/repositories?q={}&page={page}&per_page=30", url::form_urlencoded::byte_serialize(q.as_bytes()).collect::<String>());
            let result: SearchResult = self.request(url).await?.json().await?;
            if result.items.is_empty() { break; }
            for repo in result.items { out.push(self.to_discovery(repo).await?); }
        }
        Ok(out)
    }

    async fn to_discovery(&self, repo: Repo) -> Result<ProjectDiscovery> {
        let readme = self.fetch_readme(&repo.owner.login, &repo.name).await.ok();
        let text = readme.clone().unwrap_or_default().to_lowercase();
        let mut terms = Vec::new();
        for t in ["api", "rest", "graphql", "openapi", "swagger", "endpoint", "webhook"] {
            if text.contains(t) || repo.description.clone().unwrap_or_default().to_lowercase().contains(t) { terms.push(t.into()); }
        }
        let mut stack = Vec::new();
        if let Some(l) = &repo.language { stack.push(l.clone()); }
        for t in ["react", "next", "fastapi", "django", "express", "docker", "kubernetes"] {
            if text.contains(t) { stack.push(t.into()); }
        }
        Ok(ProjectDiscovery { owner: repo.owner.login, repository: repo.name, full_name: repo.full_name, description: repo.description, stars: repo.stargazers_count, forks: repo.forks_count, language: repo.language, topics: repo.topics, html_url: repo.html_url, api_related_terms: terms, detected_stack: stack, readme_excerpt: readme.map(|x| x.chars().take(2000).collect()), evidence: vec!["Collected from public GitHub repository metadata".into()] })
    }

    pub async fn fetch_readme(&self, owner: &str, repo: &str) -> Result<String> {
        let c: Content = self.request(format!("{API}/repos/{owner}/{repo}/readme")).await?.json().await?;
        decode(c)
    }

    pub async fn fetch_code_file(&self, owner: &str, repo: &str, path: &str) -> Result<String> {
        let c: Content = self.request(format!("{API}/repos/{owner}/{repo}/contents/{path}")).await?.json().await?;
        decode(c)
    }

    async fn request(&self, url: String) -> Result<reqwest::Response> {
        for attempt in 0..5 {
            let r = self.client.get(&url).send().await?;
            if r.status().is_success() { return Ok(r); }
            if r.status().as_u16() == 403 || r.status().as_u16() == 429 { sleep(Duration::from_secs(2u64.pow(attempt))).await; continue; }
            return Err(anyhow!("GitHub API error {}", r.status()));
        }
        Err(anyhow!("GitHub rate limit exceeded"))
    }
}

fn decode(c: Content) -> Result<String> {
    if c.encoding != "base64" { return Err(anyhow!("Unsupported encoding")); }
    Ok(String::from_utf8_lossy(&base64::engine::general_purpose::STANDARD.decode(c.content.replace('\n', ""))?).to_string())
}
