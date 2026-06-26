import { useEffect, useRef, useState } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { HardDrive, Database, Import, FolderOpen, Loader2, ScanLine, Usb, X, Share2 } from "lucide-react";
import { api, onVolumeAdded, onVolumeRemoved, type VolumeInfo } from "./lib/ipc";
import { formatBytes, formatCount } from "./lib/format";
import { useCatalog } from "./store/catalog";
import { ScanDialog } from "./components/ScanDialog";
import { Sidebar } from "./components/Sidebar";
import { ContentTable } from "./components/ContentTable";
import { ContentToolbar } from "./components/ContentToolbar";
import { Inspector } from "./components/Inspector";
import { SearchBar } from "./components/SearchBar";
import { ShareDialog } from "./components/ShareDialog";
import { Button } from "@/components/ui/button";
import { TooltipProvider, Hint } from "@/components/ui/tooltip";

function App() {
  const { disks, error, loading, lastImport, catalogPath, setImportResult, setError, setLoading } =
    useCatalog();
  const [status, setStatus] = useState<string>("");
  const [scanOpen, setScanOpen] = useState(false);
  const [scanningPath, setScanningPath] = useState<string | null>(null);
  const [detected, setDetected] = useState<VolumeInfo | null>(null);
  const [shareOpen, setShareOpen] = useState(false);
  const catalogPathRef = useRef<string | null>(null);
  catalogPathRef.current = catalogPath;

  useEffect(() => {
    api.ping().then((p) => p !== "pong" && setStatus(`IPC inesperada: ${p}`));
    api.startVolumeWatch();
    const unlisteners = [
      onVolumeAdded((v) => setDetected(v)),
      onVolumeRemoved(() => {
        if (catalogPathRef.current) useCatalog.getState().refreshOnlineFromDisk();
      }),
    ];
    return () => unlisteners.forEach((p) => p.then((un) => un()));
  }, []);

  async function ensureCatalog(): Promise<boolean> {
    if (catalogPathRef.current) return true;
    const path = await save({
      title: "Crear un catálogo nuevo para guardar el escaneo",
      defaultPath: "catalog.dccat",
      filters: [{ name: "Catálogo DiskDex", extensions: ["dccat"] }],
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
      title: "Elegí el archivo .dcmf de DiskCatalogMaker",
      filters: [{ name: "DiskCatalogMaker", extensions: ["dcmf", "dcmd"] }],
      multiple: false,
      directory: false,
    });
    if (!dcmfPath || typeof dcmfPath !== "string") return;
    const catalogPath = await save({
      title: "Guardar catálogo como…",
      defaultPath: "catalog.dccat",
      filters: [{ name: "Catálogo DiskDex", extensions: ["dccat"] }],
    });
    if (!catalogPath) return;

    setLoading(true);
    setStatus("Importando… (inflando bloques y poblando SQLite)");
    try {
      const summary = await api.importDcmf(dcmfPath, catalogPath);
      await setImportResult(summary);
      useCatalog.getState().addOpenCatalog(summary.catalog_path);
      await useCatalog.getState().refreshOnlineFromDisk();
      setStatus(
        `Importado: ${summary.disks} discos, ${formatCount(summary.entries)} entradas en ${(
          summary.elapsed_ms / 1000
        ).toFixed(1)} s`
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
      title: "Abrir catálogo .dccat",
      filters: [{ name: "Catálogo DiskDex", extensions: ["dccat"] }],
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
      setStatus(`Catálogo abierto: ${path}`);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }

  async function handleScan(mountPath: string, name: string) {
    setError(null);
    if (!(await ensureCatalog())) return;
    setScanningPath(mountPath);
    setStatus(`Escaneando ${name}…`);
    try {
      const r = await api.scanDisk(mountPath, name);
      await useCatalog.getState().refreshDisks();
      await useCatalog.getState().refreshOnlineFromDisk();
      setStatus(
        `${r.replaced ? "Re-escaneado" : "Escaneado"} ${r.name}: ${formatCount(
          r.entries
        )} entradas (${formatCount(r.files)} archivos) en ${(r.elapsed_ms / 1000).toFixed(1)} s`
      );
      setScanOpen(false);
      setDetected(null);
      // Cacheo de miniaturas en segundo plano: el disco sigue montado, así las
      // imágenes quedan visibles aunque luego se desconecte (offline). No bloquea.
      api
        .cacheDiskThumbnails(r.disk_id)
        .then((t) => {
          if (t.generated > 0) setStatus(`${formatCount(t.generated)} miniaturas cacheadas en ${name}`);
        })
        .catch(() => {});
    } catch (e) {
      setError(String(e));
      setStatus("");
    } finally {
      setScanningPath(null);
    }
  }

  const hasCatalog = disks.length > 0;

  return (
    <TooltipProvider delayDuration={350} skipDelayDuration={120}>
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
            Escanear
          </Button>
          <Button onClick={handleImport} disabled={loading}>
            {loading ? <Loader2 className="animate-spin" /> : <Import />}
            Importar
          </Button>
          <Button variant="outline" onClick={handleOpen} disabled={loading}>
            <FolderOpen />
            Abrir
          </Button>
          <Hint label="Conector remoto seguro">
            <Button variant="ghost" size="icon" onClick={() => setShareOpen(true)} aria-label="Compartir">
              <Share2 />
            </Button>
          </Hint>
        </div>
      </header>

      <TabBar />

      {detected && (
        <DetectBanner
          volume={detected}
          busy={scanningPath === detected.mount_path}
          onScan={() => handleScan(detected.mount_path, detected.name)}
          onDismiss={() => setDetected(null)}
        />
      )}

      {(error || (status && !error)) && (
        <div className="border-b border-neutral-800 px-4 py-1.5 text-xs">
          {error ? <span className="text-red-300">{error}</span> : <span className="text-neutral-400">{status}</span>}
        </div>
      )}

      {/* Workspace de 3 paneles (M2/M3) o estado vacío */}
      {hasCatalog ? (
        <div className="grid flex-1 grid-cols-[260px_1fr_320px] overflow-hidden">
          <aside className="overflow-auto border-r border-neutral-800">
            <Sidebar />
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
        {catalogPath ? <span className="font-mono">{catalogPath}</span> : <span>Sin catálogo abierto</span>}
        {lastImport && (
          <span>· {lastImport.disks} discos · {formatCount(lastImport.entries)} entradas</span>
        )}
        {hasCatalog && <span className="ml-auto">{disks.length} discos en el catálogo</span>}
      </footer>

      {scanOpen && (
        <ScanDialog onClose={() => setScanOpen(false)} onScan={handleScan} scanningPath={scanningPath} />
      )}
      {shareOpen && <ShareDialog onClose={() => setShareOpen(false)} />}
    </div>
    </TooltipProvider>
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

function DetectBanner({
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
  return (
    <div className="flex items-center gap-3 border-b border-amber-900/50 bg-amber-950/30 px-4 py-2 text-sm">
      <Usb className="h-4 w-4 text-amber-400" />
      <span>
        Disco detectado: <span className="font-medium">{volume.name}</span>{" "}
        <span className="text-neutral-400">({formatBytes(volume.total_space)})</span>
      </span>
      <div className="ml-auto flex items-center gap-2">
        <button
          onClick={onScan}
          disabled={busy}
          className="inline-flex items-center gap-1.5 rounded-md bg-amber-600 px-3 py-1 text-xs font-medium text-white hover:bg-amber-500 disabled:opacity-60"
        >
          {busy && <Loader2 className="h-3.5 w-3.5 animate-spin" />}
          {busy ? "Escaneando…" : "Escanear ahora"}
        </button>
        <button onClick={onDismiss} className="rounded p-1 text-neutral-400 hover:bg-neutral-800 hover:text-neutral-200">
          <X className="h-4 w-4" />
        </button>
      </div>
    </div>
  );
}

function EmptyState() {
  return (
    <div className="flex h-full flex-col items-center justify-center gap-3 text-center">
      <HardDrive className="h-12 w-12 text-neutral-700" />
      <h2 className="text-base font-medium text-neutral-300">No hay discos todavía</h2>
      <p className="max-w-md text-sm text-neutral-500">
        Importá tu catálogo de DiskCatalogMaker (<span className="font-mono">.dcmf</span>), abrí un
        catálogo <span className="font-mono">.dccat</span>, o <span className="text-sky-400">escaneá un
        disco</span> conectado. Conectá un disco y aparece un aviso para escanearlo.
      </p>
    </div>
  );
}

export default App;
