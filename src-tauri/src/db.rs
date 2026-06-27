//! Capa de base de datos: un archivo SQLite (`.dccat`) por catálogo, con FTS5
//! para búsqueda full-text instantánea por nombre (sección 4).
//!
//! Ingesta masiva optimizada: inserts en una sola transacción por disco con
//! statement preparado, y reconstrucción del índice FTS al final (mucho más
//! rápido que mantenerlo por triggers durante una carga de millones de filas).

use crate::dcmf::{DcmfDisk, DcmfEntry};
use crate::scan::EntryEnrichment;
use rusqlite::{params, params_from_iter, Connection, OptionalExtension, ToSql};
use serde::{Deserialize, Serialize};
use std::path::Path;

pub type DbResult<T> = Result<T, rusqlite::Error>;

/// Fila de entrada para navegación e inspector (M2).
#[derive(Debug, Clone, Serialize)]
pub struct EntryRow {
    pub id: i64,
    pub disk_id: i64,
    pub parent_id: Option<i64>,
    pub name: String,
    pub is_folder: bool,
    pub size_logical: i64,
    pub size_physical: i64,
    pub created_at: Option<i64>,
    pub modified_at: Option<i64>,
    pub ext: Option<String>,
    pub comment: Option<String>,
    /// Cantidad de hijos directos (para mostrar disclosure en el árbol sin contar aparte).
    pub child_count: i64,
}

/// Metadata enriquecida de una entrada (A2/A2-meta): hash + GPS/cámara/captura.
#[derive(Debug, Clone, Serialize, Default)]
pub struct EntryMeta {
    pub content_hash: Option<String>,
    pub gps_lat: Option<f64>,
    pub gps_lon: Option<f64>,
    pub gps_place: Option<String>,
    pub captured_at: Option<i64>,
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub light_phase: Option<String>,
}

impl EntryMeta {
    pub fn is_empty(&self) -> bool {
        self.content_hash.is_none()
            && self.gps_lat.is_none()
            && self.gps_place.is_none()
            && self.captured_at.is_none()
            && self.camera_make.is_none()
            && self.camera_model.is_none()
            && self.light_phase.is_none()
    }
}

/// Lee la metadata enriquecida de una entrada (columnas A2/A2-meta).
pub fn get_entry_meta(conn: &Connection, entry_id: i64) -> DbResult<EntryMeta> {
    conn.query_row(
        "SELECT content_hash, gps_lat, gps_lon, gps_place, captured_at, camera_make, camera_model, light_phase \
         FROM entries WHERE id = ?1",
        params![entry_id],
        |r| {
            Ok(EntryMeta {
                content_hash: r.get(0)?,
                gps_lat: r.get(1)?,
                gps_lon: r.get(2)?,
                gps_place: r.get(3)?,
                captured_at: r.get(4)?,
                camera_make: r.get(5)?,
                camera_model: r.get(6)?,
                light_phase: r.get(7)?,
            })
        },
    )
}

/// Resultado de búsqueda (M3): incluye disco y ruta completa.
#[derive(Debug, Clone, Serialize)]
pub struct SearchItem {
    pub id: i64,
    pub disk_id: i64,
    pub disk_name: String,
    pub name: String,
    pub is_folder: bool,
    pub size_logical: i64,
    pub modified_at: Option<i64>,
    pub path: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchResult {
    /// Total de coincidencias (puede superar a `items.len()` por el límite).
    pub total: i64,
    pub items: Vec<SearchItem>,
    pub truncated: bool,
}

/// Esquema base (sección 4). Idempotente.
pub const SCHEMA: &str = r#"
PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;
-- Esperar (en vez de fallar con "database is locked") si otra conexión está
-- escribiendo: permite ingestas de escaneo en su propia conexión + lecturas de
-- la UI en paralelo, y dos escaneos concurrentes que serializan su commit.
PRAGMA busy_timeout = 60000;

CREATE TABLE IF NOT EXISTS disks (
  id            INTEGER PRIMARY KEY,
  name          TEXT NOT NULL,
  kind          TEXT,
  volume_uuid   TEXT,
  capacity      INTEGER,
  total_size    INTEGER,
  file_count    INTEGER,
  folder_count  INTEGER,
  is_online     INTEGER DEFAULT 0,
  mount_path    TEXT,
  location      TEXT,
  category      TEXT,
  comment       TEXT,
  scanned_at    INTEGER
);

CREATE TABLE IF NOT EXISTS entries (
  id            INTEGER PRIMARY KEY,
  disk_id       INTEGER NOT NULL REFERENCES disks(id),
  parent_id     INTEGER,
  name          TEXT NOT NULL,
  is_folder     INTEGER NOT NULL,
  size_logical  INTEGER DEFAULT 0,
  size_physical INTEGER DEFAULT 0,
  created_at    INTEGER,
  modified_at   INTEGER,
  ext           TEXT,
  comment       TEXT,
  tags          TEXT
);

CREATE INDEX IF NOT EXISTS idx_entries_parent ON entries(disk_id, parent_id);
CREATE INDEX IF NOT EXISTS idx_entries_ext    ON entries(ext);
CREATE INDEX IF NOT EXISTS idx_entries_size   ON entries(size_logical);

CREATE VIRTUAL TABLE IF NOT EXISTS entries_fts USING fts5(
  name, content='entries', content_rowid='id', tokenize='unicode61 remove_diacritics 2'
);

CREATE TABLE IF NOT EXISTS locations  (id INTEGER PRIMARY KEY, name TEXT);
CREATE TABLE IF NOT EXISTS categories (id INTEGER PRIMARY KEY, name TEXT);
CREATE TABLE IF NOT EXISTS tags       (id INTEGER PRIMARY KEY, name TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS entry_tags (entry_id INTEGER, tag_id INTEGER, PRIMARY KEY(entry_id, tag_id));
CREATE UNIQUE INDEX IF NOT EXISTS idx_tags_name      ON tags(name);
CREATE INDEX IF NOT EXISTS        idx_entry_tags_tag ON entry_tags(tag_id);

-- Thumbnails cacheados EN el catálogo: el .dccat queda autocontenido y portable,
-- y las miniaturas se ven aunque el disco esté desconectado (clave vs DiskCatalogMaker).
CREATE TABLE IF NOT EXISTS thumbnails (
  entry_id   INTEGER PRIMARY KEY,
  png        BLOB NOT NULL,
  w          INTEGER,
  h          INTEGER,
  created_at INTEGER
);

-- Video (Fase B): metadata técnica + tira de frames/escenas, cacheadas en el .dccat.
CREATE TABLE IF NOT EXISTS video_meta (
  entry_id    INTEGER PRIMARY KEY,
  duration_ms INTEGER,
  width       INTEGER,
  height      INTEGER,
  fps         REAL,
  vcodec      TEXT,
  acodec      TEXT,
  bitrate     INTEGER,
  probed_at   INTEGER
);
CREATE TABLE IF NOT EXISTS video_frames (
  id       INTEGER PRIMARY KEY,
  entry_id INTEGER NOT NULL,
  pos_ms   INTEGER,
  png      BLOB NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_video_frames_entry ON video_frames(entry_id);

-- Embeddings semánticos (IA): un vector por entrada visual. Para imágenes hay
-- UNA fila (frame_ts NULL); para VIDEO hay varias (una por frame muestreado, con
-- `frame_ts` en segundos) → permite "buscar el momento" dentro del clip. `vec` es
-- f32[] en bytes little-endian; `model` permite reindexar si se cambia de modelo.
CREATE TABLE IF NOT EXISTS embeddings (
  id       INTEGER PRIMARY KEY,
  entry_id INTEGER NOT NULL,
  model    TEXT NOT NULL,
  frame_ts REAL,
  dim      INTEGER NOT NULL,
  vec      BLOB NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_embeddings_model ON embeddings(model);
CREATE INDEX IF NOT EXISTS idx_embeddings_entry ON embeddings(entry_id, model);

-- Transcripciones de audio (IA Fase 4, Whisper): el texto de lo que se DICE en
-- videos/audios, para buscarlo full-text. `transcripts_fts` es un FTS5 standalone
-- (rowid = entry_id, lo manejamos a mano) → independiente del FTS de nombres.
CREATE TABLE IF NOT EXISTS transcripts (
  entry_id   INTEGER PRIMARY KEY,
  model      TEXT NOT NULL,
  lang       TEXT,
  text       TEXT NOT NULL,
  created_at INTEGER
);
CREATE VIRTUAL TABLE IF NOT EXISTS transcripts_fts USING fts5(
  text, tokenize='unicode61 remove_diacritics 2'
);

-- Contenido indexado de archivos comprimidos (Fase B): ZIP/7z/RAR.
CREATE TABLE IF NOT EXISTS archive_entries (
  id       INTEGER PRIMARY KEY,
  entry_id INTEGER NOT NULL,
  path     TEXT NOT NULL,
  name     TEXT NOT NULL,
  is_dir   INTEGER NOT NULL,
  size     INTEGER,
  modified INTEGER
);
CREATE INDEX IF NOT EXISTS idx_archive_entries_entry ON archive_entries(entry_id);

CREATE TABLE IF NOT EXISTS access_log (
  id INTEGER PRIMARY KEY, ts INTEGER, device_id TEXT, action TEXT,
  disk_id INTEGER, entry_id INTEGER, bytes INTEGER, result TEXT
);
CREATE TABLE IF NOT EXISTS devices (
  id TEXT PRIMARY KEY, name TEXT, public_key TEXT, scopes TEXT,
  created_at INTEGER, last_seen INTEGER, revoked INTEGER DEFAULT 0
);
"#;

/// Abre (o crea) un catálogo y garantiza el esquema.
pub fn open(path: &Path) -> DbResult<Connection> {
    let conn = Connection::open(path)?;
    conn.execute_batch(SCHEMA)?;
    apply_migrations(&conn)?;
    Ok(conn)
}

/// Abre un catálogo en memoria (para tests).
pub fn open_in_memory() -> DbResult<Connection> {
    let conn = Connection::open_in_memory()?;
    conn.execute_batch(SCHEMA)?;
    apply_migrations(&conn)?;
    Ok(conn)
}

/// Migraciones aditivas sobre catálogos ya existentes. `CREATE TABLE IF NOT EXISTS`
/// (en `SCHEMA`) no agrega columnas nuevas a una tabla creada por una versión vieja,
/// así que las columnas incorporadas después del esquema base se agregan acá con
/// `ALTER TABLE ... ADD COLUMN`. Idempotente: ignora el error "duplicate column name"
/// cuando la columna ya existe (re-apertura del mismo catálogo). Aditivo y de bajo
/// riesgo: no toca ni reescribe filas existentes; las columnas nuevas quedan NULL
/// hasta que un escaneo enriquecido las pueble.
///
/// Columnas agregadas (roadmap features nuevas, ver docs/DISENO-cloud-y-backup.md):
/// - `entries.content_hash/hashed_at` → auditoría de backup por hash (BLAKE3).
/// - `entries.gps_lat/gps_lon/gps_place/captured_at/camera_make/camera_model` →
///   metadata de cámara y búsqueda por ubicación ("clips de Jujuy").
/// - `entries.cloud_state` → 0=local, 1=placeholder solo-en-la-nube.
/// - `disks.cloud_provider/cloud_root` → carpeta sincronizada como disco cloud.
fn apply_migrations(conn: &Connection) -> DbResult<()> {
    const ADD_COLUMNS: &[&str] = &[
        "ALTER TABLE entries ADD COLUMN content_hash TEXT",
        "ALTER TABLE entries ADD COLUMN hashed_at    INTEGER",
        "ALTER TABLE entries ADD COLUMN cloud_state  INTEGER DEFAULT 0",
        "ALTER TABLE entries ADD COLUMN gps_lat      REAL",
        "ALTER TABLE entries ADD COLUMN gps_lon      REAL",
        "ALTER TABLE entries ADD COLUMN gps_place    TEXT",
        "ALTER TABLE entries ADD COLUMN captured_at  INTEGER",
        "ALTER TABLE entries ADD COLUMN camera_make  TEXT",
        "ALTER TABLE entries ADD COLUMN camera_model TEXT",
        "ALTER TABLE entries ADD COLUMN light_phase  TEXT",
        "ALTER TABLE disks   ADD COLUMN cloud_provider TEXT",
        "ALTER TABLE disks   ADD COLUMN cloud_root     TEXT",
    ];
    for stmt in ADD_COLUMNS {
        match conn.execute(stmt, []) {
            Ok(_) => {}
            // La columna ya existe (catálogo ya migrado): no es un error real.
            Err(rusqlite::Error::SqliteFailure(_, Some(msg)))
                if msg.contains("duplicate column name") => {}
            Err(e) => return Err(e),
        }
    }
    // Índices nuevos. Idempotentes y deben ir DESPUÉS de crear las columnas.
    // idx_entries_hash sirve también a futuro para duplicados entre discos (mismo hash).
    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_entries_hash  ON entries(content_hash);
         CREATE INDEX IF NOT EXISTS idx_entries_place ON entries(gps_place);",
    )?;
    Ok(())
}

/// Extrae la extensión en minúsculas (sin punto) de un nombre de archivo.
fn ext_of(name: &str) -> Option<String> {
    name.rsplit_once('.').and_then(|(_, e)| {
        if e.is_empty() || e.len() > 32 || e.contains('/') {
            None
        } else {
            Some(e.to_lowercase())
        }
    })
}

/// Suma recursiva de tamaños hacia los ancestros: cada archivo aporta su tamaño
/// a todas sus carpetas ancestro, de modo que carpetas y volumen muestren el
/// total recursivo (como DiskCatalogMaker). Devuelve (agg_logical, agg_physical).
fn aggregate_sizes(disk: &DcmfDisk) -> (Vec<u64>, Vec<u64>) {
    let n = disk.entries.len();
    let mut agg_log = vec![0u64; n];
    let mut agg_phys = vec![0u64; n];
    for (k, e) in disk.entries.iter().enumerate() {
        if !e.is_folder {
            // Propagar hacia arriba por la cadena de padres.
            let mut p = e.parent;
            // El propio archivo conserva su tamaño en agg para inserción uniforme.
            agg_log[k] = e.size_logical;
            agg_phys[k] = e.size_physical;
            let mut guard = 0;
            while p >= 0 && (p as usize) < n {
                let pi = p as usize;
                agg_log[pi] = agg_log[pi].saturating_add(e.size_logical);
                agg_phys[pi] = agg_phys[pi].saturating_add(e.size_physical);
                p = disk.entries[pi].parent;
                guard += 1;
                if guard > 4096 {
                    break; // ciclo defensivo
                }
            }
        }
    }
    (agg_log, agg_phys)
}

/// Inserta un conjunto de discos importados desde `.dcmf` y reconstruye el FTS.
/// Devuelve la cantidad total de entradas insertadas.
pub fn ingest_disks(conn: &mut Connection, disks: &[DcmfDisk]) -> DbResult<u64> {
    let mut total_entries: u64 = 0;

    for disk in disks {
        let (agg_log, agg_phys) = aggregate_sizes(disk);
        let file_count = disk.entries.iter().filter(|e| !e.is_folder).count() as i64;
        let folder_count = disk.entries.iter().filter(|e| e.is_folder).count() as i64;
        // total_size del disco = tamaño agregado del volumen (índice 0) si existe.
        let total_size = agg_log.first().copied().unwrap_or(0) as i64;

        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO disks (name, kind, total_size, file_count, folder_count, is_online, scanned_at)
             VALUES (?1, 'archive', ?2, ?3, ?4, 0, NULL)",
            params![disk.name, total_size, file_count, folder_count],
        )?;
        let disk_id = tx.last_insert_rowid();

        // Inserción contigua: rowids secuenciales en una transacción fresca, así
        // parent_id global = base + parent_local sin un segundo pase de UPDATE.
        let mut base: i64 = -1;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO entries
                 (disk_id, parent_id, name, is_folder, size_logical, size_physical, created_at, modified_at, ext)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            )?;
            for (k, e) in disk.entries.iter().enumerate() {
                let parent_id: Option<i64> = if e.parent >= 0 && base >= 0 {
                    Some(base + e.parent as i64)
                } else {
                    None
                };
                let ext = if e.is_folder { None } else { ext_of(&e.name) };
                let created = if e.created == 0 { None } else { Some(e.created) };
                let modified = if e.modified == 0 { None } else { Some(e.modified) };
                stmt.execute(params![
                    disk_id,
                    parent_id,
                    e.name,
                    e.is_folder as i64,
                    agg_log[k] as i64,
                    agg_phys[k] as i64,
                    created,
                    modified,
                    ext,
                ])?;
                if k == 0 {
                    base = tx.last_insert_rowid();
                }
                total_entries += 1;
            }
        }
        tx.commit()?;
    }

    // Reconstruir el índice FTS una vez al final (carga masiva).
    conn.execute_batch("INSERT INTO entries_fts(entries_fts) VALUES('rebuild');")?;
    Ok(total_entries)
}

/// Resultado de un escaneo ingestado.
#[derive(Debug, Clone, Serialize)]
pub struct ScanIngest {
    pub disk_id: i64,
    pub entries: u64,
    pub files: i64,
    pub folders: i64,
    pub replaced: bool,
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Carga el árbol catalogado de un disco como `DcmfDisk`, para reutilizar
/// subárboles sin cambios en un re-escaneo incremental. Identifica el disco por
/// `fingerprint` (Volume UUID) si está, o por `name` entre los discos sin
/// fingerprint (exFAT/NTFS de Windows) — la MISMA identidad que usa el dedupe de
/// `ingest_scanned`. Devuelve None si no hay match. Las entradas salen en orden
/// de id (padres antes que hijos), con `parent` reapuntado a índices locales.
pub fn load_disk_tree(
    conn: &Connection,
    fingerprint: Option<&str>,
    name: &str,
) -> DbResult<Option<DcmfDisk>> {
    let found: Option<(i64, String)> = match fingerprint {
        Some(fp) => conn
            .query_row(
                "SELECT id, name FROM disks WHERE volume_uuid = ?1 ORDER BY id DESC LIMIT 1",
                params![fp],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?,
        None => conn
            .query_row(
                "SELECT id, name FROM disks WHERE volume_uuid IS NULL AND name = ?1 \
                 ORDER BY id DESC LIMIT 1",
                params![name],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?,
    };
    let (disk_id, name) = match found {
        Some(x) => x,
        None => return Ok(None),
    };

    let mut stmt = conn.prepare(
        "SELECT id, parent_id, name, is_folder, size_logical, size_physical, created_at, modified_at \
         FROM entries WHERE disk_id = ?1 ORDER BY id",
    )?;
    type Raw = (i64, Option<i64>, String, bool, i64, i64, Option<i64>, Option<i64>);
    let raw: Vec<Raw> = stmt
        .query_map(params![disk_id], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, Option<i64>>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)? != 0,
                r.get::<_, i64>(4)?,
                r.get::<_, i64>(5)?,
                r.get::<_, Option<i64>>(6)?,
                r.get::<_, Option<i64>>(7)?,
            ))
        })?
        .collect::<Result<_, _>>()?;

    let mut idx = std::collections::HashMap::with_capacity(raw.len());
    for (i, row) in raw.iter().enumerate() {
        idx.insert(row.0, i as i32);
    }
    let entries = raw
        .iter()
        .map(|(_id, parent_id, name, is_folder, sl, sp, c, m)| DcmfEntry {
            name: name.clone(),
            parent: parent_id.and_then(|p| idx.get(&p).copied()).unwrap_or(-1),
            is_folder: *is_folder,
            is_volume: parent_id.is_none(),
            size_logical: *sl as u64,
            size_physical: *sp as u64,
            created: c.unwrap_or(0),
            modified: m.unwrap_or(0),
        })
        .collect();
    Ok(Some(DcmfDisk { name, entries }))
}

/// Ingesta un disco escaneado (sección 7). Si ya existe un disco con el mismo
/// `volume_uuid`, lo reemplaza (re-escaneo). Mantiene el FTS de forma incremental
/// (sin reconstruir todo el índice) para no penalizar catálogos grandes.
/// Enriquecimiento preservado de un escaneo anterior (A2-preserve), keyed por ruta
/// relativa. El re-escaneo es full-replace (borra + reinserta), así que sin esto un
/// re-escaneo SIN `enrich` perdería los hashes/GPS ya calculados. Se restaura sólo si
/// el archivo no cambió (mismo tamaño + mismo mtime), para no arrastrar un hash viejo
/// de un archivo editado.
struct PreservedEnrichment {
    size: i64,
    modified: Option<i64>,
    content_hash: Option<String>,
    hashed_at: Option<i64>,
    gps_lat: Option<f64>,
    gps_lon: Option<f64>,
    gps_place: Option<String>,
    captured_at: Option<i64>,
    camera_make: Option<String>,
    camera_model: Option<String>,
    light_phase: Option<String>,
}

/// Rutas relativas (con `/`) de cada entrada de un `DcmfDisk` (raíz = ""). Asume
/// padre-antes-que-hijo (garantizado por el formato/escaneo). Mismo criterio que
/// `collect_subtree_files`, para que las claves matcheen el snapshot del disco viejo.
fn tree_rel_paths(disk: &DcmfDisk) -> Vec<String> {
    let mut paths = vec![String::new(); disk.entries.len()];
    for (i, e) in disk.entries.iter().enumerate() {
        if e.parent < 0 {
            continue;
        }
        let base = &paths[e.parent as usize];
        paths[i] = if base.is_empty() {
            e.name.clone()
        } else {
            format!("{base}/{}", e.name)
        };
    }
    paths
}

/// Snapshot del enriquecimiento (hash/GPS) de los discos viejos `old_ids`, por ruta
/// relativa, ANTES de borrarlos. Sólo incluye archivos con algo que preservar; si un
/// disco no tiene ninguna fila enriquecida, ni siquiera recorre su árbol (guard barato).
fn snapshot_enrichment(
    conn: &Connection,
    old_ids: &[i64],
) -> DbResult<std::collections::HashMap<String, PreservedEnrichment>> {
    let mut map = std::collections::HashMap::new();
    for &old in old_ids {
        // Guard: ¿hay algo enriquecido en este disco? Si no, evitamos la CTE.
        let has_any: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM entries WHERE disk_id = ?1 \
             AND (content_hash IS NOT NULL OR gps_lat IS NOT NULL OR gps_place IS NOT NULL))",
            params![old],
            |r| r.get(0),
        )?;
        if !has_any {
            continue;
        }
        let mut stmt = conn.prepare(
            "WITH RECURSIVE sub(id, name, is_folder, size_logical, modified_at, content_hash, hashed_at,
                                gps_lat, gps_lon, gps_place, captured_at, camera_make, camera_model, light_phase, rel) AS (
               SELECT id, name, is_folder, size_logical, modified_at, content_hash, hashed_at,
                      gps_lat, gps_lon, gps_place, captured_at, camera_make, camera_model, light_phase, ''
                 FROM entries WHERE disk_id = ?1 AND parent_id IS NULL
               UNION ALL
               SELECT e.id, e.name, e.is_folder, e.size_logical, e.modified_at, e.content_hash, e.hashed_at,
                      e.gps_lat, e.gps_lon, e.gps_place, e.captured_at, e.camera_make, e.camera_model, e.light_phase,
                      CASE WHEN s.rel = '' THEN e.name ELSE s.rel || '/' || e.name END
                 FROM entries e JOIN sub s ON e.parent_id = s.id
                WHERE e.disk_id = ?1
             )
             SELECT rel, size_logical, modified_at, content_hash, hashed_at,
                    gps_lat, gps_lon, gps_place, captured_at, camera_make, camera_model, light_phase
             FROM sub
             WHERE is_folder = 0 AND (content_hash IS NOT NULL OR gps_lat IS NOT NULL OR gps_place IS NOT NULL)",
        )?;
        let rows = stmt.query_map(params![old], |r| {
            Ok((
                r.get::<_, String>(0)?,
                PreservedEnrichment {
                    size: r.get(1)?,
                    modified: r.get(2)?,
                    content_hash: r.get(3)?,
                    hashed_at: r.get(4)?,
                    gps_lat: r.get(5)?,
                    gps_lon: r.get(6)?,
                    gps_place: r.get(7)?,
                    captured_at: r.get(8)?,
                    camera_make: r.get(9)?,
                    camera_model: r.get(10)?,
                    light_phase: r.get(11)?,
                },
            ))
        })?;
        for row in rows {
            let (rel, pe) = row?;
            map.insert(rel, pe);
        }
    }
    Ok(map)
}

pub fn ingest_scanned(
    conn: &mut Connection,
    disk: &DcmfDisk,
    volume_uuid: Option<&str>,
    kind: &str,
    capacity: Option<i64>,
    mount_path: &str,
    enrichment: Option<&[EntryEnrichment]>,
) -> DbResult<ScanIngest> {
    let (agg_log, agg_phys) = aggregate_sizes(disk);
    let file_count = disk.entries.iter().filter(|e| !e.is_folder).count() as i64;
    let folder_count = disk.entries.iter().filter(|e| e.is_folder).count() as i64;
    let total_size = agg_log.first().copied().unwrap_or(0) as i64;

    let tx = conn.transaction()?;
    let mut replaced = false;

    // Re-escaneo: eliminar el/los disco(s) previo(s) que representen el MISMO
    // disco físico (y su FTS). Con fingerprint (Volume UUID) matcheamos por él.
    // Sin fingerprint —típico en exFAT/NTFS de Windows, que no exponen un UUID
    // vía `diskutil`— caemos al nombre del volumen entre los discos también sin
    // fingerprint, para no acumular un duplicado en cada re-escaneo.
    let old_ids: Vec<i64> = {
        let (sql, key): (&str, &str) = match volume_uuid {
            Some(uuid) => ("SELECT id FROM disks WHERE volume_uuid = ?1", uuid),
            None => (
                "SELECT id FROM disks WHERE volume_uuid IS NULL AND name = ?1",
                disk.name.as_str(),
            ),
        };
        let mut stmt = tx.prepare(sql)?;
        let ids = stmt.query_map(params![key], |r| r.get::<_, i64>(0))?;
        ids.collect::<Result<_, _>>()?
    };
    // A2-preserve: snapshotear el enriquecimiento (hash/GPS) de los discos viejos
    // ANTES de borrarlos, para restaurarlo en los archivos que no cambiaron.
    let preserved = snapshot_enrichment(&tx, &old_ids)?;
    for &old in &old_ids {
        // Borrado del FTS externo: comando 'delete' fila por fila vía SELECT.
        tx.execute(
            "INSERT INTO entries_fts(entries_fts, rowid, name) \
             SELECT 'delete', id, name FROM entries WHERE disk_id = ?1",
            params![old],
        )?;
        // Limpiar datos derivados de las entradas que se van.
        let derived = [
            "DELETE FROM thumbnails WHERE entry_id IN (SELECT id FROM entries WHERE disk_id = ?1)",
            "DELETE FROM entry_tags WHERE entry_id IN (SELECT id FROM entries WHERE disk_id = ?1)",
            "DELETE FROM video_meta WHERE entry_id IN (SELECT id FROM entries WHERE disk_id = ?1)",
            "DELETE FROM video_frames WHERE entry_id IN (SELECT id FROM entries WHERE disk_id = ?1)",
            "DELETE FROM archive_entries WHERE entry_id IN (SELECT id FROM entries WHERE disk_id = ?1)",
        ];
        for sql in derived {
            tx.execute(sql, params![old])?;
        }
        tx.execute("DELETE FROM entries WHERE disk_id = ?1", params![old])?;
        tx.execute("DELETE FROM disks WHERE id = ?1", params![old])?;
        replaced = true;
    }

    tx.execute(
        "INSERT INTO disks
         (name, kind, volume_uuid, capacity, total_size, file_count, folder_count, is_online, mount_path, scanned_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1, ?8, ?9)",
        params![
            disk.name,
            kind,
            volume_uuid,
            capacity,
            total_size,
            file_count,
            folder_count,
            mount_path,
            now_secs(),
        ],
    )?;
    let disk_id = tx.last_insert_rowid();

    // Rutas relativas del árbol nuevo (para casar contra el snapshot del viejo).
    let new_rels = tree_rel_paths(disk);

    let mut base: i64 = -1;
    {
        let mut stmt = tx.prepare(
            "INSERT INTO entries
             (disk_id, parent_id, name, is_folder, size_logical, size_physical, created_at, modified_at, ext,
              content_hash, hashed_at, gps_lat, gps_lon, gps_place, captured_at, camera_make, camera_model, light_phase)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
        )?;
        let now = now_secs();
        for (k, e) in disk.entries.iter().enumerate() {
            let parent_id: Option<i64> = if e.parent >= 0 && base >= 0 {
                Some(base + e.parent as i64)
            } else {
                None
            };
            let ext = if e.is_folder { None } else { ext_of(&e.name) };
            let created = if e.created == 0 { None } else { Some(e.created) };
            let modified = if e.modified == 0 { None } else { Some(e.modified) };
            // Enriquecimiento (A2 + A2-preserve): preferir el hash/GPS FRESCO de este
            // escaneo; si no hay, reutilizar el snapshot del disco viejo SOLO si el
            // archivo no cambió (mismo tamaño + mismo mtime) — así un re-escaneo sin
            // `enrich` no pierde los hashes, pero un archivo editado no arrastra el viejo.
            let fresh = enrichment.and_then(|v| v.get(k));
            let size_now = agg_log[k] as i64;
            let snap = if e.is_folder { None } else { preserved.get(&new_rels[k]) };
            let snap = snap.filter(|s| modified.is_some() && s.modified == modified && s.size == size_now);

            let fresh_hash = fresh.and_then(|x| x.content_hash.clone());
            let (content_hash, hashed_at): (Option<String>, Option<i64>) = match (&fresh_hash, snap) {
                (Some(_), _) => (fresh_hash.clone(), Some(now)),
                (None, Some(s)) => (s.content_hash.clone(), s.hashed_at),
                (None, None) => (None, None),
            };
            let gps_lat = fresh.and_then(|x| x.gps_lat).or_else(|| snap.and_then(|s| s.gps_lat));
            let gps_lon = fresh.and_then(|x| x.gps_lon).or_else(|| snap.and_then(|s| s.gps_lon));
            let gps_place = fresh
                .and_then(|x| x.gps_place.clone())
                .or_else(|| snap.and_then(|s| s.gps_place.clone()));
            let captured_at = fresh.and_then(|x| x.captured_at).or_else(|| snap.and_then(|s| s.captured_at));
            let camera_make = fresh
                .and_then(|x| x.camera_make.clone())
                .or_else(|| snap.and_then(|s| s.camera_make.clone()));
            let camera_model = fresh
                .and_then(|x| x.camera_model.clone())
                .or_else(|| snap.and_then(|s| s.camera_model.clone()));
            let light_phase = fresh
                .and_then(|x| x.light_phase.clone())
                .or_else(|| snap.and_then(|s| s.light_phase.clone()));
            stmt.execute(params![
                disk_id,
                parent_id,
                e.name,
                e.is_folder as i64,
                agg_log[k] as i64,
                agg_phys[k] as i64,
                created,
                modified,
                ext,
                content_hash,
                hashed_at,
                gps_lat,
                gps_lon,
                gps_place,
                captured_at,
                camera_make,
                camera_model,
                light_phase,
            ])?;
            if k == 0 {
                base = tx.last_insert_rowid();
            }
        }
    }

    // FTS incremental: indexar sólo las filas nuevas de este disco.
    tx.execute(
        "INSERT INTO entries_fts(rowid, name) SELECT id, name FROM entries WHERE disk_id = ?1",
        params![disk_id],
    )?;

    tx.commit()?;

    Ok(ScanIngest {
        disk_id,
        entries: disk.entries.len() as u64,
        files: file_count,
        folders: folder_count,
        replaced,
    })
}

/// Columnas comunes + conteo de hijos directos (subconsulta correlacionada).
const ENTRY_COLS: &str = "e.id, e.disk_id, e.parent_id, e.name, e.is_folder, \
     e.size_logical, e.size_physical, e.created_at, e.modified_at, e.ext, e.comment, \
     (SELECT COUNT(*) FROM entries c WHERE c.disk_id = e.disk_id AND c.parent_id = e.id) AS child_count";

fn row_to_entry(r: &rusqlite::Row) -> rusqlite::Result<EntryRow> {
    Ok(EntryRow {
        id: r.get(0)?,
        disk_id: r.get(1)?,
        parent_id: r.get(2)?,
        name: r.get(3)?,
        is_folder: r.get::<_, i64>(4)? != 0,
        size_logical: r.get(5)?,
        size_physical: r.get(6)?,
        created_at: r.get(7)?,
        modified_at: r.get(8)?,
        ext: r.get(9)?,
        comment: r.get(10)?,
        child_count: r.get(11)?,
    })
}

/// Elimina una entrada del catálogo (tras mover el original a la papelera).
/// Limpia FTS y tablas derivadas. Pensado para archivos (no subárboles).
pub fn delete_entry(conn: &mut Connection, entry_id: i64) -> DbResult<()> {
    let tx = conn.transaction()?;
    // Quitar del índice FTS externo antes de borrar la fila.
    tx.execute(
        "INSERT INTO entries_fts(entries_fts, rowid, name) \
         SELECT 'delete', id, name FROM entries WHERE id = ?1",
        params![entry_id],
    )?;
    for sql in [
        "DELETE FROM thumbnails WHERE entry_id = ?1",
        "DELETE FROM entry_tags WHERE entry_id = ?1",
        "DELETE FROM video_meta WHERE entry_id = ?1",
        "DELETE FROM video_frames WHERE entry_id = ?1",
        "DELETE FROM archive_entries WHERE entry_id = ?1",
        "DELETE FROM entries WHERE id = ?1",
    ] {
        tx.execute(sql, params![entry_id])?;
    }
    tx.commit()?;
    Ok(())
}

/// Elimina una entrada y TODO su subárbol del catálogo (tras mover el original
/// a la papelera). Limpia FTS y tablas derivadas. Sirve para archivos (subárbol
/// de un solo nodo) y para carpetas (todos sus descendientes).
pub fn delete_subtree(conn: &mut Connection, entry_id: i64) -> DbResult<()> {
    let tx = conn.transaction()?;
    // Ids del subárbol (incluye la raíz) vía CTE recursiva.
    let ids: Vec<i64> = {
        let mut stmt = tx.prepare(
            "WITH RECURSIVE sub(id) AS (
               SELECT id FROM entries WHERE id = ?1
               UNION ALL
               SELECT e.id FROM entries e JOIN sub ON e.parent_id = sub.id
             )
             SELECT id FROM sub",
        )?;
        let rows = stmt.query_map(params![entry_id], |r| r.get::<_, i64>(0))?;
        rows.collect::<Result<_, _>>()?
    };
    for id in &ids {
        // Quitar del índice FTS externo antes de borrar la fila.
        tx.execute(
            "INSERT INTO entries_fts(entries_fts, rowid, name) \
             SELECT 'delete', id, name FROM entries WHERE id = ?1",
            params![id],
        )?;
        for sql in [
            "DELETE FROM thumbnails WHERE entry_id = ?1",
            "DELETE FROM entry_tags WHERE entry_id = ?1",
            "DELETE FROM video_meta WHERE entry_id = ?1",
            "DELETE FROM video_frames WHERE entry_id = ?1",
            "DELETE FROM archive_entries WHERE entry_id = ?1",
            "DELETE FROM entries WHERE id = ?1",
        ] {
            tx.execute(sql, params![id])?;
        }
    }
    tx.commit()?;
    Ok(())
}

/// Quita un disco entero del catálogo (sus entradas, FTS y tablas derivadas).
/// Útil para discos que ya no existen. No toca el original en el filesystem.
pub fn delete_disk(conn: &mut Connection, disk_id: i64) -> DbResult<()> {
    let tx = conn.transaction()?;
    tx.execute(
        "INSERT INTO entries_fts(entries_fts, rowid, name) \
         SELECT 'delete', id, name FROM entries WHERE disk_id = ?1",
        params![disk_id],
    )?;
    for sql in [
        "DELETE FROM thumbnails WHERE entry_id IN (SELECT id FROM entries WHERE disk_id = ?1)",
        "DELETE FROM entry_tags WHERE entry_id IN (SELECT id FROM entries WHERE disk_id = ?1)",
        "DELETE FROM video_meta WHERE entry_id IN (SELECT id FROM entries WHERE disk_id = ?1)",
        "DELETE FROM video_frames WHERE entry_id IN (SELECT id FROM entries WHERE disk_id = ?1)",
        "DELETE FROM archive_entries WHERE entry_id IN (SELECT id FROM entries WHERE disk_id = ?1)",
        "DELETE FROM entries WHERE disk_id = ?1",
        "DELETE FROM disks WHERE id = ?1",
    ] {
        tx.execute(sql, params![disk_id])?;
    }
    tx.commit()?;
    Ok(())
}

/// Edita el comentario de una entrada (M7).
pub fn set_entry_comment(conn: &Connection, entry_id: i64, comment: Option<&str>) -> DbResult<()> {
    conn.execute(
        "UPDATE entries SET comment = ?1 WHERE id = ?2",
        params![comment, entry_id],
    )?;
    Ok(())
}

/// Edita ubicación / categoría / comentario de un disco (M7).
pub fn set_disk_meta(
    conn: &Connection,
    disk_id: i64,
    location: Option<&str>,
    category: Option<&str>,
    comment: Option<&str>,
) -> DbResult<()> {
    conn.execute(
        "UPDATE disks SET location = ?1, category = ?2, comment = ?3 WHERE id = ?4",
        params![location, category, comment, disk_id],
    )?;
    Ok(())
}

// ---------- Tags / keywords (Fase A) ----------

#[derive(Debug, Clone, Serialize)]
pub struct TagStat {
    pub name: String,
    pub count: i64,
}

/// Normaliza un tag: trim + minúsculas. Devuelve `None` si queda vacío o es absurdo.
fn norm_tag(name: &str) -> Option<String> {
    let t = name.trim().to_lowercase();
    if t.is_empty() || t.chars().count() > 64 {
        None
    } else {
        Some(t)
    }
}

/// Agrega un tag a una entrada (crea el tag si no existe). Idempotente.
pub fn add_entry_tag(conn: &Connection, entry_id: i64, name: &str) -> DbResult<()> {
    let tag = match norm_tag(name) {
        Some(t) => t,
        None => return Ok(()),
    };
    conn.execute("INSERT OR IGNORE INTO tags(name) VALUES (?1)", params![tag])?;
    let tag_id: i64 = conn.query_row("SELECT id FROM tags WHERE name = ?1", params![tag], |r| r.get(0))?;
    conn.execute(
        "INSERT OR IGNORE INTO entry_tags(entry_id, tag_id) VALUES (?1, ?2)",
        params![entry_id, tag_id],
    )?;
    Ok(())
}

/// Quita un tag de una entrada. Si el tag queda sin uso, lo elimina del catálogo.
pub fn remove_entry_tag(conn: &Connection, entry_id: i64, name: &str) -> DbResult<()> {
    let tag = match norm_tag(name) {
        Some(t) => t,
        None => return Ok(()),
    };
    let tag_id: Option<i64> = conn
        .query_row("SELECT id FROM tags WHERE name = ?1", params![tag], |r| r.get(0))
        .optional()?;
    if let Some(tid) = tag_id {
        conn.execute(
            "DELETE FROM entry_tags WHERE entry_id = ?1 AND tag_id = ?2",
            params![entry_id, tid],
        )?;
        conn.execute(
            "DELETE FROM tags WHERE id = ?1 AND NOT EXISTS (SELECT 1 FROM entry_tags WHERE tag_id = ?1)",
            params![tid],
        )?;
    }
    Ok(())
}

/// Tags de una entrada, en orden alfabético.
pub fn entry_tags(conn: &Connection, entry_id: i64) -> DbResult<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT t.name FROM entry_tags et JOIN tags t ON t.id = et.tag_id \
         WHERE et.entry_id = ?1 ORDER BY t.name COLLATE NOCASE",
    )?;
    let rows = stmt.query_map(params![entry_id], |r| r.get::<_, String>(0))?;
    rows.collect()
}

/// Todos los tags del catálogo con su conteo de uso (nube de tags / autocompletado).
pub fn list_tags(conn: &Connection) -> DbResult<Vec<TagStat>> {
    let mut stmt = conn.prepare(
        "SELECT t.name, COUNT(et.entry_id) AS c FROM tags t \
         LEFT JOIN entry_tags et ON et.tag_id = t.id \
         GROUP BY t.id ORDER BY c DESC, t.name COLLATE NOCASE",
    )?;
    let rows = stmt.query_map([], |r| Ok(TagStat { name: r.get(0)?, count: r.get(1)? }))?;
    rows.collect()
}

// ---------- Thumbnails offline (Fase A) ----------

/// Guarda (o reemplaza) el thumbnail PNG de una entrada dentro del catálogo.
pub fn store_thumbnail(conn: &Connection, entry_id: i64, png: &[u8], w: u32, h: u32) -> DbResult<()> {
    conn.execute(
        "INSERT OR REPLACE INTO thumbnails(entry_id, png, w, h, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![entry_id, png, w as i64, h as i64, now_secs()],
    )?;
    Ok(())
}

/// Devuelve el thumbnail PNG cacheado (si existe). Funciona con el disco offline.
pub fn get_cached_thumbnail(conn: &Connection, entry_id: i64) -> DbResult<Option<Vec<u8>>> {
    conn.query_row(
        "SELECT png FROM thumbnails WHERE entry_id = ?1",
        params![entry_id],
        |r| r.get::<_, Vec<u8>>(0),
    )
    .optional()
}

/// IDs de imágenes de un disco (por extensión) que aún no tienen thumbnail cacheado.
pub fn image_entries_without_thumb(
    conn: &Connection,
    disk_id: i64,
    exts: &[&str],
) -> DbResult<Vec<i64>> {
    if exts.is_empty() {
        return Ok(Vec::new());
    }
    let ph = exts.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = format!(
        "SELECT e.id FROM entries e \
         WHERE e.disk_id = ? AND e.is_folder = 0 AND e.ext IN ({ph}) \
         AND NOT EXISTS (SELECT 1 FROM thumbnails th WHERE th.entry_id = e.id)"
    );
    let mut bind: Vec<Box<dyn ToSql>> = Vec::with_capacity(exts.len() + 1);
    bind.push(Box::new(disk_id));
    for e in exts {
        bind.push(Box::new(e.to_string()));
    }
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params_from_iter(bind.iter().map(|b| b.as_ref())), |r| {
        r.get::<_, i64>(0)
    })?;
    rows.collect()
}

// ---------- Video: metadata + frames (Fase B) ----------

#[derive(Debug, Clone, Serialize)]
pub struct VideoMetaRow {
    pub duration_ms: i64,
    pub width: i64,
    pub height: i64,
    pub fps: f64,
    pub vcodec: Option<String>,
    pub acodec: Option<String>,
    pub bitrate: i64,
}

/// Guarda (o reemplaza) la metadata técnica de un video.
pub fn store_video_meta(conn: &Connection, entry_id: i64, m: &VideoMetaRow) -> DbResult<()> {
    conn.execute(
        "INSERT OR REPLACE INTO video_meta \
         (entry_id, duration_ms, width, height, fps, vcodec, acodec, bitrate, probed_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            entry_id, m.duration_ms, m.width, m.height, m.fps, m.vcodec, m.acodec, m.bitrate, now_secs()
        ],
    )?;
    Ok(())
}

/// Metadata técnica de un video (si fue indexada).
pub fn get_video_meta(conn: &Connection, entry_id: i64) -> DbResult<Option<VideoMetaRow>> {
    conn.query_row(
        "SELECT duration_ms, width, height, fps, vcodec, acodec, bitrate FROM video_meta WHERE entry_id = ?1",
        params![entry_id],
        |r| {
            Ok(VideoMetaRow {
                duration_ms: r.get(0)?,
                width: r.get(1)?,
                height: r.get(2)?,
                fps: r.get(3)?,
                vcodec: r.get(4)?,
                acodec: r.get(5)?,
                bitrate: r.get(6)?,
            })
        },
    )
    .optional()
}

/// IDs de videos (por extensión) de un disco aún sin metadata indexada.
pub fn video_entries_without_meta(
    conn: &Connection,
    disk_id: i64,
    exts: &[&str],
) -> DbResult<Vec<i64>> {
    if exts.is_empty() {
        return Ok(Vec::new());
    }
    let ph = exts.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = format!(
        "SELECT e.id FROM entries e \
         WHERE e.disk_id = ? AND e.is_folder = 0 AND e.ext IN ({ph}) \
         AND NOT EXISTS (SELECT 1 FROM video_meta vm WHERE vm.entry_id = e.id)"
    );
    let mut bind: Vec<Box<dyn ToSql>> = Vec::with_capacity(exts.len() + 1);
    bind.push(Box::new(disk_id));
    for e in exts {
        bind.push(Box::new(e.to_string()));
    }
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params_from_iter(bind.iter().map(|b| b.as_ref())), |r| {
        r.get::<_, i64>(0)
    })?;
    rows.collect()
}

/// Reemplaza la tira de frames de un video.
pub fn replace_video_frames(conn: &Connection, entry_id: i64, frames: &[(i64, Vec<u8>)]) -> DbResult<()> {
    conn.execute("DELETE FROM video_frames WHERE entry_id = ?1", params![entry_id])?;
    let mut stmt =
        conn.prepare("INSERT INTO video_frames(entry_id, pos_ms, png) VALUES (?1, ?2, ?3)")?;
    for (pos_ms, png) in frames {
        stmt.execute(params![entry_id, pos_ms, png])?;
    }
    Ok(())
}

/// Tira de frames cacheada de un video (orden temporal).
pub fn get_video_frames(conn: &Connection, entry_id: i64) -> DbResult<Vec<Vec<u8>>> {
    let mut stmt =
        conn.prepare("SELECT png FROM video_frames WHERE entry_id = ?1 ORDER BY pos_ms")?;
    let rows = stmt.query_map(params![entry_id], |r| r.get::<_, Vec<u8>>(0))?;
    rows.collect()
}

// ---------- Contenido de archivos comprimidos (Fase B) ----------

#[derive(Debug, Clone, Serialize)]
pub struct ArchiveEntryRow {
    pub path: String,
    pub name: String,
    pub is_dir: bool,
    pub size: i64,
    pub modified: i64,
}

/// Reemplaza el índice de contenido de un archivo comprimido.
pub fn store_archive_entries(
    conn: &mut Connection,
    entry_id: i64,
    items: &[crate::archive::ArchiveItem],
) -> DbResult<()> {
    let tx = conn.transaction()?;
    tx.execute("DELETE FROM archive_entries WHERE entry_id = ?1", params![entry_id])?;
    {
        let mut stmt = tx.prepare(
            "INSERT INTO archive_entries(entry_id, path, name, is_dir, size, modified) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )?;
        for it in items {
            let name = it.path.rsplit('/').find(|s| !s.is_empty()).unwrap_or(&it.path);
            stmt.execute(params![
                entry_id,
                it.path,
                name,
                it.is_dir as i64,
                it.size as i64,
                it.modified
            ])?;
        }
    }
    tx.commit()?;
    Ok(())
}

/// Lista el contenido indexado de un archivo (carpetas primero, luego por nombre).
pub fn list_archive_entries(conn: &Connection, entry_id: i64) -> DbResult<Vec<ArchiveEntryRow>> {
    let mut stmt = conn.prepare(
        "SELECT path, name, is_dir, size, modified FROM archive_entries \
         WHERE entry_id = ?1 ORDER BY is_dir DESC, path COLLATE NOCASE",
    )?;
    let rows = stmt.query_map(params![entry_id], |r| {
        Ok(ArchiveEntryRow {
            path: r.get(0)?,
            name: r.get(1)?,
            is_dir: r.get::<_, i64>(2)? != 0,
            size: r.get(3)?,
            modified: r.get(4)?,
        })
    })?;
    rows.collect()
}

/// Cantidad de entradas indexadas de un archivo (0 = sin indexar / vacío).
pub fn archive_entry_count(conn: &Connection, entry_id: i64) -> DbResult<i64> {
    conn.query_row(
        "SELECT COUNT(*) FROM archive_entries WHERE entry_id = ?1",
        params![entry_id],
        |r| r.get(0),
    )
}

/// IDs de archivos comprimidos (por extensión) de un disco aún sin indexar.
pub fn archive_files_without_index(
    conn: &Connection,
    disk_id: i64,
    exts: &[&str],
) -> DbResult<Vec<i64>> {
    if exts.is_empty() {
        return Ok(Vec::new());
    }
    let ph = exts.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = format!(
        "SELECT e.id FROM entries e \
         WHERE e.disk_id = ? AND e.is_folder = 0 AND e.ext IN ({ph}) \
         AND NOT EXISTS (SELECT 1 FROM archive_entries ae WHERE ae.entry_id = e.id)"
    );
    let mut bind: Vec<Box<dyn ToSql>> = Vec::with_capacity(exts.len() + 1);
    bind.push(Box::new(disk_id));
    for e in exts {
        bind.push(Box::new(e.to_string()));
    }
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params_from_iter(bind.iter().map(|b| b.as_ref())), |r| {
        r.get::<_, i64>(0)
    })?;
    rows.collect()
}

/// Lista los hijos directos de `parent_id` en un disco. Si `parent_id` es `None`,
/// devuelve la raíz del disco (el nodo volumen, con `parent_id IS NULL`).
/// Carpetas primero, luego por nombre (orden tipo Finder).
pub fn list_children(
    conn: &Connection,
    disk_id: i64,
    parent_id: Option<i64>,
) -> DbResult<Vec<EntryRow>> {
    // Saltar el nodo-volumen redundante: el disco YA representa el volumen, así
    // que si la raíz del disco tiene un único nodo (el volumen escaneado),
    // devolvemos directamente sus hijos. Evita que el disco aparezca anidado
    // dentro de una carpeta con su mismo nombre. (Las rutas y la resolución del
    // original siguen incluyendo el nombre del volumen, intactas.)
    let effective_parent = match parent_id {
        Some(p) => Some(p),
        None => {
            let roots: Vec<(i64, bool)> = {
                let mut s = conn.prepare(
                    "SELECT id, is_folder FROM entries WHERE disk_id = ?1 AND parent_id IS NULL",
                )?;
                let rows = s.query_map(params![disk_id], |r| {
                    Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)? != 0))
                })?;
                rows.collect::<Result<_, _>>()?
            };
            if roots.len() == 1 && roots[0].1 {
                Some(roots[0].0)
            } else {
                None
            }
        }
    };
    let sql = format!(
        "SELECT {ENTRY_COLS} FROM entries e \
         WHERE e.disk_id = ?1 AND e.parent_id IS ?2 \
         ORDER BY e.is_folder DESC, e.name COLLATE NOCASE ASC"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params![disk_id, effective_parent], row_to_entry)?;
    rows.collect()
}

/// Reconstruye la ruta completa de una entrada caminando hacia la raíz (CTE recursiva).
/// Devuelve algo como `/SF28/HUFNAGL PILAR/.../C0001.MP4`.
pub fn entry_path(conn: &Connection, entry_id: i64) -> DbResult<String> {
    let sql = "WITH RECURSIVE anc(id, parent_id, name, depth) AS (
                 SELECT id, parent_id, name, 0 FROM entries WHERE id = ?1
                 UNION ALL
                 SELECT e.id, e.parent_id, e.name, anc.depth + 1
                 FROM entries e JOIN anc ON e.id = anc.parent_id
               )
               SELECT name FROM anc ORDER BY depth DESC";
    let mut stmt = conn.prepare(sql)?;
    let names: Vec<String> = stmt
        .query_map(params![entry_id], |r| r.get::<_, String>(0))?
        .collect::<Result<_, _>>()?;
    if names.is_empty() {
        Ok(String::new())
    } else {
        Ok(format!("/{}", names.join("/")))
    }
}

/// Trae una entrada por id (para el inspector).
pub fn get_entry(conn: &Connection, entry_id: i64) -> DbResult<Option<EntryRow>> {
    let sql = format!("SELECT {ENTRY_COLS} FROM entries e WHERE e.id = ?1");
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query_map(params![entry_id], row_to_entry)?;
    match rows.next() {
        Some(r) => Ok(Some(r?)),
        None => Ok(None),
    }
}

/// Construye una query FTS5 segura a partir de texto libre del usuario:
/// tokeniza por no-alfanuméricos, entrecomilla cada token y agrega `*` (prefijo)
/// al último para búsqueda incremental. Devuelve `None` si no hay tokens.
pub fn build_fts_query(q: &str) -> Option<String> {
    let tokens: Vec<&str> = q
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .collect();
    if tokens.is_empty() {
        return None;
    }
    let n = tokens.len();
    let parts: Vec<String> = tokens
        .iter()
        .enumerate()
        .map(|(i, t)| {
            if i == n - 1 {
                format!("\"{t}\"*")
            } else {
                format!("\"{t}\"")
            }
        })
        .collect();
    Some(parts.join(" "))
}

/// Búsqueda full-text por nombre sobre todo el catálogo (M3). Devuelve el total
/// de coincidencias y hasta `limit` items con disco + ruta completa.
pub fn search(conn: &Connection, query: &str, limit: i64) -> DbResult<SearchResult> {
    let fts = match build_fts_query(query) {
        Some(f) => f,
        None => {
            return Ok(SearchResult {
                total: 0,
                items: Vec::new(),
                truncated: false,
            })
        }
    };

    let total: i64 = conn.query_row(
        "SELECT COUNT(*) FROM entries_fts WHERE entries_fts MATCH ?1",
        params![fts],
        |r| r.get(0),
    )?;

    let sql = "SELECT e.id, e.disk_id, d.name, e.name, e.is_folder, e.size_logical, e.modified_at
               FROM entries_fts f
               JOIN entries e ON e.id = f.rowid
               JOIN disks d ON d.id = e.disk_id
               WHERE f.entries_fts MATCH ?1
               ORDER BY rank
               LIMIT ?2";
    let mut stmt = conn.prepare(sql)?;
    let raw: Vec<(i64, i64, String, String, bool, i64, Option<i64>)> = stmt
        .query_map(params![fts, limit], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get::<_, i64>(4)? != 0,
                r.get(5)?,
                r.get(6)?,
            ))
        })?
        .collect::<Result<_, _>>()?;

    let mut items = Vec::with_capacity(raw.len());
    for (id, disk_id, disk_name, name, is_folder, size_logical, modified_at) in raw {
        let path = entry_path(conn, id)?;
        items.push(SearchItem {
            id,
            disk_id,
            disk_name,
            name,
            is_folder,
            size_logical,
            modified_at,
            path,
        });
    }

    Ok(SearchResult {
        truncated: total > items.len() as i64,
        total,
        items,
    })
}

// ---------- Embeddings semánticos (IA Fase 1) ----------

/// Item de búsqueda semántica: un `SearchItem` más el score de similitud coseno.
/// `frame_ts` = segundo del clip donde mejor matchea (None para imágenes).
#[derive(Debug, Clone, Serialize)]
pub struct SemanticItem {
    #[serde(flatten)]
    pub item: SearchItem,
    pub score: f32,
    pub frame_ts: Option<f64>,
    /// Fragmento de la transcripción donde matchea (Fase 4); None para hits visuales.
    #[serde(default)]
    pub snippet: Option<String>,
}

fn vec_to_blob(v: &[f32]) -> Vec<u8> {
    let mut b = Vec::with_capacity(v.len() * 4);
    for x in v {
        b.extend_from_slice(&x.to_le_bytes());
    }
    b
}

fn blob_to_vec(b: &[u8]) -> Vec<f32> {
    b.chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

/// Inserta un embedding (imagen → `frame_ts = None`; frame de video → `Some(seg)`).
/// Permite varias filas por entrada (un clip tiene varios frames).
pub fn store_embedding(
    conn: &Connection,
    entry_id: i64,
    model: &str,
    frame_ts: Option<f64>,
    vec: &[f32],
) -> DbResult<()> {
    conn.execute(
        "INSERT INTO embeddings (entry_id, model, frame_ts, dim, vec) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![entry_id, model, frame_ts, vec.len() as i64, vec_to_blob(vec)],
    )?;
    Ok(())
}

/// Borra los embeddings de una entrada para un modelo (antes de reindexarla).
pub fn delete_embeddings_for_entry(conn: &Connection, entry_id: i64, model: &str) -> DbResult<()> {
    conn.execute(
        "DELETE FROM embeddings WHERE entry_id = ?1 AND model = ?2",
        params![entry_id, model],
    )?;
    Ok(())
}

/// Borra TODOS los embeddings de un modelo (reindex completo).
pub fn clear_embeddings(conn: &Connection, model: &str) -> DbResult<()> {
    conn.execute("DELETE FROM embeddings WHERE model = ?1", params![model])?;
    Ok(())
}

/// Cantidad de ENTRADAS (no filas) con al menos un embedding para el modelo.
pub fn count_embeddings(conn: &Connection, model: &str) -> DbResult<i64> {
    conn.query_row(
        "SELECT COUNT(DISTINCT entry_id) FROM embeddings WHERE model = ?1",
        params![model],
        |r| r.get(0),
    )
}

/// Candidatos a indexar: entradas con thumbnail cacheado (visuales, embebibles
/// offline) que todavía NO tienen embedding para este modelo. Devuelve
/// (entry_id, png_bytes). Si `rebuild`, ignora los ya embebidos.
pub fn embedding_candidates(
    conn: &Connection,
    model: &str,
    rebuild: bool,
) -> DbResult<Vec<(i64, Vec<u8>)>> {
    let sql = if rebuild {
        "SELECT t.entry_id, t.png FROM thumbnails t \
         JOIN entries e ON e.id = t.entry_id WHERE e.is_folder = 0"
            .to_string()
    } else {
        "SELECT t.entry_id, t.png FROM thumbnails t \
         JOIN entries e ON e.id = t.entry_id \
         WHERE e.is_folder = 0 \
           AND NOT EXISTS (SELECT 1 FROM embeddings m WHERE m.entry_id = t.entry_id AND m.model = ?1)"
            .to_string()
    };
    let mut stmt = conn.prepare(&sql)?;
    let rows = if rebuild {
        stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Vec<u8>>(1)?)))?
            .collect::<Result<Vec<_>, _>>()?
    } else {
        stmt.query_map(params![model], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, Vec<u8>>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?
    };
    Ok(rows)
}

/// Total de candidatos visuales (entradas con thumbnail).
pub fn count_thumbnailed(conn: &Connection) -> DbResult<i64> {
    conn.query_row("SELECT COUNT(*) FROM thumbnails", [], |r| r.get(0))
}

/// Una fila de embedding para rankear en memoria: (entry_id, frame_ts, vector).
pub type EmbeddingRow = (i64, Option<f64>, Vec<f32>);

/// Carga todos los embeddings del modelo (todas las filas: 1 por imagen, N por clip).
pub fn load_embeddings(conn: &Connection, model: &str) -> DbResult<Vec<EmbeddingRow>> {
    let mut stmt =
        conn.prepare("SELECT entry_id, frame_ts, vec FROM embeddings WHERE model = ?1")?;
    let rows = stmt
        .query_map(params![model], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, Option<f64>>(1)?,
                blob_to_vec(&r.get::<_, Vec<u8>>(2)?),
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Carga los vectores de UNA entrada (1 para imagen, N para clip de video).
/// Sirve para "buscar similares": el caller promedia + normaliza para tener la
/// firma del archivo.
pub fn load_entry_embeddings(
    conn: &Connection,
    entry_id: i64,
    model: &str,
) -> DbResult<Vec<Vec<f32>>> {
    let mut stmt =
        conn.prepare("SELECT vec FROM embeddings WHERE entry_id = ?1 AND model = ?2")?;
    let rows = stmt
        .query_map(params![entry_id, model], |r| {
            Ok(blob_to_vec(&r.get::<_, Vec<u8>>(0)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Entradas de VIDEO de un disco (por extensión) que aún no tienen embedding para
/// el modelo. Devuelve los ids; el caller resuelve la ruta real y muestrea frames.
pub fn video_embedding_candidates(
    conn: &Connection,
    disk_id: i64,
    model: &str,
    exts: &[&str],
) -> DbResult<Vec<i64>> {
    if exts.is_empty() {
        return Ok(Vec::new());
    }
    let placeholders = exts.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    // Excluye clips que ya tienen frames muestreados (frame_ts NOT NULL). Los que
    // solo tienen el frame único de Fase 1 (ts NULL) SÍ califican para el upgrade.
    let sql = format!(
        "SELECT e.id FROM entries e \
         WHERE e.disk_id = ?1 AND e.is_folder = 0 \
           AND lower(e.ext) IN ({placeholders}) \
           AND NOT EXISTS (SELECT 1 FROM embeddings m \
                           WHERE m.entry_id = e.id AND m.model = ?2 AND m.frame_ts IS NOT NULL)"
    );
    let mut stmt = conn.prepare(&sql)?;
    // params: disk_id, model, exts...
    let mut p: Vec<Box<dyn ToSql>> = vec![Box::new(disk_id), Box::new(model.to_string())];
    for e in exts {
        p.push(Box::new(e.to_lowercase()));
    }
    let rows = stmt
        .query_map(params_from_iter(p.iter().map(|b| b.as_ref())), |r| {
            r.get::<_, i64>(0)
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Construye `SearchItem`s (con disco + ruta) para una lista de ids, preservando
/// el orden de entrada (el ranking lo decide el caller). Ids inexistentes se omiten.
pub fn search_items_by_ids(conn: &Connection, ids: &[i64]) -> DbResult<Vec<SearchItem>> {
    let mut out = Vec::with_capacity(ids.len());
    let mut stmt = conn.prepare(
        "SELECT e.id, e.disk_id, d.name, e.name, e.is_folder, e.size_logical, e.modified_at \
         FROM entries e JOIN disks d ON d.id = e.disk_id WHERE e.id = ?1",
    )?;
    for &id in ids {
        let row = stmt
            .query_map(params![id], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, i64>(4)? != 0,
                    r.get::<_, i64>(5)?,
                    r.get::<_, Option<i64>>(6)?,
                ))
            })?
            .next();
        if let Some(Ok((id, disk_id, disk_name, name, is_folder, size_logical, modified_at))) = row {
            let path = entry_path(conn, id)?;
            out.push(SearchItem {
                id,
                disk_id,
                disk_name,
                name,
                is_folder,
                size_logical,
                modified_at,
                path,
            });
        }
    }
    Ok(out)
}

// ---------- Transcripciones (IA Fase 4, Whisper) ----------

/// Guarda (o reemplaza) la transcripción de una entrada y la reindexa en el FTS.
pub fn store_transcript(
    conn: &Connection,
    entry_id: i64,
    model: &str,
    lang: Option<&str>,
    text: &str,
    created_at: i64,
) -> DbResult<()> {
    conn.execute(
        "INSERT OR REPLACE INTO transcripts (entry_id, model, lang, text, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![entry_id, model, lang, text, created_at],
    )?;
    // FTS standalone (rowid = entry_id): borrar la fila vieja y reinsertar.
    conn.execute("DELETE FROM transcripts_fts WHERE rowid = ?1", params![entry_id])?;
    conn.execute(
        "INSERT INTO transcripts_fts (rowid, text) VALUES (?1, ?2)",
        params![entry_id, text],
    )?;
    Ok(())
}

/// Cantidad de entradas con transcripción.
pub fn count_transcripts(conn: &Connection) -> DbResult<i64> {
    conn.query_row("SELECT COUNT(*) FROM transcripts", [], |r| r.get(0))
}

/// Entradas de audio/video de un disco (por extensión) que aún no tienen transcripción.
pub fn transcript_candidates(
    conn: &Connection,
    disk_id: i64,
    exts: &[&str],
) -> DbResult<Vec<i64>> {
    if exts.is_empty() {
        return Ok(Vec::new());
    }
    let placeholders = exts.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = format!(
        "SELECT e.id FROM entries e \
         WHERE e.disk_id = ?1 AND e.is_folder = 0 \
           AND lower(e.ext) IN ({placeholders}) \
           AND NOT EXISTS (SELECT 1 FROM transcripts t WHERE t.entry_id = e.id)"
    );
    let mut stmt = conn.prepare(&sql)?;
    let mut p: Vec<Box<dyn ToSql>> = vec![Box::new(disk_id)];
    for e in exts {
        p.push(Box::new(e.to_lowercase()));
    }
    let rows = stmt
        .query_map(params_from_iter(p.iter().map(|b| b.as_ref())), |r| {
            r.get::<_, i64>(0)
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Busca en las transcripciones (FTS) → (entry_id, snippet con el match resaltado…).
/// Devuelve hasta `limit` resultados ordenados por relevancia.
pub fn search_transcripts(
    conn: &Connection,
    query: &str,
    limit: i64,
) -> DbResult<Vec<(i64, String)>> {
    let fts = match build_fts_query(query) {
        Some(f) => f,
        None => return Ok(Vec::new()),
    };
    let mut stmt = conn.prepare(
        "SELECT rowid, snippet(transcripts_fts, 0, '«', '»', '…', 12) \
         FROM transcripts_fts WHERE transcripts_fts MATCH ?1 ORDER BY rank LIMIT ?2",
    )?;
    let rows = stmt
        .query_map(params![fts, limit], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Filtros de búsqueda avanzada (M4). Todos opcionales; se combinan con AND.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct SearchFilters {
    pub text: String,               // términos de nombre (FTS)
    pub exts: Vec<String>,          // extensiones (sin punto), OR entre ellas
    pub tags: Vec<String>,          // keywords; la entrada debe tener TODAS (AND)
    pub min_size: Option<i64>,      // bytes
    pub max_size: Option<i64>,
    pub modified_after: Option<i64>, // unix secs
    pub modified_before: Option<i64>,
    pub kind: Option<String>,       // "file" | "folder"
    pub disk_id: Option<i64>,       // limitar a un disco
    pub place: Option<String>,      // ubicación (gps_place LIKE), C1
    pub light: Option<String>,      // fase de luz (light_phase LIKE): sunset/golden/night…, C2
}

impl SearchFilters {
    fn is_empty(&self) -> bool {
        self.text.trim().is_empty()
            && self.exts.is_empty()
            && self.tags.is_empty()
            && self.min_size.is_none()
            && self.max_size.is_none()
            && self.modified_after.is_none()
            && self.modified_before.is_none()
            && self.kind.is_none()
            && self.disk_id.is_none()
            && self.place.as_ref().map_or(true, |p| p.trim().is_empty())
            && self.light.as_ref().map_or(true, |p| p.trim().is_empty())
    }
}

/// Búsqueda por atributos / booleana (M4). Traduce los filtros a SQL, usando FTS
/// para el nombre cuando hay texto. Devuelve total + items con disco y ruta.
pub fn search_advanced(conn: &Connection, f: &SearchFilters, limit: i64) -> DbResult<SearchResult> {
    if f.is_empty() {
        return Ok(SearchResult { total: 0, items: Vec::new(), truncated: false });
    }

    let fts = build_fts_query(&f.text);

    // Cláusulas + params en orden (el MATCH de FTS va primero si existe).
    let mut clauses: Vec<String> = Vec::new();
    let mut bind: Vec<Box<dyn ToSql>> = Vec::new();
    if let Some(q) = &fts {
        clauses.push("f.entries_fts MATCH ?".to_string());
        bind.push(Box::new(q.clone()));
    }
    if let Some(k) = &f.kind {
        clauses.push("e.is_folder = ?".to_string());
        bind.push(Box::new(if k == "folder" { 1i64 } else { 0i64 }));
    }
    if !f.exts.is_empty() {
        let ph = f.exts.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        clauses.push(format!("e.ext IN ({ph})"));
        for e in &f.exts {
            bind.push(Box::new(e.to_lowercase()));
        }
    }
    if !f.tags.is_empty() {
        // La entrada debe tener TODOS los tags pedidos (AND): se cuentan los
        // distintos que matchean y se exige que igualen la cantidad pedida.
        let ph = f.tags.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        clauses.push(format!(
            "e.id IN (SELECT et.entry_id FROM entry_tags et JOIN tags t ON t.id = et.tag_id \
             WHERE t.name IN ({ph}) GROUP BY et.entry_id HAVING COUNT(DISTINCT t.id) = ?)"
        ));
        for t in &f.tags {
            bind.push(Box::new(t.trim().to_lowercase()));
        }
        bind.push(Box::new(f.tags.len() as i64));
    }
    if let Some(v) = f.min_size {
        clauses.push("e.size_logical >= ?".to_string());
        bind.push(Box::new(v));
    }
    if let Some(v) = f.max_size {
        clauses.push("e.size_logical <= ?".to_string());
        bind.push(Box::new(v));
    }
    if let Some(v) = f.modified_after {
        clauses.push("e.modified_at >= ?".to_string());
        bind.push(Box::new(v));
    }
    if let Some(v) = f.modified_before {
        clauses.push("e.modified_at <= ?".to_string());
        bind.push(Box::new(v));
    }
    if let Some(v) = f.disk_id {
        clauses.push("e.disk_id = ?".to_string());
        bind.push(Box::new(v));
    }
    if let Some(p) = f.place.as_ref().map(|s| s.trim()).filter(|s| !s.is_empty()) {
        // Ubicación (C1): coincide con el nombre de lugar resuelto del GPS.
        clauses.push("e.gps_place LIKE ?".to_string());
        bind.push(Box::new(format!("%{p}%")));
    }
    if let Some(l) = f.light.as_ref().map(|s| s.trim()).filter(|s| !s.is_empty()) {
        // Fase de luz (C2): atardecer/golden/noche derivado de la posición solar.
        clauses.push("e.light_phase LIKE ?".to_string());
        bind.push(Box::new(format!("%{}%", l.to_lowercase())));
    }

    let from = if fts.is_some() {
        "entries_fts f JOIN entries e ON e.id = f.rowid JOIN disks d ON d.id = e.disk_id"
    } else {
        "entries e JOIN disks d ON d.id = e.disk_id"
    };
    let where_sql = format!("WHERE {}", clauses.join(" AND "));
    let order = if fts.is_some() { "ORDER BY rank" } else { "ORDER BY e.size_logical DESC" };

    // Total.
    let count_sql = format!("SELECT COUNT(*) FROM {from} {where_sql}");
    let total: i64 = conn.query_row(&count_sql, params_from_iter(bind.iter().map(|b| b.as_ref())), |r| r.get(0))?;

    // Items (agrega el LIMIT al final).
    let sel_sql = format!(
        "SELECT e.id, e.disk_id, d.name, e.name, e.is_folder, e.size_logical, e.modified_at \
         FROM {from} {where_sql} {order} LIMIT ?"
    );
    let mut sel_bind = bind;
    sel_bind.push(Box::new(limit));

    let mut stmt = conn.prepare(&sel_sql)?;
    let raw: Vec<(i64, i64, String, String, bool, i64, Option<i64>)> = stmt
        .query_map(params_from_iter(sel_bind.iter().map(|b| b.as_ref())), |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get::<_, i64>(4)? != 0, r.get(5)?, r.get(6)?))
        })?
        .collect::<Result<_, _>>()?;

    let mut items = Vec::with_capacity(raw.len());
    for (id, disk_id, disk_name, name, is_folder, size_logical, modified_at) in raw {
        let path = entry_path(conn, id)?;
        items.push(SearchItem { id, disk_id, disk_name, name, is_folder, size_logical, modified_at, path });
    }

    Ok(SearchResult { truncated: total > items.len() as i64, total, items })
}

// ---------- Estadísticas (M8) ----------

#[derive(Debug, Clone, Serialize)]
pub struct ExtStat {
    pub ext: String,
    pub count: i64,
    pub total_size: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct BigFile {
    pub id: i64,
    pub name: String,
    pub disk_name: String,
    pub size_logical: i64,
    pub path: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Stats {
    pub file_count: i64,
    pub folder_count: i64,
    pub total_size: i64,
    pub by_ext: Vec<ExtStat>,
    pub biggest: Vec<BigFile>,
}

/// Estadísticas del catálogo o de un disco (si `disk_id` es Some).
pub fn stats(conn: &Connection, disk_id: Option<i64>) -> DbResult<Stats> {
    let (scope, has_scope) = match disk_id {
        Some(_) => (" AND e.disk_id = ?1", true),
        None => ("", false),
    };

    // Totales: leídos de la tabla `disks` (instantáneo), no escaneando millones
    // de filas de `entries`. file_count/folder_count/total_size se guardan al
    // ingestar cada disco.
    let (file_count, folder_count, total_size): (i64, i64, i64) = {
        let sql = if has_scope {
            "SELECT COALESCE(file_count,0), COALESCE(folder_count,0), COALESCE(total_size,0) \
             FROM disks WHERE id = ?1"
        } else {
            "SELECT COALESCE(SUM(file_count),0), COALESCE(SUM(folder_count),0), COALESCE(SUM(total_size),0) \
             FROM disks"
        };
        if has_scope {
            conn.query_row(sql, params![disk_id.unwrap()], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        } else {
            conn.query_row(sql, [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        }
    };

    // Distribución por extensión (top 25 por tamaño total).
    let by_ext: Vec<ExtStat> = {
        let sql = format!(
            "SELECT e.ext, COUNT(*), SUM(e.size_logical) FROM entries e \
             WHERE e.is_folder = 0 AND e.ext IS NOT NULL{scope} \
             GROUP BY e.ext ORDER BY SUM(e.size_logical) DESC LIMIT 25"
        );
        let mut stmt = conn.prepare(&sql)?;
        let map = |r: &rusqlite::Row| Ok(ExtStat { ext: r.get(0)?, count: r.get(1)?, total_size: r.get(2)? });
        let rows = if has_scope {
            stmt.query_map(params![disk_id.unwrap()], map)?.collect::<Result<_, _>>()?
        } else {
            stmt.query_map([], map)?.collect::<Result<_, _>>()?
        };
        rows
    };

    // Archivos más grandes (top 25).
    let biggest: Vec<BigFile> = {
        let sql = format!(
            "SELECT e.id, e.name, d.name, e.size_logical FROM entries e JOIN disks d ON d.id = e.disk_id \
             WHERE e.is_folder = 0{scope} ORDER BY e.size_logical DESC LIMIT 25"
        );
        let mut stmt = conn.prepare(&sql)?;
        let map = |r: &rusqlite::Row| -> rusqlite::Result<(i64, String, String, i64)> {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        };
        let raw: Vec<(i64, String, String, i64)> = if has_scope {
            stmt.query_map(params![disk_id.unwrap()], map)?.collect::<Result<_, _>>()?
        } else {
            stmt.query_map([], map)?.collect::<Result<_, _>>()?
        };
        let mut v = Vec::with_capacity(raw.len());
        for (id, name, disk_name, size_logical) in raw {
            v.push(BigFile { id, name, disk_name, size_logical, path: entry_path(conn, id)? });
        }
        v
    };

    Ok(Stats { file_count, folder_count, total_size, by_ext, biggest })
}

// ---------- Duplicados (M8) ----------

#[derive(Debug, Clone, Serialize)]
pub struct DupGroup {
    pub name: String,
    pub size: i64,
    pub count: i64,
    /// Espacio desperdiciado = (count-1) * size.
    pub wasted: i64,
    pub items: Vec<BigFile>,
}

/// Encuentra archivos duplicados por nombre+tamaño (P2). Ordena por espacio
/// desperdiciado. `min_size` evita el ruido de miles de archivitos iguales.
pub fn duplicates(conn: &Connection, min_size: i64, limit: i64) -> DbResult<Vec<DupGroup>> {
    let mut stmt = conn.prepare(
        "SELECT name, size_logical, COUNT(*) c FROM entries \
         WHERE is_folder = 0 AND size_logical >= ?1 \
         GROUP BY name, size_logical HAVING c > 1 \
         ORDER BY (c - 1) * size_logical DESC LIMIT ?2",
    )?;
    let groups: Vec<(String, i64, i64)> = stmt
        .query_map(params![min_size, limit], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<Result<_, _>>()?;

    let mut out = Vec::with_capacity(groups.len());
    for (name, size, count) in groups {
        let mut istmt = conn.prepare(
            "SELECT e.id, e.name, d.name, e.size_logical FROM entries e JOIN disks d ON d.id = e.disk_id \
             WHERE e.is_folder = 0 AND e.name = ?1 AND e.size_logical = ?2 LIMIT 100",
        )?;
        let raw: Vec<(i64, String, String, i64)> = istmt
            .query_map(params![name, size], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
            .collect::<Result<_, _>>()?;
        let mut items = Vec::with_capacity(raw.len());
        for (id, n, disk_name, sz) in raw {
            items.push(BigFile { id, name: n, disk_name, size_logical: sz, path: entry_path(conn, id)? });
        }
        out.push(DupGroup { name, size, count, wasted: (count - 1) * size, items });
    }
    Ok(out)
}

// ─────────────────────────── Auditoría de backup (B1) ───────────────────────────
//
// Compara dos subárboles del catálogo (source vs destination) y reporta qué archivos
// del source faltan / difieren / no se pueden verificar en el destino. OFFLINE: opera
// sobre el catálogo, no necesita los discos montados. La identidad de "mismo archivo"
// es la RUTA RELATIVA al root elegido; la verificación de contenido usa el hash BLAKE3
// (poblado por el escaneo enriquecido, A2). Sin hash → se cae a comparación por tamaño.

/// Referencia a un archivo del source para mostrar/accionar en el reporte.
#[derive(Debug, Clone, Serialize)]
pub struct FileRef {
    pub entry_id: i64,
    pub rel_path: String,
    pub name: String,
    pub size: i64,
}

/// Resultado de comparar un subárbol source contra uno destination.
#[derive(Debug, Clone, Serialize)]
pub struct BackupReport {
    /// Archivos del source presentes en dest y verificados por hash idéntico.
    pub ok: u64,
    /// Archivos del source que NO existen en dest (mismo rel_path no encontrado).
    pub missing: Vec<FileRef>,
    /// Existen en dest con el mismo rel_path pero contenido distinto (hash difiere)
    /// o tamaño distinto sin hash → copia parcial/corrupta/versión vieja.
    pub mismatch: Vec<FileRef>,
    /// Existen en dest con el mismo rel_path y tamaño, pero falta hash de algún lado
    /// → presentes pero NO verificados por contenido.
    pub unverified: Vec<FileRef>,
    /// Cantidad de archivos en dest que no están en source (informativo).
    pub extra: u64,
    /// Bytes lógicos de los archivos faltantes (para estimar la copia).
    pub missing_bytes: i64,
    /// Total de archivos del source comparados.
    pub source_total: u64,
    /// true si no falta nada y nada difiere (lo `unverified` no bloquea, pero la UI lo muestra).
    pub fully_backed_up: bool,
}

/// Archivo de un subárbol con su ruta relativa al root elegido.
struct SubtreeFile {
    entry_id: i64,
    rel_path: String,
    name: String,
    size: i64,
    hash: Option<String>,
}

/// Entrada raíz (volumen) de un disco: la fila sin padre.
pub fn disk_root_entry(conn: &Connection, disk_id: i64) -> DbResult<Option<i64>> {
    conn.query_row(
        "SELECT id FROM entries WHERE disk_id = ?1 AND parent_id IS NULL",
        params![disk_id],
        |r| r.get(0),
    )
    .optional()
}

/// Todos los ARCHIVOS (no carpetas) descendientes de `root_entry`, con su ruta
/// relativa al root (el root queda como ""). Descenso por CTE recursiva usando el
/// índice (disk_id, parent_id).
fn collect_subtree_files(conn: &Connection, root_entry: i64) -> DbResult<Vec<SubtreeFile>> {
    let disk_id: i64 =
        conn.query_row("SELECT disk_id FROM entries WHERE id = ?1", params![root_entry], |r| r.get(0))?;
    let mut stmt = conn.prepare(
        "WITH RECURSIVE sub(id, name, is_folder, size_logical, content_hash, rel) AS (
           SELECT id, name, is_folder, size_logical, content_hash, ''
             FROM entries WHERE id = ?1
           UNION ALL
           SELECT e.id, e.name, e.is_folder, e.size_logical, e.content_hash,
                  CASE WHEN s.rel = '' THEN e.name ELSE s.rel || '/' || e.name END
             FROM entries e JOIN sub s ON e.parent_id = s.id
            WHERE e.disk_id = ?2
         )
         SELECT id, rel, name, size_logical, content_hash FROM sub WHERE is_folder = 0",
    )?;
    let rows = stmt.query_map(params![root_entry, disk_id], |r| {
        Ok(SubtreeFile {
            entry_id: r.get(0)?,
            rel_path: r.get(1)?,
            name: r.get(2)?,
            size: r.get(3)?,
            hash: r.get(4)?,
        })
    })?;
    rows.collect()
}

/// Compara el subárbol `source_root` contra `dest_root` (ids de entrada raíz).
/// Nota de rendimiento: carga ambos subárboles en memoria (HashMap por rel_path).
/// Apto para comparar carpetas de proyecto / tarjetas; para discos enteros de
/// millones de archivos puede ser pesado (aceptable para B1).
pub fn compare_subtrees(conn: &Connection, source_root: i64, dest_root: i64) -> DbResult<BackupReport> {
    use std::collections::{HashMap, HashSet};

    let src = collect_subtree_files(conn, source_root)?;
    let dst = collect_subtree_files(conn, dest_root)?;

    let to_ref = |f: &SubtreeFile| FileRef {
        entry_id: f.entry_id,
        rel_path: f.rel_path.clone(),
        name: f.name.clone(),
        size: f.size,
    };

    let mut dest_by_rel: HashMap<&str, &SubtreeFile> = HashMap::with_capacity(dst.len());
    for d in &dst {
        dest_by_rel.insert(d.rel_path.as_str(), d);
    }
    let src_rels: HashSet<&str> = src.iter().map(|f| f.rel_path.as_str()).collect();

    let mut ok: u64 = 0;
    let mut missing: Vec<FileRef> = Vec::new();
    let mut mismatch: Vec<FileRef> = Vec::new();
    let mut unverified: Vec<FileRef> = Vec::new();
    let mut missing_bytes: i64 = 0;

    for f in &src {
        match dest_by_rel.get(f.rel_path.as_str()) {
            None => {
                missing_bytes += f.size.max(0);
                missing.push(to_ref(f));
            }
            Some(d) => match (f.hash.as_deref(), d.hash.as_deref()) {
                (Some(a), Some(b)) if a == b => ok += 1,
                (Some(_), Some(_)) => mismatch.push(to_ref(f)), // hashes difieren
                _ => {
                    // Falta hash de algún lado: no verificable por contenido.
                    if f.size == d.size {
                        unverified.push(to_ref(f)); // mismo tamaño, sin verificar
                    } else {
                        mismatch.push(to_ref(f)); // tamaño distinto → copia parcial/distinta
                    }
                }
            },
        }
    }

    let extra = dst.iter().filter(|d| !src_rels.contains(d.rel_path.as_str())).count() as u64;
    let fully_backed_up = missing.is_empty() && mismatch.is_empty();

    Ok(BackupReport {
        ok,
        missing,
        mismatch,
        unverified,
        extra,
        missing_bytes,
        source_total: src.len() as u64,
        fully_backed_up,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dcmf::{DcmfDisk, DcmfEntry};

    fn sample_disk() -> DcmfDisk {
        DcmfDisk {
            name: "SF28".into(),
            entries: vec![
                DcmfEntry { name: "SF28".into(), parent: -1, is_folder: true, is_volume: true, size_logical: 0, size_physical: 0, created: 0, modified: 0 },
                DcmfEntry { name: "CLIP".into(), parent: 0, is_folder: true, is_volume: false, size_logical: 0, size_physical: 0, created: 0, modified: 0 },
                DcmfEntry { name: "C0001.MP4".into(), parent: 1, is_folder: false, is_volume: false, size_logical: 4_563_402_752, size_physical: 4_563_406_848, created: 1_685_577_600, modified: 1_685_581_200 },
                DcmfEntry { name: "B-ROLL.MOV".into(), parent: 1, is_folder: false, is_volume: false, size_logical: 1_000_000_000, size_physical: 1_000_001_024, created: 0, modified: 0 },
            ],
        }
    }

    #[test]
    fn ingests_and_counts() {
        let mut conn = open_in_memory().unwrap();
        let n = ingest_disks(&mut conn, &[sample_disk()]).unwrap();
        assert_eq!(n, 4);
        let disks: i64 = conn.query_row("SELECT COUNT(*) FROM disks", [], |r| r.get(0)).unwrap();
        assert_eq!(disks, 1);
        let files: i64 = conn.query_row("SELECT file_count FROM disks", [], |r| r.get(0)).unwrap();
        assert_eq!(files, 2);
    }

    #[test]
    fn parent_links_are_correct() {
        let mut conn = open_in_memory().unwrap();
        ingest_disks(&mut conn, &[sample_disk()]).unwrap();
        // El archivo C0001.MP4 debe colgar de la carpeta CLIP.
        let parent_name: String = conn
            .query_row(
                "SELECT p.name FROM entries c JOIN entries p ON c.parent_id = p.id WHERE c.name = 'C0001.MP4'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(parent_name, "CLIP");
    }

    #[test]
    fn folder_size_is_recursive_sum() {
        let mut conn = open_in_memory().unwrap();
        ingest_disks(&mut conn, &[sample_disk()]).unwrap();
        let clip_size: i64 = conn
            .query_row("SELECT size_logical FROM entries WHERE name='CLIP'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(clip_size as u64, 4_563_402_752 + 1_000_000_000);
        // El volumen agrega lo mismo (todo cuelga de CLIP).
        let vol_size: i64 = conn
            .query_row("SELECT total_size FROM disks", [], |r| r.get(0))
            .unwrap();
        assert_eq!(vol_size as u64, 4_563_402_752 + 1_000_000_000);
    }

    #[test]
    fn size_over_4gb_not_truncated_in_db() {
        let mut conn = open_in_memory().unwrap();
        ingest_disks(&mut conn, &[sample_disk()]).unwrap();
        let size: i64 = conn
            .query_row("SELECT size_logical FROM entries WHERE name='C0001.MP4'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(size as u64, 4_563_402_752);
    }

    #[test]
    fn ext_is_lowercased() {
        let mut conn = open_in_memory().unwrap();
        ingest_disks(&mut conn, &[sample_disk()]).unwrap();
        let ext: String = conn
            .query_row("SELECT ext FROM entries WHERE name='C0001.MP4'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(ext, "mp4");
    }

    #[test]
    fn fts_finds_by_name() {
        let mut conn = open_in_memory().unwrap();
        ingest_disks(&mut conn, &[sample_disk()]).unwrap();
        // Buscar por extensión .mov vía FTS sobre el nombre.
        let hits: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM entries_fts WHERE entries_fts MATCH 'mov'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(hits, 1);
    }

    #[test]
    fn list_children_navigates_tree() {
        let mut conn = open_in_memory().unwrap();
        ingest_disks(&mut conn, &[sample_disk()]).unwrap();
        let disk_id: i64 = conn.query_row("SELECT id FROM disks", [], |r| r.get(0)).unwrap();

        // Raíz del disco = se SALTA el nodo volumen y se ven sus hijos directos (CLIP).
        let root = list_children(&conn, disk_id, None).unwrap();
        assert_eq!(root.len(), 1);
        assert_eq!(root[0].name, "CLIP");
        assert!(root[0].is_folder);
        assert_eq!(root[0].child_count, 2); // contiene los 2 archivos

        // Hijos de CLIP: dos archivos, ordenados por nombre.
        let lvl2 = list_children(&conn, disk_id, Some(root[0].id)).unwrap();
        assert_eq!(lvl2.len(), 2);
        assert!(!lvl2[0].is_folder);
        assert_eq!(lvl2[0].name, "B-ROLL.MOV"); // B antes que C
        assert_eq!(lvl2[1].name, "C0001.MP4");
    }

    #[test]
    fn entry_path_reconstructs_full_path() {
        let mut conn = open_in_memory().unwrap();
        ingest_disks(&mut conn, &[sample_disk()]).unwrap();
        let id: i64 = conn
            .query_row("SELECT id FROM entries WHERE name='C0001.MP4'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(entry_path(&conn, id).unwrap(), "/SF28/CLIP/C0001.MP4");
    }

    #[test]
    fn search_returns_path_and_disk() {
        let mut conn = open_in_memory().unwrap();
        ingest_disks(&mut conn, &[sample_disk()]).unwrap();
        let res = search(&conn, ".mov", 100).unwrap();
        assert_eq!(res.total, 1);
        assert_eq!(res.items.len(), 1);
        assert_eq!(res.items[0].name, "B-ROLL.MOV");
        assert_eq!(res.items[0].disk_name, "SF28");
        assert_eq!(res.items[0].path, "/SF28/CLIP/B-ROLL.MOV");
        assert!(!res.truncated);
    }

    #[test]
    fn search_prefix_matches_partial_name() {
        let mut conn = open_in_memory().unwrap();
        ingest_disks(&mut conn, &[sample_disk()]).unwrap();
        // "C000" como prefijo debe encontrar C0001.MP4.
        let res = search(&conn, "C000", 100).unwrap();
        assert!(res.items.iter().any(|i| i.name == "C0001.MP4"));
    }

    #[test]
    fn build_fts_query_tokenizes_and_prefixes() {
        assert_eq!(build_fts_query(".mov"), Some("\"mov\"*".into()));
        assert_eq!(build_fts_query("C0001"), Some("\"C0001\"*".into()));
        assert_eq!(build_fts_query("foo bar"), Some("\"foo\" \"bar\"*".into()));
        assert_eq!(build_fts_query("   "), None);
        assert_eq!(build_fts_query(""), None);
    }

    #[test]
    fn search_empty_query_is_empty() {
        let mut conn = open_in_memory().unwrap();
        ingest_disks(&mut conn, &[sample_disk()]).unwrap();
        let res = search(&conn, "  ", 100).unwrap();
        assert_eq!(res.total, 0);
        assert!(res.items.is_empty());
    }

    #[test]
    fn scan_ingest_sets_online_and_fingerprint() {
        let mut conn = open_in_memory().unwrap();
        let r = ingest_scanned(&mut conn, &sample_disk(), Some("UUID-1"), "ssd", Some(1000), "/Volumes/SF28", None).unwrap();
        assert!(!r.replaced);
        let (online, uuid, mount): (i64, String, String) = conn
            .query_row(
                "SELECT is_online, volume_uuid, mount_path FROM disks WHERE id=?1",
                params![r.disk_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(online, 1);
        assert_eq!(uuid, "UUID-1");
        assert_eq!(mount, "/Volumes/SF28");
        // El FTS incremental encuentra el archivo recién escaneado.
        let res = search(&conn, "C0001", 10).unwrap();
        assert_eq!(res.total, 1);
    }

    #[test]
    fn advanced_filters_by_ext() {
        let mut conn = open_in_memory().unwrap();
        ingest_disks(&mut conn, &[sample_disk()]).unwrap();
        let f = SearchFilters { exts: vec!["mov".into()], ..Default::default() };
        let r = search_advanced(&conn, &f, 100).unwrap();
        assert_eq!(r.total, 1);
        assert_eq!(r.items[0].name, "B-ROLL.MOV");
    }

    #[test]
    fn advanced_filters_by_size_and_ext_combined() {
        let mut conn = open_in_memory().unwrap();
        ingest_disks(&mut conn, &[sample_disk()]).unwrap();
        // mp4 mayores a 2 GB → solo C0001.MP4 (4.25 GB), no el de 1 GB (que es .mov).
        let f = SearchFilters {
            exts: vec!["mp4".into()],
            min_size: Some(2_000_000_000),
            ..Default::default()
        };
        let r = search_advanced(&conn, &f, 100).unwrap();
        assert_eq!(r.total, 1);
        assert_eq!(r.items[0].name, "C0001.MP4");
    }

    #[test]
    fn advanced_text_plus_filter() {
        let mut conn = open_in_memory().unwrap();
        ingest_disks(&mut conn, &[sample_disk()]).unwrap();
        // nombre que contiene "C000" y es archivo.
        let f = SearchFilters {
            text: "C000".into(),
            kind: Some("file".into()),
            ..Default::default()
        };
        let r = search_advanced(&conn, &f, 100).unwrap();
        assert!(r.items.iter().any(|i| i.name == "C0001.MP4"));
    }

    #[test]
    fn advanced_folders_only() {
        let mut conn = open_in_memory().unwrap();
        ingest_disks(&mut conn, &[sample_disk()]).unwrap();
        let f = SearchFilters { kind: Some("folder".into()), ..Default::default() };
        let r = search_advanced(&conn, &f, 100).unwrap();
        // volumen SF28 + carpeta CLIP
        assert_eq!(r.total, 2);
        assert!(r.items.iter().all(|i| i.is_folder));
    }

    #[test]
    fn stats_summarizes_catalog() {
        let mut conn = open_in_memory().unwrap();
        ingest_disks(&mut conn, &[sample_disk()]).unwrap();
        let s = stats(&conn, None).unwrap();
        assert_eq!(s.file_count, 2);
        assert_eq!(s.folder_count, 2); // SF28 (volumen) + CLIP
        assert_eq!(s.total_size as u64, 4_563_402_752 + 1_000_000_000);
        assert_eq!(s.biggest[0].name, "C0001.MP4");
        assert!(s.by_ext.iter().any(|e| e.ext == "mp4"));
        assert!(s.by_ext.iter().any(|e| e.ext == "mov"));
    }

    #[test]
    fn set_comment_persists() {
        let mut conn = open_in_memory().unwrap();
        ingest_disks(&mut conn, &[sample_disk()]).unwrap();
        let id: i64 = conn.query_row("SELECT id FROM entries WHERE name='C0001.MP4'", [], |r| r.get(0)).unwrap();
        set_entry_comment(&conn, id, Some("toma buena")).unwrap();
        let e = get_entry(&conn, id).unwrap().unwrap();
        assert_eq!(e.comment.as_deref(), Some("toma buena"));
    }

    #[test]
    fn duplicates_found_across_disks() {
        let mut conn = open_in_memory().unwrap();
        // Mismo disco ingestado dos veces → C0001.MP4 duplicado por nombre+tamaño.
        ingest_disks(&mut conn, &[sample_disk(), sample_disk()]).unwrap();
        let dups = duplicates(&conn, 1, 100).unwrap();
        let c0001 = dups.iter().find(|g| g.name == "C0001.MP4").unwrap();
        assert_eq!(c0001.count, 2);
        assert_eq!(c0001.wasted as u64, 4_563_402_752);
        assert_eq!(c0001.items.len(), 2);
    }

    #[test]
    fn tags_add_list_and_remove() {
        let mut conn = open_in_memory().unwrap();
        ingest_disks(&mut conn, &[sample_disk()]).unwrap();
        let id: i64 = conn.query_row("SELECT id FROM entries WHERE name='C0001.MP4'", [], |r| r.get(0)).unwrap();

        // Agregar (normaliza a minúsculas) e idempotencia.
        add_entry_tag(&conn, id, "Boda").unwrap();
        add_entry_tag(&conn, id, "boda").unwrap(); // duplicado: no rompe
        add_entry_tag(&conn, id, "  4K  ").unwrap();
        let tags = entry_tags(&conn, id).unwrap();
        assert_eq!(tags, vec!["4k".to_string(), "boda".to_string()]);

        // El conteo global refleja el uso.
        let all = list_tags(&conn).unwrap();
        assert!(all.iter().any(|t| t.name == "boda" && t.count == 1));

        // Quitar: el tag huérfano desaparece del catálogo.
        remove_entry_tag(&conn, id, "boda").unwrap();
        assert_eq!(entry_tags(&conn, id).unwrap(), vec!["4k".to_string()]);
        let orphan: i64 = conn.query_row("SELECT COUNT(*) FROM tags WHERE name='boda'", [], |r| r.get(0)).unwrap();
        assert_eq!(orphan, 0);
    }

    #[test]
    fn search_filters_by_tags_and() {
        let mut conn = open_in_memory().unwrap();
        ingest_disks(&mut conn, &[sample_disk()]).unwrap();
        let mp4: i64 = conn.query_row("SELECT id FROM entries WHERE name='C0001.MP4'", [], |r| r.get(0)).unwrap();
        let mov: i64 = conn.query_row("SELECT id FROM entries WHERE name='B-ROLL.MOV'", [], |r| r.get(0)).unwrap();
        add_entry_tag(&conn, mp4, "boda").unwrap();
        add_entry_tag(&conn, mp4, "seleccion").unwrap();
        add_entry_tag(&conn, mov, "boda").unwrap();

        // Un solo tag: matchea ambos.
        let f1 = SearchFilters { tags: vec!["boda".into()], ..Default::default() };
        assert_eq!(search_advanced(&conn, &f1, 100).unwrap().total, 2);

        // AND de dos tags: sólo el que tiene ambos.
        let f2 = SearchFilters { tags: vec!["boda".into(), "seleccion".into()], ..Default::default() };
        let r2 = search_advanced(&conn, &f2, 100).unwrap();
        assert_eq!(r2.total, 1);
        assert_eq!(r2.items[0].name, "C0001.MP4");

        // Tag + extensión combinados.
        let f3 = SearchFilters { tags: vec!["boda".into()], exts: vec!["mov".into()], ..Default::default() };
        let r3 = search_advanced(&conn, &f3, 100).unwrap();
        assert_eq!(r3.total, 1);
        assert_eq!(r3.items[0].name, "B-ROLL.MOV");
    }

    #[test]
    fn thumbnails_store_get_and_pending() {
        let mut conn = open_in_memory().unwrap();
        ingest_disks(&mut conn, &[sample_disk()]).unwrap();
        // Insertar una imagen falsa para tener una extensión de imagen.
        conn.execute(
            "INSERT INTO entries (disk_id, parent_id, name, is_folder, ext) \
             VALUES ((SELECT id FROM disks LIMIT 1), NULL, 'foto.jpg', 0, 'jpg')",
            [],
        )
        .unwrap();
        let img: i64 = conn.query_row("SELECT id FROM entries WHERE name='foto.jpg'", [], |r| r.get(0)).unwrap();
        let disk_id: i64 = conn.query_row("SELECT id FROM disks LIMIT 1", [], |r| r.get(0)).unwrap();

        // Pendiente antes de cachear.
        let pending = image_entries_without_thumb(&conn, disk_id, &["jpg", "png"]).unwrap();
        assert_eq!(pending, vec![img]);
        assert!(get_cached_thumbnail(&conn, img).unwrap().is_none());

        // Guardar y recuperar.
        store_thumbnail(&conn, img, &[1, 2, 3, 4], 64, 48).unwrap();
        assert_eq!(get_cached_thumbnail(&conn, img).unwrap(), Some(vec![1, 2, 3, 4]));

        // Ya no figura como pendiente.
        assert!(image_entries_without_thumb(&conn, disk_id, &["jpg", "png"]).unwrap().is_empty());
    }

    #[test]
    fn embeddings_store_candidates_load_and_items() {
        let mut conn = open_in_memory().unwrap();
        ingest_disks(&mut conn, &[sample_disk()]).unwrap();
        // Dos archivos con thumbnail cacheado (candidatos visuales).
        conn.execute(
            "INSERT INTO entries (disk_id, parent_id, name, is_folder, ext) \
             VALUES ((SELECT id FROM disks LIMIT 1), NULL, 'a.jpg', 0, 'jpg'), \
                    ((SELECT id FROM disks LIMIT 1), NULL, 'b.png', 0, 'png')",
            [],
        )
        .unwrap();
        let a: i64 = conn.query_row("SELECT id FROM entries WHERE name='a.jpg'", [], |r| r.get(0)).unwrap();
        let b: i64 = conn.query_row("SELECT id FROM entries WHERE name='b.png'", [], |r| r.get(0)).unwrap();
        store_thumbnail(&conn, a, &[1, 2, 3], 4, 4).unwrap();
        store_thumbnail(&conn, b, &[4, 5, 6], 4, 4).unwrap();

        let model = "test-model";
        // Antes de embeber: 2 candidatos, 0 embeddings.
        assert_eq!(count_thumbnailed(&conn).unwrap(), 2);
        assert_eq!(count_embeddings(&conn, model).unwrap(), 0);
        assert_eq!(embedding_candidates(&conn, model, false).unwrap().len(), 2);

        // Embeber 'a' (imagen → frame_ts None).
        store_embedding(&conn, a, model, None, &[1.0, 0.0, 0.0]).unwrap();
        assert_eq!(count_embeddings(&conn, model).unwrap(), 1);
        // Solo 'b' queda pendiente (a menos que rebuild).
        let pend = embedding_candidates(&conn, model, false).unwrap();
        assert_eq!(pend.iter().map(|(id, _)| *id).collect::<Vec<_>>(), vec![b]);
        assert_eq!(embedding_candidates(&conn, model, true).unwrap().len(), 2);

        // 'b' como "video": dos frames con timestamp → cuenta como 1 entrada.
        store_embedding(&conn, b, model, Some(1.0), &[0.0, 1.0, 0.0]).unwrap();
        store_embedding(&conn, b, model, Some(2.0), &[0.0, 0.0, 1.0]).unwrap();
        assert_eq!(count_embeddings(&conn, model).unwrap(), 2);
        let loaded = load_embeddings(&conn, model).unwrap();
        assert_eq!(loaded.len(), 3); // 1 (a) + 2 frames (b)
        let va = loaded.iter().find(|(id, ts, _)| *id == a && ts.is_none()).unwrap().2.clone();
        assert_eq!(va, vec![1.0, 0.0, 0.0]);

        // delete_embeddings_for_entry borra solo las filas de esa entrada.
        delete_embeddings_for_entry(&conn, b, model).unwrap();
        assert_eq!(count_embeddings(&conn, model).unwrap(), 1);

        // search_items_by_ids preserva el orden pedido y trae disco + ruta.
        let items = search_items_by_ids(&conn, &[b, a]).unwrap();
        assert_eq!(items.iter().map(|i| i.id).collect::<Vec<_>>(), vec![b, a]);
        assert_eq!(items[0].name, "b.png");
        assert!(!items[0].disk_name.is_empty());
    }

    #[test]
    fn transcripts_store_search_and_candidates() {
        let mut conn = open_in_memory().unwrap();
        ingest_disks(&mut conn, &[sample_disk()]).unwrap();
        conn.execute(
            "INSERT INTO entries (disk_id, parent_id, name, is_folder, ext) \
             VALUES ((SELECT id FROM disks LIMIT 1), NULL, 'clip.mp4', 0, 'mp4'), \
                    ((SELECT id FROM disks LIMIT 1), NULL, 'nota.txt', 0, 'txt')",
            [],
        )
        .unwrap();
        let a: i64 = conn.query_row("SELECT id FROM entries WHERE name='clip.mp4'", [], |r| r.get(0)).unwrap();
        let disk_id: i64 = conn.query_row("SELECT id FROM disks LIMIT 1", [], |r| r.get(0)).unwrap();
        let exts = &["mp4", "mov", "mp3"];

        // El mp4 nuevo es candidato; el txt no (no es A/V). (sample_disk ya trae
        // sus propios .mp4/.mov, así que el candidato exacto no es solo `a`.)
        let cands = transcript_candidates(&conn, disk_id, exts).unwrap();
        assert!(cands.contains(&a));
        assert!(!cands.contains(
            &conn.query_row("SELECT id FROM entries WHERE name='nota.txt'", [], |r| r.get::<_, i64>(0)).unwrap()
        ));

        store_transcript(&conn, a, "whisper-base", Some("es"), "hola esto es una prueba de perros", 123).unwrap();
        assert_eq!(count_transcripts(&conn).unwrap(), 1);
        // Ya no es candidato.
        assert!(!transcript_candidates(&conn, disk_id, exts).unwrap().contains(&a));
        // Se encuentra por lo que se dice.
        let hits = search_transcripts(&conn, "perros", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].0, a);
        assert!(!hits[0].1.is_empty());

        // Re-transcribir REEMPLAZA en el FTS (sin duplicar): perros ya no está, gatos sí.
        store_transcript(&conn, a, "whisper-base", Some("es"), "ahora habla de gatos", 124).unwrap();
        assert!(search_transcripts(&conn, "perros", 10).unwrap().is_empty());
        assert_eq!(search_transcripts(&conn, "gatos", 10).unwrap().len(), 1);
        assert_eq!(count_transcripts(&conn).unwrap(), 1);
    }

    #[test]
    fn video_meta_store_get_and_pending() {
        let mut conn = open_in_memory().unwrap();
        ingest_disks(&mut conn, &[sample_disk()]).unwrap();
        let mp4: i64 = conn.query_row("SELECT id FROM entries WHERE name='C0001.MP4'", [], |r| r.get(0)).unwrap();
        let disk_id: i64 = conn.query_row("SELECT id FROM disks LIMIT 1", [], |r| r.get(0)).unwrap();

        // mp4 figura como pendiente antes de indexar.
        let pending = video_entries_without_meta(&conn, disk_id, &["mp4", "mov"]).unwrap();
        assert!(pending.contains(&mp4));
        assert!(get_video_meta(&conn, mp4).unwrap().is_none());

        let m = VideoMetaRow {
            duration_ms: 4500,
            width: 3840,
            height: 2160,
            fps: 29.97,
            vcodec: Some("hevc".into()),
            acodec: Some("aac".into()),
            bitrate: 120_000_000,
        };
        store_video_meta(&conn, mp4, &m).unwrap();
        let got = get_video_meta(&conn, mp4).unwrap().unwrap();
        assert_eq!(got.width, 3840);
        assert_eq!(got.vcodec.as_deref(), Some("hevc"));
        assert!((got.fps - 29.97).abs() < 0.01);

        // Frames.
        replace_video_frames(&conn, mp4, &[(1000, vec![1, 2]), (2000, vec![3, 4])]).unwrap();
        assert_eq!(get_video_frames(&conn, mp4).unwrap().len(), 2);

        // Ya no figura pendiente.
        assert!(!video_entries_without_meta(&conn, disk_id, &["mp4", "mov"]).unwrap().contains(&mp4));
    }

    #[test]
    fn archive_index_store_list_and_pending() {
        use crate::archive::ArchiveItem;
        let mut conn = open_in_memory().unwrap();
        ingest_disks(&mut conn, &[sample_disk()]).unwrap();
        conn.execute(
            "INSERT INTO entries (disk_id, parent_id, name, is_folder, ext) \
             VALUES ((SELECT id FROM disks LIMIT 1), NULL, 'backup.zip', 0, 'zip')",
            [],
        )
        .unwrap();
        let zip: i64 = conn.query_row("SELECT id FROM entries WHERE name='backup.zip'", [], |r| r.get(0)).unwrap();
        let disk_id: i64 = conn.query_row("SELECT id FROM disks LIMIT 1", [], |r| r.get(0)).unwrap();

        let pending = archive_files_without_index(&conn, disk_id, &["zip", "7z", "rar"]).unwrap();
        assert_eq!(pending, vec![zip]);

        let items = vec![
            ArchiveItem { path: "fotos".into(), size: 0, modified: 0, is_dir: true },
            ArchiveItem { path: "fotos/a.jpg".into(), size: 2048, modified: 1700000000, is_dir: false },
            ArchiveItem { path: "leeme.txt".into(), size: 12, modified: 0, is_dir: false },
        ];
        store_archive_entries(&mut conn, zip, &items).unwrap();

        assert_eq!(archive_entry_count(&conn, zip).unwrap(), 3);
        let listed = list_archive_entries(&conn, zip).unwrap();
        // Carpetas primero.
        assert!(listed[0].is_dir);
        let jpg = listed.iter().find(|e| e.path == "fotos/a.jpg").unwrap();
        assert_eq!(jpg.name, "a.jpg");
        assert_eq!(jpg.size, 2048);

        // Re-indexar reemplaza (no duplica).
        store_archive_entries(&mut conn, zip, &items).unwrap();
        assert_eq!(archive_entry_count(&conn, zip).unwrap(), 3);
        // Ya no pendiente.
        assert!(archive_files_without_index(&conn, disk_id, &["zip"]).unwrap().is_empty());
    }

    #[test]
    fn delete_entry_removes_row_and_fts() {
        let mut conn = open_in_memory().unwrap();
        ingest_disks(&mut conn, &[sample_disk()]).unwrap();
        let id: i64 = conn.query_row("SELECT id FROM entries WHERE name='C0001.MP4'", [], |r| r.get(0)).unwrap();
        add_entry_tag(&conn, id, "borrar").unwrap();
        store_thumbnail(&conn, id, &[1, 2, 3], 4, 4).unwrap();

        delete_entry(&mut conn, id).unwrap();

        let rows: i64 = conn.query_row("SELECT COUNT(*) FROM entries WHERE id=?1", params![id], |r| r.get(0)).unwrap();
        assert_eq!(rows, 0);
        // No quedó en el FTS ni en derivadas.
        assert_eq!(search(&conn, "C0001", 10).unwrap().total, 0);
        let th: i64 = conn.query_row("SELECT COUNT(*) FROM thumbnails WHERE entry_id=?1", params![id], |r| r.get(0)).unwrap();
        assert_eq!(th, 0);
    }

    #[test]
    fn delete_subtree_removes_folder_and_descendants() {
        let mut conn = open_in_memory().unwrap();
        ingest_disks(&mut conn, &[sample_disk()]).unwrap();
        let clip: i64 = conn.query_row("SELECT id FROM entries WHERE name='CLIP'", [], |r| r.get(0)).unwrap();
        let file: i64 = conn.query_row("SELECT id FROM entries WHERE name='C0001.MP4'", [], |r| r.get(0)).unwrap();
        add_entry_tag(&conn, file, "boda").unwrap();
        store_thumbnail(&conn, file, &[1, 2, 3], 4, 4).unwrap();

        // Borrar la carpeta debe arrastrar a todos sus descendientes (y derivadas).
        delete_subtree(&mut conn, clip).unwrap();

        let remaining: i64 = conn.query_row("SELECT COUNT(*) FROM entries", [], |r| r.get(0)).unwrap();
        assert_eq!(remaining, 1, "solo debe quedar la raíz del disco");
        assert_eq!(search(&conn, "C0001", 10).unwrap().total, 0, "el FTS no debe retener al hijo");
        let th: i64 = conn.query_row("SELECT COUNT(*) FROM thumbnails", [], |r| r.get(0)).unwrap();
        let tg: i64 = conn.query_row("SELECT COUNT(*) FROM entry_tags", [], |r| r.get(0)).unwrap();
        assert_eq!(th, 0);
        assert_eq!(tg, 0);
    }

    #[test]
    fn rescan_clears_thumbnails_and_tags() {
        let mut conn = open_in_memory().unwrap();
        ingest_scanned(&mut conn, &sample_disk(), Some("UUID-1"), "ssd", None, "/Volumes/SF28", None).unwrap();
        let id: i64 = conn.query_row("SELECT id FROM entries WHERE name='C0001.MP4'", [], |r| r.get(0)).unwrap();
        add_entry_tag(&conn, id, "boda").unwrap();
        store_thumbnail(&conn, id, &[9, 9, 9], 10, 10).unwrap();

        // Re-escaneo del mismo fingerprint: limpia thumbnails y vínculos huérfanos.
        ingest_scanned(&mut conn, &sample_disk(), Some("UUID-1"), "ssd", None, "/Volumes/SF28", None).unwrap();
        let thumbs: i64 = conn.query_row("SELECT COUNT(*) FROM thumbnails", [], |r| r.get(0)).unwrap();
        let links: i64 = conn.query_row("SELECT COUNT(*) FROM entry_tags", [], |r| r.get(0)).unwrap();
        assert_eq!(thumbs, 0);
        assert_eq!(links, 0);
    }

    #[test]
    fn advanced_empty_filters_returns_nothing() {
        let mut conn = open_in_memory().unwrap();
        ingest_disks(&mut conn, &[sample_disk()]).unwrap();
        let r = search_advanced(&conn, &SearchFilters::default(), 100).unwrap();
        assert_eq!(r.total, 0);
    }

    #[test]
    fn rescan_replaces_same_fingerprint() {
        let mut conn = open_in_memory().unwrap();
        ingest_scanned(&mut conn, &sample_disk(), Some("UUID-1"), "ssd", None, "/Volumes/SF28", None).unwrap();
        // Segundo escaneo del mismo disco (mismo UUID) no debe duplicar.
        let r2 = ingest_scanned(&mut conn, &sample_disk(), Some("UUID-1"), "ssd", None, "/Volumes/SF28", None).unwrap();
        assert!(r2.replaced);
        let disk_count: i64 = conn.query_row("SELECT COUNT(*) FROM disks", [], |r| r.get(0)).unwrap();
        assert_eq!(disk_count, 1);
        // El FTS no quedó con fantasmas del disco viejo.
        let res = search(&conn, "C0001", 10).unwrap();
        assert_eq!(res.total, 1);
    }

    #[test]
    fn rescan_without_fingerprint_dedupes_by_name() {
        // Discos exFAT/NTFS sin Volume UUID: el re-escaneo debe reemplazar por
        // nombre (entre los discos sin fingerprint), no acumular duplicados.
        let mut conn = open_in_memory().unwrap();
        ingest_scanned(&mut conn, &sample_disk(), None, "hdd", None, "/Volumes/SF41", None).unwrap();
        let r2 = ingest_scanned(&mut conn, &sample_disk(), None, "hdd", None, "/Volumes/SF41", None).unwrap();
        assert!(r2.replaced);
        let same_name: i64 = conn
            .query_row("SELECT COUNT(*) FROM disks WHERE name = ?1", params![sample_disk().name], |r| r.get(0))
            .unwrap();
        assert_eq!(same_name, 1);
        // Un disco con fingerprint y mismo nombre no debe ser tocado por el
        // re-escaneo sin fingerprint (identidad por UUID tiene prioridad).
        ingest_scanned(&mut conn, &sample_disk(), Some("UUID-X"), "ssd", None, "/Volumes/SF41", None).unwrap();
        ingest_scanned(&mut conn, &sample_disk(), None, "hdd", None, "/Volumes/SF41", None).unwrap();
        let total: i64 = conn.query_row("SELECT COUNT(*) FROM disks", [], |r| r.get(0)).unwrap();
        assert_eq!(total, 2); // uno con UUID-X + uno sin fingerprint
    }

    #[test]
    fn migrations_add_columns_and_are_idempotent() {
        // Simula un catálogo "viejo": tablas mínimas sin las columnas nuevas,
        // tal como las crearía una versión anterior de la app.
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE disks (id INTEGER PRIMARY KEY, name TEXT NOT NULL);
             CREATE TABLE entries (id INTEGER PRIMARY KEY, disk_id INTEGER NOT NULL,
                                   name TEXT NOT NULL, is_folder INTEGER NOT NULL);",
        )
        .unwrap();

        // Primera migración: agrega todas las columnas + índices.
        apply_migrations(&conn).unwrap();
        // Segunda corrida sobre el MISMO catálogo: no debe fallar (idempotente).
        apply_migrations(&conn).unwrap();

        // Verifica que las columnas nuevas existan en entries.
        let entry_cols: Vec<String> = conn
            .prepare("SELECT name FROM pragma_table_info('entries')")
            .unwrap()
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        for c in [
            "content_hash", "hashed_at", "cloud_state", "gps_lat", "gps_lon", "gps_place",
            "captured_at", "camera_make", "camera_model",
        ] {
            assert!(entry_cols.contains(&c.to_string()), "falta columna entries.{c}");
        }

        // Y en disks.
        let disk_cols: Vec<String> = conn
            .prepare("SELECT name FROM pragma_table_info('disks')")
            .unwrap()
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert!(disk_cols.contains(&"cloud_provider".to_string()));
        assert!(disk_cols.contains(&"cloud_root".to_string()));

        // Índices creados.
        let idx_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='index'
                 AND name IN ('idx_entries_hash','idx_entries_place')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(idx_count, 2);
    }

    #[test]
    fn ingest_persists_enrichment_hashes() {
        let mut conn = open_in_memory().unwrap();
        let disk = sample_disk();
        // Enriquecimiento alineado por índice: hash sólo para el archivo C0001.MP4 (índice 2).
        let mut enr = vec![EntryEnrichment::default(); disk.entries.len()];
        enr[2].content_hash = Some("deadbeef".into());
        enr[2].gps_place = Some("Jujuy, Argentina".into());
        ingest_scanned(&mut conn, &disk, Some("UUID-1"), "ssd", None, "/Volumes/SF28", Some(&enr))
            .unwrap();

        let (hash, place, hashed_at): (Option<String>, Option<String>, Option<i64>) = conn
            .query_row(
                "SELECT content_hash, gps_place, hashed_at FROM entries WHERE name = 'C0001.MP4'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(hash.as_deref(), Some("deadbeef"));
        assert_eq!(place.as_deref(), Some("Jujuy, Argentina"));
        assert!(hashed_at.is_some(), "hashed_at se setea cuando hay hash");

        // Una entrada sin enriquecimiento queda con hash NULL (no rompe).
        let other: Option<String> = conn
            .query_row("SELECT content_hash FROM entries WHERE name = 'B-ROLL.MOV'", [], |r| r.get(0))
            .unwrap();
        assert!(other.is_none());

        // Sin enrichment (None) tampoco rompe y deja todo NULL.
        ingest_scanned(&mut conn, &sample_disk(), Some("UUID-2"), "ssd", None, "/Volumes/SF99", None)
            .unwrap();
    }

    #[test]
    fn compare_subtrees_classifies_files() {
        let mut conn = open_in_memory().unwrap();
        let folder = |name: &str, parent: i32| DcmfEntry {
            name: name.into(), parent, is_folder: true, is_volume: parent < 0,
            size_logical: 0, size_physical: 0, created: 0, modified: 0,
        };
        let file = |name: &str, parent: i32, size: u64| DcmfEntry {
            name: name.into(), parent, is_folder: false, is_volume: false,
            size_logical: size, size_physical: size, created: 0, modified: 0,
        };

        // SOURCE: DCIM/{A.mov(hashA), B.mov(hashB), C.mov(sin hash, 50B)}
        let src = DcmfDisk {
            name: "SRC".into(),
            entries: vec![
                folder("SRC", -1), folder("DCIM", 0),
                file("A.mov", 1, 100), file("B.mov", 1, 200), file("C.mov", 1, 50),
            ],
        };
        let mut src_enr = vec![EntryEnrichment::default(); src.entries.len()];
        src_enr[2].content_hash = Some("hashA".into());
        src_enr[3].content_hash = Some("hashB".into());
        // C.mov queda sin hash a propósito.
        ingest_scanned(&mut conn, &src, Some("U-SRC"), "ssd", None, "/Volumes/SRC", Some(&src_enr)).unwrap();

        // DEST: DCIM/{A.mov(hash distinto → mismatch), C.mov(sin hash, mismo 50B → unverified)}
        //       B.mov ausente → missing.
        let dst = DcmfDisk {
            name: "DST".into(),
            entries: vec![
                folder("DST", -1), folder("DCIM", 0),
                file("A.mov", 1, 100), file("C.mov", 1, 50),
            ],
        };
        let mut dst_enr = vec![EntryEnrichment::default(); dst.entries.len()];
        dst_enr[2].content_hash = Some("hashA-DISTINTO".into());
        ingest_scanned(&mut conn, &dst, Some("U-DST"), "ssd", None, "/Volumes/DST", Some(&dst_enr)).unwrap();

        let src_disk: i64 = conn.query_row("SELECT id FROM disks WHERE name='SRC'", [], |r| r.get(0)).unwrap();
        let dst_disk: i64 = conn.query_row("SELECT id FROM disks WHERE name='DST'", [], |r| r.get(0)).unwrap();
        let src_root = disk_root_entry(&conn, src_disk).unwrap().unwrap();
        let dst_root = disk_root_entry(&conn, dst_disk).unwrap().unwrap();

        let rep = compare_subtrees(&conn, src_root, dst_root).unwrap();
        assert_eq!(rep.source_total, 3);
        assert_eq!(rep.ok, 0);
        assert_eq!(rep.missing.len(), 1, "B.mov falta");
        assert_eq!(rep.missing[0].name, "B.mov");
        assert_eq!(rep.missing[0].rel_path, "DCIM/B.mov");
        assert_eq!(rep.missing_bytes, 200);
        assert_eq!(rep.mismatch.len(), 1, "A.mov difiere por hash");
        assert_eq!(rep.mismatch[0].name, "A.mov");
        assert_eq!(rep.unverified.len(), 1, "C.mov presente, mismo tamaño, sin hash");
        assert_eq!(rep.unverified[0].name, "C.mov");
        assert_eq!(rep.extra, 0);
        assert!(!rep.fully_backed_up);

        // Comparar el source contra sí mismo → A y B verificados (OK), C sin hash queda unverified.
        let same = compare_subtrees(&conn, src_root, src_root).unwrap();
        assert_eq!(same.ok, 2);
        assert_eq!(same.unverified.len(), 1);
        assert!(same.missing.is_empty() && same.mismatch.is_empty());
        assert!(same.fully_backed_up);
    }

    #[test]
    fn rescan_preserves_hash_when_unchanged() {
        let mut conn = open_in_memory().unwrap();
        let disk = sample_disk(); // C0001.MP4 tiene size + mtime reales (índice 2)
        let mut enr = vec![EntryEnrichment::default(); disk.entries.len()];
        enr[2].content_hash = Some("HASH-C0001".into());
        ingest_scanned(&mut conn, &disk, Some("UUID-1"), "ssd", None, "/Volumes/SF28", Some(&enr)).unwrap();

        // Re-escaneo SIN enrich (None): el hash debe PRESERVARSE (archivo sin cambios).
        ingest_scanned(&mut conn, &sample_disk(), Some("UUID-1"), "ssd", None, "/Volumes/SF28", None).unwrap();
        let h: Option<String> = conn
            .query_row("SELECT content_hash FROM entries WHERE name='C0001.MP4'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(h.as_deref(), Some("HASH-C0001"), "el hash debe sobrevivir a un re-escaneo sin enrich");

        // Re-escaneo con el MISMO archivo pero distinto TAMAÑO → NO se preserva (cambió).
        let mut changed = sample_disk();
        changed.entries[2].size_logical = 999;
        ingest_scanned(&mut conn, &changed, Some("UUID-1"), "ssd", None, "/Volumes/SF28", None).unwrap();
        let h2: Option<String> = conn
            .query_row("SELECT content_hash FROM entries WHERE name='C0001.MP4'", [], |r| r.get(0))
            .unwrap();
        assert!(h2.is_none(), "un cambio de tamaño debe descartar el hash viejo");
    }

    #[test]
    fn search_by_place_filters_on_gps_place() {
        let mut conn = open_in_memory().unwrap();
        let disk = sample_disk(); // C0001.MP4 (idx 2), B-ROLL.MOV (idx 3)
        let mut enr = vec![EntryEnrichment::default(); disk.entries.len()];
        enr[2].gps_place = Some("San Salvador de Jujuy, Jujuy, AR".into());
        enr[3].gps_place = Some("Buenos Aires, Buenos Aires F.D., AR".into());
        ingest_scanned(&mut conn, &disk, Some("U"), "ssd", None, "/Volumes/SF28", Some(&enr)).unwrap();

        let f = SearchFilters { place: Some("Jujuy".into()), ..Default::default() };
        let res = search_advanced(&conn, &f, 50).unwrap();
        assert_eq!(res.total, 1);
        assert_eq!(res.items[0].name, "C0001.MP4");

        // Combinado con un filtro de tipo: sigue matcheando.
        let f2 = SearchFilters { place: Some("AR".into()), kind: Some("file".into()), ..Default::default() };
        assert_eq!(search_advanced(&conn, &f2, 50).unwrap().total, 2);
    }

    #[test]
    fn search_by_light_filters_on_light_phase() {
        let mut conn = open_in_memory().unwrap();
        let disk = sample_disk();
        let mut enr = vec![EntryEnrichment::default(); disk.entries.len()];
        enr[2].light_phase = Some("golden dusk sunset".into()); // C0001.MP4 = atardecer
        enr[3].light_phase = Some("day".into()); // B-ROLL.MOV = día
        ingest_scanned(&mut conn, &disk, Some("U"), "ssd", None, "/Volumes/SF28", Some(&enr)).unwrap();

        let f = SearchFilters { light: Some("sunset".into()), ..Default::default() };
        let res = search_advanced(&conn, &f, 50).unwrap();
        assert_eq!(res.total, 1);
        assert_eq!(res.items[0].name, "C0001.MP4");
    }
}
