import { useEffect, useMemo, useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import {
  ChevronRight,
  ChevronUp,
  ChevronDown,
  Folder,
  File as FileIcon,
  Search,
  Loader2,
  FolderSearch,
  ExternalLink,
  Copy,
  Trash2,
  Clock,
  Check,
  AlertTriangle,
  HelpCircle,
  Sparkles,
} from "lucide-react";
import { useCatalog } from "../store/catalog";
import { formatBytes, formatDate, formatCount, formatAge, formatDuration } from "../lib/format";
import { api, type SearchItem, type SemanticItem, type EntryRow, type DiskDetail } from "../lib/ipc";
import { revealOriginal, openOriginal, copyText } from "../lib/actions";
import { useT, useI18n } from "../lib/i18n";

/** Firma de la función de traducción (para pasarla a helpers fuera de componentes). */
type TFn = (key: string, vars?: Record<string, string | number>) => string;

// ── Miniaturas para la vista galería ──────────────────────────────────────────
// Caché en memoria (sobrevive cambios de vista/scroll) + límite de concurrencia
// para no disparar decenas de ffmpeg/decodificaciones a la vez al hacer scroll.
const THUMB_CACHE = new Map<number, string>();
let thumbActive = 0;
const thumbQueue: (() => void)[] = [];
const THUMB_CONCURRENCY = 4;

function loadThumb(id: number): Promise<string> {
  const hit = THUMB_CACHE.get(id);
  if (hit) return Promise.resolve(hit);
  return new Promise<string>((resolve, reject) => {
    const run = () => {
      thumbActive++;
      api
        .getThumbnail(id, 220)
        .then((d) => {
          THUMB_CACHE.set(id, d);
          resolve(d);
        })
        .catch(reject)
        .finally(() => {
          thumbActive--;
          const next = thumbQueue.shift();
          if (next) next();
        });
    };
    if (thumbActive < THUMB_CONCURRENCY) run();
    else thumbQueue.push(run);
  });
}

type SortDir = "asc" | "desc";
interface SortState {
  key: string;
  dir: SortDir;
}

/** Estado de orden persistido + toggle (clic en cabecera alterna asc/desc). */
function useSort(storageKey: string, def: SortState) {
  const [sort, setSort] = useState<SortState>(() => {
    try {
      const saved = localStorage.getItem(storageKey);
      if (saved) return { ...def, ...JSON.parse(saved) };
    } catch {
      /* ignore */
    }
    return def;
  });

  useEffect(() => {
    localStorage.setItem(storageKey, JSON.stringify(sort));
  }, [storageKey, sort]);

  const toggle = (key: string) =>
    setSort((s) => (s.key === key ? { key, dir: s.dir === "asc" ? "desc" : "asc" } : { key, dir: "asc" }));

  return { sort, toggle };
}

/** Comparador de entradas en browse: carpetas primero, luego la columna elegida. */
function compareEntries(a: EntryRow, b: EntryRow, sort: SortState): number {
  if (a.is_folder !== b.is_folder) return a.is_folder ? -1 : 1;
  const dir = sort.dir === "asc" ? 1 : -1;
  switch (sort.key) {
    case "size":
      return (a.size_logical - b.size_logical) * dir;
    case "modified":
      return ((a.modified_at ?? 0) - (b.modified_at ?? 0)) * dir;
    case "type":
      return ((a.ext ?? "").localeCompare(b.ext ?? "") || a.name.localeCompare(b.name)) * dir;
    default:
      return a.name.localeCompare(b.name, undefined, { numeric: true, sensitivity: "base" }) * dir;
  }
}

/** Comparador de resultados de búsqueda. */
function compareSearch(a: SearchItem, b: SearchItem, sort: SortState): number {
  const dir = sort.dir === "asc" ? 1 : -1;
  switch (sort.key) {
    case "disk":
      return (a.disk_name.localeCompare(b.disk_name) || a.name.localeCompare(b.name)) * dir;
    case "size":
      return (a.size_logical - b.size_logical) * dir;
    case "path":
      return a.path.localeCompare(b.path) * dir;
    default:
      return a.name.localeCompare(b.name, undefined, { numeric: true, sensitivity: "base" }) * dir;
  }
}

/** Selección con modificadores (Cmd/Ctrl = toggle, Shift = rango) sobre la lista
 *  ordenada visible. `anchorRef` guarda el ancla del último rango/click simple. */
function applyClickSelection<T extends { id: number }>(
  e: React.MouseEvent,
  rows: T[],
  index: number,
  selectedIds: number[],
  anchorRef: React.MutableRefObject<number | null>,
  setSelection: (ids: number[], primary: number | null) => void
) {
  const id = rows[index].id;
  if (e.shiftKey && anchorRef.current != null) {
    const a = rows.findIndex((r) => r.id === anchorRef.current);
    if (a >= 0) {
      const [lo, hi] = a < index ? [a, index] : [index, a];
      setSelection(
        rows.slice(lo, hi + 1).map((r) => r.id),
        id
      );
      return;
    }
  }
  if (e.metaKey || e.ctrlKey) {
    const set = new Set(selectedIds);
    if (set.has(id)) set.delete(id);
    else set.add(id);
    anchorRef.current = id;
    const ids = [...set];
    setSelection(ids, set.has(id) ? id : ids[ids.length - 1] ?? null);
    return;
  }
  anchorRef.current = id;
  setSelection([id], id);
}

/** Mueve un lote (o un único ítem) a la Papelera, con confirmación y recarga. */
export async function trashIds(
  ids: number[],
  reload: () => Promise<void>,
  setError: (e: string) => void,
  t: TFn
) {
  if (ids.length === 0) return;
  let confirmMsg: string;
  if (ids.length === 1) {
    let p = "";
    try {
      p = await api.entryPath(ids[0]);
    } catch {
      /* ignore */
    }
    confirmMsg = t("table.confirmTrashOne", { path: p });
  } else {
    confirmMsg = t("table.confirmTrashMany", { n: ids.length });
  }
  if (!window.confirm(confirmMsg)) return;
  try {
    const res = await api.moveEntriesToTrash(ids);
    await reload();
    if (res.failed.length) {
      setError(
        t("table.trashFailed", {
          n: res.failed.length,
          names: res.failed.map((f) => f.name).join(", "),
        })
      );
    }
  } catch (e) {
    setError(String(e));
  }
}

interface MenuState {
  x: number;
  y: number;
  id: number;
  isFolder: boolean;
}

/** Menú contextual propio (clic derecho) sobre un ítem. */
function RowContextMenu({ menu, onClose }: { menu: MenuState; onClose: () => void }) {
  const t = useT();
  const setError = useCatalog((s) => s.setError);
  const reloadCurrent = useCatalog((s) => s.reloadCurrent);
  const selectedIds = useCatalog((s) => s.selectedIds);
  const aiAvailable = useCatalog((s) => s.aiAvailable);
  const runSimilar = useCatalog((s) => s.runSimilar);

  // El borrado opera sobre la selección si el ítem clickeado forma parte de ella;
  // si no, sobre el ítem solo.
  const batchIds = selectedIds.includes(menu.id) && selectedIds.length > 1 ? selectedIds : [menu.id];

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
      label: t("table.revealInFinder"),
      icon: <FolderSearch className="h-3.5 w-3.5" />,
      fn: () => revealOriginal(menu.id),
    },
    ...(!menu.isFolder
      ? [
          {
            label: t("table.openInSystem"),
            icon: <ExternalLink className="h-3.5 w-3.5" />,
            fn: () => openOriginal(menu.id),
          },
        ]
      : []),
    {
      label: t("table.copyPath"),
      icon: <Copy className="h-3.5 w-3.5" />,
      fn: async () => {
        const p = await api.entryPath(menu.id);
        await copyText(p);
      },
    },
    ...(aiAvailable && !menu.isFolder
      ? [
          {
            label: t("ai.similar"),
            icon: <Sparkles className="h-3.5 w-3.5 text-violet-400" />,
            fn: () => runSimilar(menu.id),
          },
        ]
      : []),
    {
      label:
        batchIds.length > 1
          ? t("table.moveNToTrash", { n: batchIds.length })
          : t("table.moveToTrash"),
      icon: <Trash2 className="h-3.5 w-3.5" />,
      danger: true,
      fn: () => trashIds(batchIds, reloadCurrent, setError, t),
    },
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

  // El handle vive en el borde IZQUIERDO de `rightKey` (el divisor entre dos
  // columnas). `leftKey` es la columna vecina de la izquierda, o null si esa
  // vecina es la flexible ("Nombre", que absorbe el sobrante).
  //  - vecino fijo:  zero-sum → arrastrar derecha agranda el izquierdo y achica
  //    el derecho (nada más se mueve, el divisor sigue al cursor).
  //  - vecino flex:  solo se ajusta el derecho; "Nombre" absorbe la diferencia.
  const clamp = (v: number) => Math.max(48, Math.min(900, v));
  const startResize = (leftKey: string | null, rightKey: string) => (e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    const startX = e.clientX;
    const startRight = widths[rightKey];
    const startLeft = leftKey ? widths[leftKey] : 0;
    const onMove = (ev: MouseEvent) => {
      const dx = ev.clientX - startX;
      setWidths((prev) => {
        const next = { ...prev, [rightKey]: clamp(startRight - dx) };
        if (leftKey) next[leftKey] = clamp(startLeft + dx);
        return next;
      });
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
  sort,
  onSort,
}: {
  cols: ColDef[];
  widths: Record<string, number>;
  startResize: (leftKey: string | null, rightKey: string) => (e: React.MouseEvent) => void;
  sort?: SortState;
  onSort?: (key: string) => void;
}) {
  const t = useT();
  return (
    <div className="flex items-center gap-3 border-b border-neutral-800 bg-neutral-900/40 px-3 py-1.5 text-[11px] font-medium uppercase tracking-wide text-neutral-500">
      {cols.map((c, i) => {
        const active = sort?.key === c.key;
        // El divisor a la izquierda de esta columna ajusta esta columna y su
        // vecina izquierda (zero-sum). Si la vecina es la flexible, va null.
        const prev = i > 0 ? cols[i - 1] : null;
        const leftKey = prev && !prev.flex ? prev.key : null;
        return (
          <div
            key={c.key}
            className={`relative flex items-center ${c.flex ? "min-w-0 flex-1" : ""} ${
              c.align === "right" ? "justify-end" : ""
            }`}
            style={c.flex ? undefined : { width: widths[c.key] }}
          >
            {onSort ? (
              <button
                onClick={() => onSort(c.key)}
                className={`flex min-w-0 items-center gap-1 uppercase tracking-wide transition-colors hover:text-neutral-300 ${
                  active ? "text-neutral-300" : ""
                } ${c.align === "right" ? "flex-row-reverse" : ""}`}
                title={t("table.sortBy")}
              >
                <span className="truncate">{c.label}</span>
                {active &&
                  (sort!.dir === "asc" ? (
                    <ChevronUp className="h-3 w-3 shrink-0" />
                  ) : (
                    <ChevronDown className="h-3 w-3 shrink-0" />
                  ))}
              </button>
            ) : (
              <span className="truncate">{c.label}</span>
            )}
            {!c.flex && (
              <span
                onMouseDown={startResize(leftKey, c.key)}
                title={t("table.resize")}
                className="group absolute left-0 top-0 z-20 flex h-full w-3 -translate-x-1/2 cursor-col-resize items-stretch justify-center"
              >
                {/* Línea divisoria siempre visible; se engrosa/colorea al pasar. */}
                <span className="h-full w-px bg-neutral-700 transition-colors group-hover:w-0.5 group-hover:bg-primary" />
              </span>
            )}
          </div>
        );
      })}
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
  const t = useT();
  const entries = useCatalog((s) => s.contentEntries);
  const loading = useCatalog((s) => s.contentLoading);
  const selectedDiskId = useCatalog((s) => s.selectedDiskId);
  const selectedEntryId = useCatalog((s) => s.selectedEntryId);
  const selectedIds = useCatalog((s) => s.selectedIds);
  const selectEntry = useCatalog((s) => s.selectEntry);
  const setSelection = useCatalog((s) => s.setSelection);
  const reloadCurrent = useCatalog((s) => s.reloadCurrent);
  const openFolder = useCatalog((s) => s.openFolder);
  const breadcrumb = useCatalog((s) => s.breadcrumb);
  const setError = useCatalog((s) => s.setError);
  const parentRef = useRef<HTMLDivElement>(null);
  const anchorRef = useRef<number | null>(null);
  const [menu, setMenu] = useState<MenuState | null>(null);
  const selSet = useMemo(() => new Set(selectedIds), [selectedIds]);
  const viewMode = useCatalog((s) => s.viewMode);
  const { widths, startResize } = useColWidths("diskdex:cols:browse", {
    size: 112,
    modified: 176,
    type: 80,
  });
  const { sort, toggle } = useSort("diskdex:sort:browse", { key: "name", dir: "asc" });

  const rows = useMemo(
    () => [...entries].sort((a, b) => compareEntries(a, b, sort)),
    [entries, sort]
  );

  const rv = useVirtualizer({
    count: rows.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 30,
    overscan: 25,
  });

  // Navegación con teclado: ↑/↓ mueven la selección (no la ventana) y Enter abre carpeta.
  const selectedIndex = rows.findIndex((e) => e.id === selectedEntryId);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const el = document.activeElement as HTMLElement | null;
      if (el && (el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.isContentEditable)) return;
      if (rows.length === 0) return;
      // Cmd/Ctrl+Borrar → mover la selección a la Papelera (estilo Finder).
      if ((e.key === "Backspace" || e.key === "Delete") && (e.metaKey || e.ctrlKey)) {
        e.preventDefault();
        const ids = selectedIds.length ? selectedIds : selectedEntryId != null ? [selectedEntryId] : [];
        if (ids.length) trashIds(ids, reloadCurrent, setError, t);
        return;
      }
      // Cmd/Ctrl+A → seleccionar todo lo visible.
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "a") {
        e.preventDefault();
        setSelection(rows.map((r) => r.id), rows[rows.length - 1]?.id ?? null);
        return;
      }
      if (e.key === "Enter") {
        const cur = rows[selectedIndex];
        if (cur?.is_folder) {
          e.preventDefault();
          openFolder(cur);
        }
        return;
      }
      if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
      e.preventDefault();
      const delta = e.key === "ArrowDown" ? 1 : -1;
      const next = selectedIndex < 0 ? 0 : Math.min(rows.length - 1, Math.max(0, selectedIndex + delta));
      const target = rows[next];
      if (!target) return;
      if (e.shiftKey) {
        // Extender el rango desde el ancla hasta la nueva posición.
        if (anchorRef.current == null) anchorRef.current = rows[selectedIndex]?.id ?? target.id;
        const a = rows.findIndex((r) => r.id === anchorRef.current);
        const [lo, hi] = a < 0 ? [next, next] : a < next ? [a, next] : [next, a];
        setSelection(rows.slice(lo, hi + 1).map((r) => r.id), target.id);
      } else {
        anchorRef.current = target.id;
        selectEntry(target.id);
      }
      rv.scrollToIndex(next, { align: "auto" });
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [rows, selectedIndex, selectEntry, setSelection, selectedIds, selectedEntryId, reloadCurrent, setError, openFolder, rv, t]);

  if (selectedDiskId == null) {
    return (
      <Centered>
        <Folder className="h-10 w-10 text-neutral-700" />
        <p className="text-sm text-neutral-500">{t("table.pickDisk")}</p>
      </Centered>
    );
  }

  return (
    <div className="flex h-full flex-col">
      {menu && <RowContextMenu menu={menu} onClose={() => setMenu(null)} />}
      <Breadcrumb />
      {breadcrumb.length === 1 && selectedDiskId != null && (
        <>
          <DiskInfoPanel diskId={selectedDiskId} />
          <DiskMetaBar diskId={selectedDiskId} />
        </>
      )}
      {viewMode === "grid" ? (
        <GalleryGrid
          items={rows}
          selectedIds={selectedIds}
          selectedEntryId={selectedEntryId}
          onPick={(e, i) => applyClickSelection(e, rows, i, selectedIds, anchorRef, setSelection)}
          onOpen={(it) => {
            if (it.is_folder) openFolder(it);
            else openOriginal(it.id).catch((err) => setError(String(err)));
          }}
          onMenu={(e, it) => {
            e.preventDefault();
            if (!selSet.has(it.id)) {
              anchorRef.current = it.id;
              setSelection([it.id], it.id);
            }
            setMenu({ x: e.clientX, y: e.clientY, id: it.id, isFolder: it.is_folder });
          }}
          empty={loading ? t("common.loading") : t("table.emptyFolder")}
        />
      ) : (
        <>
      <ResizableHeaderRow
        cols={[
          { key: "name", label: t("table.colName"), flex: true },
          { key: "size", label: t("table.colSize"), align: "right" },
          { key: "modified", label: t("table.colModified") },
          { key: "type", label: t("table.colType") },
        ]}
        widths={widths}
        startResize={startResize}
        sort={sort}
        onSort={toggle}
      />
      <div ref={parentRef} className="relative flex-1 overflow-auto">
        {loading && <RowOverlay text={t("common.loading")} />}
        {!loading && rows.length === 0 && <RowOverlay text={t("table.emptyFolder")} />}
        <div style={{ height: rv.getTotalSize(), position: "relative" }}>
          {rv.getVirtualItems().map((vi) => {
            const e = rows[vi.index];
            const isSel = selSet.has(e.id);
            return (
              <div
                key={e.id}
                onClick={(ev) => applyClickSelection(ev, rows, vi.index, selectedIds, anchorRef, setSelection)}
                onDoubleClick={() => {
                  if (e.is_folder) openFolder(e);
                  else openOriginal(e.id).catch((err) => setError(String(err)));
                }}
                onContextMenu={(ev) => {
                  ev.preventDefault();
                  if (!selSet.has(e.id)) {
                    anchorRef.current = e.id;
                    setSelection([e.id], e.id);
                  }
                  setMenu({ x: ev.clientX, y: ev.clientY, id: e.id, isFolder: e.is_folder });
                }}
                className={`absolute left-0 right-0 flex cursor-pointer items-center gap-3 px-3 text-sm transition-colors duration-150 ${
                  isSel
                    ? `bg-primary/10 text-foreground${
                        selectedEntryId === e.id ? " shadow-[inset_2px_0_0_0_hsl(var(--primary))]" : ""
                      }`
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
                  {e.is_folder ? t("common.folderWord") : e.ext ?? "—"}
                </span>
              </div>
            );
          })}
        </div>
      </div>
        </>
      )}
    </div>
  );
}

function SearchTable() {
  const t = useT();
  const result = useCatalog((s) => s.searchResult);
  const searching = useCatalog((s) => s.searching);
  const selectedEntryId = useCatalog((s) => s.selectedEntryId);
  const selectedIds = useCatalog((s) => s.selectedIds);
  const selectEntry = useCatalog((s) => s.selectEntry);
  const setSelection = useCatalog((s) => s.setSelection);
  const reloadCurrent = useCatalog((s) => s.reloadCurrent);
  const setError = useCatalog((s) => s.setError);
  const parentRef = useRef<HTMLDivElement>(null);
  const anchorRef = useRef<number | null>(null);
  const [menu, setMenu] = useState<MenuState | null>(null);
  const selSet = useMemo(() => new Set(selectedIds), [selectedIds]);
  const viewMode = useCatalog((s) => s.viewMode);
  const rawItems: SearchItem[] = result?.items ?? [];
  const { widths, startResize } = useColWidths("diskdex:cols:search", {
    disk: 128,
    size: 96,
    path: 360,
  });
  const { sort, toggle } = useSort("diskdex:sort:search", { key: "name", dir: "asc" });

  const items = useMemo(
    () => [...rawItems].sort((a, b) => compareSearch(a, b, sort)),
    [rawItems, sort]
  );

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
      if (items.length === 0) return;
      if ((e.key === "Backspace" || e.key === "Delete") && (e.metaKey || e.ctrlKey)) {
        e.preventDefault();
        const ids = selectedIds.length ? selectedIds : selectedEntryId != null ? [selectedEntryId] : [];
        if (ids.length) trashIds(ids, reloadCurrent, setError, t);
        return;
      }
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "a") {
        e.preventDefault();
        setSelection(items.map((r) => r.id), items[items.length - 1]?.id ?? null);
        return;
      }
      if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
      e.preventDefault();
      const delta = e.key === "ArrowDown" ? 1 : -1;
      const next = selectedIndex < 0 ? 0 : Math.min(items.length - 1, Math.max(0, selectedIndex + delta));
      const target = items[next];
      if (!target) return;
      if (e.shiftKey) {
        if (anchorRef.current == null) anchorRef.current = items[selectedIndex]?.id ?? target.id;
        const a = items.findIndex((r) => r.id === anchorRef.current);
        const [lo, hi] = a < 0 ? [next, next] : a < next ? [a, next] : [next, a];
        setSelection(items.slice(lo, hi + 1).map((r) => r.id), target.id);
      } else {
        anchorRef.current = target.id;
        selectEntry(target.id);
      }
      rv.scrollToIndex(next, { align: "auto" });
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [items, selectedIndex, selectEntry, setSelection, selectedIds, selectedEntryId, reloadCurrent, setError, rv, t]);

  return (
    <div className="flex h-full flex-col">
      {menu && <RowContextMenu menu={menu} onClose={() => setMenu(null)} />}
      <div className="flex flex-wrap items-center gap-x-2 gap-y-1 border-b border-neutral-800 px-3 py-1.5 text-xs text-neutral-400">
        <Search className="h-3.5 w-3.5" />
        {searching ? (
          <span className="flex items-center gap-1.5">
            <Loader2 className="h-3 w-3 animate-spin" /> {t("common.searching")}
          </span>
        ) : result ? (
          <span>
            {t("table.results", { n: formatCount(result.total) })}
            {result.truncated && (
              <span className="ml-1 text-neutral-600">{t("table.firstN", { n: formatCount(items.length) })}</span>
            )}
          </span>
        ) : null}
        <FilterChips />
      </div>
      {viewMode === "grid" ? (
        <GalleryGrid
          items={items}
          selectedIds={selectedIds}
          selectedEntryId={selectedEntryId}
          onPick={(e, i) => applyClickSelection(e, items, i, selectedIds, anchorRef, setSelection)}
          onOpen={(it) => {
            const fn = it.is_folder ? revealOriginal : openOriginal;
            fn(it.id).catch((err) => setError(String(err)));
          }}
          onMenu={(e, it) => {
            e.preventDefault();
            if (!selSet.has(it.id)) {
              anchorRef.current = it.id;
              setSelection([it.id], it.id);
            }
            setMenu({ x: e.clientX, y: e.clientY, id: it.id, isFolder: it.is_folder });
          }}
          empty={!searching && result ? t("table.noResults") : undefined}
        />
      ) : (
        <>
      <ResizableHeaderRow
        cols={[
          { key: "name", label: t("table.colName"), flex: true },
          { key: "disk", label: t("table.colDisk") },
          { key: "size", label: t("table.colSize"), align: "right" },
          { key: "path", label: t("table.colPath") },
        ]}
        widths={widths}
        startResize={startResize}
        sort={sort}
        onSort={toggle}
      />
      <div ref={parentRef} className="relative flex-1 overflow-auto">
        {!searching && result && items.length === 0 && <RowOverlay text={t("table.noResults")} />}
        <div style={{ height: rv.getTotalSize(), position: "relative" }}>
          {rv.getVirtualItems().map((vi) => {
            const it = items[vi.index];
            const isSel = selSet.has(it.id);
            return (
              <div
                key={it.id}
                onClick={(ev) => applyClickSelection(ev, items, vi.index, selectedIds, anchorRef, setSelection)}
                onDoubleClick={() => {
                  const fn = it.is_folder ? revealOriginal : openOriginal;
                  fn(it.id).catch((err) => setError(String(err)));
                }}
                onContextMenu={(ev) => {
                  ev.preventDefault();
                  if (!selSet.has(it.id)) {
                    anchorRef.current = it.id;
                    setSelection([it.id], it.id);
                  }
                  setMenu({ x: ev.clientX, y: ev.clientY, id: it.id, isFolder: it.is_folder });
                }}
                className={`absolute left-0 right-0 flex cursor-pointer items-center gap-3 px-3 text-sm transition-colors duration-150 ${
                  isSel
                    ? `bg-primary/10 text-foreground${
                        selectedEntryId === it.id ? " shadow-[inset_2px_0_0_0_hsl(var(--primary))]" : ""
                      }`
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
                  {(it as SemanticItem).frame_ts != null && (
                    <span
                      className="shrink-0 rounded bg-violet-600/20 px-1 font-mono text-[10px] text-violet-300"
                      title={t("ai.momentTip")}
                    >
                      ▶ {formatDuration((it as SemanticItem).frame_ts! * 1000)}
                    </span>
                  )}
                  {(it as SemanticItem).snippet && (
                    <span
                      className="min-w-0 flex-1 truncate text-[11px] italic text-violet-300/80"
                      title={(it as SemanticItem).snippet!}
                    >
                      💬 {(it as SemanticItem).snippet}
                    </span>
                  )}
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
        </>
      )}
    </div>
  );
}

/** Panel de info del disco (al pararse en su raíz): fecha de escaneo, si está
 *  al día y uso de espacio. Usa datos en vivo si el disco está montado. */
function DiskInfoPanel({ diskId }: { diskId: number }) {
  const t = useT();
  const lang = useI18n((s) => s.lang);
  const [d, setD] = useState<DiskDetail | null>(null);

  useEffect(() => {
    let alive = true;
    setD(null);
    api
      .diskDetail(diskId)
      .then((x) => alive && setD(x))
      .catch(() => alive && setD(null));
    return () => {
      alive = false;
    };
  }, [diskId]);

  if (!d) return null;

  // Espacio: preferir lo real (disco montado), si no, lo cataloga.
  const total = d.live_total ?? d.capacity ?? null;
  const free = d.live_free ?? (d.capacity != null ? d.capacity - d.total_size : null);
  const used = total != null && free != null ? total - free : d.total_size;
  const pct = total && total > 0 ? Math.min(100, Math.round((used / total) * 100)) : null;

  // Heurística "up-to-date": comparar el uso real del volumen con lo catalogado.
  let status: { label: string; tone: string; icon: "ok" | "warn" | "unknown"; title?: string };
  if (!d.is_online) {
    status = {
      label: t("table.unverified"),
      tone: "text-neutral-500",
      icon: "unknown",
      title: t("table.unverifiedTip"),
    };
  } else if (d.live_total != null) {
    const liveUsed = d.live_total - (d.live_free ?? 0);
    const tol = Math.max(5 * 1024 ** 3, d.live_total * 0.15);
    if (Math.abs(liveUsed - d.total_size) <= tol) {
      status = {
        label: t("table.upToDate"),
        tone: "text-emerald-400",
        icon: "ok",
        title: t("table.upToDateTip"),
      };
    } else {
      status = {
        label: t("table.maybeStale"),
        tone: "text-amber-400",
        icon: "warn",
        title: t("table.maybeStaleTip"),
      };
    }
  } else {
    status = { label: t("table.mounted"), tone: "text-emerald-400", icon: "ok" };
  }

  return (
    <div className="border-b border-neutral-800 bg-neutral-900/30 px-3 py-2">
      <div className="flex flex-wrap items-center gap-x-4 gap-y-1 text-xs">
        <span className="flex items-center gap-1.5">
          <span className={`h-2 w-2 rounded-full ${d.is_online ? "bg-emerald-500" : "bg-neutral-600"}`} />
          <span className="text-neutral-400">{d.is_online ? t("common.online") : t("common.offline")}</span>
        </span>
        <span className="flex items-center gap-1.5 text-neutral-400" title={formatDate(d.scanned_at)}>
          <Clock className="h-3.5 w-3.5 text-neutral-500" />
          {t("table.scannedAgo", { age: formatAge(d.scanned_at, lang) })}
        </span>
        <span className={`flex items-center gap-1 ${status.tone}`} title={status.title}>
          {status.icon === "warn" ? (
            <AlertTriangle className="h-3.5 w-3.5" />
          ) : status.icon === "ok" ? (
            <Check className="h-3.5 w-3.5" />
          ) : (
            <HelpCircle className="h-3.5 w-3.5" />
          )}
          {status.label}
        </span>
        <span className="text-neutral-500">
          {t("table.counts", {
            files: formatCount(d.file_count),
            folders: formatCount(d.folder_count),
          })}
        </span>
      </div>
      {total != null && (
        <div className="mt-1.5 flex items-center gap-2">
          <div className="h-1.5 flex-1 overflow-hidden rounded-full bg-neutral-800">
            <div
              className={`h-full rounded-full ${pct != null && pct > 90 ? "bg-amber-500" : "bg-primary"}`}
              style={{ width: `${pct ?? 0}%` }}
            />
          </div>
          <span className="shrink-0 font-mono text-[11px] text-neutral-400">
            {free != null
              ? t("table.spaceLineFree", {
                  used: formatBytes(used),
                  free: formatBytes(free),
                  total: formatBytes(total),
                })
              : t("table.spaceLineNoFree", { used: formatBytes(used), total: formatBytes(total) })}
          </span>
        </div>
      )}
    </div>
  );
}

function DiskMetaBar({ diskId }: { diskId: number }) {
  const t = useT();
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
      <input className={`${field} w-36`} placeholder={t("table.locationPh")} value={location} onChange={(e) => setLocation(e.target.value)} onBlur={save} />
      <input className={`${field} w-32`} placeholder={t("table.categoryPh")} value={category} onChange={(e) => setCategory(e.target.value)} onBlur={save} />
      <input className={`${field} flex-1`} placeholder={t("table.diskCommentPh")} value={comment} onChange={(e) => setComment(e.target.value)} onBlur={save} />
      {saved && <span className="text-[11px] text-emerald-400">{t("common.saved")}</span>}
    </div>
  );
}

function FilterChips() {
  const t = useT();
  const f = useCatalog((s) => s.parsedFilters);
  if (!f) return null;
  const chips: string[] = [];
  if (f.text) chips.push(t("table.chipName", { x: f.text }));
  if (f.exts.length) chips.push(t("table.chipExt", { x: f.exts.join(", ") }));
  if (f.tags.length) chips.push(t("table.chipTag", { x: f.tags.join(", ") }));
  if (f.min_size !== undefined) chips.push(t("table.chipMin", { x: formatBytes(f.min_size) }));
  if (f.max_size !== undefined) chips.push(t("table.chipMax", { x: formatBytes(f.max_size) }));
  if (f.modified_after !== undefined) chips.push(t("table.chipAfter", { x: formatDate(f.modified_after) }));
  if (f.modified_before !== undefined) chips.push(t("table.chipBefore", { x: formatDate(f.modified_before) }));
  if (f.kind) chips.push(f.kind === "folder" ? t("table.chipFolders") : t("table.chipFiles"));
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

// ── Vista galería (grilla de miniaturas, virtualizada por filas) ───────────────
type GalleryItem = { id: number; name: string; is_folder: boolean };

function GalleryGrid<T extends GalleryItem>({
  items,
  selectedIds,
  selectedEntryId,
  onPick,
  onOpen,
  onMenu,
  empty,
}: {
  items: T[];
  selectedIds: number[];
  selectedEntryId: number | null;
  onPick: (e: React.MouseEvent, index: number) => void;
  onOpen: (item: T) => void;
  onMenu: (e: React.MouseEvent, item: T) => void;
  empty?: string;
}) {
  const parentRef = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(0);
  useEffect(() => {
    const el = parentRef.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setWidth(el.clientWidth));
    ro.observe(el);
    setWidth(el.clientWidth);
    return () => ro.disconnect();
  }, []);
  const selSet = useMemo(() => new Set(selectedIds), [selectedIds]);

  const CELL = 156;
  const GAP = 10;
  const PAD = 12;
  const cols = Math.max(1, Math.floor((width - PAD * 2 + GAP) / (CELL + GAP)));
  const rowH = CELL + 28 + GAP; // miniatura + etiqueta + separación
  const rowCount = Math.ceil(items.length / cols);
  const rv = useVirtualizer({
    count: rowCount,
    getScrollElement: () => parentRef.current,
    estimateSize: () => rowH,
    overscan: 3,
  });

  return (
    <div ref={parentRef} className="relative flex-1 overflow-auto py-2">
      {items.length === 0 && empty && <RowOverlay text={empty} />}
      <div style={{ height: rv.getTotalSize(), position: "relative" }}>
        {rv.getVirtualItems().map((vr) => {
          const start = vr.index * cols;
          const rowItems = items.slice(start, start + cols);
          return (
            <div
              key={vr.key}
              className="absolute left-0 right-0 flex gap-2.5 px-3"
              style={{ height: rowH, transform: `translateY(${vr.start}px)` }}
            >
              {rowItems.map((it, ci) => (
                <GalleryCell
                  key={it.id}
                  item={it}
                  width={CELL}
                  selected={selSet.has(it.id)}
                  primary={selectedEntryId === it.id}
                  onClick={(e) => onPick(e, start + ci)}
                  onOpen={() => onOpen(it)}
                  onMenu={(e) => onMenu(e, it)}
                />
              ))}
            </div>
          );
        })}
      </div>
    </div>
  );
}

function GalleryCell({
  item,
  width,
  selected,
  primary,
  onClick,
  onOpen,
  onMenu,
}: {
  item: GalleryItem;
  width: number;
  selected: boolean;
  primary: boolean;
  onClick: (e: React.MouseEvent) => void;
  onOpen: () => void;
  onMenu: (e: React.MouseEvent) => void;
}) {
  return (
    <div
      style={{ width }}
      onClick={onClick}
      onDoubleClick={onOpen}
      onContextMenu={onMenu}
      className={`flex cursor-pointer flex-col gap-1 rounded-lg p-1.5 transition-colors ${
        selected ? "bg-primary/15 ring-1 ring-primary/40" : "hover:bg-accent/50"
      }`}
    >
      <div className="relative aspect-square w-full overflow-hidden rounded-md bg-neutral-900/80 ring-1 ring-neutral-800">
        <Thumb item={item} />
      </div>
      <span
        className={`truncate px-0.5 text-[11px] ${primary ? "text-foreground" : "text-neutral-400"}`}
        title={item.name}
      >
        {item.name}
      </span>
    </div>
  );
}

function Thumb({ item }: { item: GalleryItem }) {
  const [src, setSrc] = useState<string | null>(() => THUMB_CACHE.get(item.id) ?? null);
  const [failed, setFailed] = useState(false);
  useEffect(() => {
    if (item.is_folder) return;
    const cached = THUMB_CACHE.get(item.id);
    if (cached) {
      setSrc(cached);
      return;
    }
    let alive = true;
    setSrc(null);
    setFailed(false);
    loadThumb(item.id)
      .then((d) => alive && setSrc(d))
      .catch(() => alive && setFailed(true));
    return () => {
      alive = false;
    };
  }, [item.id, item.is_folder]);

  if (item.is_folder)
    return (
      <div className="grid h-full place-items-center">
        <Folder className="h-12 w-12 text-sky-400/70" />
      </div>
    );
  if (src) return <img src={src} alt="" loading="lazy" className="h-full w-full object-cover" />;
  return (
    <div className="grid h-full place-items-center text-neutral-700">
      {failed ? (
        <FileIcon className="h-9 w-9" />
      ) : (
        <Loader2 className="h-5 w-5 animate-spin text-neutral-600" />
      )}
    </div>
  );
}
