import { useEffect, useState, type ReactNode } from "react";
import { HardDrive, Usb, Loader2, X, RefreshCw, FolderPlus, Network, Image as ImageIcon, Film, Package, Ban } from "lucide-react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { api, type VolumeInfo, type ScanProgress, type DiskRow } from "../lib/ipc";
import { formatBytes, formatCount } from "../lib/format";
import { useT } from "../lib/i18n";
import { useCatalog } from "../store/catalog";

type ScanState = "new" | "upToDate" | "changed";

/** Estado de un volumen montado vs. el catálogo, por tamaño usado (lectura rápida):
 *  no escaneado (gris) / al día (verde) / cambió el tamaño (amarillo). */
function volumeScanState(v: VolumeInfo, disks: DiskRow[]): ScanState {
  const disk = disks.find((d) => d.name === v.name);
  if (!disk) return "new";
  const used = v.total_space - v.available_space;
  const tol = Math.max(5 * 1024 ** 3, v.total_space * 0.15);
  return Math.abs(used - disk.total_size) <= tol ? "upToDate" : "changed";
}

export interface PostScanOptions {
  thumbnails: boolean;
  videos: boolean;
  archives: boolean;
  /** Saltar basura (node_modules, caches, papeleras…) durante el recorrido. OPT-IN. */
  excludeJunk: boolean;
}

interface Props {
  onClose: () => void;
  onScan: (mountPath: string, name: string) => void;
  scanning: Record<string, ScanProgress>;
  options: PostScanOptions;
  setOptions: (o: PostScanOptions) => void;
}

export function ScanDialog({ onClose, onScan, scanning, options, setOptions }: Props) {
  const t = useT();
  const disks = useCatalog((s) => s.disks);
  const [volumes, setVolumes] = useState<VolumeInfo[]>([]);
  const [loading, setLoading] = useState(true);

  async function refresh() {
    setLoading(true);
    try {
      setVolumes(await api.listVolumes());
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    refresh();
  }, []);

  async function pickFolder() {
    // Abrir el selector nativo directamente en /Volumes: ahí están todos los
    // discos y shares de red YA montados (Finder → Cmd+K). Se elige la unidad
    // de otra compu desde ahí mismo, sin pasos extra.
    const dir = await openDialog({
      title: t("scandlg.pickFolderTitle"),
      directory: true,
      multiple: false,
      defaultPath: "/Volumes",
    });
    if (dir && typeof dir === "string") {
      const name = dir.split("/").filter(Boolean).pop() ?? dir;
      onScan(dir, name);
    }
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/70 p-4 backdrop-blur-sm animate-fade-in">
      <div className="w-full max-w-lg rounded-xl border border-border bg-card shadow-pop animate-zoom-in">
        <div className="flex items-center justify-between border-b border-border px-4 py-3">
          <h2 className="text-sm font-semibold">{t("scandlg.title")}</h2>
          <div className="flex items-center gap-1">
            <button
              onClick={refresh}
              className="rounded p-1.5 text-neutral-400 hover:bg-neutral-800 hover:text-neutral-200"
              title={t("scandlg.refresh")}
            >
              <RefreshCw className={`h-4 w-4 ${loading ? "animate-spin" : ""}`} />
            </button>
            <button
              onClick={onClose}
              className="rounded p-1.5 text-neutral-400 hover:bg-neutral-800 hover:text-neutral-200"
            >
              <X className="h-4 w-4" />
            </button>
          </div>
        </div>

        <div className="max-h-[60vh] overflow-auto p-2">
          {volumes.length === 0 && !loading && (
            <p className="px-3 py-6 text-center text-sm text-neutral-500">
              {t("scandlg.noVolumes")}
            </p>
          )}
          {volumes.map((v) => {
            const used = v.total_space - v.available_space;
            const pct = v.total_space > 0 ? Math.min(100, (used / v.total_space) * 100) : 0;
            const prog = scanning[v.mount_path];
            const busy = !!prog;
            const st = volumeScanState(v, disks);
            const stStyle =
              st === "upToDate"
                ? { dot: "bg-emerald-500", text: "text-emerald-400", bar: "bg-emerald-500/70" }
                : st === "changed"
                ? { dot: "bg-amber-500", text: "text-amber-400", bar: "bg-amber-500/70" }
                : { dot: "bg-neutral-600", text: "text-neutral-500", bar: "bg-neutral-600" };
            const stLabel =
              st === "upToDate"
                ? t("scandlg.stUpToDate")
                : st === "changed"
                ? t("scandlg.stChanged")
                : t("scandlg.stNew");
            return (
              <div
                key={v.mount_path}
                className="flex items-center gap-3 rounded-lg px-3 py-2.5 hover:bg-neutral-800/60"
              >
                {v.is_removable ? (
                  <Usb className="h-5 w-5 shrink-0 text-amber-400" />
                ) : (
                  <HardDrive className="h-5 w-5 shrink-0 text-neutral-400" />
                )}
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-2">
                    <span className="truncate text-sm font-medium" title={v.name}>
                      {v.name}
                    </span>
                    <span className="rounded bg-neutral-800 px-1.5 py-0.5 text-[10px] uppercase text-neutral-400">
                      {v.kind}
                    </span>
                    {!busy && (
                      <span className={`flex items-center gap-1 text-[10px] ${stStyle.text}`} title={stLabel}>
                        <span className={`h-1.5 w-1.5 rounded-full ${stStyle.dot}`} />
                        {stLabel}
                      </span>
                    )}
                  </div>
                  <div className="mt-1 h-1 w-full overflow-hidden rounded-full bg-neutral-800">
                    <div className={`h-full ${stStyle.bar}`} style={{ width: `${pct}%` }} />
                  </div>
                  <div className="mt-1 font-mono text-[11px] text-neutral-500">
                    {formatBytes(used)} / {formatBytes(v.total_space)} · {v.mount_path}
                  </div>
                  {busy && prog && (
                    <div className="mt-1.5">
                      <div className="flex items-center gap-1.5 text-[10px] text-emerald-300/90">
                        <Loader2 className="h-3 w-3 animate-spin" />
                        <span className="font-mono">
                          {t("scandlg.entries", { count: formatCount(prog.count) })}
                        </span>
                      </div>
                      {prog.pct >= 0 && (
                        <div className="mt-1 h-1 w-full overflow-hidden rounded-full bg-neutral-800">
                          <div
                            className="h-full rounded-full bg-emerald-500 transition-[width] duration-200"
                            style={{ width: `${prog.pct}%` }}
                          />
                        </div>
                      )}
                    </div>
                  )}
                </div>
                <button
                  onClick={() => onScan(v.mount_path, v.name)}
                  disabled={busy}
                  className={`inline-flex shrink-0 items-center gap-1.5 rounded-md px-3 py-1.5 text-xs font-medium text-white disabled:opacity-60 ${
                    st === "changed"
                      ? "bg-amber-600 hover:bg-amber-500"
                      : st === "upToDate"
                      ? "bg-neutral-700 hover:bg-neutral-600"
                      : "bg-emerald-600 hover:bg-emerald-500"
                  }`}
                  title={st === "new" ? t("scandlg.scan") : t("scandlg.rescan")}
                >
                  {busy && <Loader2 className="h-3.5 w-3.5 animate-spin" />}
                  {busy ? t("scandlg.scanning") : st === "new" ? t("scandlg.scan") : t("scandlg.rescan")}
                </button>
              </div>
            );
          })}
        </div>

        <div className="flex items-center gap-2 border-t border-neutral-800 px-3 py-2">
          <button
            onClick={pickFolder}
            className="inline-flex items-center gap-1.5 rounded-md border border-neutral-700 px-3 py-1.5 text-xs hover:bg-neutral-800"
          >
            <FolderPlus className="h-3.5 w-3.5" /> {t("scandlg.scanFolder")}
          </button>
          <span className="inline-flex items-center gap-1 text-[11px] text-neutral-500">
            <Network className="h-3.5 w-3.5" /> {t("scandlg.nasNote")}
          </span>
        </div>
        <div className="flex flex-wrap items-center gap-1.5 border-t border-border px-3 py-2">
          <span className="mr-1 text-[11px] text-neutral-500">{t("scandlg.afterScan")}</span>
          <OptToggle
            icon={<ImageIcon className="h-3.5 w-3.5" />}
            label={t("scandlg.thumbnails")}
            on={options.thumbnails}
            onClick={() => setOptions({ ...options, thumbnails: !options.thumbnails })}
          />
          <OptToggle
            icon={<Film className="h-3.5 w-3.5" />}
            label={t("scandlg.analyzeVideos")}
            on={options.videos}
            onClick={() => setOptions({ ...options, videos: !options.videos })}
          />
          <OptToggle
            icon={<Package className="h-3.5 w-3.5" />}
            label={t("scandlg.archiveContents")}
            on={options.archives}
            onClick={() => setOptions({ ...options, archives: !options.archives })}
          />
        </div>

        <div className="flex flex-wrap items-center gap-1.5 border-t border-border px-3 py-2">
          <span className="mr-1 text-[11px] text-neutral-500">{t("scandlg.scanOptions")}</span>
          <OptToggle
            icon={<Ban className="h-3.5 w-3.5" />}
            label={t("scandlg.excludeJunk")}
            on={options.excludeJunk}
            onClick={() => setOptions({ ...options, excludeJunk: !options.excludeJunk })}
          />
        </div>

        <div className="border-t border-border px-4 py-2 text-[11px] text-neutral-500">
          {t("scandlg.helpText")}
        </div>
      </div>
    </div>
  );
}

/** Chip-toggle para una opción de post-escaneo. */
function OptToggle({
  icon,
  label,
  on,
  onClick,
}: {
  icon: ReactNode;
  label: string;
  on: boolean;
  onClick: () => void;
}) {
  const t = useT();
  void t;
  return (
    <button
      onClick={onClick}
      role="switch"
      aria-checked={on}
      className={`inline-flex items-center gap-1.5 rounded-full border px-2.5 py-1 text-[11px] font-medium transition-colors duration-150 ${
        on
          ? "border-primary/40 bg-primary/15 text-primary"
          : "border-border bg-transparent text-neutral-500 hover:bg-accent/60"
      }`}
    >
      {icon}
      {label}
    </button>
  );
}
