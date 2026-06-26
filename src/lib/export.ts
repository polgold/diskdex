// Export de resultados / subárbol (M7). La UI arma las filas del view actual,
// generamos el texto y lo guardamos vía diálogo + write_text_file. Para "PDF"
// generamos un HTML imprimible y lo abrimos (Imprimir → Guardar como PDF).
import { save } from "@tauri-apps/plugin-dialog";
import { openPath } from "@tauri-apps/plugin-opener";
import { api } from "./ipc";
import { formatBytes, formatDate } from "./format";

export interface ExportRow {
  disk: string;
  path: string;
  name: string;
  type: string;
  size: number;
  modified: number | null;
}

export type ExportFormat = "csv" | "tsv" | "json" | "html";

const COLUMNS: { key: keyof ExportRow; label: string }[] = [
  { key: "disk", label: "Disco" },
  { key: "path", label: "Ruta" },
  { key: "name", label: "Nombre" },
  { key: "type", label: "Tipo" },
  { key: "size", label: "Tamaño (bytes)" },
  { key: "modified", label: "Modificado" },
];

function escapeDelimited(value: string, sep: string): string {
  if (value.includes(sep) || value.includes('"') || value.includes("\n")) {
    return `"${value.replace(/"/g, '""')}"`;
  }
  return value;
}

function toDelimited(rows: ExportRow[], sep: string): string {
  const header = COLUMNS.map((c) => c.label).join(sep);
  const lines = rows.map((r) =>
    COLUMNS.map((c) => {
      const v = r[c.key];
      const s = v === null || v === undefined ? "" : String(v);
      return escapeDelimited(s, sep);
    }).join(sep)
  );
  return [header, ...lines].join("\n");
}

function toJson(rows: ExportRow[]): string {
  return JSON.stringify(rows, null, 2);
}

function toHtml(rows: ExportRow[], title: string): string {
  const body = rows
    .map(
      (r) => `<tr>
        <td>${esc(r.disk)}</td>
        <td class="path">${esc(r.path)}</td>
        <td>${esc(r.name)}</td>
        <td>${esc(r.type)}</td>
        <td class="num">${r.size ? formatBytes(r.size) : "—"}</td>
        <td>${formatDate(r.modified)}</td>
      </tr>`
    )
    .join("\n");
  return `<!doctype html><html lang="es"><head><meta charset="utf-8">
<title>${esc(title)}</title>
<style>
  body{font:13px -apple-system,system-ui,sans-serif;color:#111;margin:24px}
  h1{font-size:18px;margin:0 0 4px} .meta{color:#666;margin:0 0 16px;font-size:12px}
  table{border-collapse:collapse;width:100%} th,td{border-bottom:1px solid #ddd;padding:4px 8px;text-align:left;vertical-align:top}
  th{background:#f4f4f4;font-size:11px;text-transform:uppercase;letter-spacing:.04em}
  .path{font-family:ui-monospace,Menlo,monospace;font-size:11px;color:#444}
  .num{text-align:right;white-space:nowrap;font-variant-numeric:tabular-nums}
  @media print{body{margin:0}}
</style></head><body>
<h1>${esc(title)}</h1>
<p class="meta">DiskDex · ${rows.length.toLocaleString()} filas · generado para imprimir / guardar como PDF</p>
<table><thead><tr>${COLUMNS.map((c) => `<th>${c.label}</th>`).join("")}</tr></thead>
<tbody>${body}</tbody></table>
</body></html>`;
}

function esc(s: string): string {
  return s.replace(/[&<>]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;" }[c]!));
}

const EXT: Record<ExportFormat, string> = { csv: "csv", tsv: "tsv", json: "json", html: "html" };

/** Guarda las filas en el formato pedido. Para HTML, lo abre para imprimir. */
export async function exportRows(
  rows: ExportRow[],
  format: ExportFormat,
  defaultName: string,
  title = "DiskDex — export"
): Promise<boolean> {
  const path = await save({
    title: "Exportar…",
    defaultPath: `${defaultName}.${EXT[format]}`,
    filters: [{ name: format.toUpperCase(), extensions: [EXT[format]] }],
  });
  if (!path) return false;

  let contents: string;
  if (format === "csv") contents = toDelimited(rows, ",");
  else if (format === "tsv") contents = toDelimited(rows, "\t");
  else if (format === "json") contents = toJson(rows);
  else contents = toHtml(rows, title);

  await api.writeTextFile(path, contents);
  if (format === "html") await openPath(path);
  return true;
}
