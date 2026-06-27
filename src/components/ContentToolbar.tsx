import { useState } from "react";
import {
  Download,
  BarChart3,
  Copy,
  ChevronDown,
  Image as ImageIcon,
  Film,
  Music,
  FileText,
  Package,
} from "lucide-react";
import { useCatalog } from "../store/catalog";
import { exportRows, type ExportRow, type ExportFormat } from "../lib/export";
import { FILE_CATEGORIES } from "../lib/query-parser";
import { StatsDialog } from "./StatsDialog";
import { DuplicatesDialog } from "./DuplicatesDialog";

const CATEGORY_ICONS: Record<string, React.ReactNode> = {
  imagen: <ImageIcon className="h-3.5 w-3.5" />,
  video: <Film className="h-3.5 w-3.5" />,
  audio: <Music className="h-3.5 w-3.5" />,
  documento: <FileText className="h-3.5 w-3.5" />,
  comprimido: <Package className="h-3.5 w-3.5" />,
};

/** Chips de filtro por tipo de archivo (estilo Dropbox). */
function CategoryChips() {
  const runSearch = useCatalog((s) => s.runSearch);
  const clearSearch = useCatalog((s) => s.clearSearch);
  const searchQuery = useCatalog((s) => s.searchQuery);
  return (
    <div className="flex flex-wrap items-center gap-1">
      {Object.entries(FILE_CATEGORIES).map(([key, c]) => {
        const active = searchQuery.trim() === `cat:${key}`;
        return (
          <button
            key={key}
            onClick={() => (active ? clearSearch() : runSearch(`cat:${key}`))}
            className={`inline-flex items-center gap-1 rounded-full border px-2 py-1 text-[11px] font-medium transition-colors ${
              active
                ? "border-primary/40 bg-primary/15 text-primary"
                : "border-border text-neutral-400 hover:bg-accent/60 hover:text-neutral-200"
            }`}
          >
            {CATEGORY_ICONS[key]}
            {c.label}
          </button>
        );
      })}
    </div>
  );
}

const FORMATS: { f: ExportFormat; label: string }[] = [
  { f: "csv", label: "CSV" },
  { f: "tsv", label: "TSV" },
  { f: "json", label: "JSON" },
  { f: "html", label: "HTML / PDF" },
];

function extOf(name: string): string {
  const i = name.lastIndexOf(".");
  return i > 0 ? name.slice(i + 1).toLowerCase() : "archivo";
}

export function ContentToolbar() {
  const mode = useCatalog((s) => s.mode);
  const contentEntries = useCatalog((s) => s.contentEntries);
  const breadcrumb = useCatalog((s) => s.breadcrumb);
  const searchResult = useCatalog((s) => s.searchResult);
  const setError = useCatalog((s) => s.setError);

  const [menuOpen, setMenuOpen] = useState(false);
  const [stats, setStats] = useState(false);
  const [dups, setDups] = useState(false);

  function buildRows(): { rows: ExportRow[]; name: string } {
    if (mode === "search") {
      const rows = (searchResult?.items ?? []).map<ExportRow>((it) => ({
        disk: it.disk_name,
        path: it.path,
        name: it.name,
        type: it.is_folder ? "carpeta" : extOf(it.name),
        size: it.size_logical,
        modified: it.modified_at,
      }));
      return { rows, name: "busqueda" };
    }
    const base = breadcrumb.map((c) => c.name).join("/");
    const rows = contentEntries.map<ExportRow>((e) => ({
      disk: breadcrumb[0]?.name ?? "",
      path: `/${base}/${e.name}`,
      name: e.name,
      type: e.is_folder ? "carpeta" : e.ext ?? "archivo",
      size: e.size_logical,
      modified: e.modified_at,
    }));
    return { rows, name: breadcrumb[breadcrumb.length - 1]?.name ?? "export" };
  }

  async function doExport(f: ExportFormat) {
    setMenuOpen(false);
    const { rows, name } = buildRows();
    if (rows.length === 0) {
      setError("No hay filas para exportar en la vista actual.");
      return;
    }
    try {
      await exportRows(rows, f, name, `DiskDex — ${name}`);
    } catch (e) {
      setError(String(e));
    }
  }

  const count = mode === "search" ? searchResult?.items.length ?? 0 : contentEntries.length;

  return (
    <div className="flex flex-wrap items-center gap-1.5 border-b border-neutral-800 px-3 py-1.5">
      <div className="relative">
        <button
          onClick={() => setMenuOpen((v) => !v)}
          onBlur={() => setTimeout(() => setMenuOpen(false), 150)}
          disabled={count === 0}
          className="inline-flex items-center gap-1 rounded border border-neutral-700 px-2 py-1 text-xs hover:bg-neutral-800 disabled:opacity-40"
          title="Exportar la vista actual"
        >
          <Download className="h-3.5 w-3.5" /> Exportar <ChevronDown className="h-3 w-3" />
        </button>
        {menuOpen && (
          <div className="absolute left-0 top-full z-20 mt-1 w-32 overflow-hidden rounded-md border border-neutral-700 bg-neutral-900 shadow-xl">
            {FORMATS.map((o) => (
              <button
                key={o.f}
                onMouseDown={() => doExport(o.f)}
                className="block w-full px-3 py-1.5 text-left text-xs hover:bg-neutral-800"
              >
                {o.label}
              </button>
            ))}
          </div>
        )}
      </div>

      <button
        onClick={() => setStats(true)}
        className="inline-flex items-center gap-1 rounded border border-neutral-700 px-2 py-1 text-xs hover:bg-neutral-800"
      >
        <BarChart3 className="h-3.5 w-3.5" /> Estadísticas
      </button>
      <button
        onClick={() => setDups(true)}
        className="inline-flex items-center gap-1 rounded border border-neutral-700 px-2 py-1 text-xs hover:bg-neutral-800"
      >
        <Copy className="h-3.5 w-3.5" /> Duplicados
      </button>

      <div className="mx-1 h-4 w-px bg-neutral-800" />
      <CategoryChips />

      <span className="ml-auto text-[11px] text-neutral-600">
        {count > 0 && `${count.toLocaleString()} filas en la vista`}
      </span>

      {stats && <StatsDialog onClose={() => setStats(false)} />}
      {dups && <DuplicatesDialog onClose={() => setDups(false)} />}
    </div>
  );
}
