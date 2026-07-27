import { useState } from "react";
import { ClipboardPaste, Loader2, CheckCircle2, AlertTriangle } from "lucide-react";
import { api, type ReportApply } from "../lib/ipc";
import { useCatalog } from "../store/catalog";
import { useT } from "../lib/i18n";
import { Modal } from "./StatsDialog";

/**
 * Pega el reporte de discos de DiskCatalogMaker (menú Print → guardar como PDF /
 * copiar texto) y actualiza free/capacity/kind de cada disco por nombre. Evita
 * reconectar los discos uno por uno para tener su espacio libre.
 */
export function DiskReportDialog({ onClose }: { onClose: () => void }) {
  const t = useT();
  const refreshDisks = useCatalog((s) => s.refreshDisks);
  const [text, setText] = useState("");
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<ReportApply | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function apply() {
    setBusy(true);
    setError(null);
    setResult(null);
    try {
      const r = await api.applyDiskReport(text);
      setResult(r);
      await refreshDisks(); // reflejar los free/capacity nuevos en la tabla
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <Modal onClose={onClose} title={t("report.title")} icon={<ClipboardPaste className="h-4 w-4 text-sky-400" />}>
      <p className="text-xs text-neutral-400">{t("report.help")}</p>

      <textarea
        value={text}
        onChange={(e) => setText(e.target.value)}
        placeholder={t("report.placeholder")}
        spellCheck={false}
        className="mt-2 h-48 w-full resize-none rounded border border-border bg-neutral-950/60 p-2 font-mono text-[11px] text-neutral-300 outline-none focus:border-sky-700"
      />

      <div className="mt-3 flex items-center gap-2">
        <button
          onClick={apply}
          disabled={busy || text.trim().length === 0}
          className="inline-flex items-center gap-1.5 rounded bg-sky-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-sky-500 disabled:opacity-40"
        >
          {busy ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <CheckCircle2 className="h-3.5 w-3.5" />}
          {t("report.apply")}
        </button>
        <span className="text-[11px] text-neutral-500">{t("report.privacy")}</span>
      </div>

      {error && (
        <div className="mt-3 rounded border border-red-900/60 bg-red-950/40 px-3 py-2 text-xs text-red-300">{error}</div>
      )}

      {result && (
        <div className="mt-3 space-y-2 text-xs">
          <div className="flex items-center gap-1.5 text-emerald-300">
            <CheckCircle2 className="h-4 w-4" />
            {t("report.updated", { n: result.updated.length })}
          </div>
          {result.unmatched.length > 0 && (
            <div>
              <div className="flex items-center gap-1.5 text-amber-400">
                <AlertTriangle className="h-3.5 w-3.5" />
                {t("report.unmatched", { n: result.unmatched.length })}
              </div>
              <div className="mt-1 max-h-24 overflow-auto rounded border border-neutral-800 px-2 py-1 text-[11px] text-neutral-500">
                {result.unmatched.join(", ")}
              </div>
            </div>
          )}
        </div>
      )}
    </Modal>
  );
}
