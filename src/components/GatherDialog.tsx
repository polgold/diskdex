import { useEffect, useState } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { FolderInput, Loader2, HardDrive, Check, FolderOpen, RefreshCw, AlertTriangle } from "lucide-react";
import { api, type GatherPlan, type GatherGroup, type CopyResult, type GatherProgress } from "../lib/ipc";
import { useCatalog } from "../store/catalog";
import { formatBytes, formatCount } from "../lib/format";
import { useT } from "../lib/i18n";
import { Modal } from "./StatsDialog";

/**
 * D — Reunir: junta archivos repartidos en varios discos (varios desconectados) en
 * una carpeta destino, guiando disco por disco. Reusa la copia verificada por hash.
 */
export function GatherDialog({ entryIds, onClose }: { entryIds: number[]; onClose: () => void }) {
  const t = useT();
  const refreshOnlineFromDisk = useCatalog((s) => s.refreshOnlineFromDisk);
  const [plan, setPlan] = useState<GatherPlan | null>(null);
  const [destDir, setDestDir] = useState<string>("");
  const [copying, setCopying] = useState<number | null>(null);
  const [prog, setProg] = useState<GatherProgress | null>(null);
  const [results, setResults] = useState<Record<number, CopyResult>>({});
  const [error, setError] = useState<string | null>(null);

  async function loadPlan() {
    try {
      setPlan(await api.gatherPlan(entryIds));
    } catch (e) {
      setError(String(e));
    }
  }

  useEffect(() => {
    loadPlan();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  async function refresh() {
    await refreshOnlineFromDisk();
    await loadPlan();
  }

  async function pickDest(): Promise<string | null> {
    const sel = await openDialog({ directory: true, title: t("gather.pickDest") });
    if (typeof sel === "string") {
      setDestDir(sel);
      return sel;
    }
    return null;
  }

  async function copyGroup(g: GatherGroup) {
    let dest = destDir;
    if (!dest) {
      const picked = await pickDest();
      if (!picked) return;
      dest = picked;
    }
    setCopying(g.disk_id);
    setError(null);
    setProg(null);
    let unlisten: UnlistenFn | null = null;
    try {
      unlisten = await listen<GatherProgress>("gather-progress", (e) => setProg(e.payload));
      const r = await api.gatherCopy(g.files.map((f) => f.entry_id), dest);
      setResults((prev) => ({ ...prev, [g.disk_id]: r }));
    } catch (e) {
      setError(String(e));
    } finally {
      if (unlisten) unlisten();
      setCopying(null);
      setProg(null);
    }
  }

  return (
    <Modal onClose={onClose} title={t("gather.title")} icon={<FolderInput className="h-4 w-4 text-sky-400" />}>
      {!plan ? (
        <div className="flex items-center gap-2 py-8 text-sm text-neutral-500">
          <Loader2 className="h-4 w-4 animate-spin" /> {t("common.loading")}
        </div>
      ) : (
        <div className="space-y-3">
          <div className="flex items-center justify-between gap-2 text-xs">
            <span className="text-neutral-400">
              {t("gather.summary", {
                files: formatCount(plan.total_files),
                bytes: formatBytes(plan.total_bytes),
                disks: plan.groups.length,
              })}
              {plan.skipped_folders > 0 && (
                <span className="text-neutral-600"> · {t("gather.skippedFolders", { n: plan.skipped_folders })}</span>
              )}
            </span>
            <button onClick={refresh} className="inline-flex items-center gap-1 rounded border border-neutral-700 px-2 py-1 hover:bg-neutral-800">
              <RefreshCw className="h-3 w-3" /> {t("gather.refresh")}
            </button>
          </div>

          {/* Destino */}
          <div className="flex items-center gap-2 rounded border border-neutral-800 bg-neutral-900/40 px-2.5 py-2 text-xs">
            <FolderOpen className="h-3.5 w-3.5 shrink-0 text-neutral-500" />
            <span className="flex-1 truncate font-mono text-[11px] text-neutral-300">{destDir || t("gather.noDest")}</span>
            <button onClick={pickDest} className="rounded border border-neutral-700 px-2 py-0.5 hover:bg-neutral-800">
              {t("gather.chooseDest")}
            </button>
          </div>

          {/* Grupos por disco */}
          <div className="space-y-2">
            {plan.groups.map((g) => {
              const done = results[g.disk_id];
              const busy = copying === g.disk_id;
              return (
                <div key={g.disk_id} className="rounded-lg border border-neutral-800 p-2.5">
                  <div className="flex items-center gap-2">
                    <HardDrive className={`h-4 w-4 shrink-0 ${g.is_online ? "text-emerald-400" : "text-neutral-600"}`} />
                    <span className="font-medium text-neutral-200">{g.disk_name}</span>
                    <span className="text-[11px] text-neutral-500">{t("gather.groupCount", { n: formatCount(g.total), bytes: formatBytes(g.total_bytes) })}</span>
                    <span className="ml-auto">
                      {done ? (
                        <span className="inline-flex items-center gap-1 text-[11px] text-emerald-400">
                          <Check className="h-3.5 w-3.5" />
                          {t("gather.copiedN", { n: formatCount(done.copied), skipped: formatCount(done.skipped) })}
                        </span>
                      ) : busy ? (
                        <span className="inline-flex items-center gap-1.5 text-[11px] text-sky-300">
                          <Loader2 className="h-3.5 w-3.5 animate-spin" />
                          {t("gather.copying", { count: formatCount(prog?.count ?? 0), total: formatCount(prog?.total ?? g.total) })}
                          <button onClick={() => api.cancelGather(destDir)} className="rounded border border-neutral-700 px-1.5 py-0.5 text-neutral-300 hover:bg-neutral-800">
                            {t("common.cancel")}
                          </button>
                        </span>
                      ) : g.is_online ? (
                        <button
                          onClick={() => copyGroup(g)}
                          disabled={copying != null}
                          className="rounded border border-emerald-800/60 bg-emerald-950/40 px-2 py-0.5 text-[11px] text-emerald-200 hover:bg-emerald-900/50 disabled:opacity-40"
                        >
                          {t("gather.copyGroup")}
                        </button>
                      ) : (
                        <span className="inline-flex items-center gap-1 text-[11px] text-amber-400">
                          <AlertTriangle className="h-3.5 w-3.5" /> {t("gather.connect", { name: g.disk_name })}
                        </span>
                      )}
                    </span>
                  </div>
                  {done && done.failed.length > 0 && (
                    <div className="mt-1.5 text-[11px] text-red-400">{t("gather.failedN", { n: done.failed.length })}</div>
                  )}
                </div>
              );
            })}
          </div>

          {error && <div className="rounded border border-red-900/60 bg-red-950/40 px-3 py-2 text-xs text-red-300">{error}</div>}
          <p className="text-[11px] text-neutral-500">{t("gather.help")}</p>
        </div>
      )}
    </Modal>
  );
}
