import { create } from "zustand";
import {
  api,
  type DiskRow,
  type EntryRow,
  type ImportSummary,
  type SearchResult,
} from "../lib/ipc";
import { parseQuery, hasCriteria, type SearchFilters } from "../lib/query-parser";
import { parseNaturalQuery, applyNLFilters, hasStructured, type NLQuery } from "../lib/nl-parser";
import { claudeNLToQuery } from "../lib/claude-nl";
import { getClaudeKey, getNlClaudeEnabled, setNlClaudeEnabled } from "../lib/settings";

export interface Crumb {
  id: number | null; // null = raíz del disco
  name: string;
}

type Mode = "browse" | "search";
export type ViewMode = "table" | "grid";

function initialViewMode(): ViewMode {
  try {
    return localStorage.getItem("diskdex:viewmode") === "grid" ? "grid" : "table";
  } catch {
    return "table";
  }
}

export interface OpenCatalog {
  path: string;
  name: string;
}

interface CatalogState {
  catalogPath: string | null;
  openCatalogs: OpenCatalog[];
  disks: DiskRow[];
  loading: boolean;
  error: string | null;
  lastImport: ImportSummary | null;

  // Navegación (M2)
  mode: Mode;
  selectedDiskId: number | null;
  breadcrumb: Crumb[];
  contentEntries: EntryRow[];
  selectedEntryId: number | null; // primario (último clickeado) → alimenta el inspector
  selectedIds: number[]; // multi-selección (incluye al primario)
  contentLoading: boolean;

  // Vista (tabla / galería)
  viewMode: ViewMode;
  setViewMode: (m: ViewMode) => void;

  // Búsqueda (M3/M4)
  searchQuery: string;
  searchResult: SearchResult | null;
  searching: boolean;
  parsedFilters: SearchFilters | null;

  // Búsqueda semántica (IA Fase 1)
  semantic: boolean;
  semanticThreshold: number;
  setSemantic: (b: boolean) => void;
  setSemanticThreshold: (t: number) => void;
  // IA disponible en el build (feature `ai`) — la UI muestra/oculta lo semántico.
  aiAvailable: boolean;
  setAiAvailable: (b: boolean) => void;
  // C3 — interpretar la búsqueda con Claude (cada usuario pone su API key).
  nlClaude: boolean;
  setNlClaude: (b: boolean) => void;
  // Buscar visualmente similares a una entrada (Fase 5)
  runSimilar: (entryId: number) => Promise<void>;

  refreshDisks: () => Promise<void>;
  refreshOnlineFromDisk: () => Promise<void>;
  setImportResult: (summary: ImportSummary) => Promise<void>;
  setError: (e: string | null) => void;
  setLoading: (b: boolean) => void;

  openDisk: (disk: DiskRow) => Promise<void>;
  openFolder: (entry: EntryRow) => Promise<void>;
  gotoFolder: (diskId: number, parentId: number | null, breadcrumb: Crumb[]) => Promise<void>;
  navigateToCrumb: (index: number) => Promise<void>;
  selectEntry: (id: number | null) => void;
  setSelection: (ids: number[], primary: number | null) => void;

  runSearch: (query: string) => Promise<void>;
  clearSearch: () => void;
  reloadCurrent: () => Promise<void>;

  addOpenCatalog: (path: string) => void;
  switchCatalog: (path: string) => Promise<void>;
  closeCatalog: (path: string) => Promise<void>;
}

function baseName(path: string): string {
  return path.replace(/\\/g, "/").split("/").filter(Boolean).pop() ?? path;
}

// Tokens de secuencia: cada navegación/búsqueda toma uno; al resolver, solo
// aplica su resultado si sigue siendo el último pedido. Evita que una respuesta
// lenta y vieja pise el contenido nuevo (cambiar de disco no refrescaba) o deje
// el "cargando…"/"buscando…" colgado para siempre.
let navToken = 0;
let searchToken = 0;

const RESET_NAV = {
  mode: "browse" as Mode,
  selectedDiskId: null,
  breadcrumb: [],
  contentEntries: [],
  selectedEntryId: null,
  selectedIds: [],
  searchQuery: "",
  searchResult: null,
  parsedFilters: null,
};

export const useCatalog = create<CatalogState>((set, get) => ({
  catalogPath: null,
  openCatalogs: [],
  disks: [],
  loading: false,
  error: null,
  lastImport: null,

  mode: "browse",
  selectedDiskId: null,
  breadcrumb: [],
  contentEntries: [],
  selectedEntryId: null,
  selectedIds: [],
  contentLoading: false,

  viewMode: initialViewMode(),
  setViewMode: (viewMode) => {
    try {
      localStorage.setItem("diskdex:viewmode", viewMode);
    } catch {
      /* ignore */
    }
    set({ viewMode });
  },

  searchQuery: "",
  searchResult: null,
  searching: false,
  parsedFilters: null,

  semantic: false,
  semanticThreshold: (() => {
    try {
      const v = parseFloat(localStorage.getItem("diskdex:semanticThreshold") ?? "");
      return Number.isFinite(v) ? v : 0.05;
    } catch {
      return 0.05;
    }
  })(),
  setSemantic: (b) => {
    set({ semantic: b });
    void get().runSearch(get().searchQuery);
  },
  setSemanticThreshold: (th) => {
    try {
      localStorage.setItem("diskdex:semanticThreshold", String(th));
    } catch {
      /* ignore */
    }
    set({ semanticThreshold: th });
  },

  aiAvailable: false,
  setAiAvailable: (b) => set({ aiAvailable: b }),

  nlClaude: getNlClaudeEnabled(),
  setNlClaude: (b) => {
    setNlClaudeEnabled(b);
    set({ nlClaude: b });
    void get().runSearch(get().searchQuery);
  },
  runSimilar: async (entryId) => {
    const token = ++searchToken;
    set({
      mode: "search",
      searching: true,
      searchResult: null,
      selectedEntryId: null,
      selectedIds: [],
      parsedFilters: null,
      searchQuery: "",
    });
    try {
      const items = await api.aiSimilar(entryId, 0, 300);
      if (token === searchToken) {
        set({ searchResult: { total: items.length, items, truncated: false }, searching: false });
      }
    } catch (e) {
      if (token === searchToken) {
        set({ searching: false });
        get().setError(String(e));
      }
    }
  },

  refreshDisks: async () => {
    try {
      set({ disks: await api.listDisks() });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  refreshOnlineFromDisk: async () => {
    try {
      set({ disks: await api.refreshOnlineStatus() });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  setImportResult: async (summary) => {
    set({ catalogPath: summary.catalog_path, lastImport: summary, error: null });
    await get().refreshDisks();
  },

  setError: (error) => set({ error }),
  setLoading: (loading) => set({ loading }),

  openDisk: async (disk) => {
    const token = ++navToken;
    set({
      mode: "browse",
      selectedDiskId: disk.id,
      breadcrumb: [{ id: null, name: disk.name }],
      selectedEntryId: null,
      selectedIds: [],
      contentEntries: [],
      contentLoading: true,
    });
    try {
      const entries = await api.listChildren(disk.id, null);
      if (token === navToken) set({ contentEntries: entries, contentLoading: false });
    } catch (e) {
      if (token === navToken) set({ error: String(e), contentLoading: false });
    }
  },

  openFolder: async (entry) => {
    const token = ++navToken;
    const diskId = get().selectedDiskId ?? entry.disk_id;
    set((s) => ({
      mode: "browse",
      selectedDiskId: diskId,
      breadcrumb: [...s.breadcrumb, { id: entry.id, name: entry.name }],
      selectedEntryId: null,
      selectedIds: [],
      contentEntries: [],
      contentLoading: true,
    }));
    try {
      const entries = await api.listChildren(diskId, entry.id);
      if (token === navToken) set({ contentEntries: entries, contentLoading: false });
    } catch (e) {
      if (token === navToken) set({ error: String(e), contentLoading: false });
    }
  },

  gotoFolder: async (diskId, parentId, breadcrumb) => {
    const token = ++navToken;
    set({
      mode: "browse",
      selectedDiskId: diskId,
      breadcrumb,
      selectedEntryId: null,
      selectedIds: [],
      contentEntries: [],
      contentLoading: true,
    });
    try {
      const entries = await api.listChildren(diskId, parentId);
      if (token === navToken) set({ contentEntries: entries, contentLoading: false });
    } catch (e) {
      if (token === navToken) set({ error: String(e), contentLoading: false });
    }
  },

  navigateToCrumb: async (index) => {
    const { breadcrumb, selectedDiskId } = get();
    if (selectedDiskId == null) return;
    const token = ++navToken;
    const target = breadcrumb[index];
    set({
      breadcrumb: breadcrumb.slice(0, index + 1),
      selectedEntryId: null,
      selectedIds: [],
      contentEntries: [],
      contentLoading: true,
      mode: "browse",
    });
    try {
      const entries = await api.listChildren(selectedDiskId, target.id);
      if (token === navToken) set({ contentEntries: entries, contentLoading: false });
    } catch (e) {
      if (token === navToken) set({ error: String(e), contentLoading: false });
    }
  },

  selectEntry: (id) => set({ selectedEntryId: id, selectedIds: id == null ? [] : [id] }),

  setSelection: (ids, primary) => set({ selectedIds: ids, selectedEntryId: primary }),

  runSearch: async (query) => {
    set({ searchQuery: query });

    // C3 — Lenguaje natural vía Claude (si el usuario lo activó y cargó su API key).
    // Claude interpreta la frase → filtros (lugar/luz/fecha/tipo/tamaño) + concepto
    // visual residual. Funciona aunque NO haya modelo CLIP: cae a búsqueda por
    // atributos. Si la API falla, degrada al parser local.
    const claudeKey = getClaudeKey();
    if (get().nlClaude && claudeKey.trim()) {
      if (!query.trim()) {
        set({ mode: "browse", searchResult: null, searching: false, parsedFilters: null });
        return;
      }
      const token = ++searchToken;
      set({ mode: "search", searching: true, searchResult: null, selectedEntryId: null, selectedIds: [], parsedFilters: null });
      try {
        let nl: NLQuery;
        try {
          nl = await claudeNLToQuery(query, claudeKey);
        } catch {
          nl = parseNaturalQuery(query); // degradar al parser local si la API falla
        }
        if (token !== searchToken) return;
        set({ parsedFilters: nl.filters });
        const hasConcept = nl.concept.trim().length > 0;
        let items: SearchResult["items"];
        if (hasConcept && get().aiAvailable) {
          const [sem, spoken] = await Promise.all([
            api.aiSearch(nl.concept, get().semanticThreshold, 2000),
            api.aiSearchTranscripts(nl.concept, 300, nl.lang).catch(() => []),
          ]);
          const visual = applyNLFilters(sem, nl.filters);
          const spokenF = applyNLFilters(spoken, nl.filters);
          const seen = new Set(visual.map((i) => i.id));
          items = [...visual, ...spokenF.filter((i) => !seen.has(i.id))].slice(0, 300);
        } else {
          // Sin modelo visual: si quedó un concepto, lo usamos como texto (FTS por nombre).
          const f: SearchFilters = { ...nl.filters };
          if (hasConcept && !f.text) f.text = nl.concept;
          const r = await api.searchAdvanced(f, 2000);
          items = r.items;
        }
        if (token === searchToken) {
          set({ searchResult: { total: items.length, items, truncated: false }, searching: false });
        }
      } catch (e) {
        if (token === searchToken) set({ error: String(e), searching: false });
      }
      return;
    }

    // Modo semántico (IA Fase 3): la query es lenguaje natural → se separa en
    // filtros estructurados (tipo/fecha/tamaño) + concepto visual. El concepto se
    // embebe y rankea por contenido; los filtros se aplican sobre el resultado.
    if (get().semantic) {
      const nl = parseNaturalQuery(query);
      const hasConcept = nl.concept.trim().length > 0;
      if (!hasConcept && !hasStructured(nl.filters)) {
        set({ mode: "browse", searchResult: null, searching: false, parsedFilters: null });
        return;
      }
      const token = ++searchToken;
      set({ mode: "search", searching: true, searchResult: null, selectedEntryId: null, selectedIds: [], parsedFilters: nl.filters });
      try {
        let items: SearchResult["items"];
        if (hasConcept) {
          // Busca por contenido VISUAL (embeddings) y por lo que se DICE
          // (transcripciones, Fase 4) en paralelo, y fusiona. Pido límite amplio
          // para que el post-filtro estructurado no se quede corto.
          const [sem, spoken] = await Promise.all([
            api.aiSearch(nl.concept, get().semanticThreshold, 2000),
            api.aiSearchTranscripts(nl.concept, 300, nl.lang).catch(() => []),
          ]);
          const visual = applyNLFilters(sem, nl.filters);
          const spokenF = applyNLFilters(spoken, nl.filters);
          const seen = new Set(visual.map((i) => i.id));
          items = [...visual, ...spokenF.filter((i) => !seen.has(i.id))].slice(0, 300);
        } else {
          // Solo filtros → búsqueda por atributos clásica (sin IA).
          const r = await api.searchAdvanced(nl.filters, 2000);
          items = r.items;
        }
        if (token === searchToken) {
          set({ searchResult: { total: items.length, items, truncated: false }, searching: false });
        }
      } catch (e) {
        if (token === searchToken) set({ error: String(e), searching: false });
      }
      return;
    }

    const filters = parseQuery(query);
    if (!hasCriteria(filters)) {
      set({ mode: "browse", searchResult: null, searching: false, parsedFilters: null });
      return;
    }
    const token = ++searchToken;
    set({ mode: "search", searching: true, searchResult: null, selectedEntryId: null, selectedIds: [], parsedFilters: filters });
    try {
      const result = await api.searchAdvanced(filters, 2000);
      // Solo el último pedido aplica su resultado (y siempre apaga "buscando…").
      if (token === searchToken) set({ searchResult: result, searching: false });
    } catch (e) {
      if (token === searchToken) set({ error: String(e), searching: false });
    }
  },

  clearSearch: () =>
    set({ mode: "browse", searchQuery: "", searchResult: null, parsedFilters: null }),

  // Recarga el listado actual (carpeta en browse, o resultados en search) — p.ej.
  // tras mover un archivo a la papelera.
  reloadCurrent: async () => {
    const s = get();
    if (s.mode === "search") {
      await get().runSearch(s.searchQuery);
    } else if (s.selectedDiskId != null) {
      const token = ++navToken;
      const parent = s.breadcrumb[s.breadcrumb.length - 1]?.id ?? null;
      try {
        const entries = await api.listChildren(s.selectedDiskId, parent);
        if (token === navToken) set({ contentEntries: entries, selectedEntryId: null, selectedIds: [] });
      } catch (e) {
        if (token === navToken) set({ error: String(e) });
      }
    }
  },

  addOpenCatalog: (path) =>
    set((s) =>
      s.openCatalogs.some((c) => c.path === path)
        ? s
        : { openCatalogs: [...s.openCatalogs, { path, name: baseName(path) }] }
    ),

  switchCatalog: async (path) => {
    if (get().catalogPath === path) return;
    try {
      await api.openCatalog(path);
      set({ catalogPath: path, ...RESET_NAV });
      await get().refreshOnlineFromDisk();
    } catch (e) {
      set({ error: String(e) });
    }
  },

  closeCatalog: async (path) => {
    const remaining = get().openCatalogs.filter((c) => c.path !== path);
    set({ openCatalogs: remaining });
    if (get().catalogPath === path) {
      if (remaining.length > 0) {
        await get().switchCatalog(remaining[remaining.length - 1].path);
      } else {
        set({ catalogPath: null, disks: [], ...RESET_NAV });
      }
    }
  },
}));
