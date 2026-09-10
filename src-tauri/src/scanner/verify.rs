use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum VerifyOutcome {
    Valid { detail: String },
    Invalid { detail: String },
    Unverifiable { reason: String },
}

pub struct ProviderVerifier {
    client: Client,
}

impl ProviderVerifier {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
        }
    }

    pub async fn verify_openai(&self, token: &str) -> Result<VerifyOutcome, String> {
        self.verify_bearer("https://api.openai.com/v1/models", token)
            .await
    }

    pub async fn verify_anthropic(&self, token: &str) -> Result<VerifyOutcome, String> {
        let response = self
            .client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", token)
            .header("anthropic-version", "2023-06-01")
            .json(&json!({
                "model": "claude-3-haiku-20240307",
                "max_tokens": 1,
                "messages": [{"role": "user", "content": "."}]
            }))
            .send()
            .await
            .map_err(|error| error.to_string())?;

        Ok(outcome_for_status(response.status(), "Anthropic"))
    }

    pub async fn verify_github(&self, token: &str) -> Result<VerifyOutcome, String> {
        let response = self
            .client
            .get("https://api.github.com/user")
            .header("Authorization", format!("token {token}"))
            .header("User-Agent", "Aegis-Project-Scout")
            .send()
            .await
            .map_err(|error| error.to_string())?;

        Ok(outcome_for_status(response.status(), "GitHub"))
    }

    pub async fn verify_aws(
        &self,
        _access_key: &str,
        _secret_key: &str,
    ) -> Result<VerifyOutcome, String> {
        Ok(VerifyOutcome::Unverifiable {
            reason: "AWS verification is not implemented yet.".into(),
        })
    }

    pub async fn verify_generic(&self, token: &str, url: &str) -> Result<VerifyOutcome, String> {
        let response = self
            .client
            .get(url)
            .header("Authorization", format!("Bearer {token}"))
            .send()
            .await
            .map_err(|error| error.to_string())?;

        Ok(outcome_for_status(response.status(), "The provider"))
    }

    async fn verify_bearer(&self, url: &str, token: &str) -> Result<VerifyOutcome, String> {
        let response = self
            .client
            .get(url)
            .header("Authorization", format!("Bearer {token}"))
            .send()
            .await
            .map_err(|error| error.to_string())?;

        Ok(outcome_for_status(response.status(), "The provider"))
    }
}

fn outcome_for_status(status: StatusCode, provider: &str) -> VerifyOutcome {
    if status.is_success() {
        VerifyOutcome::Valid {
            detail: format!("{provider} accepted the credential ({status})."),
        }
    } else if matches!(status, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN) {
        VerifyOutcome::Invalid {
            detail: format!("{provider} rejected the credential ({status})."),
        }
    } else {
        VerifyOutcome::Unverifiable {
            reason: format!("{provider} returned {status}; credential validity could not be determined."),
        }
    }
}

impl Default for ProviderVerifier {
    fn default() -> Self {
        Self::new()
    }
}
