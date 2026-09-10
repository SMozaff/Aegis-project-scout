import { useState } from "react";
import type { AppSettings, ScanProgress } from "../types";

interface ScannerConfigProps {
  settings: AppSettings;
  onChange: (settings: AppSettings) => void;
  onRun: () => Promise<void>;
  running: boolean;
  progress: ScanProgress | null;
  tokenConfigured: boolean;
  verifyCredentials: boolean;
  onVerifyCredentialsChange: (enabled: boolean) => void;
}

const languageOptions = ["TypeScript", "JavaScript", "Python", "Go", "Rust", "Java", "Ruby", "PHP"];

export function ScannerConfig({
  settings,
  onChange,
  onRun,
  running,
  progress,
  tokenConfigured,
  verifyCredentials,
  onVerifyCredentialsChange,
}: ScannerConfigProps) {
  const [webSearch, setWebSearch] = useState(false);

  const update = (changes: Partial<AppSettings>) => onChange({ ...settings, ...changes });
  const toggleLanguage = (language: string) => {
    const languages = settings.languages.includes(language)
      ? settings.languages.filter((item) => item !== language)
      : [...settings.languages, language];
    update({ languages });
  };

  return (
    <div className="space-y-6">
      <div>
        <div className="label">Scanner</div>
        <h1 className="mt-2 text-3xl font-semibold text-white">Configure repository scan</h1>
        <p className="mt-2 max-w-3xl text-sm leading-6 text-slate-400">
          Select the technologies to search for and bound the amount of public repository data to inspect.
        </p>
      </div>

      <div className="grid grid-cols-[1.3fr_1fr] gap-5">
        <div className="panel p-6">
          <div className="label">Languages and technologies</div>
          <div className="mt-4 grid grid-cols-2 gap-3 sm:grid-cols-3">
            {languageOptions.map((language) => (
              <label key={language} className="flex items-center gap-2 rounded-lg border border-slate-800 bg-slate-950/60 px-3 py-2 text-sm text-slate-300">
                <input
                  type="checkbox"
                  checked={settings.languages.includes(language)}
                  onChange={() => toggleLanguage(language)}
                  className="accent-sky-500"
                />
                {language}
              </label>
            ))}
          </div>
          <label className="mt-5 flex items-center gap-3 text-sm text-slate-300">
            <input
              type="checkbox"
              checked={webSearch}
              onChange={(event) => setWebSearch(event.target.checked)}
              className="accent-sky-500"
            />
            Enable optional web search
            <span className="text-xs text-slate-500">(not included in the current backend payload)</span>
          </label>
        </div>

        <div className="panel p-6">
          <div className="label">Scan limits</div>
          <div className="mt-4 space-y-4">
            <label className="block text-sm text-slate-400">
              Lookback days
              <input
                className="input mt-2"
                type="number"
                min={1}
                max={365}
                value={settings.lookback_days}
                onChange={(event) => update({ lookback_days: Number(event.target.value) })}
              />
            </label>
            <label className="block text-sm text-slate-400">
              Maximum repositories
              <input
                className="input mt-2"
                type="number"
                min={1}
                max={100}
                value={settings.max_repositories}
                onChange={(event) => update({ max_repositories: Number(event.target.value) })}
              />
            </label>
            <label className="block text-sm text-slate-400">
              Files per repository
              <input
                className="input mt-2"
                type="number"
                min={1}
                max={100}
                value={settings.max_files_per_repository}
                onChange={(event) => update({ max_files_per_repository: Number(event.target.value) })}
              />
            </label>
            <label className="flex items-center gap-3 text-sm text-slate-300">
              <input
                type="checkbox"
                checked={settings.health_check}
                onChange={(event) => update({ health_check: event.target.checked })}
                className="accent-sky-500"
              />
              Run safe endpoint health checks
            </label>
            <label className="flex items-center gap-3 text-sm text-slate-300">
              <input
                type="checkbox"
                checked={verifyCredentials}
                onChange={(event) => onVerifyCredentialsChange(event.target.checked)}
                className="accent-sky-500"
              />
              Verify credentials (may consume provider API quota)
            </label>
          </div>
        </div>
      </div>

      <div className="panel flex items-center justify-between gap-6 p-5">
        <div>
          <div className="font-semibold text-slate-100">Ready to scan</div>
          <div className="mt-1 text-sm text-slate-500">
            {tokenConfigured ? "Authenticated GitHub access configured." : "Using anonymous GitHub access."}
          </div>
        </div>
        <button type="button" className="button-primary min-w-32" disabled={running} onClick={() => void onRun()}>
          {running ? "Scanning…" : "Run Scan"}
        </button>
      </div>

      {progress && (
        <div className="panel p-5">
          <div className="flex items-center justify-between gap-4">
            <div>
              <div className="label">Scan progress</div>
              <div className="mt-2 font-medium text-slate-100">{progress.message}</div>
            </div>
            <div className="text-sm font-semibold text-sky-300">
              {progress.completed} / {progress.total}
            </div>
          </div>
          <div className="mt-4 h-2 overflow-hidden rounded-full bg-slate-800">
            <div
              className="h-full rounded-full bg-sky-500 transition-all"
              style={{ width: `${progress.total > 0 ? Math.min(100, (progress.completed / progress.total) * 100) : 0}%` }}
            />
          </div>
          <div className="mt-3 flex justify-between text-xs uppercase tracking-wide text-slate-600">
            <span>{progress.stage}</span>
            {progress.repository && <span>{progress.repository}</span>}
          </div>
        </div>
      )}
    </div>
  );
}
