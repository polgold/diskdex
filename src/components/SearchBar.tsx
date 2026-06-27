import { useEffect, useRef, useState } from "react";
import { Search, X, Sparkles, RefreshCw, Loader2, Clapperboard, Mic, Settings as SettingsIcon } from "lucide-react";
import { useCatalog } from "../store/catalog";
import { useT } from "../lib/i18n";
import { SettingsDialog } from "./SettingsDialog";
import {
  api,
  onAiIndexProgress,
  onAiTranscribeProgress,
  type AiStatus,
  type AiIndexProgress,
} from "../lib/ipc";

/** Buscador global (M3) con debounce y atajo ⌘/Ctrl+F. Incluye el modo de
 *  búsqueda semántica por contenido (IA Fase 1) cuando el build la trae. */
export function SearchBar() {
  const t = useT();
  const [value, setValue] = useState("");
  const runSearch = useCatalog((s) => s.runSearch);
  const clearSearch = useCatalog((s) => s.clearSearch);
  const semantic = useCatalog((s) => s.semantic);
  const setSemantic = useCatalog((s) => s.setSemantic);
  const threshold = useCatalog((s) => s.semanticThreshold);
  const setThreshold = useCatalog((s) => s.setSemanticThreshold);
  const catalogPath = useCatalog((s) => s.catalogPath);
  const selectedDiskId = useCatalog((s) => s.selectedDiskId);
  const disks = useCatalog((s) => s.disks);
  const inputRef = useRef<HTMLInputElement>(null);

  const nlClaude = useCatalog((s) => s.nlClaude);
  const [showSettings, setShowSettings] = useState(false);
  const [aiAvail, setAiAvail] = useState(false);
  const [status, setStatus] = useState<AiStatus | null>(null);
  const [prog, setProg] = useState<AiIndexProgress | null>(null);
  const [indexing, setIndexing] = useState(false);

  // Debounce de 180 ms: búsqueda incremental sin saturar el backend.
  useEffect(() => {
    const id = setTimeout(() => runSearch(value), 180);
    return () => clearTimeout(id);
  }, [value, runSearch]);

  // ⌘/Ctrl+F enfoca el buscador.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "f") {
        e.preventDefault();
        inputRef.current?.focus();
        inputRef.current?.select();
      }
      if (e.key === "Escape" && document.activeElement === inputRef.current) {
        setValue("");
        clearSearch();
        inputRef.current?.blur();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [clearSearch]);

  // ¿Hay IA en este build? (oculta toda la UI semántica si no). Lo replico en el
  // store para que el menú contextual ("Buscar similares") también lo sepa.
  const setAiAvailStore = useCatalog((s) => s.setAiAvailable);
  useEffect(() => {
    api
      .aiAvailable()
      .then((b) => {
        setAiAvail(b);
        setAiAvailStore(b);
      })
      .catch(() => setAiAvail(false));
  }, [setAiAvailStore]);

  // Progreso del indexado semántico y de la transcripción (eventos del backend).
  useEffect(() => {
    const a = onAiIndexProgress(setProg);
    const b = onAiTranscribeProgress(setProg);
    return () => {
      void a.then((f) => f());
      void b.then((f) => f());
    };
  }, []);

  const refreshStatus = () => {
    if (!aiAvail) return;
    api.aiStatus().then(setStatus).catch(() => setStatus(null));
  };

  // Refresca el estado del índice al activar la IA o cambiar de catálogo.
  useEffect(() => {
    if (semantic && aiAvail) refreshStatus();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [semantic, aiAvail, catalogPath]);

  const doIndex = async (rebuild: boolean) => {
    setIndexing(true);
    try {
      await api.aiIndex(rebuild);
    } catch (e) {
      useCatalog.getState().setError(String(e));
    } finally {
      setIndexing(false);
      setProg(null);
      refreshStatus();
      // Re-corre la búsqueda actual ahora que hay más vectores.
      if (value.trim()) runSearch(value);
    }
  };

  const doIndexVideos = async () => {
    if (selectedDiskId == null) return;
    setIndexing(true);
    try {
      await api.aiIndexVideos(selectedDiskId, 8);
    } catch (e) {
      useCatalog.getState().setError(String(e));
    } finally {
      setIndexing(false);
      setProg(null);
      refreshStatus();
      if (value.trim()) runSearch(value);
    }
  };

  const doTranscribe = async () => {
    if (selectedDiskId == null) return;
    setIndexing(true);
    try {
      await api.aiTranscribeDisk(selectedDiskId);
    } catch (e) {
      useCatalog.getState().setError(String(e));
    } finally {
      setIndexing(false);
      setProg(null);
      if (value.trim()) runSearch(value);
    }
  };

  const selectedDisk = disks.find((d) => d.id === selectedDiskId) ?? null;

  const pct =
    prog && prog.total > 0 ? Math.round((prog.done / prog.total) * 100) : null;
  const needIndex = !!status && status.embedded === 0;

  return (
    <div className="relative w-full max-w-md">
      <div className="relative w-full">
        {semantic ? (
          <Sparkles className="pointer-events-none absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-violet-400" />
        ) : (
          <Search className="pointer-events-none absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-neutral-500" />
        )}
        <input
          id="diskdex-search"
          ref={inputRef}
          value={value}
          onChange={(e) => setValue(e.target.value)}
          placeholder={semantic ? t("ai.placeholder") : t("search.placeholder")}
          title={semantic ? t("ai.toggle") : t("search.tokensHint")}
          className={`w-full rounded-md border bg-neutral-900 py-1.5 pl-8 text-xs text-neutral-200 placeholder:text-neutral-600 focus:outline-none ${
            aiAvail ? "pr-16" : "pr-8"
          } ${semantic ? "border-violet-600/60 focus:border-violet-500" : "border-neutral-700 focus:border-neutral-500"}`}
        />
        <div className="absolute right-1.5 top-1/2 flex -translate-y-1/2 items-center gap-0.5">
          {value && (
            <button
              onClick={() => {
                setValue("");
                clearSearch();
              }}
              className="rounded p-0.5 text-neutral-500 hover:text-neutral-200"
            >
              <X className="h-3.5 w-3.5" />
            </button>
          )}
          {aiAvail && (
            <button
              onClick={() => setSemantic(!semantic)}
              title={t("ai.toggle")}
              className={`rounded p-0.5 ${
                semantic
                  ? "text-violet-400 hover:text-violet-300"
                  : "text-neutral-500 hover:text-neutral-200"
              }`}
            >
              <Sparkles className="h-3.5 w-3.5" />
            </button>
          )}
          <button
            onClick={() => setShowSettings(true)}
            title={t("settings.title")}
            className={`rounded p-0.5 ${nlClaude ? "text-violet-400 hover:text-violet-300" : "text-neutral-500 hover:text-neutral-200"}`}
          >
            <SettingsIcon className="h-3.5 w-3.5" />
          </button>
        </div>
      </div>

      {showSettings && <SettingsDialog onClose={() => setShowSettings(false)} />}

      {/* Panel IA: estado del índice, indexar, umbral. */}
      {aiAvail && semantic && (
        <div className="absolute left-0 right-0 top-full z-30 mt-1 rounded-md border border-neutral-700 bg-neutral-900 p-2 text-[11px] text-neutral-300 shadow-lg">
          {indexing || (prog && prog.total < 0) ? (
            <div className="space-y-1">
              <div className="flex items-center gap-1.5 text-neutral-400">
                <Loader2 className="h-3 w-3 animate-spin" />
                {prog && prog.total < 0
                  ? t("ai.loadingModel")
                  : t("ai.indexing", { done: prog?.done ?? 0, total: prog?.total ?? 0 })}
              </div>
              <div className="h-1 w-full overflow-hidden rounded bg-neutral-800">
                <div
                  className="h-full bg-violet-500 transition-all"
                  style={{ width: pct != null ? `${pct}%` : "30%" }}
                />
              </div>
            </div>
          ) : (
            <div className="flex items-center justify-between gap-2">
              <span className="text-neutral-400">
                {status
                  ? t("ai.status", { embedded: status.embedded, candidates: status.candidates })
                  : "…"}
              </span>
              <div className="flex items-center gap-1">
                <button
                  onClick={() => doIndex(false)}
                  className="rounded bg-violet-600/80 px-2 py-0.5 font-medium text-white hover:bg-violet-600"
                >
                  {t("ai.index")}
                </button>
                {status && status.embedded > 0 && (
                  <button
                    onClick={() => doIndex(true)}
                    title={t("ai.reindex")}
                    className="rounded p-1 text-neutral-500 hover:text-neutral-200"
                  >
                    <RefreshCw className="h-3 w-3" />
                  </button>
                )}
              </div>
            </div>
          )}

          {/* Fase 2/4 — procesar el disco seleccionado (montado): frames de video y audio. */}
          {!indexing && selectedDisk && (
            <div className="mt-1.5 space-y-1">
              <button
                onClick={doIndexVideos}
                title={t("ai.indexVideosHint")}
                className="flex w-full items-center justify-center gap-1 rounded border border-violet-700/50 px-2 py-1 text-violet-300 hover:bg-violet-600/15"
              >
                <Clapperboard className="h-3 w-3" />
                {t("ai.indexVideos", { disk: selectedDisk.name })}
              </button>
              <button
                onClick={doTranscribe}
                title={t("ai.transcribeHint")}
                className="flex w-full items-center justify-center gap-1 rounded border border-violet-700/50 px-2 py-1 text-violet-300 hover:bg-violet-600/15"
              >
                <Mic className="h-3 w-3" />
                {t("ai.transcribe", { disk: selectedDisk.name })}
              </button>
            </div>
          )}

          {needIndex && !indexing && (
            <div className="mt-1 text-neutral-500">{t("ai.needIndex")}</div>
          )}

          {/* Umbral de relevancia. */}
          <div className="mt-2 flex items-center gap-2">
            <span className="text-neutral-500">{t("ai.threshold")}</span>
            <input
              type="range"
              min={0}
              max={0.2}
              step={0.005}
              value={threshold}
              onChange={(e) => {
                setThreshold(parseFloat(e.target.value));
                if (value.trim()) runSearch(value);
              }}
              className="h-1 flex-1 accent-violet-500"
            />
            <span className="w-8 text-right tabular-nums text-neutral-400">
              {threshold.toFixed(3)}
            </span>
          </div>
        </div>
      )}
    </div>
  );
}
