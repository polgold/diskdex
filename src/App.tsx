import { useEffect, useRef, useState } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { listen } from "@tauri-apps/api/event";
import { HardDrive, Database, Import, FolderOpen, Loader2, ScanLine, Usb, X, Share2, Languages } from "lucide-react";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  api,
  onVolumeAdded,
  onVolumeRemoved,
  onScanProgress,
  onIndexProgress,
  type VolumeInfo,
  type IndexProgress,
  type ScanProgress,
} from "./lib/ipc";
import { formatBytes, formatCount } from "./lib/format";
import { useCatalog } from "./store/catalog";
import { useT, useI18n } from "./lib/i18n";
import { localizeError } from "./lib/errors";
import { ScanDialog } from "./components/ScanDialog";
import { Sidebar } from "./components/Sidebar";
import { ContentTable } from "./components/ContentTable";
import { ContentToolbar } from "./components/ContentToolbar";
import { Inspector } from "./components/Inspector";
import { SearchBar } from "./components/SearchBar";
import { ShareDialog } from "./components/ShareDialog";
import { Splash } from "./components/Splash";
import { Button } from "@/components/ui/button";
import { TooltipProvider, Hint } from "@/components/ui/tooltip";

function App() {
  const t = useT();
  const lang = useI18n((s) => s.lang);
  const toggleLang = useI18n((s) => s.toggle);
  const { disks, error, loading, lastImport, catalogPath, setError, setLoading } = useCatalog();
  const openCatalogs = useCatalog((s) => s.openCatalogs);
  // Sesión guardada (catálogos abiertos) leída una sola vez al montar.
  const [savedSession] = useState(() => localStorage.getItem("diskdex:session"));
  const [status, setStatus] = useState<string>("");
  // Qué hacer DESPUÉS de escanear (cacheo pesado): el usuario decide. En discos
  // grandes/lentos (exFAT, NAS) conviene desactivar lo que no necesita.
  // Previews on-demand por defecto: NO generar todas las miniaturas/frames al
  // escanear (lento). Se generan al pararse en un archivo (rápido). El indexado de
  // archivos comprimidos sí queda activo (no es un "preview", da valor offline).
  const [postScan, setPostScan] = useState({ thumbnails: false, videos: false, archives: true, excludeJunk: true, enrich: false });
  // Escaneos en curso, indexados por mount path (permite varios simultáneos).
  // `cancelling` marca que ya se pidió cancelar (feedback inmediato en la barra).
  const [scanning, setScanning] = useState<
    Record<string, ScanProgress & { cancelling?: boolean; name?: string }>
  >({});
  const [indexProg, setIndexProg] = useState<IndexProgress | null>(null);
  const [scanOpen, setScanOpen] = useState(false);
  const [detected, setDetected] = useState<VolumeInfo | null>(null);
  const [shareOpen, setShareOpen] = useState(false);
  const [splash, setSplash] = useState(true);
  const catalogPathRef = useRef<string | null>(null);
  catalogPathRef.current = catalogPath;

  useEffect(() => {
    api.ping().then((p) => p !== "pong" && setStatus(t("app.ipcUnexpected", { p })));
    api.startVolumeWatch();
    const unlisteners = [
      onVolumeAdded((v) => {
        // Conectar un disco lo pone online, se quiera escanear o no: son cosas
        // distintas. Antes solo se ofrecía el escaneo, así que decir "no" dejaba
        // el disco marcado offline y bloqueaba copiar/comparar contra él.
        if (catalogPathRef.current) useCatalog.getState().refreshOnlineFromDisk();
        setDetected(v);
      }),
      onVolumeRemoved(() => {
        if (catalogPathRef.current) useCatalog.getState().refreshOnlineFromDisk();
      }),
      onScanProgress((p) =>
        setScanning((prev) => ({
          ...prev,
          // Preservar nombre y estado de cancelación (el evento solo trae mount/count/pct).
          [p.mount]: { ...p, name: prev[p.mount]?.name, cancelling: prev[p.mount]?.cancelling },
        }))
      ),
      onIndexProgress((p) => setIndexProg(p.done >= p.total ? null : p)),
      // Acciones desde el icono del tray (menú): abrir escaneo / enfocar búsqueda.
      listen("tray://scan", () => setScanOpen(true)),
      listen("tray://search", () => {
        setTimeout(() => document.getElementById("diskdex-search")?.focus(), 50);
      }),
    ];
    // Desactivar el menú contextual del navegador (Inspect/Search…) salvo en
    // campos de texto, donde el copiar/pegar nativo sí es útil.
    const onContextMenu = (e: MouseEvent) => {
      const el = e.target as HTMLElement | null;
      if (el && (el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.isContentEditable)) return;
      e.preventDefault();
    };
    window.addEventListener("contextmenu", onContextMenu);
    return () => {
      unlisteners.forEach((p) => p.then((un) => un()));
      window.removeEventListener("contextmenu", onContextMenu);
    };
  }, []);

  // Reabrir el último catálogo usado al iniciar. Fuente durable: archivo de
  // sesión del backend (sobrevive cierres y no depende del localStorage del
  // webview). Fallback: localStorage (migración). NUNCA borra la sesión ante un
  // error transitorio (p.ej. el .dccat en Dropbox sincronizando) — así no se
  // pierde el catálogo: solo avisa y el usuario reintenta.
  useEffect(() => {
    let cancelled = false;
    (async () => {
      let raw: string | null = null;
      try {
        raw = await api.loadSession();
      } catch {
        /* ignore */
      }
      if (!raw) raw = savedSession; // migración desde localStorage
      if (!raw || cancelled) return;
      let data: { catalogPath?: string; openCatalogs?: string[] };
      try {
        data = JSON.parse(raw);
      } catch {
        return;
      }
      if (!data.catalogPath || cancelled) return;
      try {
        await api.openCatalog(data.catalogPath);
        if (cancelled) return;
        (data.openCatalogs ?? []).forEach((p) => useCatalog.getState().addOpenCatalog(p));
        useCatalog.getState().addOpenCatalog(data.catalogPath);
        useCatalog.setState({ catalogPath: data.catalogPath });
        await useCatalog.getState().refreshOnlineFromDisk();
      } catch {
        if (!cancelled) setStatus(t("app.reopenFailed", { path: data.catalogPath }));
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  // Persistir la sesión en cada cambio (catálogos abiertos + activo). Se guarda
  // en disco vía backend (durable) y en localStorage (fallback/migración).
  useEffect(() => {
    if (!catalogPath) return;
    const json = JSON.stringify({ catalogPath, openCatalogs: openCatalogs.map((c) => c.path) });
    try {
      localStorage.setItem("diskdex:session", json);
    } catch {
      /* ignore */
    }
    api.saveSession(json).catch(() => {});
  }, [catalogPath, openCatalogs]);

  async function ensureCatalog(): Promise<boolean> {
    if (catalogPathRef.current) return true;
    const path = await save({
      title: t("app.dlgNewCatalogTitle"),
      defaultPath: "catalog.dccat",
      filters: [{ name: t("app.dlgCatalogFilter"), extensions: ["dccat"] }],
    });
    if (!path) return false;
    await api.openCatalog(path);
    useCatalog.setState({ catalogPath: path });
    useCatalog.getState().addOpenCatalog(path);
    return true;
  }

  async function handleImport() {
    setError(null);
    const dcmfPath = await open({
      title: t("app.dlgPickDcmfTitle"),
      filters: [{ name: t("app.dlgDcmfFilter"), extensions: ["dcmf", "dcmd"] }],
      multiple: false,
      directory: false,
    });
    if (!dcmfPath || typeof dcmfPath !== "string") return;

    // Importa DENTRO del catálogo abierto. Si no hay ninguno, crea uno primero.
    if (!(await ensureCatalog())) return;

    setLoading(true);
    setStatus(t("app.importing"));
    try {
      // Detectar conflictos (discos del .dcmf que ya existen) y preguntar.
      let replace = false;
      try {
        const names = await api.dcmfDiskNames(dcmfPath);
        const existing = new Set(useCatalog.getState().disks.map((d) => d.name));
        const conflicts = names.filter((n) => existing.has(n));
        if (conflicts.length > 0) {
          replace = window.confirm(t("app.importConflict", { disks: conflicts.join(", ") }));
        }
      } catch {
        /* si falla el preview, importa igual sin reemplazar */
      }

      const summary = await api.importDcmfMerge(dcmfPath, replace);
      await useCatalog.getState().refreshDisks();
      await useCatalog.getState().refreshOnlineFromDisk();
      setStatus(
        t("app.imported", {
          disks: summary.disks,
          entries: formatCount(summary.entries),
          secs: (summary.elapsed_ms / 1000).toFixed(1),
        })
      );
    } catch (e) {
      setError(String(e));
      setStatus("");
    } finally {
      setLoading(false);
    }
  }

  async function handleOpen() {
    setError(null);
    const path = await open({
      title: t("app.dlgOpenTitle"),
      filters: [{ name: t("app.dlgCatalogFilter"), extensions: ["dccat"] }],
      multiple: false,
      directory: false,
    });
    if (!path || typeof path !== "string") return;
    setLoading(true);
    try {
      await api.openCatalog(path);
      useCatalog.setState({ catalogPath: path });
      useCatalog.getState().addOpenCatalog(path);
      await useCatalog.getState().refreshOnlineFromDisk();
      setStatus(t("app.opened", { path }));
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }

  async function handleScan(mountPath: string, name: string) {
    setError(null);
    if (scanning[mountPath]) return; // ya se está escaneando este disco
    if (!(await ensureCatalog())) return;
    setScanning((prev) => ({ ...prev, [mountPath]: { mount: mountPath, count: 0, pct: -1, name } }));
    setStatus(t("app.scanning", { name }));
    try {
      const r = await api.scanDisk(mountPath, name, { exclude_junk: postScan.excludeJunk, enrich: postScan.enrich });
      await useCatalog.getState().refreshDisks();
      await useCatalog.getState().refreshOnlineFromDisk();
      setStatus(
        t("app.scannedDone", {
          verb: r.replaced ? t("app.verbRescanned") : t("app.verbScanned"),
          name: r.name,
          entries: formatCount(r.entries),
          files: formatCount(r.files),
          secs: (r.elapsed_ms / 1000).toFixed(1),
        }) +
          (r.reused_dirs > 0
            ? t("app.scannedIncremental", { n: formatCount(r.reused_dirs) })
            : "")
      );
      setScanOpen(false);
      setDetected(null);
      // Post-escaneo en segundo plano (con el disco aún montado): miniaturas de
      // imágenes, metadata+frames de video y contenido de archivos comprimidos.
      // Todo queda cacheado en el .dccat y visible offline. No bloquea la UI.
      if (postScan.thumbnails || postScan.videos || postScan.archives) {
        void (async () => {
          try {
            if (postScan.thumbnails) {
              const tb = await api.cacheDiskThumbnails(r.disk_id);
              if (tb.generated > 0)
                setStatus(t("app.thumbsCached", { n: formatCount(tb.generated), name }));
            }
            if (postScan.videos) {
              const v = await api.indexDiskVideos(r.disk_id);
              if (v.indexed > 0)
                setStatus(
                  t("app.videosAnalyzed", {
                    n: formatCount(v.indexed),
                    name,
                    frames: formatCount(v.frames),
                  })
                );
            }
            if (postScan.archives) {
              const a = await api.indexDiskArchives(r.disk_id);
              if (a.indexed > 0)
                setStatus(t("app.archivesIndexed", { n: formatCount(a.indexed), name }));
            }
          } catch {
            /* el indexado es best-effort; no interrumpe el flujo */
          } finally {
            setIndexProg(null);
          }
        })();
      }
    } catch (e) {
      // La cancelación a pedido no es un error: se informa como estado.
      if (String(e).toLowerCase().includes("cancel")) {
        setStatus(t("app.scanCancelled", { name }));
      } else {
        setError(String(e));
        setStatus("");
      }
    } finally {
      setScanning((prev) => {
        const next = { ...prev };
        delete next[mountPath];
        return next;
      });
    }
  }

  function handleCancelScan(mountPath: string) {
    api.cancelScan(mountPath).catch(() => {});
    // Feedback inmediato: marcar la barra como "cancelando…".
    setScanning((prev) =>
      prev[mountPath] ? { ...prev, [mountPath]: { ...prev[mountPath], cancelling: true } } : prev
    );
  }

  const hasCatalog = disks.length > 0;

  return (
    <TooltipProvider delayDuration={350} skipDelayDuration={120}>
    {splash && <Splash onDone={() => setSplash(false)} />}
    <div className="flex h-full flex-col bg-neutral-950 text-neutral-200">
      <header className="flex items-center gap-3 border-b border-border bg-gradient-to-b from-neutral-900/80 to-neutral-950 px-4 py-2 shadow-[0_1px_0_0_rgba(255,255,255,0.03)]">
        <div className="flex shrink-0 items-center gap-2">
          <span className="grid h-7 w-7 place-items-center rounded-md bg-primary/15 ring-1 ring-primary/30">
            <Database className="h-4 w-4 text-primary" />
          </span>
          <h1 className="text-sm font-semibold tracking-tight">DiskDex</h1>
        </div>
        <div className="mx-2 flex-1">{hasCatalog && <SearchBar />}</div>
        <div className="flex shrink-0 items-center gap-1.5">
          <Button variant="accent" onClick={() => setScanOpen(true)} disabled={loading}>
            <ScanLine />
            {t("app.scan")}
          </Button>
          <Button onClick={handleImport} disabled={loading}>
            {loading ? <Loader2 className="animate-spin" /> : <Import />}
            {t("app.import")}
          </Button>
          <Button variant="outline" onClick={handleOpen} disabled={loading}>
            <FolderOpen />
            {t("app.open")}
          </Button>
          <Hint label={t("app.shareTip")}>
            <Button variant="ghost" size="icon" onClick={() => setShareOpen(true)} aria-label={t("app.share")}>
              <Share2 />
            </Button>
          </Hint>
          <Hint label={t("app.langTip")}>
            <button
              onClick={toggleLang}
              aria-label={t("app.langTip")}
              className="inline-flex items-center gap-1 rounded-md px-2 py-1.5 text-xs font-medium text-neutral-400 transition-colors hover:bg-accent/60 hover:text-neutral-200"
            >
              <Languages className="h-4 w-4" />
              {lang.toUpperCase()}
            </button>
          </Hint>
        </div>
      </header>

      <TabBar />

      {detected && (
        <DetectDialog
          volume={detected}
          busy={!!scanning[detected.mount_path]}
          onScan={() => handleScan(detected.mount_path, detected.name)}
          onDismiss={() => setDetected(null)}
        />
      )}

      {(error || (status && !error)) && (
        <div className="border-b border-neutral-800 px-4 py-1.5 text-xs">
          {error ? (
            <span className="text-red-300">{localizeError(error, t)}</span>
          ) : (
            <span className="text-neutral-400">{status}</span>
          )}
        </div>
      )}

      {(Object.keys(scanning).length > 0 || indexProg) && (
        <div className="space-y-1.5 border-b border-border bg-neutral-900/40 px-4 py-1.5">
          {Object.values(scanning).map((s) => (
            <ScanProgressBar
              key={s.mount}
              progress={s}
              name={s.name}
              cancelling={s.cancelling}
              onCancel={() => handleCancelScan(s.mount)}
            />
          ))}
          {indexProg && <ProgressBar progress={indexProg} />}
        </div>
      )}

      {/* Workspace de 3 paneles (M2/M3) o estado vacío */}
      {hasCatalog ? (
        <div className="grid flex-1 grid-cols-[260px_1fr_320px] overflow-hidden">
          <aside className="overflow-auto border-r border-neutral-800">
            <Sidebar onRescan={handleScan} />
          </aside>
          <section className="flex flex-col overflow-hidden border-r border-neutral-800">
            <ContentToolbar />
            <div className="flex-1 overflow-hidden">
              <ContentTable />
            </div>
          </section>
          <aside className="overflow-auto">
            <Inspector />
          </aside>
        </div>
      ) : (
        <main className="flex-1 overflow-auto p-4">
          <EmptyState />
        </main>
      )}

      <footer className="flex items-center gap-2 border-t border-neutral-800 px-4 py-1.5 text-[11px] text-neutral-500">
        {catalogPath ? <span className="font-mono">{catalogPath}</span> : <span>{t("app.noCatalog")}</span>}
        {lastImport && (
          <span>{t("app.footerImport", { disks: lastImport.disks, entries: formatCount(lastImport.entries) })}</span>
        )}
        <button
          onClick={() => openUrl("https://exitmedia.com.ar")}
          className="ml-auto text-neutral-500 transition-colors hover:text-primary"
          title="exitmedia.com.ar"
        >
          Desarrollado por Pablo Goldberg · <span className="font-medium text-neutral-400">ExitMedia</span>
        </button>
        {hasCatalog && <span className="text-neutral-600">·</span>}
        {hasCatalog && <span>{t("app.disksInCatalog", { n: disks.length })}</span>}
      </footer>

      {scanOpen && (
        <ScanDialog
          onClose={() => setScanOpen(false)}
          onScan={handleScan}
          scanning={scanning}
          options={postScan}
          setOptions={setPostScan}
        />
      )}
      {shareOpen && <ShareDialog onClose={() => setShareOpen(false)} />}
    </div>
    </TooltipProvider>
  );
}

const PHASE_KEYS: Record<IndexProgress["phase"], string> = {
  thumbnails: "app.phaseThumbnails",
  videos: "app.phaseVideos",
  archives: "app.phaseArchives",
};

function ProgressBar({ progress }: { progress: IndexProgress }) {
  const t = useT();
  const pct = progress.total > 0 ? Math.round((progress.done / progress.total) * 100) : 0;
  return (
    <div>
      <div className="flex items-center justify-between text-[11px] text-neutral-400">
        <span className="flex items-center gap-1.5">
          <Loader2 className="h-3 w-3 animate-spin text-primary" />
          {t(PHASE_KEYS[progress.phase])}…
        </span>
        <span className="font-mono text-neutral-300">
          {formatCount(progress.done)} / {formatCount(progress.total)} ({pct}%)
        </span>
      </div>
      <div className="mt-1 h-1 w-full overflow-hidden rounded-full bg-neutral-800">
        <div
          className="h-full rounded-full bg-primary transition-[width] duration-200"
          style={{ width: `${pct}%` }}
        />
      </div>
    </div>
  );
}

function ScanProgressBar({
  progress,
  name,
  cancelling,
  onCancel,
}: {
  progress: ScanProgress;
  name?: string;
  cancelling?: boolean;
  onCancel?: () => void;
}) {
  const t = useT();
  // Preferir el nombre del disco; el mount puede ser "/" (volumen raíz).
  const disk = name || progress.mount.split("/").filter(Boolean).pop() || progress.mount;
  const saving = progress.pct === -2; // fase de guardado (ingesta en el catálogo)
  const hashing = progress.pct === -3; // fase de enriquecimiento (hash BLAKE3 por archivo)
  return (
    <div>
      <div className="flex items-center justify-between gap-2 text-[11px] text-neutral-400">
        <span className="flex items-center gap-1.5">
          <Loader2 className="h-3 w-3 animate-spin text-primary" />
          {cancelling ? (
            <>{t("app.cancellingDisk")} <span className="text-neutral-200">{disk}</span>…</>
          ) : hashing ? (
            <>{t("app.hashingDisk")} <span className="text-neutral-200">{disk}</span>…</>
          ) : saving ? (
            <>{t("app.savingDisk")} <span className="text-neutral-200">{disk}</span>…</>
          ) : (
            <>{t("app.scanningDisk")} <span className="text-neutral-200">{disk}</span>…</>
          )}
        </span>
        <span className="flex items-center gap-2">
          {/* Sin % (no es estimable de forma creíble durante el recorrido): mostramos
              el conteo real de entradas, que es honesto y va subiendo. */}
          <span className="font-mono text-neutral-300">
            {t("app.scanningEntries", { count: formatCount(progress.count) })}
          </span>
          {onCancel && !saving && (
            <button
              onClick={onCancel}
              disabled={cancelling}
              className="inline-flex items-center gap-1 rounded border border-neutral-700 px-1.5 py-0.5 text-[10px] text-neutral-400 transition-colors hover:border-red-900/60 hover:bg-red-950/40 hover:text-red-300 disabled:opacity-50"
              title={t("app.cancelScanTip")}
            >
              <X className="h-3 w-3" />
              {t("common.cancel")}
            </button>
          )}
        </span>
      </div>
      {/* Barra verde que se llena por bytes recorridos (avance real, sin mostrar
          el % numérico que confunde cuando quedan muchos archivos chicos). Si el
          total se desconoce (escaneo de subcarpeta) → barra llena con pulso suave
          (sin el haz que cruza, que distraía). */}
      <div className="mt-1 h-1 w-full overflow-hidden rounded-full bg-neutral-800">
        {cancelling ? (
          <div className="h-full w-1/3 rounded-full bg-neutral-600" />
        ) : progress.pct >= 0 ? (
          <div
            className="h-full rounded-full bg-primary transition-[width] duration-300"
            style={{ width: `${progress.pct}%` }}
          />
        ) : (
          <div className="h-full w-full animate-pulse rounded-full bg-primary/50" />
        )}
      </div>
    </div>
  );
}

function TabBar() {
  const openCatalogs = useCatalog((s) => s.openCatalogs);
  const catalogPath = useCatalog((s) => s.catalogPath);
  const switchCatalog = useCatalog((s) => s.switchCatalog);
  const closeCatalog = useCatalog((s) => s.closeCatalog);
  if (openCatalogs.length <= 1) return null;
  return (
    <div className="flex items-stretch gap-px overflow-x-auto border-b border-neutral-800 bg-neutral-900/30">
      {openCatalogs.map((c) => {
        const active = c.path === catalogPath;
        return (
          <div
            key={c.path}
            onClick={() => switchCatalog(c.path)}
            className={`group flex cursor-pointer items-center gap-1.5 border-r border-neutral-800 px-3 py-1.5 text-xs ${
              active ? "bg-neutral-950 text-neutral-100" : "text-neutral-400 hover:bg-neutral-800/40"
            }`}
            title={c.path}
          >
            <Database className={`h-3 w-3 ${active ? "text-emerald-400" : "text-neutral-600"}`} />
            <span className="max-w-[160px] truncate">{c.name}</span>
            <button
              onClick={(e) => {
                e.stopPropagation();
                closeCatalog(c.path);
              }}
              className="rounded p-0.5 text-neutral-600 opacity-0 hover:bg-neutral-700 hover:text-neutral-200 group-hover:opacity-100"
            >
              <X className="h-3 w-3" />
            </button>
          </div>
        );
      })}
    </div>
  );
}

/** Popup modal al detectar un disco nuevo montado: pregunta si escanearlo. */
function DetectDialog({
  volume,
  busy,
  onScan,
  onDismiss,
}: {
  volume: VolumeInfo;
  busy: boolean;
  onScan: () => void;
  onDismiss: () => void;
}) {
  const t = useT();
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onDismiss();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onDismiss]);

  return (
    <div
      className="fixed inset-0 z-[150] flex items-center justify-center bg-black/50 p-4 animate-fade-in"
      onClick={onDismiss}
    >
      <div
        className="w-full max-w-sm rounded-xl border border-border bg-popover p-5 shadow-pop animate-zoom-in"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center gap-2.5">
          <span className="grid h-9 w-9 shrink-0 place-items-center rounded-lg bg-amber-500/15 ring-1 ring-amber-500/30">
            <Usb className="h-5 w-5 text-amber-400" />
          </span>
          <h2 className="text-sm font-semibold text-neutral-100">{t("app.detectedTitle")}</h2>
        </div>
        <p className="mt-3 text-sm text-neutral-300">
          {t("app.detectedBody", { name: volume.name, size: formatBytes(volume.total_space) })}
        </p>
        <div className="mt-5 flex justify-end gap-2">
          <button
            onClick={onDismiss}
            disabled={busy}
            className="rounded-md border border-neutral-700 px-3 py-1.5 text-xs text-neutral-300 hover:bg-neutral-800 disabled:opacity-50"
          >
            {t("app.later")}
          </button>
          <button
            onClick={onScan}
            disabled={busy}
            className="inline-flex items-center gap-1.5 rounded-md bg-amber-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-amber-500 disabled:opacity-60"
          >
            {busy && <Loader2 className="h-3.5 w-3.5 animate-spin" />}
            {busy ? t("app.scanningShort") : t("app.scanNow")}
          </button>
        </div>
      </div>
    </div>
  );
}

function EmptyState() {
  const t = useT();
  return (
    <div className="flex h-full flex-col items-center justify-center gap-3 text-center">
      <HardDrive className="h-12 w-12 text-neutral-700" />
      <h2 className="text-base font-medium text-neutral-300">{t("app.emptyTitle")}</h2>
      <p className="max-w-md text-sm text-neutral-500">
        <span className="text-sky-400">{t("app.emptyScan")}</span>
        {t("app.emptyBody", { dccat: ".dccat", dcmf: ".dcmf" })}
      </p>
    </div>
  );
}

export default App;
