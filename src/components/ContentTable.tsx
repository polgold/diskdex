import { useEffect, useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { ChevronRight, Folder, File as FileIcon, Search, Loader2 } from "lucide-react";
import { useCatalog } from "../store/catalog";
import { formatBytes, formatDate, formatCount } from "../lib/format";
import { api, type SearchItem } from "../lib/ipc";

export function ContentTable() {
  const mode = useCatalog((s) => s.mode);
  return mode === "search" ? <SearchTable /> : <BrowseTable />;
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
  const parentRef = useRef<HTMLDivElement>(null);

  const rv = useVirtualizer({
    count: entries.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 30,
    overscan: 25,
  });

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
      <Breadcrumb />
      {breadcrumb.length === 1 && selectedDiskId != null && <DiskMetaBar diskId={selectedDiskId} />}
      <HeaderRow
        cols={[
          ["Nombre", "flex-1"],
          ["Tamaño", "w-28 text-right"],
          ["Modificado", "w-44"],
          ["Tipo", "w-20"],
        ]}
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
                onDoubleClick={() => e.is_folder && openFolder(e)}
                className={`absolute left-0 right-0 flex items-center px-3 text-sm ${
                  selectedEntryId === e.id ? "bg-sky-950/40" : "hover:bg-neutral-800/40"
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
                <span className="w-28 text-right font-mono text-xs text-neutral-400">
                  {e.is_folder && e.size_logical === 0 ? "—" : formatBytes(e.size_logical)}
                </span>
                <span className="w-44 font-mono text-xs text-neutral-500">{formatDate(e.modified_at)}</span>
                <span className="w-20 truncate text-xs text-neutral-500">
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
  const parentRef = useRef<HTMLDivElement>(null);
  const items: SearchItem[] = result?.items ?? [];

  const rv = useVirtualizer({
    count: items.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 38,
    overscan: 25,
  });

  return (
    <div className="flex h-full flex-col">
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
      <HeaderRow
        cols={[
          ["Nombre", "flex-1"],
          ["Disco", "w-32"],
          ["Tamaño", "w-24 text-right"],
          ["Ruta", "w-[40%]"],
        ]}
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
                className={`absolute left-0 right-0 flex items-center px-3 text-sm ${
                  selectedEntryId === it.id ? "bg-sky-950/40" : "hover:bg-neutral-800/40"
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
                <span className="w-32 truncate text-xs text-neutral-400">{it.disk_name}</span>
                <span className="w-24 text-right font-mono text-xs text-neutral-400">
                  {it.is_folder ? "—" : formatBytes(it.size_logical)}
                </span>
                <span className="w-[40%] truncate font-mono text-[11px] text-neutral-500" title={it.path}>
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

function HeaderRow({ cols }: { cols: [string, string][] }) {
  return (
    <div className="flex items-center border-b border-neutral-800 bg-neutral-900/40 px-3 py-1.5 text-[11px] font-medium uppercase tracking-wide text-neutral-500">
      {cols.map(([label, cls]) => (
        <span key={label} className={cls}>
          {label}
        </span>
      ))}
    </div>
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
