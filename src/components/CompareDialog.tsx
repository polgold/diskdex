import { useEffect, useState } from "react";
import {
  GitCompareArrows,
  Loader2,
  AlertTriangle,
  FileWarning,
  FilePlus2,
  Copy,
  X,
  Folder,
  ChevronRight,
  HardDrive,
  HelpCircle,
  RefreshCw,
  ShieldCheck,
  Zap,
  CheckCircle2,
} from "lucide-react";
import {
  api,
  type DiskRow,
  type DiskDiff,
  type DiffEntry,
  type EntryRow,
  type MissingNode,
} from "../lib/ipc";
import { formatBytes, formatCount } from "../lib/format";
import { useT } from "../lib/i18n";
import { Modal } from "./StatsDialog";
import { useCopy } from "../store/copy";
import { MissingTree } from "./MissingTree";
import { confirm as confirmDialog } from "@tauri-apps/plugin-dialog";

/** Un lado de la comparación: disco + carpeta raíz opcional (subárbol). */
interface Scope {
  diskId: number | null;
  rootId: number | null; // null = disco entero
  crumbs: { id: number | null; name: string }[]; // [{null, DiscoName}, {id, Carpeta}, …]
}

const emptyScope: Scope = { diskId: null, rootId: null, crumbs: [] };

/** Selector de disco + navegador de carpetas. La carpeta “actual” (última miga)
 *  es el subárbol elegido; el nivel raíz = disco entero. */
function ScopePicker({
  label,
  disks,
  scope,
  onChange,
}: {
  label: string;
  disks: DiskRow[];
  scope: Scope;
  onChange: (s: Scope) => void;
}) {
  const t = useT();
  const [folders, setFolders] = useState<EntryRow[]>([]);
  const [loading, setLoading] = useState(false);

  // Carga las subcarpetas del nivel actual (rootId) del disco elegido.
  useEffect(() => {
    if (scope.diskId == null) {
      setFolders([]);
      return;
    }
    setLoading(true);
    api
      .listChildren(scope.diskId, scope.rootId)
      .then((rows) => setFolders(rows.filter((r) => r.is_folder)))
      .catch(() => setFolders([]))
      .finally(() => setLoading(false));
  }, [scope.diskId, scope.rootId]);

  function pickDisk(id: number | null) {
    if (id == null) {
      onChange(emptyScope);
      return;
    }
    const disk = disks.find((d) => d.id === id);
    onChange({ diskId: id, rootId: null, crumbs: [{ id: null, name: disk?.name ?? "?" }] });
  }

  function enterFolder(f: EntryRow) {
    onChange({
      diskId: scope.diskId,
      rootId: f.id,
      crumbs: [...scope.crumbs, { id: f.id, name: f.name }],
    });
  }

  // Vuelve a la miga en índice `i` (recorta el resto).
  function gotoCrumb(i: number) {
    const crumbs = scope.crumbs.slice(0, i + 1);
    onChange({ diskId: scope.diskId, rootId: crumbs[crumbs.length - 1].id, crumbs });
  }

  return (
    <div className="flex min-w-0 flex-col gap-1.5 text-xs">
      <span className="text-neutral-400">{label}</span>
      <select
        value={scope.diskId ?? ""}
        onChange={(e) => pickDisk(e.target.value ? Number(e.target.value) : null)}
        className="rounded border border-border bg-neutral-900 px-2 py-1.5 text-xs"
      >
        <option value="">{t("compare.selectDisk")}</option>
        {disks.map((d) => (
          <option key={d.id} value={d.id}>
            {d.name} {d.is_online ? "" : `· ${t("compare.offline")}`}
          </option>
        ))}
      </select>

      {scope.diskId != null && (
        <div className="rounded border border-border bg-neutral-950/40">
          {/* Breadcrumb del subárbol elegido */}
          <div className="flex flex-wrap items-center gap-0.5 border-b border-border/60 px-2 py-1 text-[11px]">
            {scope.crumbs.map((c, i) => (
              <span key={i} className="flex items-center gap-0.5">
                {i > 0 && <ChevronRight className="h-3 w-3 text-neutral-600" />}
                <button
                  onClick={() => gotoCrumb(i)}
                  className={`rounded px-1 ${i === scope.crumbs.length - 1 ? "font-semibold text-sky-300" : "text-neutral-400 hover:text-neutral-200"}`}
                >
                  {i === 0 ? <HardDrive className="mr-0.5 inline h-3 w-3" /> : null}
                  {c.name}
                </button>
              </span>
            ))}
            <span className="ml-1 text-neutral-600">
              {scope.rootId == null ? t("compare.rootLevel") : ""}
            </span>
          </div>
          {/* Lista de subcarpetas para drill-down */}
          <div className="max-h-32 overflow-auto">
            {loading ? (
              <div className="flex items-center gap-1.5 px-2 py-2 text-neutral-500">
                <Loader2 className="h-3 w-3 animate-spin" /> …
              </div>
            ) : folders.length === 0 ? (
              <div className="px-2 py-2 text-neutral-600">{t("compare.noSubfolders")}</div>
            ) : (
              folders.map((f) => (
                <button
                  key={f.id}
                  onClick={() => enterFolder(f)}
                  className="flex w-full items-center gap-1.5 px-2 py-1 text-left text-neutral-300 hover:bg-neutral-800"
                  title={f.name}
                >
                  <Folder className="h-3 w-3 shrink-0 text-sky-400/70" />
                  <span className="truncate">{f.name}</span>
                </button>
              ))
            )}
          </div>
        </div>
      )}
    </div>
  );
}

/** Lista scrollable de diferencias (rutas relativas al subárbol comparado). */
function DiffList({
  entries,
  count,
  kind,
}: {
  entries: DiffEntry[];
  count: number;
  kind: "missing" | "mismatch" | "extra" | "conflict";
}) {
  const t = useT();
  const hidden = count - entries.length;

  // En un conflicto no hay tamaños que comparar: lo que cambia es el tipo.
  // `is_folder` es el del origen, así que el destino es siempre el opuesto.
  const typeLabel = (isFolder: boolean) => (isFolder ? t("compare.folder") : t("compare.file"));
  return (
    <div className="max-h-40 overflow-auto rounded border border-border bg-neutral-950/40">
      {entries.map((e) => (
        <div key={e.rel_path} className="flex items-center justify-between gap-3 px-2 py-1 text-xs">
          <span className="truncate font-mono text-neutral-300" title={e.rel_path}>
            {e.is_folder ? "📁 " : ""}
            {e.rel_path}
          </span>
          <span className="shrink-0 text-neutral-500">
            {kind === "conflict"
              ? `${typeLabel(e.is_folder)} → ${typeLabel(!e.is_folder)}`
              : kind === "mismatch"
                ? `${formatBytes(e.dst_size)} → ${formatBytes(e.src_size)}`
                : formatBytes(kind === "extra" ? e.dst_size : e.src_size)}
          </span>
        </div>
      ))}
      {hidden > 0 && (
        <div className="px-2 py-1 text-center text-[11px] text-neutral-500">
          {t("compare.andMore", { count: formatCount(hidden) })}
        </div>
      )}
    </div>
  );
}

/** Pestaña del selector de criterio (rápido / profundo). */
function ModeButton({
  active,
  onClick,
  icon,
  label,
}: {
  active: boolean;
  onClick: () => void;
  icon: React.ReactNode;
  label: string;
}) {
  return (
    <button
      onClick={onClick}
      className={`inline-flex items-center gap-1 rounded px-2.5 py-1 text-xs ${
        active ? "bg-sky-600 font-medium text-white" : "text-neutral-400 hover:text-neutral-200"
      }`}
    >
      {icon}
      {label}
    </button>
  );
}

export function CompareDialog({ onClose }: { onClose: () => void }) {
  const t = useT();
  const [disks, setDisks] = useState<DiskRow[]>([]);
  const [src, setSrc] = useState<Scope>(emptyScope);
  const [dst, setDst] = useState<Scope>(emptyScope);
  const [diff, setDiff] = useState<DiskDiff | null>(null);
  const [deep, setDeep] = useState(false);
  const [comparing, setComparing] = useState(false);
  const [includeMismatch, setIncludeMismatch] = useState(false);
  // La copia vive en un store global: así cerrar el diálogo no la corta y se
  // puede seguir usando la app (escanear otro disco) mientras corre.
  const copyRunning = useCopy((s) => s.running);
  const progress = useCopy((s) => s.progress);
  const startCopy = useCopy((s) => s.start);
  const cancelCopy = useCopy((s) => s.cancel);
  const copying = copyRunning !== null;
  const [result, setResult] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [rechecking, setRechecking] = useState(false);
  const [tree, setTree] = useState<MissingNode[]>([]);
  // Conjunto mínimo de carpetas elegidas. "" = todo (el default tras comparar).
  const [selected, setSelected] = useState<Set<string>>(new Set([""]));

  // Al abrir, re-chequear qué está montado ahora en vez de confiar en el
  // `is_online` guardado: el catálogo puede venir de otra sesión, y copiar
  // depende de que el estado sea el real, no el de la última vez.
  useEffect(() => {
    api.refreshOnlineStatus().then(setDisks).catch((e) => setError(String(e)));
  }, []);

  const srcDisk = disks.find((d) => d.id === src.diskId) ?? null;
  const dstDisk = disks.find((d) => d.id === dst.diskId) ?? null;
  const ready = src.diskId != null && dst.diskId != null;
  const sameScope = ready && src.diskId === dst.diskId && src.rootId === dst.rootId;
  const bothOnline = !!srcDisk?.is_online && !!dstDisk?.is_online;

  // Todo lo faltante, carpetas incluidas: el mirror las crea para que el destino
  // quede idéntico. Los conflictos de tipo no entran (nunca se copian encima).
  const toCopyCount = diff ? diff.missing_count + (includeMismatch ? diff.mismatch_count : 0) : 0;

  // Lo que realmente se va a copiar según las carpetas tildadas. Con "" (todo)
  // vale el total; si no, se suman los nodos elegidos (que ya traen el total de
  // su subárbol, y por ser el conjunto mínimo nunca se solapan entre sí).
  const selectedTotals = (() => {
    if (selected.has("")) {
      const root = tree.find((n) => n.rel_path === "");
      return { files: root?.files ?? diff?.missing_file_count ?? 0, bytes: root?.bytes ?? diff?.missing_bytes ?? 0 };
    }
    let files = 0, bytes = 0;
    for (const p of selected) {
      const n = tree.find((x) => x.rel_path === p);
      if (n) { files += n.files; bytes += n.bytes; }
    }
    return { files, bytes };
  })();
  const selectedCount = selectedTotals.files;

  const scopeLabel = (s: Scope) =>
    s.crumbs.length <= 1 ? t("compare.wholeDisk") : s.crumbs.map((c) => c.name).join("/");

  // Vuelve a mirar qué volúmenes están montados. No toca el diff: comparar es
  // offline, así que lo ya comparado sigue siendo válido.
  async function recheckOnline() {
    setRechecking(true);
    try {
      setDisks(await api.refreshOnlineStatus());
    } catch (e) {
      setError(String(e));
    } finally {
      setRechecking(false);
    }
  }

  // Cambiar de criterio invalida lo mostrado: un diff por tamaño y uno por hash
  // no son comparables, y dejarlo en pantalla induciría a copiar con el criterio
  // equivocado.
  function changeMode(next: boolean) {
    setDeep(next);
    setDiff(null);
    setResult(null);
  }

  async function runCompare() {
    if (!ready || sameScope) return;
    setComparing(true);
    setDiff(null);
    setResult(null);
    setError(null);
    try {
      const d = await api.compareDisks(src.diskId!, dst.diskId!, src.rootId, dst.rootId, deep);
      setDiff(d);
      // Árbol de faltantes por carpeta: exacto, sin el recorte del diff.
      setSelected(new Set([""]));
      if (d.missing_count > 0) {
        setTree(await api.missingTree(src.diskId!, dst.diskId!, src.rootId, dst.rootId, deep));
      } else {
        setTree([]);
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setComparing(false);
    }
  }

  async function runCopy() {
    if (!ready || !diff) return;
    const ok = await confirmDialog(
      t("compare.confirm", { count: formatCount(selectedCount), size: formatBytes(selectedTotals.bytes) }),
    );
    if (!ok) return;
    setResult(null);
    setError(null);
    // No se espera: la copia queda corriendo en el store y la barra de arriba la
    // sigue mostrando aunque cierres este diálogo.
    void startCopy({
      srcDiskId: src.diskId!,
      dstDiskId: dst.diskId!,
      srcRootId: src.rootId,
      dstRootId: dst.rootId,
      deep,
      includeMismatch,
      // "" (todo) se manda como lista vacía: es lo que el backend interpreta
      // como "sin filtro", y evita depender de que "" matchee como prefijo.
      prefixes: selected.has("") ? [] : Array.from(selected),
      label: `${scopeLabel(src)} → ${scopeLabel(dst)}`,
      planned: selectedCount,
    });
  }

  const pct = progress && progress.total > 0 ? Math.round((progress.done / progress.total) * 100) : 0;

  // Cuántos quedan por copiar según lo que ya reportó el backend. El plan incluye
  // carpetas además de archivos, así que es una estimación — pero baja en vivo,
  // que es lo que importa mientras mirás la barra.
  const remaining =
    diff && progress ? Math.max(0, toCopyCount - progress.done) : diff ? toCopyCount : 0;

  // Al terminar la copia, re-comparar solo si el diálogo sigue abierto: el diff
  // en pantalla quedó viejo y dejarlo induce a copiar de nuevo lo ya copiado.
  const lastSummary = useCopy((s) => s.lastSummary);
  useEffect(() => {
    if (lastSummary && diff && !copying) void runCompare();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [lastSummary]);

  return (
    <Modal onClose={onClose} title={t("compare.title")} icon={<GitCompareArrows className="h-4 w-4 text-sky-400" />}>
      {/* Selectores origen / destino (disco + carpeta) */}
      <div className="mb-2 grid grid-cols-[1fr_auto_1fr] items-start gap-2">
        <ScopePicker label={t("compare.source")} disks={disks} scope={src} onChange={setSrc} />
        <GitCompareArrows className="mt-6 h-4 w-4 text-neutral-600" />
        <ScopePicker label={t("compare.dest")} disks={disks} scope={dst} onChange={setDst} />
      </div>

      {ready && (
        <p className="mb-3 truncate text-[11px] text-neutral-500" title={`${scopeLabel(src)}  →  ${scopeLabel(dst)}`}>
          <span className="text-neutral-400">{scopeLabel(src)}</span> → <span className="text-neutral-400">{scopeLabel(dst)}</span>
        </p>
      )}
      {sameScope && <p className="mb-3 text-xs text-amber-400">{t("compare.sameDisk")}</p>}

      {/* Criterio de comparación. Rápido alcanza para "¿está todo?"; profundo
          responde "¿está todo Y sano?", que es lo que importa en un backup. */}
      <div className="mb-3 flex flex-col gap-1.5">
        <div className="inline-flex w-fit rounded border border-border p-0.5">
          <ModeButton active={!deep} onClick={() => changeMode(false)} icon={<Zap className="h-3 w-3" />} label={t("compare.modeFast")} />
          <ModeButton active={deep} onClick={() => changeMode(true)} icon={<ShieldCheck className="h-3 w-3" />} label={t("compare.modeDeep")} />
        </div>
        <p className="text-[11px] text-neutral-500">
          {deep ? t("compare.modeDeepHelp") : t("compare.modeFastHelp")}
        </p>
        <p className="text-[11px] text-neutral-600">{t("compare.ignoresNoise")}</p>
      </div>

      <button
        onClick={runCompare}
        disabled={!ready || sameScope || comparing}
        className="mb-4 inline-flex items-center gap-1.5 rounded bg-sky-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-sky-500 disabled:opacity-40"
      >
        {comparing ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <GitCompareArrows className="h-3.5 w-3.5" />}
        {comparing ? t("compare.comparing") : t("compare.run")}
      </button>

      {/* Zona de copia: va ARRIBA de las listas. Con miles de diferencias, dejarla
          al final la escondía debajo de un scroll larguísimo y no se entendía por
          qué no se podía copiar. La acción y su motivo de bloqueo van juntos. */}
      {diff && toCopyCount > 0 && (
        <div className="mb-4 space-y-2 rounded-lg border border-border bg-neutral-950/40 p-3">
          {diff.mismatch_count > 0 && (
            <label className="flex items-center gap-2 text-xs text-neutral-300">
              <input type="checkbox" checked={includeMismatch} onChange={(e) => setIncludeMismatch(e.target.checked)} />
              {t("compare.includeMismatch")}
            </label>
          )}

          {copying && (
            <div className="space-y-1">
              <div className="h-1.5 w-full overflow-hidden rounded bg-neutral-800">
                <div className="h-full bg-sky-500 transition-all" style={{ width: `${pct}%` }} />
              </div>
              <p className="truncate font-mono text-[11px] text-neutral-500">
                {progress ? `${progress.done}/${progress.total} · ${progress.current}` : t("compare.copying")}
              </p>
            </div>
          )}

          <div className="flex flex-wrap items-center gap-2">
            {copying ? (
              <button
                onClick={cancelCopy}
                className="inline-flex items-center gap-1.5 rounded border border-red-900/60 px-3 py-1.5 text-xs text-red-300 hover:bg-red-950/50"
              >
                <X className="h-3.5 w-3.5" /> {t("compare.cancel")}
              </button>
            ) : (
              <button
                onClick={runCopy}
                disabled={!bothOnline || selectedCount === 0}
                className="inline-flex items-center gap-1.5 rounded bg-emerald-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-emerald-500 disabled:opacity-40"
              >
                <Copy className="h-3.5 w-3.5" /> {t("compare.copy")} ({formatCount(selectedCount)} ·{" "}
                {formatBytes(selectedTotals.bytes)})
              </button>
            )}
          </div>

          {/* Nombrar QUÉ disco falta conectar: con dos selectores, "conectá ambos
              discos" obliga a adivinar cuál de los dos es el que está offline. */}
          {!bothOnline && !copying && (
            <div className="flex items-start gap-1.5 text-xs text-amber-400">
              <AlertTriangle className="mt-0.5 h-3.5 w-3.5 shrink-0" />
              <div className="space-y-1.5">
                <p>
                  {t("compare.needOnlineDisks", {
                    disks: [srcDisk, dstDisk]
                      .filter((d) => d && !d.is_online)
                      .map((d) => d!.name)
                      .join(", "),
                  })}
                </p>
                {/* Re-chequear sin cerrar el diálogo: conectás el disco y seguís
                    desde acá, sin perder la comparación ya hecha. */}
                <button
                  onClick={recheckOnline}
                  disabled={rechecking}
                  className="inline-flex items-center gap-1 rounded border border-amber-800/60 px-2 py-0.5 text-amber-200 hover:bg-amber-950/40 disabled:opacity-40"
                >
                  {rechecking ? <Loader2 className="h-3 w-3 animate-spin" /> : <RefreshCw className="h-3 w-3" />}
                  {t("compare.recheck")}
                </button>
              </div>
            </div>
          )}
        </div>
      )}

      {diff && tree.length > 0 && !copying && (
        <div className="mb-4">
          <MissingTree nodes={tree} selected={selected} onChange={setSelected} />
        </div>
      )}

      {diff && (
        <div className="space-y-4">
          {diff.missing_count === 0 && diff.mismatch_count === 0 && diff.conflict_count === 0 ? (
            <p className="flex items-center gap-2 rounded border border-emerald-800/50 bg-emerald-950/30 px-3 py-2 text-sm text-emerald-300">
              <CheckCircle2 className="h-4 w-4 shrink-0" />
              {deep && diff.unverified_count > 0
                ? t("compare.identicalPartial", {
                    ok: formatCount(diff.ok_count),
                    n: formatCount(diff.unverified_count),
                  })
                : t("compare.identical")}
            </p>
          ) : (
            <>
              {diff.missing_count > 0 && (
                <section>
                  <h3 className="mb-1.5 flex items-center gap-1.5 text-xs font-semibold text-red-300">
                    <FileWarning className="h-3.5 w-3.5" /> {t("compare.missing")} ·{" "}
                    {t("compare.filesCount", { count: formatCount(diff.missing_file_count) })} ·{" "}
                    <span className="text-red-400">{formatBytes(diff.missing_bytes)}</span>
                    {copying && (
                      <span className="font-normal text-sky-300">
                        · {t("compare.remaining", { n: formatCount(remaining) })}
                      </span>
                    )}
                  </h3>
                  <DiffList entries={diff.missing} count={diff.missing_count} kind="missing" />
                </section>
              )}
              {diff.mismatch_count > 0 && (
                <section>
                  <h3 className="mb-1.5 flex items-center gap-1.5 text-xs font-semibold text-amber-300">
                    <AlertTriangle className="h-3.5 w-3.5" /> {t("compare.mismatch")} ·{" "}
                    {t("compare.filesCount", { count: formatCount(diff.mismatch_count) })}
                  </h3>
                  <DiffList entries={diff.size_mismatch} count={diff.mismatch_count} kind="mismatch" />
                </section>
              )}
              {diff.conflict_count > 0 && (
                <section>
                  <h3 className="mb-1.5 flex items-center gap-1.5 text-xs font-semibold text-orange-300">
                    <FileWarning className="h-3.5 w-3.5" /> {t("compare.conflicts")} ·{" "}
                    {t("compare.itemsCount", { count: formatCount(diff.conflict_count) })}
                  </h3>
                  <p className="mb-1.5 text-[11px] text-neutral-500">{t("compare.conflictHint")}</p>
                  <DiffList entries={diff.conflicts} count={diff.conflict_count} kind="conflict" />
                </section>
              )}
              {/* Solo el modo profundo produce "no verificado": están presentes y
                  del mismo tamaño, pero falta el hash para poder afirmar que el
                  contenido coincide. No es un error, es una zona ciega. */}
              {diff.unverified_count > 0 && (
                <section>
                  <h3 className="mb-1.5 flex items-center gap-1.5 text-xs font-semibold text-neutral-300">
                    <HelpCircle className="h-3.5 w-3.5" /> {t("compare.unverified")} ·{" "}
                    {t("compare.filesCount", { count: formatCount(diff.unverified_count) })}
                  </h3>
                  <p className="mb-1.5 text-[11px] text-neutral-500">{t("compare.unverifiedHint")}</p>
                  <DiffList entries={diff.unverified} count={diff.unverified_count} kind="missing" />
                </section>
              )}
              {diff.extra_count > 0 && (
                <section>
                  <h3 className="mb-1.5 flex items-center gap-1.5 text-xs font-semibold text-neutral-400">
                    <FilePlus2 className="h-3.5 w-3.5" /> {t("compare.extra")} ·{" "}
                    {t("compare.filesCount", { count: formatCount(diff.extra_count) })}
                  </h3>
                  <DiffList entries={diff.extra} count={diff.extra_count} kind="extra" />
                </section>
              )}

            </>
          )}
        </div>
      )}

      {result && <p className="mt-3 text-xs text-emerald-300">{result}</p>}
      {error && <p className="mt-3 whitespace-pre-wrap text-xs text-red-400">{error}</p>}
    </Modal>
  );
}
