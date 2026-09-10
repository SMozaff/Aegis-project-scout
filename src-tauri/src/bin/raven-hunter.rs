use clap::Parser;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use raven_api_hunter_lib::models::{AppSettings, RepositoryFinding, ScanConfig, ScanProgress};

#[derive(Parser, Debug)]
#[command(
    name = "raven-hunter",
    version,
    about = "Headless credential exposure scanner for public GitHub repositories"
)]
struct Cli {
    #[arg(long, env = "GH_TOKEN")]
    token: String,

    #[arg(long, value_delimiter = ',', default_value = "openai")]
    tech: Vec<String>,

    #[arg(long, default_value_t = 30)]
    lookback: u32,

    #[arg(long, default_value_t = 30)]
    max_results: u32,

    #[arg(long, default_value_t = true)]
    verify: bool,

    #[arg(long, default_value = "raven-report.json")]
    output: PathBuf,

    #[arg(long, default_value_t = false)]
    quiet: bool,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let defaults = AppSettings::default();
    let config = ScanConfig {
        github_token: Some(cli.token.clone()),
        verify_credentials: cli.verify,
        languages: cli.tech.clone(),
        lookback_days: cli.lookback.min(u16::MAX as u32) as u16,
        max_repositories: cli.max_results.min(u16::MAX as u32) as u16,
        health_check: defaults.health_check,
        max_files_per_repository: defaults.max_files_per_repository,
    };

    let counter = Arc::new(AtomicU32::new(0));
    let quiet = cli.quiet;
    let progress_callback = Box::new(move |progress: ScanProgress| {
        if !quiet {
            eprintln!(
                "[progress] stage={} repo={} count={}/{}",
                progress.stage,
                progress.repository.as_deref().unwrap_or("—"),
                progress.completed,
                progress.total
            );
        }
    });

    let counter_for_finding = counter.clone();
    let quiet_for_finding = cli.quiet;
    let finding_callback = Box::new(move |finding: RepositoryFinding| {
        let number = counter_for_finding.fetch_add(1, Ordering::SeqCst) + 1;
        if !quiet_for_finding {
            eprintln!(
                "[finding] #{} repo={} matches={} verified={}",
                number,
                finding.repository.full_name,
                finding.matches.len(),
                finding.verified_count
            );
        }
    });

    match raven_api_hunter_lib::scan_headless::run_scan_headless(
        config,
        Some(progress_callback),
        Some(finding_callback),
    )
    .await
    {
        Ok(report) => {
            let json = serde_json::to_string_pretty(&report).expect("serialize report");
            std::fs::write(&cli.output, json).expect("write report");
            if !cli.quiet {
                eprintln!(
                    "[done] report written to {} — verified credentials: {}",
                    cli.output.display(),
                    report.verified_credentials.len()
                );
            }
        }
        Err(error) => {
            eprintln!("[error] {error}");
            std::process::exit(1);
        }
    }
}
