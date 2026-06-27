import { useEffect, useState } from "react";
import { Folder, File as FileIcon, Info, FolderSearch, ExternalLink, Copy, Check, Tag, X, Film, Package, FolderClosed } from "lucide-react";
import { api, type EntryRow, type VideoMeta, type ArchiveEntry } from "../lib/ipc";
import { useCatalog } from "../store/catalog";
import { formatBytes, formatDate, formatDuration, formatBitrate, formatCount } from "../lib/format";
import { revealOriginal, openOriginal, copyText } from "../lib/actions";

/** Inspector del ítem seleccionado (M2): detalle + ruta completa. */
export function Inspector() {
  const selectedEntryId = useCatalog((s) => s.selectedEntryId);
  const [entry, setEntry] = useState<EntryRow | null>(null);
  const [path, setPath] = useState<string>("");

  useEffect(() => {
    let cancelled = false;
    if (selectedEntryId == null) {
      setEntry(null);
      setPath("");
      return;
    }
    Promise.all([api.getEntry(selectedEntryId), api.entryPath(selectedEntryId)]).then(
      ([e, p]) => {
        if (!cancelled) {
          setEntry(e);
          setPath(p);
        }
      }
    );
    return () => {
      cancelled = true;
    };
  }, [selectedEntryId]);

  if (!entry) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-2 p-4 text-center text-neutral-600">
        <Info className="h-8 w-8" />
        <p className="text-xs">Seleccioná un ítem para ver su detalle.</p>
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col overflow-auto p-4">
      <div className="flex items-start gap-2">
        {entry.is_folder ? (
          <Folder className="mt-0.5 h-6 w-6 shrink-0 text-sky-400/80" />
        ) : (
          <FileIcon className="mt-0.5 h-6 w-6 shrink-0 text-neutral-400" />
        )}
        <h2 className="break-words text-sm font-medium leading-snug">{entry.name}</h2>
      </div>

      <Actions entry={entry} catalogPath={path} />

      <ThumbnailPreview entry={entry} />

      <VideoInfo entry={entry} />

      <ArchiveContents entry={entry} />

      <dl className="mt-4 space-y-2.5 text-xs">
        <Field label="Tipo" value={entry.is_folder ? "Carpeta" : entry.ext ? `Archivo .${entry.ext}` : "Archivo"} />
        <Field
          label="Tamaño lógico"
          value={entry.is_folder && entry.size_logical === 0 ? "—" : formatBytes(entry.size_logical)}
          mono
        />
        <Field
          label="Tamaño físico"
          value={entry.size_physical === 0 ? "—" : formatBytes(entry.size_physical)}
          mono
        />
        {entry.is_folder && (
          <Field label="Elementos" value={entry.child_count.toLocaleString()} mono />
        )}
        <Field label="Creado" value={formatDate(entry.created_at)} mono />
        <Field label="Modificado" value={formatDate(entry.modified_at)} mono />
        <div>
          <dt className="text-neutral-500">Ruta completa</dt>
          <dd className="mt-1 break-all rounded bg-neutral-900 p-2 font-mono text-[11px] text-neutral-300">
            {path || "—"}
          </dd>
        </div>
      </dl>

      <TagEditor entry={entry} />

      <CommentEditor entry={entry} />
    </div>
  );
}

/** Editor de keywords/tags del ítem: chips con quitar + alta por Enter/coma. */
function TagEditor({ entry }: { entry: EntryRow }) {
  const [tags, setTags] = useState<string[]>([]);
  const [draft, setDraft] = useState("");
  const runSearch = useCatalog((s) => s.runSearch);

  useEffect(() => {
    let cancelled = false;
    api.getEntryTags(entry.id).then((t) => !cancelled && setTags(t));
    return () => {
      cancelled = true;
    };
  }, [entry.id]);

  async function add() {
    const value = draft.trim().toLowerCase();
    setDraft("");
    if (!value) return;
    // Permitir varias de un saque separadas por coma.
    let next = tags;
    for (const part of value.split(",").map((p) => p.trim()).filter(Boolean)) {
      next = await api.addEntryTag(entry.id, part);
    }
    setTags(next);
  }

  async function remove(tag: string) {
    setTags(await api.removeEntryTag(entry.id, tag));
  }

  return (
    <div className="mt-4">
      <div className="flex items-center gap-1.5 text-xs text-neutral-500">
        <Tag className="h-3.5 w-3.5" /> Keywords
      </div>
      <div className="mt-1.5 flex flex-wrap gap-1.5">
        {tags.map((t) => (
          <span
            key={t}
            className="inline-flex items-center gap-1 rounded-full bg-sky-950/60 px-2 py-0.5 text-[11px] text-sky-200 ring-1 ring-sky-900"
          >
            <button
              onClick={() => runSearch(`tag:${t}`)}
              className="hover:underline"
              title={`Buscar todo lo etiquetado «${t}»`}
            >
              {t}
            </button>
            <button onClick={() => remove(t)} className="text-sky-400/70 hover:text-sky-200" title="Quitar">
              <X className="h-3 w-3" />
            </button>
          </span>
        ))}
        {tags.length === 0 && <span className="text-[11px] text-neutral-600">sin keywords</span>}
      </div>
      <input
        value={draft}
        onChange={(e) => setDraft(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" || e.key === ",") {
            e.preventDefault();
            add();
          }
        }}
        onBlur={add}
        placeholder="Agregar keyword + Enter…"
        className="mt-1.5 w-full rounded border border-neutral-700 bg-neutral-900 px-2 py-1 text-xs text-neutral-200 placeholder:text-neutral-600 focus:border-neutral-500 focus:outline-none"
      />
    </div>
  );
}

function CommentEditor({ entry }: { entry: EntryRow }) {
  const [value, setValue] = useState(entry.comment ?? "");
  const [saved, setSaved] = useState(false);
  // Re-sincronizar al cambiar de ítem.
  useEffect(() => setValue(entry.comment ?? ""), [entry.id, entry.comment]);

  async function save() {
    const next = value.trim() === "" ? null : value;
    if ((entry.comment ?? "") === (next ?? "")) return;
    await api.setEntryComment(entry.id, next);
    setSaved(true);
    setTimeout(() => setSaved(false), 1200);
  }

  return (
    <div className="mt-4">
      <label className="flex items-center justify-between text-xs text-neutral-500">
        Comentario
        {saved && <span className="text-emerald-400">guardado</span>}
      </label>
      <textarea
        value={value}
        onChange={(e) => setValue(e.target.value)}
        onBlur={save}
        rows={3}
        placeholder="Notas sobre este ítem…"
        className="mt-1 w-full resize-none rounded border border-neutral-700 bg-neutral-900 p-2 text-xs text-neutral-200 placeholder:text-neutral-600 focus:border-neutral-500 focus:outline-none"
      />
    </div>
  );
}

const IMAGE_EXTS = new Set([
  "jpg", "jpeg", "png", "gif", "webp", "bmp", "tif", "tiff",
  // RAW de cámara (preview vía sips en macOS)
  "dng", "arw", "cr2", "cr3", "crw", "nef", "nrw", "raf", "orf", "rw2", "pef", "srw", "3fr",
  "iiq", "dcr", "mrw", "mos", "erf", "rwl",
]);
const VIDEO_EXTS = new Set([
  "mp4", "mov", "m4v", "avi", "mkv", "mxf", "mts", "m2ts", "wmv", "webm", "mpg", "mpeg", "3gp",
  "flv", "ogv", "vob", "m2v",
]);
const ARCHIVE_EXTS = new Set(["zip", "7z", "rar", "cbz", "cbr"]);

const extOf = (e: EntryRow) => (e.is_folder ? null : e.ext?.toLowerCase() ?? null);
const isImage = (e: EntryRow) => !!extOf(e) && IMAGE_EXTS.has(extOf(e)!);
const isVideo = (e: EntryRow) => !!extOf(e) && VIDEO_EXTS.has(extOf(e)!);
const isArchive = (e: EntryRow) => !!extOf(e) && ARCHIVE_EXTS.has(extOf(e)!);

function ThumbnailPreview({ entry }: { entry: EntryRow }) {
  const [src, setSrc] = useState<string | null>(null);
  const [state, setState] = useState<"idle" | "loading" | "error">("idle");
  const previewable = isImage(entry) || isVideo(entry);

  useEffect(() => {
    setSrc(null);
    if (!previewable) {
      setState("idle");
      return;
    }
    let cancelled = false;
    setState("loading");
    api
      .getThumbnail(entry.id, 320)
      .then((d) => !cancelled && (setSrc(d), setState("idle")))
      .catch(() => !cancelled && setState("error"));
    return () => {
      cancelled = true;
    };
  }, [entry.id, entry.ext, entry.is_folder, previewable]);

  if (!previewable) return null;

  return (
    <div className="mt-3 overflow-hidden rounded-lg border border-neutral-800 bg-neutral-950">
      {state === "loading" && <div className="p-6 text-center text-xs text-neutral-600">generando preview…</div>}
      {state === "error" && (
        <div className="p-3 text-center text-[11px] text-neutral-600">
          preview no disponible (disco offline o formato sin soporte)
        </div>
      )}
      {src && <img src={src} alt={entry.name} className="mx-auto max-h-56 w-full object-contain" />}
    </div>
  );
}

/** Metadata técnica + tira de frames de un clip de video (Fase B). */
function VideoInfo({ entry }: { entry: EntryRow }) {
  const show = isVideo(entry);
  const [meta, setMeta] = useState<VideoMeta | null>(null);
  const [frames, setFrames] = useState<string[]>([]);

  useEffect(() => {
    setMeta(null);
    setFrames([]);
    if (!show) return;
    let cancelled = false;
    api.getVideoMeta(entry.id).then((m) => !cancelled && setMeta(m)).catch(() => {});
    api.getVideoFrames(entry.id).then((f) => !cancelled && setFrames(f)).catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [entry.id, show]);

  if (!show || (!meta && frames.length === 0)) return null;

  return (
    <div className="mt-3">
      <div className="flex items-center gap-1.5 text-xs text-neutral-500">
        <Film className="h-3.5 w-3.5" /> Video
      </div>

      {frames.length > 0 && (
        <div className="mt-1.5 flex gap-1 overflow-x-auto rounded-lg border border-neutral-800 bg-neutral-950 p-1.5">
          {frames.map((src, i) => (
            <img
              key={i}
              src={src}
              alt={`frame ${i + 1}`}
              className="h-12 shrink-0 rounded object-cover ring-1 ring-neutral-800"
            />
          ))}
        </div>
      )}

      {meta && (
        <dl className="mt-2 grid grid-cols-2 gap-x-3 gap-y-1.5 text-xs">
          <MetaCell label="Duración" value={formatDuration(meta.duration_ms)} />
          <MetaCell
            label="Resolución"
            value={meta.width && meta.height ? `${meta.width}×${meta.height}` : "—"}
          />
          <MetaCell label="FPS" value={meta.fps ? meta.fps.toFixed(2) : "—"} />
          <MetaCell label="Bitrate" value={formatBitrate(meta.bitrate)} />
          <MetaCell label="Códec video" value={meta.vcodec ?? "—"} />
          <MetaCell label="Códec audio" value={meta.acodec ?? "—"} />
        </dl>
      )}
    </div>
  );
}

function MetaCell({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex flex-col rounded-md bg-neutral-900/60 px-2 py-1">
      <dt className="text-[10px] uppercase tracking-wide text-neutral-600">{label}</dt>
      <dd className="truncate font-mono text-neutral-200" title={value}>
        {value}
      </dd>
    </div>
  );
}

/** Contenido indexado dentro de un archivo comprimido (Fase B). */
function ArchiveContents({ entry }: { entry: EntryRow }) {
  const show = isArchive(entry);
  const [items, setItems] = useState<ArchiveEntry[] | null>(null);

  useEffect(() => {
    setItems(null);
    if (!show) return;
    let cancelled = false;
    api
      .listArchiveContents(entry.id)
      .then((x) => !cancelled && setItems(x))
      .catch(() => !cancelled && setItems([]));
    return () => {
      cancelled = true;
    };
  }, [entry.id, show]);

  if (!show || !items) return null;

  const CAP = 500;
  const files = items.filter((i) => !i.is_dir).length;
  const shown = items.slice(0, CAP);

  return (
    <div className="mt-3">
      <div className="flex items-center justify-between text-xs text-neutral-500">
        <span className="flex items-center gap-1.5">
          <Package className="h-3.5 w-3.5" /> Contenido del archivo
        </span>
        {items.length > 0 && (
          <span className="text-[11px] text-neutral-600">{formatCount(files)} archivos</span>
        )}
      </div>
      {items.length === 0 ? (
        <p className="mt-1.5 rounded-md border border-neutral-800 bg-neutral-950 p-3 text-center text-[11px] text-neutral-600">
          sin indexar (re-escaneá con el disco conectado) o archivo vacío
        </p>
      ) : (
        <div className="mt-1.5 max-h-60 overflow-auto rounded-lg border border-neutral-800 bg-neutral-950">
          {shown.map((it) => (
            <div
              key={it.path}
              className="flex items-center gap-2 px-2 py-1 text-xs odd:bg-neutral-900/30"
              title={it.path}
            >
              {it.is_dir ? (
                <FolderClosed className="h-3.5 w-3.5 shrink-0 text-sky-400/70" />
              ) : (
                <FileIcon className="h-3.5 w-3.5 shrink-0 text-neutral-600" />
              )}
              <span className="min-w-0 flex-1 truncate text-neutral-300">{it.path}</span>
              {!it.is_dir && (
                <span className="shrink-0 font-mono text-[11px] text-neutral-500">
                  {formatBytes(it.size)}
                </span>
              )}
            </div>
          ))}
          {items.length > CAP && (
            <div className="px-2 py-1.5 text-center text-[11px] text-neutral-600">
              … y {formatCount(items.length - CAP)} más
            </div>
          )}
        </div>
      )}
    </div>
  );
}

function Actions({ entry, catalogPath }: { entry: EntryRow; catalogPath: string }) {
  const setError = useCatalog((s) => s.setError);
  const [busy, setBusy] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  async function run(kind: "reveal" | "open") {
    setError(null);
    setBusy(kind);
    try {
      if (kind === "reveal") await revealOriginal(entry.id);
      else await openOriginal(entry.id);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  }

  async function copy() {
    await copyText(catalogPath);
    setCopied(true);
    setTimeout(() => setCopied(false), 1200);
  }

  return (
    <div className="mt-3 flex flex-wrap gap-1.5">
      <button
        onClick={() => run("reveal")}
        disabled={busy !== null}
        className="inline-flex items-center gap-1.5 rounded-md border border-neutral-700 px-2.5 py-1 text-xs hover:bg-neutral-800 disabled:opacity-50"
        title="Revelar el original en Finder/Explorer (requiere el disco montado)"
      >
        <FolderSearch className="h-3.5 w-3.5" /> Revelar
      </button>
      {!entry.is_folder && (
        <button
          onClick={() => run("open")}
          disabled={busy !== null}
          className="inline-flex items-center gap-1.5 rounded-md border border-neutral-700 px-2.5 py-1 text-xs hover:bg-neutral-800 disabled:opacity-50"
          title="Abrir el original con la app por defecto (requiere el disco montado)"
        >
          <ExternalLink className="h-3.5 w-3.5" /> Abrir
        </button>
      )}
      <button
        onClick={copy}
        className="inline-flex items-center gap-1.5 rounded-md border border-neutral-700 px-2.5 py-1 text-xs hover:bg-neutral-800"
        title="Copiar la ruta del catálogo"
      >
        {copied ? <Check className="h-3.5 w-3.5 text-emerald-400" /> : <Copy className="h-3.5 w-3.5" />}
        {copied ? "Copiado" : "Copiar ruta"}
      </button>
    </div>
  );
}

function Field({ label, value, mono }: { label: string; value: string; mono?: boolean }) {
  return (
    <div className="flex items-baseline justify-between gap-3">
      <dt className="shrink-0 text-neutral-500">{label}</dt>
      <dd className={`text-right text-neutral-200 ${mono ? "font-mono" : ""}`}>{value}</dd>
    </div>
  );
}
