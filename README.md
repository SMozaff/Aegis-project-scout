# Raven Hunter

Raven API Hunter is a Tauri v2 desktop application for developer security hygiene. It monitors **public GitHub repositories** for API endpoint and route patterns, performs bounded non-invasive responsiveness checks, presents project metrics in a React dashboard, and exports findings for compliance review.

## Scope and safety model

Raven is intentionally read-only and bounded:

- Repository discovery uses GitHub's public repository search and Git tree/blob APIs.
- It does not create commits, issues, branches, webhooks, pull requests, or repository changes.
- The default analyzer looks for endpoint/API patterns only; it does not hunt for credentials.
- HTTP health checks use `HEAD` only, do not follow redirects, remove URL query strings, and reject localhost, private, link-local, multicast, and common reserved/special-purpose network targets.
- Health checks are capped at 8 endpoints per repository and 30 endpoints per scan.
- GitHub tokens are not persisted in Raven settings and are never placed in exported reports.
- Candidate source/config files are capped by file size and by a configurable files-per-repository limit.

Use Raven only for repositories and endpoint monitoring activities that you are authorized to perform.

## Stack

- Tauri v2
- Rust 2021
- React 18 + TypeScript
- Vite
- Tailwind CSS
- `reqwest`, `serde`, `serde_json`, `regex`, `tokio`, `chrono`, `base64`, `csv`, `url`

## Project structure

```text
Raven-API-Hunter/
├── src/
│   ├── components/
│   │   ├── Dashboard.tsx
│   │   ├── ResultsDisplay.tsx
│   │   ├── ScannerConfig.tsx
│   │   └── Settings.tsx
│   ├── lib/tauri.ts
│   ├── App.tsx
│   ├── index.css
│   ├── main.tsx
│   └── types.ts
├── src-tauri/
│   ├── capabilities/default.json
│   ├── patterns/default_patterns.json
│   ├── src/
│   │   ├── commands.rs
│   │   ├── config.rs
│   │   ├── export.rs
│   │   ├── github.rs
│   │   ├── health.rs
│   │   ├── lib.rs
│   │   ├── main.rs
│   │   ├── models.rs
│   │   └── patterns.rs
│   ├── build.rs
│   ├── Cargo.toml
│   └── tauri.conf.json
├── package.json
├── tailwind.config.js
├── tsconfig.json
└── vite.config.ts
```

## Prerequisites

Install the standard Tauri v2 prerequisites for your platform, including:

- Rust stable toolchain
- Node.js LTS + npm
- Platform-specific Tauri dependencies (WebView2/MSVC on Windows, Xcode tooling on macOS, WebKit/GTK packages on Linux)

The official Tauri prerequisite guide is: `https://v2.tauri.app/start/prerequisites/`.

## Install

From the project root:

```bash
npm install
```

Cargo dependencies are installed automatically by Cargo when the Tauri development/build command runs.

## Run in development

```bash
npm run tauri dev
```

For a frontend-only browser build check:

```bash
npm run build
```

## Build desktop installers

```bash
npm run tauri build
```

Tauri will produce platform-appropriate bundles under `src-tauri/target/release/bundle/`.

## GitHub authentication

Open **Settings** and paste a GitHub OAuth/PAT access token. The token is held in memory for the application session and sent as an `Authorization: Bearer ...` header to GitHub.

Raven also supports the `GH_TOKEN` environment variable as a backend fallback:

```bash
export GH_TOKEN="your-token"
npm run tauri dev
```

On Windows PowerShell:

```powershell
$env:GH_TOKEN="your-token"
npm run tauri dev
```

For public repository monitoring, use the least privilege available. Raven only needs read access to public repository metadata/content plus the authenticated-user endpoint if you use the **Validate token** button.

## Running a scan

1. Open **Scanner**.
2. Select one or more target technologies/languages.
3. Set the lookback period from 1 to 365 days.
4. Set a repository limit from 1 to 100.
5. Set files sampled per repository from 5 to 100.
6. Enable or disable safe health checks.
7. Click **Run scan**.

Results stream into the **Results** view as each repository completes. The dashboard is populated with final metrics when the scan finishes.

### Discovery behavior

For each selected language, Raven searches for recently pushed public, non-archived, non-fork repositories. It then requests the repository's recursive Git tree and prioritizes likely API/configuration files, including names containing terms such as:

- `openapi`
- `swagger`
- `routes` / `router`
- `endpoint`
- `api`
- `client` / `service`
- `controller`
- `config` / `settings`
- `graphql`

Large/binary/generated/vendor files are skipped.

## Default pattern definitions

Patterns live in:

```text
src-tauri/patterns/default_patterns.json
```

The included definitions detect examples such as:

- Absolute HTTP(S) URLs
- OpenAPI/server/base-URL declarations
- `fetch(...)` / axios calls
- Python `requests.*(...)` calls
- API/base URL configuration assignments
- Express-style route declarations
- Axum-style `route(...)` declarations
- Spring mapping annotations
- Common OpenAPI path keys

Each definition includes an ID, category, confidence level, regex, and capture group. The Rust backend compiles these patterns at startup for each scan.

## Health checking

Only absolute HTTP(S) endpoints can be checked. Before a request, Raven:

1. Parses and normalizes the URL.
2. Removes query strings and fragments from the health-check target.
3. Rejects URLs with embedded username/password data.
4. Rejects local/internal hostname suffixes.
5. Resolves DNS and rejects any result pointing at private, loopback, link-local, multicast, or common special-purpose ranges.
6. Pins the HTTP client to a vetted resolved address.
7. Disables redirects.
8. Sends a single `HEAD` request with a short timeout.

Any HTTP response, including `405 Method Not Allowed`, confirms network responsiveness. Redirect responses are recorded but not followed.

## Exports

After a completed scan, use **Export JSON** or **Export CSV** in the Results screen.

Exports are written to:

```text
Documents/Raven API Hunter/exports/
```

If a Documents directory is unavailable, Raven falls back to its application data directory.

CSV output is sanitized to reduce spreadsheet formula-injection risk. Reports include repository metadata, pattern evidence, health status, and warnings. They do not contain the GitHub token.

## Configuration storage

Non-secret settings are saved as `settings.json` in Tauri's platform-specific application configuration directory. Persisted fields are:

- target languages
- lookback days
- repository limit
- files-per-repository limit
- health-check enabled/disabled

The GitHub token is deliberately excluded.

## GitHub API notes

The backend uses the versioned GitHub REST API and sends:

```text
Accept: application/vnd.github+json
X-GitHub-Api-Version: 2026-03-10
```

Unauthenticated public reads work at lower API limits. Supplying an access token is recommended for normal use.

## Extending the analyzer

To add another endpoint pattern, append a JSON entry to `default_patterns.json`:

```json
{
  "id": "framework-route-example",
  "name": "Framework route example",
  "description": "Description of what the pattern identifies.",
  "category": "rest-route",
  "confidence": "medium",
  "regex": "your-regex-with-a-capture-group",
  "extract_group": 1
}
```

Keep new rules focused on endpoint/API exposure and avoid credential/secret collection if you want to preserve Raven's intended security-hygiene scope.

## Known operational limits

- Repository search is sampling-based rather than an exhaustive GitHub-wide crawl.
- Recursive Git trees can be truncated by GitHub for extremely large repositories; Raven surfaces a warning when that occurs.
- The scanner prioritizes likely API/configuration files rather than downloading every file in every repository.
- A responsive endpoint is not necessarily healthy, vulnerable, or intended for public use; findings require human review.
- A pattern match is evidence of a source-code pattern, not proof of a security issue.

## License

No license has been selected for this generated starter. Add the license appropriate for your organization before redistribution.
