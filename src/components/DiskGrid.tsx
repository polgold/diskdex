import { useMemo, useState } from "react";
import { HardDrive, ChevronUp, ChevronDown, Circle } from "lucide-react";
import { useCatalog } from "../store/catalog";
import type { DiskRow } from "../lib/ipc";
import { formatBytes, formatCount, formatDate } from "../lib/format";
import { useT } from "../lib/i18n";

/** Columnas ordenables de la tabla de discos. */
type SortKey = "name" | "size" | "free" | "kind" | "scanned" | "count" | "capacity";
interface Sort {
  key: SortKey;
  dir: "asc" | "desc";
}

/** Valor comparable de cada columna (null va siempre al final). */
function sortValue(d: DiskRow, key: SortKey): number | string | null {
  switch (key) {
    case "name": return d.name.toLowerCase();
    case "size": return d.total_size;
    case "free": return d.free_space;
    case "kind": return d.kind?.toLowerCase() ?? null;
    case "scanned": return d.scanned_at;
    case "count": return d.file_count + d.folder_count;
    case "capacity": return d.capacity;
  }
}

/**
 * Vista de todos los discos como tabla ordenable (estilo DiskCatalogMaker), al
 * abrir el catálogo sin ninguno seleccionado. Todo sale de `listDisks`, así que
 * funciona con los discos desconectados. Click en una fila abre el disco.
 */
export function DiskGrid() {
  const t = useT();
  const disks = useCatalog((s) => s.disks);
  const openDisk = useCatalog((s) => s.openDisk);
  const [sort, setSort] = useState<Sort>({ key: "free", dir: "desc" });

  const sorted = useMemo(() => {
    const rows = [...disks];
    rows.sort((a, b) => {
      const va = sortValue(a, sort.key);
      const vb = sortValue(b, sort.key);
      // Los nulos (dato desconocido) siempre al fondo, sin importar la dirección.
      if (va == null && vb == null) return 0;
      if (va == null) return 1;
      if (vb == null) return -1;
      const cmp = typeof va === "string" ? va.localeCompare(vb as string) : (va as number) - (vb as number);
      return sort.dir === "asc" ? cmp : -cmp;
    });
    return rows;
  }, [disks, sort]);

  const totals = useMemo(
    () => disks.reduce((n, d) => n + d.file_count + d.folder_count, 0),
    [disks],
  );

  function toggle(key: SortKey) {
    setSort((s) =>
      s.key === key ? { key, dir: s.dir === "asc" ? "desc" : "asc" } : { key, dir: key === "name" ? "asc" : "desc" },
    );
  }

  if (disks.length === 0) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-2 text-neutral-600">
        <HardDrive className="h-10 w-10 text-neutral-700" />
        <p className="text-sm text-neutral-500">{t("disk.noDisks")}</p>
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col">
      <div className="flex-1 overflow-auto">
        <table className="w-full border-collapse text-xs">
          <thead className="sticky top-0 z-10 bg-neutral-900 text-neutral-400">
            <tr className="border-b border-border">
              <Th label={t("table.colName")} col="name" sort={sort} onSort={toggle} />
              <Th label={t("disk.colSize")} col="size" sort={sort} onSort={toggle} align="right" />
              <Th label={t("disk.colFree")} col="free" sort={sort} onSort={toggle} align="right" />
              <Th label={t("disk.colKind")} col="kind" sort={sort} onSort={toggle} />
              <Th label={t("disk.colScanned")} col="scanned" sort={sort} onSort={toggle} />
              <Th label={t("disk.colCount")} col="count" sort={sort} onSort={toggle} align="right" />
              <Th label={t("disk.colCapacity")} col="capacity" sort={sort} onSort={toggle} align="right" />
            </tr>
          </thead>
          <tbody>
            {sorted.map((d) => (
              <tr
                key={d.id}
                onClick={() => openDisk(d)}
                className="cursor-pointer border-b border-neutral-800/50 hover:bg-neutral-800/40"
              >
                <td className="px-3 py-1.5">
                  <span className="flex items-center gap-2">
                    <Circle
                      className={`h-1.5 w-1.5 shrink-0 ${d.is_online ? "fill-emerald-500 text-emerald-500" : "fill-neutral-600 text-neutral-600"}`}
                    />
                    <HardDrive className="h-3.5 w-3.5 shrink-0 text-amber-500/80" />
                    <span className="truncate text-neutral-200" title={d.name}>{d.name}</span>
                  </span>
                </td>
                <td className="px-3 py-1.5 text-right tabular-nums text-neutral-300">{formatBytes(d.total_size)}</td>
                <td className="px-3 py-1.5 text-right tabular-nums text-neutral-400">
                  {d.free_space != null ? formatBytes(d.free_space) : "—"}
                </td>
                <td className="px-3 py-1.5 text-neutral-500">{d.kind ?? "—"}</td>
                <td className="px-3 py-1.5 tabular-nums text-neutral-500">
                  {d.scanned_at ? formatDate(d.scanned_at) : "—"}
                </td>
                <td className="px-3 py-1.5 text-right tabular-nums text-neutral-400">
                  {t("disk.itemsCount", { n: formatCount(d.file_count + d.folder_count) })}
                </td>
                <td className="px-3 py-1.5 text-right tabular-nums text-neutral-500">
                  {d.capacity != null ? formatBytes(d.capacity) : "—"}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      {/* Totales, como el pie de DiskCatalogMaker. */}
      <div className="border-t border-border bg-neutral-900/60 px-3 py-1.5 text-center text-[11px] text-neutral-500">
        {t("disk.footer", { disks: formatCount(disks.length), items: formatCount(totals) })}
      </div>
    </div>
  );
}

function Th({
  label,
  col,
  sort,
  onSort,
  align,
}: {
  label: string;
  col: SortKey;
  sort: Sort;
  onSort: (k: SortKey) => void;
  align?: "right";
}) {
  const active = sort.key === col;
  return (
    <th
      onClick={() => onSort(col)}
      className={`cursor-pointer select-none whitespace-nowrap px-3 py-1.5 font-medium hover:text-neutral-200 ${
        align === "right" ? "text-right" : "text-left"
      }`}
    >
      <span className={`inline-flex items-center gap-1 ${align === "right" ? "flex-row-reverse" : ""}`}>
        {label}
        {active &&
          (sort.dir === "asc" ? <ChevronUp className="h-3 w-3" /> : <ChevronDown className="h-3 w-3" />)}
      </span>
    </th>
  );
}
