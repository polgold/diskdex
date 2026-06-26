//! Capa de base de datos: un archivo SQLite (`.dccat`) por catálogo, con FTS5
//! para búsqueda full-text instantánea por nombre (sección 4).
//!
//! Ingesta masiva optimizada: inserts en una sola transacción por disco con
//! statement preparado, y reconstrucción del índice FTS al final (mucho más
//! rápido que mantenerlo por triggers durante una carga de millones de filas).

use crate::dcmf::DcmfDisk;
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
    Ok(conn)
}

/// Abre un catálogo en memoria (para tests).
pub fn open_in_memory() -> DbResult<Connection> {
    let conn = Connection::open_in_memory()?;
    conn.execute_batch(SCHEMA)?;
    Ok(conn)
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

/// Ingesta un disco escaneado (sección 7). Si ya existe un disco con el mismo
/// `volume_uuid`, lo reemplaza (re-escaneo). Mantiene el FTS de forma incremental
/// (sin reconstruir todo el índice) para no penalizar catálogos grandes.
pub fn ingest_scanned(
    conn: &mut Connection,
    disk: &DcmfDisk,
    volume_uuid: Option<&str>,
    kind: &str,
    capacity: Option<i64>,
    mount_path: &str,
) -> DbResult<ScanIngest> {
    let (agg_log, agg_phys) = aggregate_sizes(disk);
    let file_count = disk.entries.iter().filter(|e| !e.is_folder).count() as i64;
    let folder_count = disk.entries.iter().filter(|e| e.is_folder).count() as i64;
    let total_size = agg_log.first().copied().unwrap_or(0) as i64;

    let tx = conn.transaction()?;
    let mut replaced = false;

    // Re-escaneo: eliminar el disco previo con el mismo fingerprint (y su FTS).
    if let Some(uuid) = volume_uuid {
        let old_ids: Vec<i64> = {
            let mut stmt = tx.prepare("SELECT id FROM disks WHERE volume_uuid = ?1")?;
            let ids = stmt.query_map(params![uuid], |r| r.get::<_, i64>(0))?;
            ids.collect::<Result<_, _>>()?
        };
        for old in old_ids {
            // Borrado del FTS externo: comando 'delete' fila por fila vía SELECT.
            tx.execute(
                "INSERT INTO entries_fts(entries_fts, rowid, name) \
                 SELECT 'delete', id, name FROM entries WHERE disk_id = ?1",
                params![old],
            )?;
            // Limpiar thumbnails y vínculos de tags de las entradas que se van.
            tx.execute(
                "DELETE FROM thumbnails WHERE entry_id IN (SELECT id FROM entries WHERE disk_id = ?1)",
                params![old],
            )?;
            tx.execute(
                "DELETE FROM entry_tags WHERE entry_id IN (SELECT id FROM entries WHERE disk_id = ?1)",
                params![old],
            )?;
            tx.execute("DELETE FROM entries WHERE disk_id = ?1", params![old])?;
            tx.execute("DELETE FROM disks WHERE id = ?1", params![old])?;
            replaced = true;
        }
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
     (SELECT COUNT(*) FROM entries c WHERE c.parent_id = e.id) AS child_count";

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

/// Lista los hijos directos de `parent_id` en un disco. Si `parent_id` es `None`,
/// devuelve la raíz del disco (el nodo volumen, con `parent_id IS NULL`).
/// Carpetas primero, luego por nombre (orden tipo Finder).
pub fn list_children(
    conn: &Connection,
    disk_id: i64,
    parent_id: Option<i64>,
) -> DbResult<Vec<EntryRow>> {
    let sql = format!(
        "SELECT {ENTRY_COLS} FROM entries e \
         WHERE e.disk_id = ?1 AND e.parent_id IS ?2 \
         ORDER BY e.is_folder DESC, e.name COLLATE NOCASE ASC"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params![disk_id, parent_id], row_to_entry)?;
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

    let file_count: i64 = {
        let sql = format!("SELECT COUNT(*) FROM entries e WHERE e.is_folder = 0{scope}");
        if has_scope {
            conn.query_row(&sql, params![disk_id.unwrap()], |r| r.get(0))?
        } else {
            conn.query_row(&sql, [], |r| r.get(0))?
        }
    };
    let folder_count: i64 = {
        let sql = format!("SELECT COUNT(*) FROM entries e WHERE e.is_folder = 1{scope}");
        if has_scope {
            conn.query_row(&sql, params![disk_id.unwrap()], |r| r.get(0))?
        } else {
            conn.query_row(&sql, [], |r| r.get(0))?
        }
    };
    let total_size: i64 = {
        let sql = format!("SELECT COALESCE(SUM(e.size_logical),0) FROM entries e WHERE e.is_folder = 0{scope}");
        if has_scope {
            conn.query_row(&sql, params![disk_id.unwrap()], |r| r.get(0))?
        } else {
            conn.query_row(&sql, [], |r| r.get(0))?
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

        // Raíz del disco = el nodo volumen.
        let root = list_children(&conn, disk_id, None).unwrap();
        assert_eq!(root.len(), 1);
        assert_eq!(root[0].name, "SF28");
        assert!(root[0].is_folder);
        assert_eq!(root[0].child_count, 1); // contiene CLIP

        // Hijos del volumen: CLIP.
        let lvl1 = list_children(&conn, disk_id, Some(root[0].id)).unwrap();
        assert_eq!(lvl1.len(), 1);
        assert_eq!(lvl1[0].name, "CLIP");
        assert_eq!(lvl1[0].child_count, 2);

        // Hijos de CLIP: dos archivos, ordenados por nombre.
        let lvl2 = list_children(&conn, disk_id, Some(lvl1[0].id)).unwrap();
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
        let r = ingest_scanned(&mut conn, &sample_disk(), Some("UUID-1"), "ssd", Some(1000), "/Volumes/SF28").unwrap();
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
    fn rescan_clears_thumbnails_and_tags() {
        let mut conn = open_in_memory().unwrap();
        ingest_scanned(&mut conn, &sample_disk(), Some("UUID-1"), "ssd", None, "/Volumes/SF28").unwrap();
        let id: i64 = conn.query_row("SELECT id FROM entries WHERE name='C0001.MP4'", [], |r| r.get(0)).unwrap();
        add_entry_tag(&conn, id, "boda").unwrap();
        store_thumbnail(&conn, id, &[9, 9, 9], 10, 10).unwrap();

        // Re-escaneo del mismo fingerprint: limpia thumbnails y vínculos huérfanos.
        ingest_scanned(&mut conn, &sample_disk(), Some("UUID-1"), "ssd", None, "/Volumes/SF28").unwrap();
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
        ingest_scanned(&mut conn, &sample_disk(), Some("UUID-1"), "ssd", None, "/Volumes/SF28").unwrap();
        // Segundo escaneo del mismo disco (mismo UUID) no debe duplicar.
        let r2 = ingest_scanned(&mut conn, &sample_disk(), Some("UUID-1"), "ssd", None, "/Volumes/SF28").unwrap();
        assert!(r2.replaced);
        let disk_count: i64 = conn.query_row("SELECT COUNT(*) FROM disks", [], |r| r.get(0)).unwrap();
        assert_eq!(disk_count, 1);
        // El FTS no quedó con fantasmas del disco viejo.
        let res = search(&conn, "C0001", 10).unwrap();
        assert_eq!(res.total, 1);
    }
}
