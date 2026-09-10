import { useEffect, useMemo, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { Dashboard } from "./components/Dashboard";
import { ResultsDisplay } from "./components/ResultsDisplay";
import { ScannerConfig } from "./components/ScannerConfig";
import { Settings } from "./components/Settings";
import { api } from "./lib/tauri";
import type {
  AppSettings,
  RepositoryFinding,
  ScanProgress,
  ScanReport,
  TokenValidation,
} from "./types";

type Tab = "dashboard" | "scanner" | "results" | "settings";

const fallbackSettings: AppSettings = {
  languages: ["TypeScript", "JavaScript", "Python", "Go", "Rust"],
  lookback_days: 30,
  max_repositories: 12,
  health_check: true,
  max_files_per_repository: 25,
};

const tabs: Array<{ id: Tab; label: string; short: string }> = [
  { id: "dashboard", label: "Dashboard", short: "DB" },
  { id: "scanner", label: "Scanner", short: "SC" },
  { id: "results", label: "Results", short: "RS" },
  { id: "settings", label: "Settings", short: "ST" },
];

function App() {
  const [tab, setTab] = useState<Tab>("dashboard");
  const [settings, setSettings] = useState<AppSettings>(fallbackSettings);
  const [token, setToken] = useState("");
  const [report, setReport] = useState<ScanReport | null>(null);
  const [liveFindings, setLiveFindings] = useState<RepositoryFinding[]>([]);
  const [progress, setProgress] = useState<ScanProgress | null>(null);
  const [running, setRunning] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [exportMessage, setExportMessage] = useState<string | null>(null);

  useEffect(() => {
    api.loadSettings().then(setSettings).catch(() => setSettings(fallbackSettings));
  }, []);

  useEffect(() => {
    let disposed = false;
    let cleanup: Array<() => void> = [];

    Promise.all([
      listen<ScanProgress>("scan://progress", (event) => {
        if (!disposed) setProgress(event.payload);
      }),
      listen<RepositoryFinding>("scan://finding", (event) => {
        if (!disposed) {
          setLiveFindings((current) => {
            const without = current.filter(
              (item) => item.repository.full_name !== event.payload.repository.full_name,
            );
            return [...without, event.payload];
          });
        }
      }),
    ]).then((unlisten) => {
      if (disposed) unlisten.forEach((fn) => fn());
      else cleanup = unlisten;
    });

    return () => {
      disposed = true;
      cleanup.forEach((fn) => fn());
    };
  }, []);

  const tokenConfigured = useMemo(() => token.trim().length > 0, [token]);

  const runScan = async (verifyCredentials: boolean) => {
    setRunning(true);
    setError(null);
    setExportMessage(null);
    setProgress({
      stage: "discovering",
      message: "Starting GitHub repository discovery…",
      completed: 0,
      total: 1,
    });
    setLiveFindings([]);
    setReport(null);
    setTab("results");

    try {
      const next = await api.runScan({
        ...settings,
        github_token: token.trim() || null,
        verify_credentials: verifyCredentials,
      });
      setReport(next);
    } catch (scanError) {
      setError(String(scanError));
    } finally {
      setRunning(false);
    }
  };

  const saveSettings = async () => {
    const saved = await api.saveSettings(settings);
    setSettings(saved);
  };

  const validateToken = async (): Promise<TokenValidation> => {
    if (!token.trim()) {
      return { authenticated: false, message: "Enter a GitHub token first." };
    }
    return api.validateGithubToken(token.trim());
  };

  const exportReport = async (format: "json" | "csv") => {
    if (!report) return;
    setExportMessage(null);
    try {
      const exported = await api.exportReport(report, format);
      setExportMessage(`${format.toUpperCase()} report saved to ${exported.path}`);
    } catch (exportError) {
      setExportMessage(`Export failed: ${String(exportError)}`);
    }
  };

  return (
    <div className="flex min-h-screen">
      <aside className="fixed inset-y-0 left-0 z-20 w-64 border-r border-slate-800/90 bg-slate-950/90 px-4 py-5 backdrop-blur-xl">
        <div className="flex items-center gap-3 px-2">
          <div className="grid h-10 w-10 place-items-center rounded-xl border border-sky-500/30 bg-sky-500/10 text-sm font-black text-sky-300">A</div>
          <div>
            <div className="font-semibold text-white">Raven API Hunter</div>
            <div className="mt-0.5 text-[11px] uppercase tracking-[0.16em] text-slate-500">Public repo hygiene</div>
          </div>
        </div>

        <nav className="mt-8 space-y-2">
          {tabs.map((item) => {
            const active = tab === item.id;
            return (
              <button
                type="button"
                key={item.id}
                onClick={() => setTab(item.id)}
                className={`flex w-full items-center gap-3 rounded-xl px-3 py-3 text-left text-sm font-medium transition ${active ? "bg-sky-500/10 text-sky-200" : "text-slate-500 hover:bg-slate-900 hover:text-slate-300"}`}
              >
                <span className={`grid h-8 w-8 place-items-center rounded-lg border text-[10px] font-bold ${active ? "border-sky-500/30 bg-sky-500/10" : "border-slate-800 bg-slate-900"}`}>{item.short}</span>
                {item.label}
              </button>
            );
          })}
        </nav>

        <div className="absolute bottom-5 left-4 right-4 rounded-xl border border-slate-800 bg-slate-900/70 p-3">
          <div className="flex items-center justify-between text-xs">
            <span className="text-slate-500">GitHub mode</span>
            <span className={tokenConfigured ? "text-emerald-300" : "text-amber-300"}>{tokenConfigured ? "Authenticated" : "Anonymous"}</span>
          </div>
          <div className="mt-2 text-[11px] leading-5 text-slate-600">v0.1.0 · read-only repository analysis</div>
        </div>
      </aside>

      <main className="ml-64 min-h-screen flex-1">
        <div className="mx-auto max-w-[1500px] p-8">
          {error && (
            <div className="mb-6 rounded-xl border border-rose-500/30 bg-rose-500/10 px-4 py-3 text-sm text-rose-200">
              {error}
            </div>
          )}

          {tab === "dashboard" && (
            <Dashboard report={report} liveFindings={liveFindings} running={running} />
          )}
          {tab === "scanner" && (
            <ScannerConfig
              settings={settings}
              onChange={setSettings}
              onRun={runScan}
              running={running}
              progress={progress}
              tokenConfigured={tokenConfigured}
            />
          )}
          {tab === "results" && (
            <ResultsDisplay
              report={report}
              liveFindings={liveFindings}
              running={running}
              onExport={exportReport}
              exportMessage={exportMessage}
            />
          )}
          {tab === "settings" && (
            <Settings
              settings={settings}
              token={token}
              onTokenChange={setToken}
              onSave={saveSettings}
              onValidate={validateToken}
            />
          )}
        </div>
      </main>
    </div>
  );
}

export default App;
