// Wrappers tipados sobre los comandos Rust (sección 10). El frontend solo
// consume datos ya indexados; nada de FS/parsing acá.
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { SearchFilters } from "./query-parser";

export interface ImportSummary {
  catalog_path: string;
  disks: number;
  entries: number;
  elapsed_ms: number;
}

export interface DiskRow {
  id: number;
  name: string;
  total_size: number;
  file_count: number;
  folder_count: number;
  is_online: boolean;
  location: string | null;
  category: string | null;
  comment: string | null;
}

export interface EntryRow {
  id: number;
  disk_id: number;
  parent_id: number | null;
  name: string;
  is_folder: boolean;
  size_logical: number;
  size_physical: number;
  created_at: number | null;
  modified_at: number | null;
  ext: string | null;
  comment: string | null;
  child_count: number;
}

export interface ExtStat {
  ext: string;
  count: number;
  total_size: number;
}

export interface BigFile {
  id: number;
  name: string;
  disk_name: string;
  size_logical: number;
  path: string;
}

export interface Stats {
  file_count: number;
  folder_count: number;
  total_size: number;
  by_ext: ExtStat[];
  biggest: BigFile[];
}

export interface DupGroup {
  name: string;
  size: number;
  count: number;
  wasted: number;
  items: BigFile[];
}

export interface SearchItem {
  id: number;
  disk_id: number;
  disk_name: string;
  name: string;
  is_folder: boolean;
  size_logical: number;
  modified_at: number | null;
  path: string;
}

export interface SearchResult {
  total: number;
  items: SearchItem[];
  truncated: boolean;
}

export interface TagStat {
  name: string;
  count: number;
}

export interface ThumbCacheSummary {
  total: number;
  generated: number;
  failed: number;
}

export interface VideoMeta {
  duration_ms: number;
  width: number;
  height: number;
  fps: number;
  vcodec: string | null;
  acodec: string | null;
  bitrate: number;
}

export interface VideoIndexSummary {
  total: number;
  indexed: number;
  failed: number;
  frames: number;
  tools_ok: boolean;
}

export interface ArchiveEntry {
  path: string;
  name: string;
  is_dir: boolean;
  size: number;
  modified: number;
}

export interface ArchiveIndexSummary {
  total: number;
  indexed: number;
  failed: number;
  items: number;
}

export interface VolumeInfo {
  name: string;
  mount_path: string;
  fingerprint: string | null;
  total_space: number;
  available_space: number;
  kind: "hdd" | "ssd" | "disk";
  is_removable: boolean;
}

export interface ScanOptions {
  follow_symlinks?: boolean;
  skip_hidden?: boolean;
  skip_time_machine?: boolean;
  exclude_names?: string[];
}

export interface ScanSummary {
  disk_id: number;
  name: string;
  entries: number;
  files: number;
  folders: number;
  replaced: boolean;
  volume_uuid: string | null;
  elapsed_ms: number;
}

export const api = {
  ping: () => invoke<string>("ping"),

  importDcmf: (dcmfPath: string, catalogPath: string) =>
    invoke<ImportSummary>("import_dcmf", { dcmfPath, catalogPath }),

  openCatalog: (catalogPath: string) => invoke<void>("open_catalog", { catalogPath }),

  listDisks: () => invoke<DiskRow[]>("list_disks"),

  // M2 — navegación
  listChildren: (diskId: number, parentId: number | null) =>
    invoke<EntryRow[]>("list_children", { diskId, parentId }),
  entryPath: (entryId: number) => invoke<string>("entry_path", { entryId }),
  getEntry: (entryId: number) => invoke<EntryRow | null>("get_entry", { entryId }),

  // M3 — búsqueda por nombre
  searchEntries: (query: string, limit?: number) =>
    invoke<SearchResult>("search_entries", { query, limit }),

  // M4 — búsqueda por atributos / booleana
  searchAdvanced: (filters: SearchFilters, limit?: number) =>
    invoke<SearchResult>("search_advanced", { filters, limit }),

  // M6 — resolver ruta real (si el disco está montado)
  resolveFsPath: (entryId: number) => invoke<string>("resolve_fs_path", { entryId }),
  // Limpieza — mover el original a la papelera y sacarlo del catálogo
  moveToTrash: (entryId: number) => invoke<string>("move_to_trash", { entryId }),

  // Thumbnails — preview cacheado (offline) o generado on-demand (data URL PNG)
  getThumbnail: (entryId: number, max?: number) =>
    invoke<string>("get_thumbnail", { entryId, max }),
  cacheDiskThumbnails: (diskId: number) =>
    invoke<ThumbCacheSummary>("cache_disk_thumbnails", { diskId }),

  // Tags / keywords
  addEntryTag: (entryId: number, tag: string) =>
    invoke<string[]>("add_entry_tag", { entryId, tag }),
  removeEntryTag: (entryId: number, tag: string) =>
    invoke<string[]>("remove_entry_tag", { entryId, tag }),
  getEntryTags: (entryId: number) => invoke<string[]>("get_entry_tags", { entryId }),
  listTags: () => invoke<TagStat[]>("list_tags"),

  // Fase B — video (ffprobe/ffmpeg) y contenido de archivos comprimidos
  mediaToolsAvailable: () => invoke<boolean>("media_tools_available"),
  indexDiskVideos: (diskId: number) =>
    invoke<VideoIndexSummary>("index_disk_videos", { diskId }),
  getVideoMeta: (entryId: number) => invoke<VideoMeta | null>("get_video_meta", { entryId }),
  getVideoFrames: (entryId: number) => invoke<string[]>("get_video_frames", { entryId }),
  detectVideoScenes: (entryId: number, threshold?: number) =>
    invoke<number[]>("detect_video_scenes", { entryId, threshold }),
  indexDiskArchives: (diskId: number) =>
    invoke<ArchiveIndexSummary>("index_disk_archives", { diskId }),
  listArchiveContents: (entryId: number) =>
    invoke<ArchiveEntry[]>("list_archive_contents", { entryId }),

  // M7 — metadata
  setEntryComment: (entryId: number, comment: string | null) =>
    invoke<void>("set_entry_comment", { entryId, comment }),
  setDiskMeta: (diskId: number, location: string | null, category: string | null, comment: string | null) =>
    invoke<void>("set_disk_meta", { diskId, location, category, comment }),
  writeTextFile: (path: string, contents: string) =>
    invoke<void>("write_text_file", { path, contents }),

  // M8 — estadísticas y duplicados
  catalogStats: (diskId?: number) => invoke<Stats>("catalog_stats", { diskId }),
  findDuplicates: (minSize?: number, limit?: number) =>
    invoke<DupGroup[]>("find_duplicates", { minSize, limit }),

  // M5 — escaneo / detección de discos
  listVolumes: () => invoke<VolumeInfo[]>("list_volumes"),
  scanDisk: (mountPath: string, name?: string, options?: ScanOptions) =>
    invoke<ScanSummary>("scan_disk", { mountPath, name, options }),
  startVolumeWatch: () => invoke<void>("start_volume_watch"),
  refreshOnlineStatus: () => invoke<DiskRow[]>("refresh_online_status"),

  // M9 — conector remoto seguro
  agentStart: (bind?: string, scopes?: string) => invoke<AgentStatus>("agent_start", { bind, scopes }),
  agentStop: () => invoke<void>("agent_stop"),
  agentStatus: () => invoke<AgentStatus>("agent_status"),
  agentPairCode: () => invoke<string>("agent_pair_code"),
  agentDevices: () => invoke<DeviceRow[]>("agent_devices"),
  agentRevoke: (deviceId: string, revoked: boolean) => invoke<void>("agent_revoke", { deviceId, revoked }),
};

export interface AgentStatus {
  running: boolean;
  addr: string | null;
}

export interface DeviceRow {
  id: string;
  name: string;
  scopes: string;
  created_at: number;
  last_seen: number;
  revoked: boolean;
}

// Eventos del watcher de volúmenes (M5).
export const onVolumeAdded = (cb: (v: VolumeInfo) => void): Promise<UnlistenFn> =>
  listen<VolumeInfo>("volume-added", (e) => cb(e.payload));
export const onVolumeRemoved = (cb: (v: VolumeInfo) => void): Promise<UnlistenFn> =>
  listen<VolumeInfo>("volume-removed", (e) => cb(e.payload));

// Progreso de escaneo / indexado (Fase B).
export interface ScanProgress {
  count: number;
  pct: number; // -1 si se desconoce el total
}
export interface IndexProgress {
  phase: "thumbnails" | "videos" | "archives";
  done: number;
  total: number;
}
export const onScanProgress = (cb: (p: ScanProgress) => void): Promise<UnlistenFn> =>
  listen<ScanProgress>("scan-progress", (e) => cb(e.payload));
export const onIndexProgress = (cb: (p: IndexProgress) => void): Promise<UnlistenFn> =>
  listen<IndexProgress>("index-progress", (e) => cb(e.payload));
