# Aegis Project Scout Architecture

## Data flow

```text
React Scanner UI
      │
      │ Tauri invoke: run_scan
      ▼
Rust command layer
      │
      ├── GitHub discovery → public repository search
      │
      ├── GitHub scanner → recursive tree → bounded candidate blobs
      │
      ├── Pattern analyzer → endpoint/route evidence
      │
      ├── Health checker → safe HEAD-only public HTTP checks
      │
      └── scan://finding + scan://progress events
      ▼
React Results / Dashboard
      │
      └── export_report → JSON or CSV in Documents/Aegis Project Scout/exports
```

## Trust boundaries

- The WebView never performs GitHub or endpoint network calls directly; network access is centralized in Rust.
- The GitHub token crosses Tauri IPC only when a scan or explicit token validation is requested.
- The token is not part of `AppSettings` or `ScanReport`, which prevents accidental configuration/report persistence.
- Repository-provided blob URLs are accepted only when they begin with `https://api.github.com/`.
- Endpoint health checks validate URL scheme/host/address, pin DNS to a vetted public address, and disable redirects.

## Scan bounds

- 1–100 repositories per scan.
- 5–100 candidate files per repository.
- Source blobs above 350 KB are skipped.
- 8 health checks per repository.
- 30 health checks per scan.
- 500 candidate blob reads allocated per scan.
- 6 second HTTP health timeout.

These constraints keep the application suitable for interactive developer hygiene work rather than high-volume probing.
