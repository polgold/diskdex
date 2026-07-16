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
  Trash2,
  List,
  LayoutGrid,
  GitCompareArrows,
} from "lucide-react";
import { useCatalog } from "../store/catalog";
import { useT } from "../lib/i18n";
import { exportRows, type ExportRow, type ExportFormat } from "../lib/export";
import { FILE_CATEGORIES } from "../lib/query-parser";
import { StatsDialog } from "./StatsDialog";
import { DuplicatesDialog } from "./DuplicatesDialog";
import { CompareDialog } from "./CompareDialog";
import { trashIds } from "./ContentTable";

const CATEGORY_ICONS: Record<string, React.ReactNode> = {
  imagen: <ImageIcon className="h-3.5 w-3.5" />,
  video: <Film className="h-3.5 w-3.5" />,
  audio: <Music className="h-3.5 w-3.5" />,
  documento: <FileText className="h-3.5 w-3.5" />,
  comprimido: <Package className="h-3.5 w-3.5" />,
};

/** Chips de filtro por tipo de archivo (estilo Dropbox). */
function CategoryChips() {
  const t = useT();
  const runSearch = useCatalog((s) => s.runSearch);
  const clearSearch = useCatalog((s) => s.clearSearch);
  const searchQuery = useCatalog((s) => s.searchQuery);
  return (
    <div className="flex flex-wrap items-center gap-1">
      {Object.entries(FILE_CATEGORIES).map(([key]) => {
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
            {t(`toolbar.category_${key}`)}
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

function extOf(name: string, fallback: string): string {
  const i = name.lastIndexOf(".");
  return i > 0 ? name.slice(i + 1).toLowerCase() : fallback;
}

export function ContentToolbar() {
  const t = useT();
  const mode = useCatalog((s) => s.mode);
  const contentEntries = useCatalog((s) => s.contentEntries);
  const breadcrumb = useCatalog((s) => s.breadcrumb);
  const searchResult = useCatalog((s) => s.searchResult);
  const setError = useCatalog((s) => s.setError);
  const selectedIds = useCatalog((s) => s.selectedIds);
  const reloadCurrent = useCatalog((s) => s.reloadCurrent);
  const viewMode = useCatalog((s) => s.viewMode);
  const setViewMode = useCatalog((s) => s.setViewMode);

  const [menuOpen, setMenuOpen] = useState(false);
  const [stats, setStats] = useState(false);
  const [dups, setDups] = useState(false);
  const [compare, setCompare] = useState(false);

  function buildRows(): { rows: ExportRow[]; name: string } {
    if (mode === "search") {
      const rows = (searchResult?.items ?? []).map<ExportRow>((it) => ({
        disk: it.disk_name,
        path: it.path,
        name: it.name,
        type: it.is_folder ? t("toolbar.typeFolder") : extOf(it.name, t("toolbar.typeFile")),
        size: it.size_logical,
        modified: it.modified_at,
      }));
      return { rows, name: t("toolbar.exportNameSearch") };
    }
    const base = breadcrumb.map((c) => c.name).join("/");
    const rows = contentEntries.map<ExportRow>((e) => ({
      disk: breadcrumb[0]?.name ?? "",
      path: `/${base}/${e.name}`,
      name: e.name,
      type: e.is_folder ? t("toolbar.typeFolder") : e.ext ?? t("toolbar.typeFile"),
      size: e.size_logical,
      modified: e.modified_at,
    }));
    return { rows, name: breadcrumb[breadcrumb.length - 1]?.name ?? "export" };
  }

  async function doExport(f: ExportFormat) {
    setMenuOpen(false);
    const { rows, name } = buildRows();
    if (rows.length === 0) {
      setError(t("toolbar.exportEmpty"));
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
          title={t("toolbar.exportTitle")}
        >
          <Download className="h-3.5 w-3.5" /> {t("toolbar.export")} <ChevronDown className="h-3 w-3" />
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
        <BarChart3 className="h-3.5 w-3.5" /> {t("toolbar.stats")}
      </button>
      <button
        onClick={() => setDups(true)}
        className="inline-flex items-center gap-1 rounded border border-neutral-700 px-2 py-1 text-xs hover:bg-neutral-800"
      >
        <Copy className="h-3.5 w-3.5" /> {t("toolbar.duplicates")}
      </button>
      <button
        onClick={() => setCompare(true)}
        className="inline-flex items-center gap-1 rounded border border-neutral-700 px-2 py-1 text-xs hover:bg-neutral-800"
      >
        <GitCompareArrows className="h-3.5 w-3.5" /> {t("toolbar.compare")}
      </button>

      {selectedIds.length > 0 && (
        <button
          onClick={() => trashIds(selectedIds, reloadCurrent, setError, t)}
          className="inline-flex items-center gap-1 rounded border border-red-900/60 px-2 py-1 text-xs text-red-300 hover:bg-red-950/50"
          title={t("toolbar.trashTitle")}
        >
          <Trash2 className="h-3.5 w-3.5" /> {t("toolbar.trash", { count: selectedIds.length })}
        </button>
      )}

      <div className="mx-1 h-4 w-px bg-neutral-800" />
      <CategoryChips />

      <div className="ml-auto flex items-center gap-2">
        {/* Toggle de vista: tabla / galería */}
        <div className="flex items-center rounded-md border border-neutral-700 p-0.5">
          <button
            onClick={() => setViewMode("table")}
            title={t("toolbar.viewTable")}
            className={`rounded p-1 ${viewMode === "table" ? "bg-neutral-700 text-neutral-100" : "text-neutral-400 hover:text-neutral-200"}`}
          >
            <List className="h-3.5 w-3.5" />
          </button>
          <button
            onClick={() => setViewMode("grid")}
            title={t("toolbar.viewGrid")}
            className={`rounded p-1 ${viewMode === "grid" ? "bg-neutral-700 text-neutral-100" : "text-neutral-400 hover:text-neutral-200"}`}
          >
            <LayoutGrid className="h-3.5 w-3.5" />
          </button>
        </div>
        <span className="text-[11px] text-neutral-600">
          {selectedIds.length > 0
            ? t("toolbar.countSelected", {
                selected: selectedIds.length.toLocaleString(),
                count: count.toLocaleString(),
              })
            : count > 0 && t("toolbar.countRows", { count: count.toLocaleString() })}
        </span>
      </div>

      {stats && <StatsDialog onClose={() => setStats(false)} />}
      {dups && <DuplicatesDialog onClose={() => setDups(false)} />}
      {compare && <CompareDialog onClose={() => setCompare(false)} />}
    </div>
  );
}
