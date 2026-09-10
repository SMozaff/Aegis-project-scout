import { invoke } from "@tauri-apps/api/core";
import type {
  AppSettings,
  ExportResult,
  ScanConfig,
  ScanReport,
  TokenValidation,
} from "../types";

export const api = {
  loadSettings: () => invoke<AppSettings>("load_settings"),
  saveSettings: (settings: AppSettings) =>
    invoke<AppSettings>("save_settings", { settings }),
  validateGithubToken: (token: string) =>
    invoke<TokenValidation>("validate_github_token", { token }),
  runScan: (config: ScanConfig) => invoke<ScanReport>("run_scan", { config }),
  exportReport: (report: ScanReport, format: "json" | "csv") =>
    invoke<ExportResult>("export_report", { report, format }),
};
