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

export interface TrashFailure {
  id: number;
  name: string;
  error: string;
}

export interface TrashSummary {
  moved: number;
  failed: TrashFailure[];
}

export interface DiskDetail {
  id: number;
  name: string;
  total_size: number;
  file_count: number;
  folder_count: number;
  is_online: boolean;
  kind: string | null;
  capacity: number | null;
  scanned_at: number | null;
  live_total: number | null;
  live_free: number | null;
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

/** A2/A2-meta — metadata enriquecida de una entrada (hash + GPS/cámara/captura). */
export interface EntryMeta {
  content_hash: string | null;
  gps_lat: number | null;
  gps_lon: number | null;
  gps_place: string | null;
  captured_at: number | null;
  camera_make: string | null;
  camera_model: string | null;
  light_phase: string | null;
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
  /** Fuerza escaneo completo (desactiva el re-escaneo incremental por mtime). */
  force_full?: boolean;
  /** Excluir basura (node_modules, caches, papeleras…). Opt-in. */
  exclude_junk?: boolean;
  /** Escaneo enriquecido: calcula hash BLAKE3 por archivo (auditoría de backup). Opt-in, lento. */
  enrich?: boolean;
}

export interface CopyFailure {
  rel_path: string;
  error: string;
}

/** B2 — resultado de copiar lo faltante. */
export interface CopyResult {
  dry_run: boolean;
  planned: number;
  planned_bytes: number;
  copied: number;
  copied_bytes: number;
  verified: number;
  skipped: number;
  cancelled: boolean;
  failed: CopyFailure[];
  sample: string[];
}

/** B2 — progreso de copia (evento "copy-progress" / "gather-progress"). */
export interface CopyProgress {
  count: number;
  total: number;
  copied: number;
  bytes: number;
}

/** D — archivo dentro de un grupo del plan de copia multi-disco. */
export interface GatherFile {
  entry_id: number;
  name: string;
  size: number;
  path: string;
}

/** D — grupo de archivos de un mismo disco. */
export interface GatherGroup {
  disk_id: number;
  disk_name: string;
  is_online: boolean;
  total: number;
  total_bytes: number;
  files: GatherFile[];
}

/** D — plan de copia multi-disco (agrupado por disco). */
export interface GatherPlan {
  groups: GatherGroup[];
  total_files: number;
  total_bytes: number;
  skipped_folders: number;
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
  /** Carpetas reutilizadas sin descender el FS (re-escaneo incremental). */
  reused_dirs: number;
}

export const api = {
  ping: () => invoke<string>("ping"),

  importDcmf: (dcmfPath: string, catalogPath: string) =>
    invoke<ImportSummary>("import_dcmf", { dcmfPath, catalogPath }),
  dcmfDiskNames: (dcmfPath: string) => invoke<string[]>("dcmf_disk_names", { dcmfPath }),
  importDcmfMerge: (dcmfPath: string, replace: boolean) =>
    invoke<ImportSummary>("import_dcmf_merge", { dcmfPath, replace }),

  openCatalog: (catalogPath: string) => invoke<void>("open_catalog", { catalogPath }),

  listDisks: () => invoke<DiskRow[]>("list_disks"),
  diskDetail: (diskId: number) => invoke<DiskDetail>("disk_detail", { diskId }),

  // M2 — navegación
  listChildren: (diskId: number, parentId: number | null) =>
    invoke<EntryRow[]>("list_children", { diskId, parentId }),
  entryPath: (entryId: number) => invoke<string>("entry_path", { entryId }),
  getEntry: (entryId: number) => invoke<EntryRow | null>("get_entry", { entryId }),
  getEntryMeta: (entryId: number) => invoke<EntryMeta>("get_entry_meta", { entryId }),

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
  moveEntriesToTrash: (entryIds: number[]) =>
    invoke<TrashSummary>("move_entries_to_trash", { entryIds }),

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
  deleteDisk: (diskId: number) => invoke<void>("delete_disk", { diskId }),
  writeTextFile: (path: string, contents: string) =>
    invoke<void>("write_text_file", { path, contents }),

  // Sesión persistente (último catálogo) — durable en disco vía backend.
  saveSession: (contents: string) => invoke<void>("save_session", { contents }),
  loadSession: () => invoke<string | null>("load_session"),

  // M8 — estadísticas y duplicados
  catalogStats: (diskId?: number) => invoke<Stats>("catalog_stats", { diskId }),
  findDuplicates: (minSize?: number, limit?: number) =>
    invoke<DupGroup[]>("find_duplicates", { minSize, limit }),

  cancelCopy: (destDiskId: number) => invoke<void>("cancel_copy", { destDiskId }),
  // D — plan de copia multi-disco (reunir archivos repartidos en varios discos)
  gatherPlan: (entryIds: number[]) => invoke<GatherPlan>("gather_plan", { entryIds }),
  gatherCopy: (entryIds: number[], destDir: string) =>
    invoke<CopyResult>("gather_copy", { args: { entry_ids: entryIds, dest_dir: destDir } }),
  cancelGather: (destDir: string) => invoke<void>("cancel_gather", { destDir }),
  // Comparación de discos/carpetas y copia de respaldo. `deep` elige el criterio:
  // false = por tamaño (instantáneo), true = por hash BLAKE3 (detecta corrupción).
  // Comparar es offline; copiar exige ambos discos montados y se cancela con cancelCopy.
  compareDisks: (
    srcDiskId: number,
    dstDiskId: number,
    srcRootId: number | null,
    dstRootId: number | null,
    deep: boolean,
    limit?: number,
  ) => invoke<DiskDiff>("compare_disks", { srcDiskId, dstDiskId, srcRootId, dstRootId, deep, limit }),
  copyMissing: (
    srcDiskId: number,
    dstDiskId: number,
    srcRootId: number | null,
    dstRootId: number | null,
    deep: boolean,
    includeMismatch: boolean,
  ) => invoke<CopySummary>("copy_missing", { srcDiskId, dstDiskId, srcRootId, dstRootId, deep, includeMismatch }),

  // M5 — escaneo / detección de discos
  listVolumes: () => invoke<VolumeInfo[]>("list_volumes"),
  scanDisk: (mountPath: string, name?: string, options?: ScanOptions) =>
    invoke<ScanSummary>("scan_disk", { mountPath, name, options }),
  cancelScan: (mountPath: string) => invoke<void>("cancel_scan", { mountPath }),
  startVolumeWatch: () => invoke<void>("start_volume_watch"),
  refreshOnlineStatus: () => invoke<DiskRow[]>("refresh_online_status"),

  // M9 — conector remoto seguro
  agentStart: (bind?: string, scopes?: string) => invoke<AgentStatus>("agent_start", { bind, scopes }),
  agentStop: () => invoke<void>("agent_stop"),
  agentStatus: () => invoke<AgentStatus>("agent_status"),
  agentPairCode: () => invoke<string>("agent_pair_code"),
  agentDevices: () => invoke<DeviceRow[]>("agent_devices"),
  agentRevoke: (deviceId: string, revoked: boolean) => invoke<void>("agent_revoke", { deviceId, revoked }),

  // IA — búsqueda semántica de imágenes (Fase 1)
  aiAvailable: () => invoke<boolean>("ai_available"),
  aiStatus: () => invoke<AiStatus>("ai_status"),
  aiIndex: (rebuild?: boolean) => invoke<number>("ai_index", { rebuild }),
  aiSearch: (query: string, threshold?: number, limit?: number) =>
    invoke<SemanticItem[]>("ai_search", { query, threshold, limit }),
  // Fase 2 — indexa el contenido de los videos de un disco montado (muestreo de frames)
  aiIndexVideos: (diskId: number, frames?: number) =>
    invoke<number>("ai_index_videos", { diskId, frames }),
  // Fase 5 — buscar archivos visualmente similares a uno dado (reusa embeddings)
  aiSimilar: (entryId: number, threshold?: number, limit?: number) =>
    invoke<SemanticItem[]>("ai_similar", { entryId, threshold, limit }),
  // Fase 5 — duplicados visuales (near-dups por contenido, mismo shape que findDuplicates)
  aiVisualDuplicates: (threshold?: number, minSize?: number, limit?: number) =>
    invoke<DupGroup[]>("ai_visual_duplicates", { threshold, minSize, limit }),
  // Fase 4 — transcribe el audio de un disco montado (Whisper) e indexa para FTS
  aiTranscribeDisk: (diskId: number) => invoke<number>("ai_transcribe_disk", { diskId }),
  // Fase 4 — busca en las transcripciones (lo que se DICE). `lang` filtra por idioma.
  aiSearchTranscripts: (query: string, limit?: number, lang?: string) =>
    invoke<SemanticItem[]>("ai_search_transcripts", { query, limit, lang }),
};

export interface AiStatus {
  available: boolean;
  loaded: boolean;
  model: string;
  embedded: number;
  candidates: number;
}

export interface SemanticItem extends SearchItem {
  score: number;
  /** Segundo del clip donde mejor matchea (null para imágenes). */
  frame_ts: number | null;
  /** Fragmento de la transcripción donde matchea (Fase 4); null para hits visuales. */
  snippet?: string | null;
}

// Progreso del indexado semántico / transcripción. `total = -1` mientras carga el modelo.
export interface AiIndexProgress {
  done: number;
  total: number;
  phase?: "loading";
}
export const onAiIndexProgress = (cb: (p: AiIndexProgress) => void): Promise<UnlistenFn> =>
  listen<AiIndexProgress>("ai://index", (e) => cb(e.payload));
export const onAiTranscribeProgress = (cb: (p: AiIndexProgress) => void): Promise<UnlistenFn> =>
  listen<AiIndexProgress>("ai://transcribe", (e) => cb(e.payload));

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
  mount: string;
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

// M9 — comparación de discos / mirror de backup.
export interface DiffEntry {
  rel_path: string;
  is_folder: boolean;
  src_size: number;
  dst_size: number;
  src_entry_id: number;
}
export interface DiskDiff {
  missing: DiffEntry[];
  /** Distinto contenido: por tamaño en modo rápido, por hash en modo profundo. */
  size_mismatch: DiffEntry[];
  /** Solo en modo profundo: presentes y del mismo tamaño, pero sin hash para verificar. */
  unverified: DiffEntry[];
  extra: DiffEntry[];
  conflicts: DiffEntry[];
  missing_count: number;
  missing_file_count: number;
  missing_bytes: number;
  mismatch_count: number;
  mismatch_bytes: number;
  extra_count: number;
  conflict_count: number;
  unverified_count: number;
  /** Archivos verificados como idénticos en el destino. */
  ok_count: number;
  truncated: boolean;
}
export interface CopySummary {
  copied: number;
  failed: number;
  bytes_copied: number;
  /** Copiados y re-leídos con hash idéntico al origen. */
  verified: number;
  /** El destino ya existía y no correspondía reemplazarlo. */
  skipped: number;
  errors: string[];
  cancelled: boolean;
  needs_rescan: boolean;
}
export interface CopyProgress {
  done: number;
  total: number;
  bytes_done: number;
  bytes_total: number;
  current: string;
}
export const onCopyProgress = (cb: (p: CopyProgress) => void): Promise<UnlistenFn> =>
  listen<CopyProgress>("compare-copy-progress", (e) => cb(e.payload));
export const onIndexProgress = (cb: (p: IndexProgress) => void): Promise<UnlistenFn> =>
  listen<IndexProgress>("index-progress", (e) => cb(e.payload));
