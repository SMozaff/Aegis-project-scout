import type { RepositoryFinding, ScanReport } from "../types";

interface DashboardProps {
  report: ScanReport | null;
  liveFindings: RepositoryFinding[];
  running: boolean;
}

function MetricCard({ label, value, hint }: { label: string; value: string | number; hint: string }) {
  return (
    <div className="panel p-5">
      <div className="label">{label}</div>
      <div className="mt-3 text-3xl font-semibold tracking-tight text-slate-900">{value}</div>
      <div className="mt-2 text-sm text-slate-600">{hint}</div>
    </div>
  );
}

export function Dashboard({ report, liveFindings, running }: DashboardProps) {
  const metrics = report?.metrics;
  const activeFindings = running ? liveFindings : report?.findings ?? [];
  const recent = [...activeFindings]
    .sort((a, b) => b.matches.length - a.matches.length)
    .slice(0, 6);

  return (
    <div className="space-y-6">
      <div className="flex items-end justify-between">
        <div>
          <div className="label">Overview</div>
          <h1 className="mt-2 text-3xl font-semibold text-slate-900">Credential exposure dashboard</h1>
          <p className="mt-2 max-w-3xl text-sm leading-6 text-slate-600">
            Scan public repositories for leaked API credentials and endpoint patterns, then measure
            whether discovered public HTTP endpoints respond to a safe HEAD request.
          </p>
        </div>
        <div className={`rounded-full border px-3 py-1.5 text-xs font-semibold ${running ? "border-sky-500/40 bg-sky-500/10 text-sky-700" : "border-slate-300 bg-slate-100 text-slate-600"}`}>
          {running ? "SCAN ACTIVE" : "IDLE"}
        </div>
      </div>

      <div className="grid grid-cols-4 gap-4">
        <MetricCard
          label="Projects scanned"
          value={running ? liveFindings.length : metrics?.repositories_scanned ?? 0}
          hint="Public repositories inspected"
        />
        <MetricCard
          label="Pattern matches"
          value={running ? liveFindings.reduce((n, f) => n + f.matches.length, 0) : metrics?.total_matches ?? 0}
          hint="Matched credential and endpoint patterns"
        />
        <MetricCard
          label="Unique endpoints"
          value={metrics?.unique_absolute_endpoints ?? 0}
          hint="Absolute HTTP(S) targets"
        />
        <MetricCard
          label="Responsive"
          value={metrics?.reachable_endpoints ?? 0}
          hint="Returned an HTTP response"
        />
      </div>

      <div className="grid grid-cols-[1.5fr_1fr] gap-5">
        <div className="panel overflow-hidden">
          <div className="flex items-center justify-between border-b border-slate-200 px-5 py-4">
            <div>
              <div className="font-semibold text-slate-900">Highest-signal projects</div>
              <div className="mt-1 text-xs text-slate-600">Sorted by number of matched patterns</div>
            </div>
          </div>
          <div className="divide-y divide-slate-200/80">
            {recent.length === 0 ? (
              <div className="px-5 py-12 text-center text-sm text-slate-600">
                Run a scan to populate project findings.
              </div>
            ) : (
              recent.map((finding) => (
                <div key={finding.repository.full_name} className="flex items-center justify-between px-5 py-4">
                  <div className="min-w-0">
                    <div className="truncate font-medium text-slate-800">{finding.repository.full_name}</div>
                    <div className="mt-1 text-xs text-slate-600">
                      {finding.repository.language ?? "Unknown"} · {finding.scanned_files} files · {finding.repository.stars} stars
                    </div>
                  </div>
                  <div className="ml-4 rounded-lg border border-slate-300 bg-slate-50 px-3 py-1.5 text-sm font-semibold text-sky-700">
                    {finding.matches.length} matches
                  </div>
                </div>
              ))
            )}
          </div>
        </div>

        <div className="panel p-5">
          <div className="font-semibold text-slate-900">Safety boundaries</div>
          <div className="mt-4 space-y-4 text-sm leading-6 text-slate-600">
            <p>Repository discovery is limited to public GitHub projects and bounded file sampling.</p>
            <p>Health checks only accept HTTP(S), disable redirects, and reject localhost, private, link-local, and reserved network targets.</p>
            <p>GitHub tokens are never written into scan reports or the saved application settings file.</p>
          </div>
        </div>
      </div>
    </div>
  );
}
