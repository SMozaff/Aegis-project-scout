import type { RepositoryFinding, ScanReport } from "../types";

interface ResultsDisplayProps {
  report: ScanReport | null;
  liveFindings: RepositoryFinding[];
  running: boolean;
  onExport: (format: "json" | "csv") => Promise<void>;
  exportMessage: string | null;
}

function durationLabel(report: ScanReport | null): string {
  if (!report) return "—";
  const start = Date.parse(report.started_at);
  const end = Date.parse(report.completed_at);
  if (!Number.isFinite(start) || !Number.isFinite(end)) return "—";
  const seconds = Math.max(0, end - start) / 1000;
  return seconds < 60 ? `${seconds.toFixed(1)}s` : `${Math.floor(seconds / 60)}m ${(seconds % 60).toFixed(0)}s`;
}

function endpointCount(finding: RepositoryFinding): number {
  return new Set(finding.matches.map((match) => match.absolute_endpoint).filter(Boolean)).size;
}

function healthLabel(finding: RepositoryFinding): string {
  if (finding.health.length === 0) return "—";
  const reachable = finding.health.filter((item) => item.reachable).length;
  return `${reachable}/${finding.health.length} responsive`;
}

export function ResultsDisplay({
  report,
  liveFindings,
  running,
  onExport,
  exportMessage,
}: ResultsDisplayProps) {
  const findings = running ? liveFindings : report?.findings ?? [];
  const metrics = report?.metrics;
  const totalMatches = running
    ? findings.reduce((total, finding) => total + finding.matches.length, 0)
    : metrics?.total_matches ?? 0;
  const totalEndpoints = running
    ? new Set(findings.flatMap((finding) => finding.matches.map((match) => match.absolute_endpoint).filter(Boolean))).size
    : metrics?.unique_absolute_endpoints ?? 0;

  return (
    <div className="space-y-6">
      <div className="flex items-end justify-between gap-6">
        <div>
          <div className="label">Results</div>
          <h1 className="mt-2 text-3xl font-semibold text-slate-900">Credential Audit Results</h1>
          <p className="mt-2 text-sm leading-6 text-slate-600">
            {running ? "Findings are arriving as each repository completes." : "Review pattern matches and endpoint responsiveness from the completed scan."}
          </p>
        </div>
        <div className="flex gap-2">
          <button type="button" className="button-secondary" disabled={!report || running} onClick={() => void onExport("json")}>
            Export Audit Report (JSON)
          </button>
          <button type="button" className="button-secondary" disabled={!report || running} onClick={() => void onExport("csv")}>
            Export Audit Report (CSV)
          </button>
        </div>
      </div>

      {exportMessage && (
        <div className="rounded-xl border border-sky-500/30 bg-sky-500/10 px-4 py-3 text-sm text-sky-700">
          {exportMessage}
        </div>
      )}

      <div className="grid grid-cols-2 gap-4 md:grid-cols-5">
        {[
          ["Repositories scanned", metrics?.repositories_scanned ?? (running ? findings.length : 0)],
          ["Files scanned", metrics?.files_scanned ?? "—"],
          ["Matches", totalMatches],
          ["Endpoints", totalEndpoints],
          ["Duration", durationLabel(report)],
        ].map(([label, value]) => (
          <div key={label} className="panel p-4">
            <div className="label">{label}</div>
            <div className="mt-2 text-2xl font-semibold text-slate-900">{value}</div>
          </div>
        ))}
      </div>

      <div className="flex items-center justify-between gap-4">
        <div className="rounded-full border border-sky-500/20 bg-sky-500/10 px-3 py-1 text-xs font-medium text-sky-700">
          Verified credentials: {report?.verified_credentials.length ?? 0}
        </div>
      </div>

      <div className="panel overflow-hidden">
        <div className="overflow-x-auto">
          <table className="w-full min-w-[760px] text-left text-sm">
            <thead className="border-b border-slate-200 bg-slate-50/60 text-xs uppercase tracking-wide text-slate-600">
              <tr>
                {[
                  "Repository",
                  "Owner",
                  "Stars",
                  "Language",
                  "Matches",
                  "Endpoints",
                  "Health",
                  "Verified",
                ].map((heading) => (
                  <th key={heading} className="px-5 py-4 font-medium">{heading}</th>
                ))}
              </tr>
            </thead>
            <tbody className="divide-y divide-slate-200/80">
              {findings.length === 0 ? (
                <tr>
                  <td colSpan={8} className="px-5 py-14 text-center text-slate-600">
                    {running ? "Waiting for repository findings…" : "Run a scan to populate results."}
                  </td>
                </tr>
              ) : (
                findings.map((finding) => {
                  const repository = finding.repository;
                  const owner = repository.full_name.split("/")[0] || "—";
                  return (
                    <tr key={repository.full_name} className="text-slate-700 hover:bg-slate-100/50">
                      <td className="px-5 py-4 font-medium text-slate-900">
                        <a className="hover:text-sky-700" href={repository.html_url} target="_blank" rel="noreferrer">
                          {repository.full_name}
                        </a>
                      </td>
                      <td className="px-5 py-4 text-slate-600">{owner}</td>
                      <td className="px-5 py-4">{repository.stars.toLocaleString()}</td>
                      <td className="px-5 py-4">{repository.language ?? "Unknown"}</td>
                      <td className="px-5 py-4 text-sky-700">{finding.matches.length}</td>
                      <td className="px-5 py-4">{endpointCount(finding)}</td>
                      <td className="px-5 py-4">{healthLabel(finding)}</td>
                      <td className="px-5 py-4">
                        {finding.matches.some((match) => match.verification?.kind === "valid")
                          ? "✓"
                          : finding.matches.some((match) => match.verification?.kind === "invalid")
                            ? "✗"
                            : "—"}
                      </td>
                    </tr>
                  );
                })
              )}
            </tbody>
          </table>
        </div>
      </div>
    </div>
  );
}
