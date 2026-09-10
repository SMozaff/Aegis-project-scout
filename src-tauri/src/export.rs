use std::{collections::HashMap, fs, path::PathBuf};

use chrono::Utc;
use tauri::{AppHandle, Manager};

use crate::models::{EndpointHealth, ExportResult, ScanReport};
use crate::scanner::verify::VerifyOutcome;

pub fn export(app: &AppHandle, report: &ScanReport, format: &str) -> Result<ExportResult, String> {
    let format = format.to_ascii_lowercase();
    if format != "json" && format != "csv" {
        return Err("Export format must be 'json' or 'csv'.".into());
    }

    let directory = export_directory(app)?;
    fs::create_dir_all(&directory)
        .map_err(|e| format!("Unable to create export directory: {e}"))?;

    let stamp = Utc::now().format("%Y%m%d-%H%M%S");
    let path = directory.join(format!("aegis-scan-{stamp}.{format}"));

    match format.as_str() {
        "json" => {
            let bytes = serde_json::to_vec_pretty(&redacted_json_report(report))
                .map_err(|e| format!("Unable to serialize JSON report: {e}"))?;
            fs::write(&path, bytes).map_err(|e| format!("Unable to write JSON report: {e}"))?;
        }
        "csv" => write_csv(&path, report)?,
        _ => unreachable!(),
    }

    Ok(ExportResult {
        path: path.to_string_lossy().to_string(),
        format,
    })
}

fn export_directory(app: &AppHandle) -> Result<PathBuf, String> {
    if let Ok(documents) = app.path().document_dir() {
        return Ok(documents.join("Aegis Project Scout").join("exports"));
    }
    app.path()
        .app_data_dir()
        .map(|path| path.join("exports"))
        .map_err(|e| format!("Unable to resolve export directory: {e}"))
}

fn write_csv(path: &PathBuf, report: &ScanReport) -> Result<(), String> {
    let mut writer =
        csv::Writer::from_path(path).map_err(|e| format!("Unable to create CSV report: {e}"))?;

    writer
        .write_record([
            "repository",
            "repository_url",
            "language",
            "stars",
            "pushed_at",
            "scanned_files",
            "pattern_id",
            "category",
            "confidence",
            "file_path",
            "line_number",
            "captured",
            "health_reachable",
            "health_blocked",
            "health_status",
            "health_latency_ms",
            "health_reason",
            "warnings",
        ])
        .map_err(|e| format!("Unable to write CSV header: {e}"))?;

    for finding in &report.findings {
        let health_by_endpoint: HashMap<&str, &EndpointHealth> = finding
            .health
            .iter()
            .map(|health| (health.endpoint.as_str(), health))
            .collect();

        if finding.matches.is_empty() {
            writer
                .write_record([
                    csv_safe(&finding.repository.full_name),
                    csv_safe(&finding.repository.html_url),
                    csv_safe(finding.repository.language.as_deref().unwrap_or("")),
                    finding.repository.stars.to_string(),
                    finding.repository.pushed_at.clone().unwrap_or_default(),
                    finding.scanned_files.to_string(),
                    String::new(),
                    String::new(),
                    String::new(),
                    String::new(),
                    String::new(),
                    String::new(),
                    String::new(),
                    String::new(),
                    String::new(),
                    String::new(),
                    String::new(),
                    csv_safe(&finding.warnings.join(" | ")),
                ])
                .map_err(|e| format!("Unable to write CSV row: {e}"))?;
            continue;
        }

        for matched in &finding.matches {
            let health = matched
                .absolute_endpoint
                .as_deref()
                .and_then(|endpoint| health_by_endpoint.get(endpoint).copied());

            writer
                .write_record([
                    csv_safe(&finding.repository.full_name),
                    csv_safe(&finding.repository.html_url),
                    csv_safe(finding.repository.language.as_deref().unwrap_or("")),
                    finding.repository.stars.to_string(),
                    finding.repository.pushed_at.clone().unwrap_or_default(),
                    finding.scanned_files.to_string(),
                    csv_safe(&matched.pattern_id),
                    csv_safe(&matched.category),
                    csv_safe(&matched.confidence),
                    csv_safe(&matched.file_path),
                    matched.line_number.to_string(),
                    csv_safe(captured_for_export(matched.category.as_str(), &matched.captured)),
                    health.map(|h| h.reachable.to_string()).unwrap_or_default(),
                    health.map(|h| h.blocked.to_string()).unwrap_or_default(),
                    health
                        .and_then(|h| h.status_code)
                        .map(|value| value.to_string())
                        .unwrap_or_default(),
                    health
                        .and_then(|h| h.latency_ms)
                        .map(|value| value.to_string())
                        .unwrap_or_default(),
                    csv_safe(health.and_then(|h| h.reason.as_deref()).unwrap_or("")),
                    csv_safe(&finding.warnings.join(" | ")),
                ])
                .map_err(|e| format!("Unable to write CSV row: {e}"))?;
        }
    }

    writer
        .write_record([""])
        .map_err(|e| format!("Unable to separate verified credentials section: {e}"))?;
    writer
        .write_record([
            "provider",
            "source_repo",
            "source_file",
            "line_number",
            "pattern_name",
            "verification_detail",
        ])
        .map_err(|e| format!("Unable to write verified credentials header: {e}"))?;
    for credential in &report.verified_credentials {
        writer
            .write_record([
                csv_safe(&credential.provider),
                csv_safe(&credential.source_repo),
                csv_safe(&credential.source_file),
                credential.line_number.to_string(),
                csv_safe(&credential.pattern_name),
                csv_safe(verification_detail(&credential.outcome)),
            ])
            .map_err(|e| format!("Unable to write verified credential row: {e}"))?;
    }

    writer
        .flush()
        .map_err(|e| format!("Unable to flush CSV report: {e}"))?;
    Ok(())
}

fn redacted_json_report(report: &ScanReport) -> serde_json::Value {
    let mut value = serde_json::to_value(report).unwrap_or_else(|_| serde_json::json!({}));
    if let Some(findings) = value.get_mut("findings").and_then(|value| value.as_array_mut()) {
        for finding in findings {
            if let Some(matches) = finding
                .get_mut("matches")
                .and_then(|value| value.as_array_mut())
            {
                for matched in matches {
                    if matched.get("category").and_then(|value| value.as_str()) == Some("auth_token")
                    {
                        matched["captured"] = serde_json::Value::String("[REDACTED]".into());
                    }
                }
            }
        }
    }

    value["verified_credentials"] = serde_json::Value::Array(
        report
            .verified_credentials
            .iter()
            .map(|credential| {
                serde_json::json!({
                    "provider": credential.provider,
                    "source_repo": credential.source_repo,
                    "source_file": credential.source_file,
                    "line_number": credential.line_number,
                    "pattern_name": credential.pattern_name,
                    "outcome": { "detail": verification_detail(&credential.outcome) },
                })
            })
            .collect(),
    );
    value
}

fn captured_for_export<'a>(category: &str, captured: &'a str) -> &'a str {
    if category == "auth_token" {
        "[REDACTED]"
    } else {
        captured
    }
}

fn verification_detail(outcome: &VerifyOutcome) -> &str {
    match outcome {
        VerifyOutcome::Valid { detail } | VerifyOutcome::Invalid { detail } => detail,
        VerifyOutcome::Unverifiable { reason } => reason,
    }
}

fn csv_safe(value: &str) -> String {
    let value = value.replace('\r', " ").replace('\n', " ");
    if matches!(value.chars().next(), Some('=' | '+' | '-' | '@')) {
        format!("'{value}")
    } else {
        value
    }
}
