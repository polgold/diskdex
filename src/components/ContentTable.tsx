import { useEffect, useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import {
  ChevronRight,
  Folder,
  File as FileIcon,
  Search,
  Loader2,
  FolderSearch,
  ExternalLink,
  Copy,
  Trash2,
} from "lucide-react";
import { useCatalog } from "../store/catalog";
import { formatBytes, formatDate, formatCount } from "../lib/format";
import { api, type SearchItem } from "../lib/ipc";
import { revealOriginal, openOriginal, copyText } from "../lib/actions";

interface MenuState {
  x: number;
  y: number;
  id: number;
  isFolder: boolean;
}

/** Menú contextual propio (clic derecho) sobre un ítem. */
function RowContextMenu({ menu, onClose }: { menu: MenuState; onClose: () => void }) {
  const setError = useCatalog((s) => s.setError);
  const reloadCurrent = useCatalog((s) => s.reloadCurrent);

  useEffect(() => {
    const close = () => onClose();
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("click", close);
    window.addEventListener("scroll", close, true);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("click", close);
      window.removeEventListener("scroll", close, true);
      window.removeEventListener("keydown", onKey);
    };
  }, [onClose]);

  async function run(fn: () => Promise<void>) {
    onClose();
    try {
      await fn();
    } catch (e) {
      setError(String(e));
    }
  }

  const items = [
    {
      label: "Revelar en Finder",
      icon: <FolderSearch className="h-3.5 w-3.5" />,
      fn: () => revealOriginal(menu.id),
    },
    ...(!menu.isFolder
      ? [
          {
            label: "Abrir en el visor del sistema",
            icon: <ExternalLink className="h-3.5 w-3.5" />,
            fn: () => openOriginal(menu.id),
          },
        ]
      : []),
    {
      label: "Copiar ruta",
      icon: <Copy className="h-3.5 w-3.5" />,
      fn: async () => {
        const p = await api.entryPath(menu.id);
        await copyText(p);
      },
    },
    ...(!menu.isFolder
      ? [
          {
            label: "Mover a la Papelera",
            icon: <Trash2 className="h-3.5 w-3.5" />,
            danger: true,
            fn: async () => {
              const p = await api.entryPath(menu.id);
              const ok = window.confirm(
                `¿Mover a la Papelera?\n\n${p}\n\nSe borra el original del disco (recuperable desde la Papelera) y se quita del catálogo.`
              );
              if (!ok) return;
              await api.moveToTrash(menu.id);
              await reloadCurrent();
            },
          },
        ]
      : []),
  ];

  // Evitar que el menú se salga de la ventana.
  const left = Math.min(menu.x, window.innerWidth - 220);
  const top = Math.min(menu.y, window.innerHeight - 16 - items.length * 32);

  return (
    <div
      className="fixed z-[100] min-w-52 overflow-hidden rounded-lg border border-border bg-popover py-1 shadow-pop animate-zoom-in"
      style={{ left, top }}
      onClick={(e) => e.stopPropagation()}
    >
      {items.map((it) => {
        const danger = (it as { danger?: boolean }).danger;
        return (
          <button
            key={it.label}
            onClick={() => run(it.fn)}
            className={`flex w-full items-center gap-2 px-3 py-1.5 text-left text-xs transition-colors ${
              danger
                ? "text-red-300 hover:bg-red-950/50"
                : "text-neutral-200 hover:bg-accent"
            }`}
          >
            {it.icon}
            {it.label}
          </button>
        );
      })}
    </div>
  );
}

export function ContentTable() {
  const mode = useCatalog((s) => s.mode);
  return mode === "search" ? <SearchTable /> : <BrowseTable />;
}

interface ColDef {
  key: string;
  label: string;
  align?: "right";
  flex?: boolean;
}

/** Anchos de columna persistidos + arrastre para redimensionar. */
function useColWidths(storageKey: string, defaults: Record<string, number>) {
  const [widths, setWidths] = useState<Record<string, number>>(() => {
    try {
      const saved = localStorage.getItem(storageKey);
      if (saved) return { ...defaults, ...JSON.parse(saved) };
    } catch {
      /* ignore */
    }
    return defaults;
  });

  useEffect(() => {
    localStorage.setItem(storageKey, JSON.stringify(widths));
  }, [storageKey, widths]);

  const startResize = (key: string) => (e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    const startX = e.clientX;
    const startW = widths[key];
    const onMove = (ev: MouseEvent) => {
      const w = Math.max(48, Math.min(900, startW + (ev.clientX - startX)));
      setWidths((prev) => ({ ...prev, [key]: w }));
    };
    const onUp = () => {
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
      document.body.style.cursor = "";
    };
    document.body.style.cursor = "col-resize";
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
  };

  return { widths, startResize };
}

function ResizableHeaderRow({
  cols,
  widths,
  startResize,
}: {
  cols: ColDef[];
  widths: Record<string, number>;
  startResize: (key: string) => (e: React.MouseEvent) => void;
}) {
  return (
    <div className="flex items-center border-b border-neutral-800 bg-neutral-900/40 px-3 py-1.5 text-[11px] font-medium uppercase tracking-wide text-neutral-500">
      {cols.map((c) => (
        <div
          key={c.key}
          className={`relative flex items-center ${c.flex ? "min-w-0 flex-1" : ""} ${
            c.align === "right" ? "justify-end" : ""
          }`}
          style={c.flex ? undefined : { width: widths[c.key] }}
        >
          <span className="truncate">{c.label}</span>
          {!c.flex && (
            <span
              onMouseDown={startResize(c.key)}
              className="group absolute -right-1.5 top-1/2 z-10 h-5 w-3 -translate-y-1/2 cursor-col-resize"
              title="Arrastrá para redimensionar"
            >
              <span className="absolute left-1/2 top-0 h-full w-px -translate-x-1/2 bg-neutral-700 group-hover:bg-primary" />
            </span>
          )}
        </div>
      ))}
    </div>
  );
}

function Breadcrumb() {
  const breadcrumb = useCatalog((s) => s.breadcrumb);
  const navigateToCrumb = useCatalog((s) => s.navigateToCrumb);
  if (breadcrumb.length === 0) return null;
  return (
    <div className="flex flex-wrap items-center gap-0.5 border-b border-neutral-800 px-3 py-1.5 text-xs text-neutral-400">
      {breadcrumb.map((c, i) => (
        <span key={`${c.id}-${i}`} className="flex items-center">
          {i > 0 && <ChevronRight className="mx-0.5 h-3 w-3 text-neutral-600" />}
          <button
            onClick={() => navigateToCrumb(i)}
            className={`rounded px-1 py-0.5 hover:bg-neutral-800 ${
              i === breadcrumb.length - 1 ? "text-neutral-200" : "hover:text-neutral-200"
            }`}
          >
            {c.name}
          </button>
        </span>
      ))}
    </div>
  );
}

function BrowseTable() {
  const entries = useCatalog((s) => s.contentEntries);
  const loading = useCatalog((s) => s.contentLoading);
  const selectedDiskId = useCatalog((s) => s.selectedDiskId);
  const selectedEntryId = useCatalog((s) => s.selectedEntryId);
  const selectEntry = useCatalog((s) => s.selectEntry);
  const openFolder = useCatalog((s) => s.openFolder);
  const breadcrumb = useCatalog((s) => s.breadcrumb);
  const setError = useCatalog((s) => s.setError);
  const parentRef = useRef<HTMLDivElement>(null);
  const [menu, setMenu] = useState<MenuState | null>(null);
  const { widths, startResize } = useColWidths("diskdex:cols:browse", {
    size: 112,
    modified: 176,
    type: 80,
  });

  const rv = useVirtualizer({
    count: entries.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 30,
    overscan: 25,
  });

  // Navegación con teclado: ↑/↓ mueven la selección (no la ventana) y Enter abre carpeta.
  const selectedIndex = entries.findIndex((e) => e.id === selectedEntryId);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const el = document.activeElement as HTMLElement | null;
      if (el && (el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.isContentEditable)) return;
      if (entries.length === 0) return;
      if (e.key === "Enter") {
        const cur = entries[selectedIndex];
        if (cur?.is_folder) {
          e.preventDefault();
          openFolder(cur);
        }
        return;
      }
      if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
      e.preventDefault();
      const delta = e.key === "ArrowDown" ? 1 : -1;
      const next = selectedIndex < 0 ? 0 : Math.min(entries.length - 1, Math.max(0, selectedIndex + delta));
      const target = entries[next];
      if (target) {
        selectEntry(target.id);
        rv.scrollToIndex(next, { align: "auto" });
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [entries, selectedIndex, selectEntry, openFolder, rv]);

  if (selectedDiskId == null) {
    return (
      <Centered>
        <Folder className="h-10 w-10 text-neutral-700" />
        <p className="text-sm text-neutral-500">Elegí un disco en el panel izquierdo.</p>
      </Centered>
    );
  }

  return (
    <div className="flex h-full flex-col">
      {menu && <RowContextMenu menu={menu} onClose={() => setMenu(null)} />}
      <Breadcrumb />
      {breadcrumb.length === 1 && selectedDiskId != null && <DiskMetaBar diskId={selectedDiskId} />}
      <ResizableHeaderRow
        cols={[
          { key: "name", label: "Nombre", flex: true },
          { key: "size", label: "Tamaño", align: "right" },
          { key: "modified", label: "Modificado" },
          { key: "type", label: "Tipo" },
        ]}
        widths={widths}
        startResize={startResize}
      />
      <div ref={parentRef} className="relative flex-1 overflow-auto">
        {loading && <RowOverlay text="cargando…" />}
        {!loading && entries.length === 0 && <RowOverlay text="carpeta vacía" />}
        <div style={{ height: rv.getTotalSize(), position: "relative" }}>
          {rv.getVirtualItems().map((vi) => {
            const e = entries[vi.index];
            return (
              <div
                key={e.id}
                onClick={() => selectEntry(e.id)}
                onDoubleClick={() => {
                  if (e.is_folder) openFolder(e);
                  else openOriginal(e.id).catch((err) => setError(String(err)));
                }}
                onContextMenu={(ev) => {
                  ev.preventDefault();
                  selectEntry(e.id);
                  setMenu({ x: ev.clientX, y: ev.clientY, id: e.id, isFolder: e.is_folder });
                }}
                className={`absolute left-0 right-0 flex cursor-pointer items-center px-3 text-sm transition-colors duration-150 ${
                  selectedEntryId === e.id
                    ? "bg-primary/10 text-foreground shadow-[inset_2px_0_0_0_hsl(var(--primary))]"
                    : "hover:bg-accent/60"
                }`}
                style={{ height: vi.size, transform: `translateY(${vi.start}px)` }}
              >
                <span className="flex min-w-0 flex-1 items-center gap-2">
                  {e.is_folder ? (
                    <Folder className="h-4 w-4 shrink-0 text-sky-400/80" />
                  ) : (
                    <FileIcon className="h-4 w-4 shrink-0 text-neutral-500" />
                  )}
                  <span className="truncate" title={e.name}>
                    {e.name}
                  </span>
                  {e.is_folder && e.child_count > 0 && (
                    <span className="text-[11px] text-neutral-600">{formatCount(e.child_count)}</span>
                  )}
                </span>
                <span
                  className="shrink-0 text-right font-mono text-xs text-neutral-400"
                  style={{ width: widths.size }}
                >
                  {e.is_folder && e.size_logical === 0 ? "—" : formatBytes(e.size_logical)}
                </span>
                <span
                  className="shrink-0 font-mono text-xs text-neutral-500"
                  style={{ width: widths.modified }}
                >
                  {formatDate(e.modified_at)}
                </span>
                <span
                  className="shrink-0 truncate text-xs text-neutral-500"
                  style={{ width: widths.type }}
                >
                  {e.is_folder ? "carpeta" : e.ext ?? "—"}
                </span>
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}

function SearchTable() {
  const result = useCatalog((s) => s.searchResult);
  const searching = useCatalog((s) => s.searching);
  const selectedEntryId = useCatalog((s) => s.selectedEntryId);
  const selectEntry = useCatalog((s) => s.selectEntry);
  const setError = useCatalog((s) => s.setError);
  const parentRef = useRef<HTMLDivElement>(null);
  const [menu, setMenu] = useState<MenuState | null>(null);
  const items: SearchItem[] = result?.items ?? [];
  const { widths, startResize } = useColWidths("diskdex:cols:search", {
    disk: 128,
    size: 96,
    path: 360,
  });

  const rv = useVirtualizer({
    count: items.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 38,
    overscan: 25,
  });

  // Navegación con teclado por los resultados.
  const selectedIndex = items.findIndex((it) => it.id === selectedEntryId);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const el = document.activeElement as HTMLElement | null;
      if (el && (el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.isContentEditable)) return;
      if (items.length === 0 || (e.key !== "ArrowDown" && e.key !== "ArrowUp")) return;
      e.preventDefault();
      const delta = e.key === "ArrowDown" ? 1 : -1;
      const next = selectedIndex < 0 ? 0 : Math.min(items.length - 1, Math.max(0, selectedIndex + delta));
      const target = items[next];
      if (target) {
        selectEntry(target.id);
        rv.scrollToIndex(next, { align: "auto" });
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [items, selectedIndex, selectEntry, rv]);

  return (
    <div className="flex h-full flex-col">
      {menu && <RowContextMenu menu={menu} onClose={() => setMenu(null)} />}
      <div className="flex flex-wrap items-center gap-x-2 gap-y-1 border-b border-neutral-800 px-3 py-1.5 text-xs text-neutral-400">
        <Search className="h-3.5 w-3.5" />
        {searching ? (
          <span className="flex items-center gap-1.5">
            <Loader2 className="h-3 w-3 animate-spin" /> buscando…
          </span>
        ) : result ? (
          <span>
            <span className="font-mono text-neutral-200">{formatCount(result.total)}</span> resultados
            {result.truncated && (
              <span className="ml-1 text-neutral-600">(primeros {formatCount(items.length)})</span>
            )}
          </span>
        ) : null}
        <FilterChips />
      </div>
      <ResizableHeaderRow
        cols={[
          { key: "name", label: "Nombre", flex: true },
          { key: "disk", label: "Disco" },
          { key: "size", label: "Tamaño", align: "right" },
          { key: "path", label: "Ruta" },
        ]}
        widths={widths}
        startResize={startResize}
      />
      <div ref={parentRef} className="relative flex-1 overflow-auto">
        {!searching && result && items.length === 0 && <RowOverlay text="sin resultados" />}
        <div style={{ height: rv.getTotalSize(), position: "relative" }}>
          {rv.getVirtualItems().map((vi) => {
            const it = items[vi.index];
            return (
              <div
                key={it.id}
                onClick={() => selectEntry(it.id)}
                onDoubleClick={() => {
                  const fn = it.is_folder ? revealOriginal : openOriginal;
                  fn(it.id).catch((err) => setError(String(err)));
                }}
                onContextMenu={(ev) => {
                  ev.preventDefault();
                  selectEntry(it.id);
                  setMenu({ x: ev.clientX, y: ev.clientY, id: it.id, isFolder: it.is_folder });
                }}
                className={`absolute left-0 right-0 flex cursor-pointer items-center px-3 text-sm transition-colors duration-150 ${
                  selectedEntryId === it.id
                    ? "bg-primary/10 text-foreground shadow-[inset_2px_0_0_0_hsl(var(--primary))]"
                    : "hover:bg-accent/60"
                }`}
                style={{ height: vi.size, transform: `translateY(${vi.start}px)` }}
              >
                <span className="flex min-w-0 flex-1 items-center gap-2">
                  {it.is_folder ? (
                    <Folder className="h-4 w-4 shrink-0 text-sky-400/80" />
                  ) : (
                    <FileIcon className="h-4 w-4 shrink-0 text-neutral-500" />
                  )}
                  <span className="truncate" title={it.name}>
                    {it.name}
                  </span>
                </span>
                <span
                  className="shrink-0 truncate text-xs text-neutral-400"
                  style={{ width: widths.disk }}
                >
                  {it.disk_name}
                </span>
                <span
                  className="shrink-0 text-right font-mono text-xs text-neutral-400"
                  style={{ width: widths.size }}
                >
                  {it.is_folder ? "—" : formatBytes(it.size_logical)}
                </span>
                <span
                  className="shrink-0 truncate font-mono text-[11px] text-neutral-500"
                  style={{ width: widths.path }}
                  title={it.path}
                >
                  {it.path}
                </span>
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}

function DiskMetaBar({ diskId }: { diskId: number }) {
  const disk = useCatalog((s) => s.disks.find((d) => d.id === diskId));
  const refreshDisks = useCatalog((s) => s.refreshDisks);
  const [location, setLocation] = useState("");
  const [category, setCategory] = useState("");
  const [comment, setComment] = useState("");
  const [saved, setSaved] = useState(false);

  useEffect(() => {
    setLocation(disk?.location ?? "");
    setCategory(disk?.category ?? "");
    setComment(disk?.comment ?? "");
  }, [diskId, disk?.location, disk?.category, disk?.comment]);

  if (!disk) return null;

  async function save() {
    await api.setDiskMeta(
      diskId,
      location.trim() || null,
      category.trim() || null,
      comment.trim() || null
    );
    await refreshDisks();
    setSaved(true);
    setTimeout(() => setSaved(false), 1200);
  }

  const field = "rounded border border-neutral-700 bg-neutral-900 px-2 py-1 text-xs text-neutral-200 placeholder:text-neutral-600 focus:border-neutral-500 focus:outline-none";

  return (
    <div className="flex flex-wrap items-center gap-2 border-b border-neutral-800 bg-neutral-900/30 px-3 py-1.5">
      <input className={`${field} w-36`} placeholder="Ubicación (caja, estante…)" value={location} onChange={(e) => setLocation(e.target.value)} onBlur={save} />
      <input className={`${field} w-32`} placeholder="Categoría" value={category} onChange={(e) => setCategory(e.target.value)} onBlur={save} />
      <input className={`${field} flex-1`} placeholder="Comentario del disco…" value={comment} onChange={(e) => setComment(e.target.value)} onBlur={save} />
      {saved && <span className="text-[11px] text-emerald-400">guardado</span>}
    </div>
  );
}

function FilterChips() {
  const f = useCatalog((s) => s.parsedFilters);
  if (!f) return null;
  const chips: string[] = [];
  if (f.text) chips.push(`nombre: ${f.text}`);
  if (f.exts.length) chips.push(`ext: ${f.exts.join(", ")}`);
  if (f.tags.length) chips.push(`tag: ${f.tags.join(", ")}`);
  if (f.min_size !== undefined) chips.push(`≥ ${formatBytes(f.min_size)}`);
  if (f.max_size !== undefined) chips.push(`≤ ${formatBytes(f.max_size)}`);
  if (f.modified_after !== undefined) chips.push(`desde ${formatDate(f.modified_after)}`);
  if (f.modified_before !== undefined) chips.push(`hasta ${formatDate(f.modified_before)}`);
  if (f.kind) chips.push(f.kind === "folder" ? "solo carpetas" : "solo archivos");
  if (chips.length === 0) return null;
  return (
    <span className="flex flex-wrap items-center gap-1">
      {chips.map((c) => (
        <span key={c} className="rounded bg-neutral-800 px-1.5 py-0.5 font-mono text-[10px] text-neutral-300">
          {c}
        </span>
      ))}
    </span>
  );
}

function RowOverlay({ text }: { text: string }) {
  return (
    <div className="pointer-events-none absolute inset-0 flex items-start justify-center pt-10 text-sm text-neutral-600">
      {text}
    </div>
  );
}

function Centered({ children }: { children: React.ReactNode }) {
  return <div className="flex h-full flex-col items-center justify-center gap-3 text-center">{children}</div>;
}
