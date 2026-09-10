export type Confidence = "low" | "medium" | "high";

export interface AppSettings {
  languages: string[];
  lookback_days: number;
  max_repositories: number;
  health_check: boolean;
  max_files_per_repository: number;
}

export interface ScanConfig extends AppSettings {
  github_token?: string | null;
  verify_credentials: boolean;
}

export interface RepositorySummary {
  full_name: string;
  html_url: string;
  description?: string | null;
  language?: string | null;
  default_branch: string;
  stars: number;
  forks: number;
  pushed_at?: string | null;
}

export interface PatternMatch {
  pattern_id: string;
  pattern_name: string;
  category: "endpoint" | "auth_token";
  confidence: Confidence;
  file_path: string;
  line_number: number;
  excerpt: string;
  captured: string;
  absolute_endpoint?: string | null;
  verification?: VerifyOutcome | null;
}

export interface EndpointHealth {
  endpoint: string;
  reachable: boolean;
  blocked: boolean;
  status_code?: number | null;
  latency_ms?: number | null;
  reason?: string | null;
}

export interface RepositoryFinding {
  repository: RepositorySummary;
  scanned_files: number;
  matches: PatternMatch[];
  health: EndpointHealth[];
  warnings: string[];
  verified_count: number;
}

export interface ScanMetrics {
  repositories_discovered: number;
  repositories_scanned: number;
  files_scanned: number;
  total_matches: number;
  unique_absolute_endpoints: number;
  reachable_endpoints: number;
  blocked_health_checks: number;
}

export interface ScanReport {
  generated_at: string;
  started_at: string;
  completed_at: string;
  settings: AppSettings;
  metrics: ScanMetrics;
  findings: RepositoryFinding[];
  verified_credentials: VerifiedCredential[];
}

export interface ScanProgress {
  stage: "discovering" | "repository" | "health" | "complete" | "error";
  message: string;
  completed: number;
  total: number;
  repository?: string | null;
}

export interface TokenValidation {
  authenticated: boolean;
  login?: string | null;
  rate_limit_remaining?: number | null;
  message: string;
}

export type VerifyOutcome =
  | { kind: "valid"; detail: string }
  | { kind: "invalid"; detail: string }
  | { kind: "unverifiable"; reason: string };

export interface VerifiedCredential {
  provider: string;
  source_repo: string;
  source_file: string;
  line_number: number;
  pattern_name: string;
  outcome: VerifyOutcome;
}

export interface ExportResult {
  path: string;
  format: "json" | "csv";
}
