use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Emitter, State};

use crate::{
    export,
    models::{
        AppSettings, ExportResult, RepositoryFinding, ScanConfig, ScanProgress, ScanReport,
        TokenValidation,
    },
    scanner::verify::{ProviderVerifier, VerifyOutcome},
    utils::config,
    AppState,
};

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

    let client = octocrab::Octocrab::builder()
        .personal_token(token)
        .build()
        .map_err(|error| error.to_string())?;

    let user = match client.current().user().await {
        Ok(user) => user,
        Err(error) => {
            return Ok(TokenValidation {
                authenticated: false,
                login: None,
                rate_limit_remaining: None,
                message: format!("GitHub rejected the token: {error}"),
            });
        }
    };

    let remaining = client
        .ratelimit()
        .get()
        .await
        .ok()
        .map(|limits| limits.resources.core.remaining);

    Ok(TokenValidation {
        authenticated: true,
        login: Some(user.login.clone()),
        rate_limit_remaining: remaining,
        message: format!("Authenticated as {}", user.login),
    })
}

#[tauri::command]
pub async fn verify_credential(provider: String, token: String) -> Result<VerifyOutcome, String> {
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
    if state.scanning.swap(true, Ordering::SeqCst) {
        return Err("A scan is already running.".into());
    }
    let _reset = ScanReset(&state.scanning);
    let progress_app = app.clone();
    let progress_callback = Box::new(move |progress: ScanProgress| {
        let _ = progress_app.emit("scan://progress", progress);
    });
    let finding_app = app.clone();
    let finding_callback = Box::new(move |finding: RepositoryFinding| {
        let _ = finding_app.emit("scan://finding", finding);
    });
    crate::scan_headless::run_scan_headless(config, Some(progress_callback), Some(finding_callback))
        .await
}

struct ScanReset<'a>(&'a AtomicBool);
impl Drop for ScanReset<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}
