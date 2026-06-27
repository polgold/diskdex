import { useEffect, useState, type ReactNode } from "react";
import { HardDrive, Usb, Loader2, X, RefreshCw, FolderPlus, Network, Image as ImageIcon, Film, Package } from "lucide-react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { api, type VolumeInfo, type ScanProgress } from "../lib/ipc";
import { formatBytes, formatCount } from "../lib/format";

export interface PostScanOptions {
  thumbnails: boolean;
  videos: boolean;
  archives: boolean;
}

interface Props {
  onClose: () => void;
  onScan: (mountPath: string, name: string) => void;
  scanning: Record<string, ScanProgress>;
  options: PostScanOptions;
  setOptions: (o: PostScanOptions) => void;
}

export function ScanDialog({ onClose, onScan, scanning, options, setOptions }: Props) {
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
    const dir = await openDialog({ title: "Elegí una carpeta o un share de red para escanear", directory: true, multiple: false });
    if (dir && typeof dir === "string") {
      const name = dir.split("/").filter(Boolean).pop() ?? dir;
      onScan(dir, name);
    }
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/70 p-4 backdrop-blur-sm animate-fade-in">
      <div className="w-full max-w-lg rounded-xl border border-border bg-card shadow-pop animate-zoom-in">
        <div className="flex items-center justify-between border-b border-border px-4 py-3">
          <h2 className="text-sm font-semibold">Escanear un disco montado</h2>
          <div className="flex items-center gap-1">
            <button
              onClick={refresh}
              className="rounded p-1.5 text-neutral-400 hover:bg-neutral-800 hover:text-neutral-200"
              title="Actualizar lista"
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
              No se detectaron volúmenes montados.
            </p>
          )}
          {volumes.map((v) => {
            const used = v.total_space - v.available_space;
            const pct = v.total_space > 0 ? Math.min(100, (used / v.total_space) * 100) : 0;
            const prog = scanning[v.mount_path];
            const busy = !!prog;
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
                  </div>
                  <div className="mt-1 h-1 w-full overflow-hidden rounded-full bg-neutral-800">
                    <div className="h-full bg-emerald-500/70" style={{ width: `${pct}%` }} />
                  </div>
                  <div className="mt-1 font-mono text-[11px] text-neutral-500">
                    {formatBytes(used)} / {formatBytes(v.total_space)} · {v.mount_path}
                  </div>
                  {busy && prog && (
                    <div className="mt-1.5">
                      <div className="flex items-center gap-1.5 text-[10px] text-emerald-300/90">
                        <Loader2 className="h-3 w-3 animate-spin" />
                        <span className="font-mono">
                          {formatCount(prog.count)} entradas
                          {prog.pct >= 0 ? ` · ${prog.pct}%` : ""}
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
                  className="inline-flex shrink-0 items-center gap-1.5 rounded-md bg-emerald-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-emerald-500 disabled:opacity-60"
                >
                  {busy && <Loader2 className="h-3.5 w-3.5 animate-spin" />}
                  {busy ? "Escaneando…" : "Escanear"}
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
            <FolderPlus className="h-3.5 w-3.5" /> Escanear carpeta…
          </button>
          <span className="inline-flex items-center gap-1 text-[11px] text-neutral-500">
            <Network className="h-3.5 w-3.5" /> incluye shares de NAS montados
          </span>
        </div>
        <div className="flex flex-wrap items-center gap-1.5 border-t border-border px-3 py-2">
          <span className="mr-1 text-[11px] text-neutral-500">Tras escanear:</span>
          <OptToggle
            icon={<ImageIcon className="h-3.5 w-3.5" />}
            label="Miniaturas"
            on={options.thumbnails}
            onClick={() => setOptions({ ...options, thumbnails: !options.thumbnails })}
          />
          <OptToggle
            icon={<Film className="h-3.5 w-3.5" />}
            label="Analizar videos"
            on={options.videos}
            onClick={() => setOptions({ ...options, videos: !options.videos })}
          />
          <OptToggle
            icon={<Package className="h-3.5 w-3.5" />}
            label="Contenido de archivos"
            on={options.archives}
            onClick={() => setOptions({ ...options, archives: !options.archives })}
          />
        </div>

        <div className="border-t border-border px-4 py-2 text-[11px] text-neutral-500">
          Se guarda el árbol completo (tamaños lógico/físico, fechas) y un fingerprint del volumen
          para reconocerlo al reconectarlo. Re-escanear reemplaza el disco anterior. El análisis de
          miniaturas/videos corre en segundo plano y puede tardar en discos lentos.
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
