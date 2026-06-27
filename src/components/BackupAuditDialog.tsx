import { useState } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { ShieldCheck, Loader2, ArrowRight, AlertTriangle, HelpCircle, CheckCircle2, Copy, XCircle } from "lucide-react";
import { api, type BackupReport, type FileRef, type CopyResult, type CopyProgress } from "../lib/ipc";
import { useCatalog } from "../store/catalog";
import { formatBytes, formatCount } from "../lib/format";
import { useT } from "../lib/i18n";
import { Modal } from "./StatsDialog";

/**
 * B1 — Auditoría de backup: elegís source y destination (discos del catálogo) y
 * DiskDex te dice qué archivos del source faltan / difieren / no se verifican en el
 * destino. OFFLINE: compara catálogo-vs-catálogo, no necesita montar nada.
 * (Comparación a nivel disco entero en esta versión; carpetas → follow-up.)
 */
export function BackupAuditDialog({ onClose }: { onClose: () => void }) {
  const t = useT();
  const disks = useCatalog((s) => s.disks);
  const selectedDiskId = useCatalog((s) => s.selectedDiskId);

  const [sourceId, setSourceId] = useState<number | null>(selectedDiskId ?? null);
  const [destId, setDestId] = useState<number | null>(null);
  const [report, setReport] = useState<BackupReport | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [copying, setCopying] = useState(false);
  const [copyProg, setCopyProg] = useState<CopyProgress | null>(null);
  const [copyResult, setCopyResult] = useState<CopyResult | null>(null);

  const canCompare = sourceId != null && destId != null && sourceId !== destId;

  async function run() {
    if (!canCompare) return;
    setLoading(true);
    setError(null);
    setReport(null);
    setCopyResult(null);
    try {
      const r = await api.compareBackup({ source_disk_id: sourceId!, dest_disk_id: destId! });
      setReport(r);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }

  async function copyMissing() {
    if (!report || !canCompare) return;
    const ok = window.confirm(t("backup.confirmCopy", { n: formatCount(report.missing.length), bytes: formatBytes(report.missing_bytes) }));
    if (!ok) return;
    setCopying(true);
    setError(null);
    setCopyResult(null);
    setCopyProg(null);
    let unlisten: UnlistenFn | null = null;
    try {
      unlisten = await listen<CopyProgress>("copy-progress", (e) => setCopyProg(e.payload));
      const r = await api.copyMissing({ source_disk_id: sourceId!, dest_disk_id: destId!, dry_run: false });
      setCopyResult(r);
      // Refrescar el reporte tras copiar (lo que falte ahora será menos).
      const fresh = await api.compareBackup({ source_disk_id: sourceId!, dest_disk_id: destId! });
      setReport(fresh);
    } catch (e) {
      setError(String(e));
    } finally {
      if (unlisten) unlisten();
      setCopying(false);
      setCopyProg(null);
    }
  }

  return (
    <Modal onClose={onClose} title={t("backup.title")} icon={<ShieldCheck className="h-4 w-4 text-sky-400" />}>
      {/* Selección source → dest */}
      <div className="flex items-end gap-2 text-xs">
        <DiskPicker label={t("backup.source")} disks={disks} value={sourceId} onChange={setSourceId} exclude={destId} t={t} />
        <ArrowRight className="mb-2 h-4 w-4 shrink-0 text-neutral-500" />
        <DiskPicker label={t("backup.dest")} disks={disks} value={destId} onChange={setDestId} exclude={sourceId} t={t} />
        <button
          onClick={run}
          disabled={!canCompare || loading}
          className="mb-0.5 inline-flex items-center gap-1 rounded border border-sky-800/60 bg-sky-950/40 px-3 py-1.5 text-sky-200 hover:bg-sky-900/50 disabled:opacity-40"
        >
          {loading ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <ShieldCheck className="h-3.5 w-3.5" />}
          {t("backup.compare")}
        </button>
      </div>

      <p className="mt-2 text-[11px] text-neutral-500">{t("backup.help")}</p>

      {error && <div className="mt-3 rounded border border-red-900/60 bg-red-950/40 px-3 py-2 text-xs text-red-300">{error}</div>}

      {report && <ReportView report={report} t={t} />}

      {/* B2 — copiar lo que falta (requiere ambos discos montados) */}
      {report && report.missing.length > 0 && (
        <div className="mt-4 border-t border-border pt-3">
          {!copying ? (
            <button
              onClick={copyMissing}
              className="inline-flex items-center gap-1.5 rounded border border-emerald-800/60 bg-emerald-950/40 px-3 py-1.5 text-xs text-emerald-200 hover:bg-emerald-900/50"
            >
              <Copy className="h-3.5 w-3.5" />
              {t("backup.copyMissing", { n: formatCount(report.missing.length), bytes: formatBytes(report.missing_bytes) })}
            </button>
          ) : (
            <div className="flex items-center gap-3 text-xs text-sky-300">
              <span className="flex items-center gap-1.5">
                <Loader2 className="h-3.5 w-3.5 animate-spin" />
                {t("backup.copying", {
                  count: formatCount(copyProg?.count ?? 0),
                  total: formatCount(copyProg?.total ?? report.missing.length),
                })}
              </span>
              <button
                onClick={() => destId != null && api.cancelCopy(destId)}
                className="rounded border border-neutral-700 px-2 py-0.5 text-neutral-300 hover:bg-neutral-800"
              >
                {t("common.cancel")}
              </button>
            </div>
          )}
          <p className="mt-1.5 text-[11px] text-neutral-500">{t("backup.copyHelp")}</p>
        </div>
      )}

      {copyResult && <CopyResultView r={copyResult} t={t} />}
    </Modal>
  );
}

function CopyResultView({ r, t }: { r: CopyResult; t: (k: string, v?: Record<string, string | number>) => string }) {
  return (
    <div className="mt-3 rounded-lg border border-sky-900/50 bg-sky-950/20 px-3 py-2.5 text-xs">
      <div className="flex items-center gap-2 font-medium text-sky-200">
        {r.cancelled ? <XCircle className="h-4 w-4 text-amber-400" /> : <CheckCircle2 className="h-4 w-4 text-emerald-400" />}
        {r.cancelled ? t("backup.copyCancelled") : t("backup.copyDone")}
      </div>
      <div className="mt-1.5 flex flex-wrap gap-4 text-neutral-300">
        <span>{t("backup.copyCopied", { n: formatCount(r.copied), bytes: formatBytes(r.copied_bytes) })}</span>
        <span className="text-emerald-400">{t("backup.copyVerified", { n: formatCount(r.verified) })}</span>
        {r.skipped > 0 && <span className="text-neutral-500">{t("backup.copySkipped", { n: formatCount(r.skipped) })}</span>}
        {r.failed.length > 0 && <span className="text-red-400">{t("backup.copyFailed", { n: formatCount(r.failed.length) })}</span>}
      </div>
      {r.failed.length > 0 && (
        <div className="mt-2 max-h-32 overflow-auto rounded border border-red-900/40">
          {r.failed.slice(0, 50).map((f) => (
            <div key={f.rel_path} className="border-b border-red-900/30 px-2 py-1 text-[11px] last:border-0">
              <span className="text-red-300">{f.rel_path}</span>
              <span className="text-neutral-500"> — {f.error}</span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

function DiskPicker({
  label,
  disks,
  value,
  onChange,
  exclude,
  t,
}: {
  label: string;
  disks: { id: number; name: string }[];
  value: number | null;
  onChange: (id: number | null) => void;
  exclude: number | null;
  t: (k: string) => string;
}) {
  return (
    <label className="flex flex-1 flex-col gap-1">
      <span className="text-[11px] text-neutral-500">{label}</span>
      <select
        value={value ?? ""}
        onChange={(e) => onChange(e.target.value ? Number(e.target.value) : null)}
        className="rounded border border-neutral-700 bg-neutral-900 px-2 py-1.5 text-xs text-neutral-200"
      >
        <option value="">{t("backup.pickDisk")}</option>
        {disks.map((d) => (
          <option key={d.id} value={d.id} disabled={d.id === exclude}>
            {d.name}
          </option>
        ))}
      </select>
    </label>
  );
}

function ReportView({ report, t }: { report: BackupReport; t: (k: string, v?: Record<string, string | number>) => string }) {
  const verified = report.fully_backed_up && report.unverified.length === 0;
  return (
    <div className="mt-4 space-y-4">
      {/* Veredicto */}
      {report.fully_backed_up ? (
        <div className="flex items-center gap-2 rounded-lg border border-emerald-800/50 bg-emerald-950/30 px-3 py-2.5 text-sm text-emerald-300">
          <CheckCircle2 className="h-5 w-5 shrink-0" />
          <span>
            {verified
              ? t("backup.allBackedUp", { n: formatCount(report.ok) })
              : t("backup.allPresentUnverified", { n: formatCount(report.unverified.length) })}
          </span>
        </div>
      ) : (
        <div className="flex items-center gap-2 rounded-lg border border-amber-800/50 bg-amber-950/30 px-3 py-2.5 text-sm text-amber-300">
          <AlertTriangle className="h-5 w-5 shrink-0" />
          <span>{t("backup.missingSummary", { n: formatCount(report.missing.length), bytes: formatBytes(report.missing_bytes) })}</span>
        </div>
      )}

      {/* Contadores */}
      <div className="flex flex-wrap gap-4 text-xs">
        <Counter n={report.ok} label={t("backup.ok")} cls="text-emerald-400" />
        <Counter n={report.missing.length} label={t("backup.missing")} cls="text-amber-400" />
        <Counter n={report.mismatch.length} label={t("backup.mismatch")} cls="text-red-400" />
        <Counter n={report.unverified.length} label={t("backup.unverified")} cls="text-neutral-400" />
        <Counter n={report.extra} label={t("backup.extra")} cls="text-neutral-500" />
      </div>

      {report.mismatch.length > 0 && (
        <FileList title={t("backup.mismatchTitle")} icon={<AlertTriangle className="h-3.5 w-3.5 text-red-400" />} files={report.mismatch} t={t} />
      )}
      {report.missing.length > 0 && (
        <FileList title={t("backup.missingTitle")} icon={<ArrowRight className="h-3.5 w-3.5 text-amber-400" />} files={report.missing} t={t} />
      )}
      {report.unverified.length > 0 && (
        <FileList title={t("backup.unverifiedTitle")} icon={<HelpCircle className="h-3.5 w-3.5 text-neutral-400" />} files={report.unverified} t={t} />
      )}
    </div>
  );
}

function Counter({ n, label, cls }: { n: number; label: string; cls: string }) {
  return (
    <div className="flex flex-col">
      <span className={`text-lg font-semibold ${cls}`}>{formatCount(n)}</span>
      <span className="text-[11px] text-neutral-500">{label}</span>
    </div>
  );
}

const MAX_SHOWN = 200;

function FileList({
  title,
  icon,
  files,
  t,
}: {
  title: string;
  icon: React.ReactNode;
  files: FileRef[];
  t: (k: string, v?: Record<string, string | number>) => string;
}) {
  const shown = files.slice(0, MAX_SHOWN);
  return (
    <div>
      <h3 className="mb-1 flex items-center gap-1.5 text-xs font-semibold text-neutral-300">
        {icon} {title} <span className="text-neutral-500">({formatCount(files.length)})</span>
      </h3>
      <div className="max-h-48 overflow-auto rounded border border-neutral-800">
        {shown.map((f) => (
          <div key={f.entry_id} className="flex items-center justify-between gap-3 border-b border-neutral-800/60 px-2 py-1 text-[11px] last:border-0">
            <span className="truncate text-neutral-300" title={f.rel_path}>
              {f.rel_path}
            </span>
            <span className="shrink-0 text-neutral-500">{formatBytes(f.size)}</span>
          </div>
        ))}
        {files.length > MAX_SHOWN && (
          <div className="px-2 py-1 text-[11px] text-neutral-500">{t("backup.andMore", { n: formatCount(files.length - MAX_SHOWN) })}</div>
        )}
      </div>
    </div>
  );
}
