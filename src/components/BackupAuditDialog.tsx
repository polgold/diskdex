import { useEffect, useState } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { confirm } from "@tauri-apps/plugin-dialog";
import { ShieldCheck, Loader2, ArrowRight, AlertTriangle, HelpCircle, CheckCircle2, Copy, XCircle, Folder, FolderOpen, ChevronRight, HardDrive } from "lucide-react";
import { api, type BackupReport, type FileRef, type CopyResult, type CopyProgress, type EntryRow } from "../lib/ipc";
import { useCatalog } from "../store/catalog";
import { formatBytes, formatCount } from "../lib/format";
import { useT } from "../lib/i18n";
import { Modal } from "./StatsDialog";

/**
 * B1 — Auditoría de backup: elegís un disco+carpeta de origen y un disco+carpeta de
 * destino, y DiskDex te dice qué archivos del subárbol origen faltan / difieren / no
 * se verifican en el destino. Comparación CARPETA contra CARPETA: navegá a una
 * subcarpeta de cada lado para acotar, o dejá la raíz para el disco entero.
 * OFFLINE: compara catálogo-vs-catálogo, no necesita montar nada.
 */
export function BackupAuditDialog({ onClose }: { onClose: () => void }) {
  const t = useT();
  const disks = useCatalog((s) => s.disks);
  const selectedDiskId = useCatalog((s) => s.selectedDiskId);

  const [sourceId, setSourceId] = useState<number | null>(selectedDiskId ?? null);
  const [destId, setDestId] = useState<number | null>(null);
  // Carpeta seleccionada de cada lado (entry_id). null = raíz del disco (disco entero).
  const [sourceEntryId, setSourceEntryId] = useState<number | null>(null);
  const [destEntryId, setDestEntryId] = useState<number | null>(null);
  // Ruta legible de cada lado (breadcrumb), para mostrarla en el reporte.
  const [sourceLabel, setSourceLabel] = useState("");
  const [destLabel, setDestLabel] = useState("");
  // Alcance congelado en el momento de comparar (para que el header del reporte
  // no cambie si después navegás a otra carpeta sin volver a comparar).
  const [scope, setScope] = useState<{ source: string; dest: string } | null>(null);
  const [report, setReport] = useState<BackupReport | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [copying, setCopying] = useState(false);
  const [copyProg, setCopyProg] = useState<CopyProgress | null>(null);
  const [copyResult, setCopyResult] = useState<CopyResult | null>(null);

  // Comparable si ambos lados están elegidos y NO son el mismo subárbol exacto.
  const canCompare =
    sourceId != null && destId != null && !(sourceId === destId && sourceEntryId === destEntryId);

  async function run() {
    if (!canCompare) return;
    setLoading(true);
    setError(null);
    setReport(null);
    setCopyResult(null);
    setScope({ source: sourceLabel, dest: destLabel });
    try {
      const r = await api.compareBackup({
        source_disk_id: sourceId!,
        source_entry_id: sourceEntryId,
        dest_disk_id: destId!,
        dest_entry_id: destEntryId,
      });
      setReport(r);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }

  async function copyMissing() {
    if (!report || !canCompare) return;
    const ok = await confirm(t("backup.confirmCopy", { n: formatCount(report.missing.length), bytes: formatBytes(report.missing_bytes) }));
    if (!ok) return;
    setCopying(true);
    setError(null);
    setCopyResult(null);
    setCopyProg(null);
    let unlisten: UnlistenFn | null = null;
    try {
      unlisten = await listen<CopyProgress>("copy-progress", (e) => setCopyProg(e.payload));
      const r = await api.copyMissing({
        source_disk_id: sourceId!,
        source_entry_id: sourceEntryId,
        dest_disk_id: destId!,
        dest_entry_id: destEntryId,
        dry_run: false,
      });
      setCopyResult(r);
      // Refrescar el reporte tras copiar (lo que falte ahora será menos).
      const fresh = await api.compareBackup({
        source_disk_id: sourceId!,
        source_entry_id: sourceEntryId,
        dest_disk_id: destId!,
        dest_entry_id: destEntryId,
      });
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
      {/* Selección de disco + carpeta a cada lado (origen → destino) */}
      <div className="grid grid-cols-[1fr_auto_1fr] items-start gap-3 text-xs">
        <SideSelector
          label={t("backup.source")}
          disks={disks}
          diskId={sourceId}
          onDiskChange={(id) => { setSourceId(id); setSourceEntryId(null); }}
          onEntryChange={(id, label) => { setSourceEntryId(id); setSourceLabel(label); }}
          exclude={null}
          t={t}
        />
        <ArrowRight className="mt-7 h-4 w-4 shrink-0 text-neutral-500" />
        <SideSelector
          label={t("backup.dest")}
          disks={disks}
          diskId={destId}
          onDiskChange={(id) => { setDestId(id); setDestEntryId(null); }}
          onEntryChange={(id, label) => { setDestEntryId(id); setDestLabel(label); }}
          exclude={null}
          t={t}
        />
      </div>

      <div className="mt-3 flex items-center justify-between gap-2">
        <p className="text-[11px] text-neutral-500">{t("backup.help")}</p>
        <button
          onClick={run}
          disabled={!canCompare || loading}
          className="inline-flex shrink-0 items-center gap-1 rounded border border-sky-800/60 bg-sky-950/40 px-3 py-1.5 text-xs text-sky-200 hover:bg-sky-900/50 disabled:opacity-40"
        >
          {loading ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <ShieldCheck className="h-3.5 w-3.5" />}
          {t("backup.compare")}
        </button>
      </div>

      {error && <div className="mt-3 rounded border border-red-900/60 bg-red-950/40 px-3 py-2 text-xs text-red-300">{error}</div>}

      {report && <ReportView report={report} scope={scope} t={t} />}

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

/** Un lado de la comparación: selector de disco + navegador de carpetas para acotar. */
function SideSelector({
  label,
  disks,
  diskId,
  onDiskChange,
  onEntryChange,
  exclude,
  t,
}: {
  label: string;
  disks: { id: number; name: string }[];
  diskId: number | null;
  onDiskChange: (id: number | null) => void;
  onEntryChange: (id: number | null, label: string) => void;
  exclude: number | null;
  t: (k: string, v?: Record<string, string | number>) => string;
}) {
  const diskName = disks.find((d) => d.id === diskId)?.name ?? t("backup.wholeDisk");
  return (
    <div className="flex min-w-0 flex-col gap-1">
      <span className="text-[11px] text-neutral-500">{label}</span>
      <select
        value={diskId ?? ""}
        onChange={(e) => onDiskChange(e.target.value ? Number(e.target.value) : null)}
        className="rounded border border-neutral-700 bg-neutral-900 px-2 py-1.5 text-xs text-neutral-200"
      >
        <option value="">{t("backup.pickDisk")}</option>
        {disks.map((d) => (
          <option key={d.id} value={d.id} disabled={d.id === exclude}>
            {d.name}
          </option>
        ))}
      </select>
      <FolderNav diskId={diskId} diskName={diskName} onSelect={onEntryChange} t={t} />
    </div>
  );
}

/**
 * Navegador de carpetas del catálogo (offline, vía `list_children`). La carpeta
 * en la que estás parado ES el subárbol elegido para comparar: breadcrumb arriba
 * (raíz = disco entero), lista de subcarpetas abajo para entrar más adentro.
 */
function FolderNav({
  diskId,
  diskName,
  onSelect,
  t,
}: {
  diskId: number | null;
  diskName: string;
  onSelect: (id: number | null, label: string) => void;
  t: (k: string, v?: Record<string, string | number>) => string;
}) {
  type Crumb = { id: number | null; name: string };
  const [path, setPath] = useState<Crumb[]>([{ id: null, name: diskName }]);
  const [children, setChildren] = useState<EntryRow[]>([]);
  const [loading, setLoading] = useState(false);

  const current = path[path.length - 1];

  // Resetear al cambiar de disco (o al limpiar la selección).
  useEffect(() => {
    setPath([{ id: null, name: diskName }]);
  }, [diskId, diskName]);

  // Cargar subcarpetas de la carpeta actual.
  useEffect(() => {
    if (diskId == null) {
      setChildren([]);
      return;
    }
    let alive = true;
    setLoading(true);
    api
      .listChildren(diskId, current.id)
      .then((rows) => {
        if (alive) setChildren(rows.filter((r) => r.is_folder));
      })
      .catch(() => {
        if (alive) setChildren([]);
      })
      .finally(() => {
        if (alive) setLoading(false);
      });
    return () => {
      alive = false;
    };
  }, [diskId, current.id]);

  // Reportar la carpeta seleccionada (= carpeta actual) + su ruta legible hacia arriba.
  useEffect(() => {
    onSelect(current.id, path.map((c) => c.name).join(" / "));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [current.id]);

  if (diskId == null) {
    return <p className="mt-1 rounded border border-neutral-800 bg-neutral-900/40 px-2 py-2 text-[11px] text-neutral-600">{t("backup.pickDiskFirst")}</p>;
  }

  return (
    <div className="mt-1 rounded border border-neutral-800 bg-neutral-900/40">
      {/* Breadcrumb: la ruta actual; click en un tramo vuelve hasta ahí. */}
      <div className="flex flex-wrap items-center gap-0.5 border-b border-neutral-800/80 px-2 py-1.5 text-[11px]">
        {path.map((c, i) => (
          <span key={`${c.id ?? "root"}-${i}`} className="flex items-center">
            {i > 0 && <ChevronRight className="h-3 w-3 text-neutral-600" />}
            <button
              onClick={() => setPath((p) => p.slice(0, i + 1))}
              className={`inline-flex items-center gap-1 rounded px-1 py-0.5 hover:bg-neutral-800 ${i === path.length - 1 ? "text-sky-300" : "text-neutral-400"}`}
              title={c.name}
            >
              {i === 0 ? <HardDrive className="h-3 w-3" /> : <FolderOpen className="h-3 w-3" />}
              <span className="max-w-[120px] truncate">{c.name}</span>
            </button>
          </span>
        ))}
      </div>
      {/* Subcarpetas: click entra (y la convierte en el subárbol elegido). */}
      <div className="max-h-40 overflow-auto">
        {loading ? (
          <div className="flex items-center gap-1.5 px-2 py-2 text-[11px] text-neutral-500">
            <Loader2 className="h-3 w-3 animate-spin" /> …
          </div>
        ) : children.length === 0 ? (
          <div className="px-2 py-2 text-[11px] text-neutral-600">{t("backup.noSubfolders")}</div>
        ) : (
          children.map((f) => (
            <button
              key={f.id}
              onClick={() => setPath((p) => [...p, { id: f.id, name: f.name }])}
              className="flex w-full items-center gap-1.5 border-b border-neutral-800/50 px-2 py-1 text-left text-[11px] text-neutral-300 last:border-0 hover:bg-neutral-800/60"
            >
              <Folder className="h-3 w-3 shrink-0 text-neutral-500" />
              <span className="truncate" title={f.name}>{f.name}</span>
              {f.child_count > 0 && <ChevronRight className="ml-auto h-3 w-3 shrink-0 text-neutral-600" />}
            </button>
          ))
        )}
      </div>
      <p className="border-t border-neutral-800/80 px-2 py-1 text-[10px] text-neutral-600">{t("backup.scopeHint")}</p>
    </div>
  );
}

function ReportView({
  report,
  scope,
  t,
}: {
  report: BackupReport;
  scope: { source: string; dest: string } | null;
  t: (k: string, v?: Record<string, string | number>) => string;
}) {
  const verified = report.fully_backed_up && report.unverified.length === 0;
  return (
    <div className="mt-4 space-y-4">
      {/* Alcance comparado: qué subárbol de origen contra qué subárbol de destino */}
      {scope && (
        <div className="flex flex-wrap items-center gap-1.5 rounded-lg border border-neutral-800 bg-neutral-900/40 px-3 py-2 text-[11px]">
          <FolderOpen className="h-3.5 w-3.5 shrink-0 text-neutral-500" />
          <span className="truncate font-medium text-neutral-300" title={scope.source}>{scope.source}</span>
          <ArrowRight className="h-3 w-3 shrink-0 text-neutral-600" />
          <span className="truncate font-medium text-neutral-300" title={scope.dest}>{scope.dest}</span>
        </div>
      )}

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
