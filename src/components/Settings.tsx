import { useState } from "react";
import type { AppSettings, TokenValidation } from "../types";

interface SettingsProps {
  settings: AppSettings;
  token: string;
  onTokenChange: (token: string) => void;
  onSave: () => Promise<void>;
  onValidate: () => Promise<TokenValidation>;
}

export function Settings({ settings, token, onTokenChange, onSave, onValidate }: SettingsProps) {
  const [showToken, setShowToken] = useState(false);
  const [validation, setValidation] = useState<TokenValidation | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const validate = async () => {
    setBusy(true);
    setMessage(null);
    try {
      setValidation(await onValidate());
    } catch (error) {
      setValidation({ authenticated: false, message: String(error) });
    } finally {
      setBusy(false);
    }
  };

  const save = async () => {
    setBusy(true);
    try {
      await onSave();
      setMessage("Non-secret scanner settings saved.");
    } catch (error) {
      setMessage(String(error));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="space-y-6">
      <div>
        <div className="label">Settings</div>
        <h1 className="mt-2 text-3xl font-semibold text-white">GitHub and application settings</h1>
        <p className="mt-2 text-sm leading-6 text-slate-400">
          The token is kept in frontend memory for the current app session. It is not written to the Raven API Hunter settings JSON file or exported reports.
        </p>
      </div>

      <div className="grid grid-cols-[1.3fr_1fr] gap-5">
        <div className="panel p-6">
          <div className="label">GitHub API token</div>
          <div className="mt-4 flex gap-2">
            <input
              type={showToken ? "text" : "password"}
              value={token}
              onChange={(e) => {
                onTokenChange(e.target.value);
                setValidation(null);
              }}
              className="input"
              placeholder="github_pat_… or OAuth access token"
              autoComplete="off"
              spellCheck={false}
            />
            <button type="button" className="button-secondary min-w-24" onClick={() => setShowToken((v) => !v)}>
              {showToken ? "Hide" : "Show"}
            </button>
          </div>
          <div className="mt-3 flex items-center gap-3">
            <button type="button" className="button-primary" disabled={busy || !token.trim()} onClick={validate}>
              Validate token
            </button>
            {validation && (
              <div className={`text-sm ${validation.authenticated ? "text-emerald-300" : "text-rose-300"}`}>
                {validation.message}
                {validation.login ? ` (${validation.login})` : ""}
                {validation.rate_limit_remaining != null ? ` · ${validation.rate_limit_remaining} API requests remaining` : ""}
              </div>
            )}
          </div>
        </div>

        <div className="panel p-6">
          <div className="label">Saved defaults</div>
          <dl className="mt-4 space-y-4 text-sm">
            <div className="flex justify-between gap-4"><dt className="text-slate-500">Languages</dt><dd className="text-right text-slate-200">{settings.languages.join(", ")}</dd></div>
            <div className="flex justify-between gap-4"><dt className="text-slate-500">Lookback</dt><dd className="text-slate-200">{settings.lookback_days} days</dd></div>
            <div className="flex justify-between gap-4"><dt className="text-slate-500">Repositories</dt><dd className="text-slate-200">{settings.max_repositories} max</dd></div>
            <div className="flex justify-between gap-4"><dt className="text-slate-500">Files/repo</dt><dd className="text-slate-200">{settings.max_files_per_repository} max</dd></div>
            <div className="flex justify-between gap-4"><dt className="text-slate-500">Health checks</dt><dd className="text-slate-200">{settings.health_check ? "Enabled" : "Disabled"}</dd></div>
          </dl>
          <button type="button" className="button-secondary mt-6 w-full" disabled={busy} onClick={save}>Save defaults</button>
          {message && <div className="mt-3 text-xs text-slate-400">{message}</div>}
        </div>
      </div>

      <div className="panel p-6">
        <div className="font-semibold text-slate-100">Recommended GitHub token scope</div>
        <p className="mt-2 max-w-4xl text-sm leading-6 text-slate-400">
          For public repository monitoring, use the least privilege available. Raven API Hunter only performs read operations against repository metadata, Git trees/blobs, and the authenticated user endpoint used for token validation. It does not create issues, commits, webhooks, branches, or repository changes.
        </p>
      </div>
    </div>
  );
}
