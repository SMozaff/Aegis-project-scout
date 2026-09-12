use anyhow::{anyhow, Result};
use base64::Engine;
use octocrab::{models, Octocrab, Page};
use std::collections::HashSet;

use crate::models::project::ProjectDiscovery;

const RESULTS_PER_QUERY: usize = 30;
const ENRICHMENT_LIMIT: usize = 20;

/// GitHub API client wrapper built on `octocrab`.
///
/// Using octocrab's typed models guarantees our deserialization always
/// matches what GitHub actually returns — the code-search response uses
/// a *reduced* repository shape, and octocrab's `models::Repository`
/// reflects that (fields like `stargazers_count` are `Option<u64>`).
///
/// For enriched repos (the first `ENRICHMENT_LIMIT` results), we make a
/// second call to `/repos/{owner}/{repo}` to fetch full metadata — stars,
/// forks, language, topics — which the code-search endpoint does not
/// provide.
#[derive(Clone)]
pub struct GithubScanner {
    client: Octocrab,
}

impl GithubScanner {
    pub fn new(token: Option<String>) -> Result<Self> {
        let client = match token {
            Some(t) if !t.is_empty() => Octocrab::builder().personal_token(t).build()?,
            _ => Octocrab::builder().build()?,
        };
        Ok(Self { client })
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

        let mut repositories: Vec<models::Repository> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();

        for technology in technologies {
            let sub_queries = [
                format!("\"{technology}\" in:file filename:.env"),
                format!("\"{technology}\" in:file filename:config"),
                format!("\"{technology}\" extension:yml OR extension:yaml OR extension:json token"),
            ];

            for search_query in sub_queries {
                let mut collected = 0usize;

                let mut page: Page<models::Code> = self
                    .client
                    .search()
                    .code(&search_query)
                    .per_page(RESULTS_PER_QUERY as u8)
                    .send()
                    .await
                    .map_err(|e| {
                        let display = format!("{}", e);
                        let detail = if display.is_empty() || display == "GitHub" {
                            format!("{:?}", e)
                        } else {
                            display
                        };
                        if detail.contains("403")
                            || detail.contains("429")
                            || detail.to_lowercase().contains("rate limit")
                        {
                            anyhow!(
                                "GitHub API rate limit hit while searching {:?}. Reduce scan limits or wait. Underlying: {}",
                                search_query,
                                detail
                            )
                        } else {
                            anyhow!(
                                "GitHub code search failed for query {:?}: {}",
                                search_query,
                                detail
                            )
                        }
                    })?;

                for item in &page.items {
                    if collected >= RESULTS_PER_QUERY {
                        break;
                    }
                    collected += 1;
                    let repo = item.repository.clone();
                    if let Some(key) = repo.full_name.clone() {
                        if !key.is_empty() && seen.insert(key) {
                            repositories.push(repo);
                        }
                    }
                }

                while collected < RESULTS_PER_QUERY {
                    let next: Option<Page<models::Code>> =
                        self.client.get_page(&page.next).await.map_err(|e| {
                            let display = format!("{}", e);
                            let detail = if display.is_empty() || display == "GitHub" {
                                format!("{:?}", e)
                            } else {
                                display
                            };
                            anyhow!("GitHub pagination fetch failed: {}", detail)
                        })?;
                    match next {
                        Some(p) => {
                            for item in &p.items {
                                if collected >= RESULTS_PER_QUERY {
                                    break;
                                }
                                collected += 1;
                                let repo = item.repository.clone();
                                if let Some(key) = repo.full_name.clone() {
                                    if !key.is_empty() && seen.insert(key) {
                                        repositories.push(repo);
                                    }
                                }
                            }
                            page = p;
                        }
                        None => break,
                    }
                }
            }
        }

        let mut out = Vec::with_capacity(repositories.len());
        for (index, repo) in repositories.into_iter().enumerate() {
            let enrich = index < ENRICHMENT_LIMIT;

            // For enriched repos, fetch full metadata from /repos/{owner}/{repo}
            let repo = if enrich {
                let owner = repo
                    .owner
                    .as_ref()
                    .map(|o| o.login.clone())
                    .unwrap_or_default();
                let name = repo.name.clone();

                if !owner.is_empty() && !name.is_empty() {
                    self.client.repos(&owner, &name).get().await.map_err(|e| {
                        let display = format!("{}", e);
                        let detail = if display.is_empty() || display == "GitHub" {
                            format!("{:?}", e)
                        } else {
                            display
                        };
                        anyhow!(
                            "GitHub repository metadata fetch failed for {}/{}: {}",
                            owner,
                            name,
                            detail
                        )
                    })?
                } else {
                    repo
                }
            } else {
                repo
            };

            out.push(self.to_discovery(repo, enrich).await?);
        }
        Ok(out)
    }

    async fn to_discovery(
        &self,
        repo: models::Repository,
        enrich: bool,
    ) -> Result<ProjectDiscovery> {
        let owner = repo
            .owner
            .as_ref()
            .map(|o| o.login.clone())
            .unwrap_or_default();
        let name = repo.name.clone();

        let readme = if enrich && !owner.is_empty() && !name.is_empty() {
            self.fetch_readme(&owner, &name).await.ok()
        } else {
            None
        };
        let code_file = if enrich && !owner.is_empty() && !name.is_empty() {
            self.fetch_code_file(&owner, &name, "README.md").await.ok()
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
            if let Some(language) = language.as_str() {
                stack.push(language.to_owned());
            }
        }
        if let Some(topics) = &repo.topics {
            for topic in topics {
                if text.contains(topic.as_str()) {
                    stack.push(topic.clone());
                }
            }
        }

        let full_name = repo
            .full_name
            .clone()
            .unwrap_or_else(|| format!("{}/{}", owner, name));
        let html_url = repo
            .html_url
            .as_ref()
            .map(|url| url.to_string())
            .unwrap_or_default();

        Ok(ProjectDiscovery {
            id: full_name.clone(),
            name: full_name,
            repository_url: html_url,
            description: repo.description.clone(),
            discovered_at: chrono::Utc::now(),
            last_updated: None,
            stars: repo.stargazers_count.unwrap_or(0),
            forks: repo.forks_count.unwrap_or(0),
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
        let readme = self
            .client
            .repos(owner, repo)
            .get_readme()
            .send()
            .await
            .map_err(|e| {
                let display = format!("{}", e);
                let detail = if display.is_empty() || display == "GitHub" {
                    format!("{:?}", e)
                } else {
                    display
                };
                anyhow!(
                    "GitHub README fetch failed for {}/{}: {}",
                    owner,
                    repo,
                    detail
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
                let display = format!("{}", e);
                let detail = if display.is_empty() || display == "GitHub" {
                    format!("{:?}", e)
                } else {
                    display
                };
                anyhow!(
                    "GitHub content fetch failed for {}/{} at {}: {}",
                    owner,
                    repo,
                    path,
                    detail
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
}
