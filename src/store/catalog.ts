import { create } from "zustand";
import {
  api,
  type DiskRow,
  type EntryRow,
  type ImportSummary,
  type SearchResult,
} from "../lib/ipc";
import { parseQuery, hasCriteria, type SearchFilters } from "../lib/query-parser";

export interface Crumb {
  id: number | null; // null = raíz del disco
  name: string;
}

type Mode = "browse" | "search";

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
  selectedEntryId: number | null;
  contentLoading: boolean;

  // Búsqueda (M3/M4)
  searchQuery: string;
  searchResult: SearchResult | null;
  searching: boolean;
  parsedFilters: SearchFilters | null;

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

  runSearch: (query: string) => Promise<void>;
  clearSearch: () => void;

  addOpenCatalog: (path: string) => void;
  switchCatalog: (path: string) => Promise<void>;
  closeCatalog: (path: string) => Promise<void>;
}

function baseName(path: string): string {
  return path.replace(/\\/g, "/").split("/").filter(Boolean).pop() ?? path;
}

const RESET_NAV = {
  mode: "browse" as Mode,
  selectedDiskId: null,
  breadcrumb: [],
  contentEntries: [],
  selectedEntryId: null,
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
  contentLoading: false,

  searchQuery: "",
  searchResult: null,
  searching: false,
  parsedFilters: null,

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
    set({
      mode: "browse",
      selectedDiskId: disk.id,
      breadcrumb: [{ id: null, name: disk.name }],
      selectedEntryId: null,
      contentLoading: true,
    });
    try {
      const entries = await api.listChildren(disk.id, null);
      set({ contentEntries: entries, contentLoading: false });
    } catch (e) {
      set({ error: String(e), contentLoading: false });
    }
  },

  openFolder: async (entry) => {
    const diskId = get().selectedDiskId ?? entry.disk_id;
    set((s) => ({
      mode: "browse",
      selectedDiskId: diskId,
      breadcrumb: [...s.breadcrumb, { id: entry.id, name: entry.name }],
      selectedEntryId: null,
      contentLoading: true,
    }));
    try {
      const entries = await api.listChildren(diskId, entry.id);
      set({ contentEntries: entries, contentLoading: false });
    } catch (e) {
      set({ error: String(e), contentLoading: false });
    }
  },

  gotoFolder: async (diskId, parentId, breadcrumb) => {
    set({
      mode: "browse",
      selectedDiskId: diskId,
      breadcrumb,
      selectedEntryId: null,
      contentLoading: true,
    });
    try {
      const entries = await api.listChildren(diskId, parentId);
      set({ contentEntries: entries, contentLoading: false });
    } catch (e) {
      set({ error: String(e), contentLoading: false });
    }
  },

  navigateToCrumb: async (index) => {
    const { breadcrumb, selectedDiskId } = get();
    if (selectedDiskId == null) return;
    const target = breadcrumb[index];
    set({
      breadcrumb: breadcrumb.slice(0, index + 1),
      selectedEntryId: null,
      contentLoading: true,
      mode: "browse",
    });
    try {
      const entries = await api.listChildren(selectedDiskId, target.id);
      set({ contentEntries: entries, contentLoading: false });
    } catch (e) {
      set({ error: String(e), contentLoading: false });
    }
  },

  selectEntry: (id) => set({ selectedEntryId: id }),

  runSearch: async (query) => {
    set({ searchQuery: query });
    const filters = parseQuery(query);
    if (!hasCriteria(filters)) {
      set({ mode: "browse", searchResult: null, searching: false, parsedFilters: null });
      return;
    }
    set({ mode: "search", searching: true, selectedEntryId: null, parsedFilters: filters });
    try {
      const result = await api.searchAdvanced(filters, 2000);
      // Evitar pisar un resultado más nuevo si el usuario siguió tipeando.
      if (get().searchQuery === query) {
        set({ searchResult: result, searching: false });
      }
    } catch (e) {
      set({ error: String(e), searching: false });
    }
  },

  clearSearch: () =>
    set({ mode: "browse", searchQuery: "", searchResult: null, parsedFilters: null }),

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
