//! Comandos Tauri expuestos a la UI. Aíslan toda la lógica de FS/parsing en el
//! lado nativo; el frontend solo consume datos ya indexados (sección 2).

use crate::agent::{self, AgentConfig};
use crate::archive;
use crate::db;
use crate::dcmf;
use crate::scan::{self, ScanOptions, VolumeInfo};
use crate::video;
use rusqlite::Connection;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::Emitter;

/// Estado global: el catálogo SQLite actualmente abierto.
#[derive(Default)]
pub struct AppState {
    pub catalog: Mutex<Option<Catalog>>,
    /// Evita arrancar el watcher de volúmenes más de una vez.
    pub watch_started: AtomicBool,
    /// Conector remoto seguro (M9), si está activo.
    pub agent: Mutex<Option<agent::AgentHandle>>,
}

pub struct Catalog {
    /// Ruta del `.dccat` abierto (se usará al reabrir/exportar — M7).
    #[allow(dead_code)]
    pub path: PathBuf,
    pub conn: Connection,
}

#[derive(Serialize)]
pub struct ImportSummary {
    pub catalog_path: String,
    pub disks: usize,
    pub entries: u64,
    pub elapsed_ms: u128,
}

#[derive(Serialize)]
pub struct DiskRow {
    pub id: i64,
    pub name: String,
    pub total_size: i64,
    pub file_count: i64,
    pub folder_count: i64,
    pub is_online: bool,
    pub location: Option<String>,
    pub category: Option<String>,
    pub comment: Option<String>,
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

/// M0: sanity check de la IPC.
#[tauri::command]
pub fn ping() -> String {
    "pong".into()
}

// ---------- Progreso (eventos a la UI) ----------

#[derive(Clone, Serialize)]
struct ScanProgress {
    count: u64,
}

/// Avance de una fase de post-procesamiento (miniaturas/videos/archivos).
#[derive(Clone, Serialize)]
struct IndexProgress {
    phase: &'static str,
    done: i64,
    total: i64,
}

fn emit_index(app: &tauri::AppHandle, phase: &'static str, done: i64, total: i64) {
    let _ = app.emit("index-progress", IndexProgress { phase, done, total });
}

// ---------- M9: conector remoto seguro ----------

#[derive(Serialize)]
pub struct AgentStatus {
    pub running: bool,
    pub addr: Option<String>,
}

#[derive(Serialize)]
pub struct DeviceRow {
    pub id: String,
    pub name: String,
    pub scopes: String,
    pub created_at: i64,
    pub last_seen: i64,
    pub revoked: bool,
}

/// Arranca el agente sobre el catálogo abierto. `bind` por defecto loopback;
/// para una malla, pasar la IP de la interfaz (Tailscale/WireGuard).
#[tauri::command]
pub fn agent_start(
    state: tauri::State<'_, AppState>,
    bind: Option<String>,
    scopes: Option<String>,
) -> Result<AgentStatus, String> {
    let catalog_path = {
        let guard = state.catalog.lock().unwrap();
        let cat = guard.as_ref().ok_or("abrí un catálogo antes de compartirlo")?;
        cat.path.clone()
    };
    let mut agent_guard = state.agent.lock().unwrap();
    if agent_guard.is_some() {
        return Err("el conector ya está activo".into());
    }
    let mut config = AgentConfig::default();
    if let Some(b) = bind {
        config.bind = b;
    }
    if let Some(s) = scopes {
        config.default_scopes = s;
    }
    let handle = agent::start(catalog_path, config)?;
    let addr = handle.addr.clone();
    *agent_guard = Some(handle);
    Ok(AgentStatus { running: true, addr: Some(addr) })
}

/// Detiene el agente.
#[tauri::command]
pub fn agent_stop(state: tauri::State<'_, AppState>) -> Result<(), String> {
    if let Some(handle) = state.agent.lock().unwrap().take() {
        handle.stop();
    }
    Ok(())
}

#[tauri::command]
pub fn agent_status(state: tauri::State<'_, AppState>) -> AgentStatus {
    let guard = state.agent.lock().unwrap();
    match guard.as_ref() {
        Some(h) => AgentStatus { running: true, addr: Some(h.addr.clone()) },
        None => AgentStatus { running: false, addr: None },
    }
}

/// Genera un código de emparejamiento (válido 5 min) para enrolar un dispositivo.
#[tauri::command]
pub fn agent_pair_code(state: tauri::State<'_, AppState>) -> Result<String, String> {
    let guard = state.agent.lock().unwrap();
    let h = guard.as_ref().ok_or("el conector no está activo")?;
    Ok(h.new_pairing_code())
}

/// Lista los dispositivos enrolados.
#[tauri::command]
pub fn agent_devices(state: tauri::State<'_, AppState>) -> Result<Vec<DeviceRow>, String> {
    let guard = state.catalog.lock().unwrap();
    let cat = guard.as_ref().ok_or("no hay catálogo abierto")?;
    let mut stmt = cat
        .conn
        .prepare("SELECT id, name, scopes, created_at, last_seen, revoked FROM devices ORDER BY created_at DESC")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok(DeviceRow {
                id: r.get(0)?,
                name: r.get(1)?,
                scopes: r.get(2)?,
                created_at: r.get(3)?,
                last_seen: r.get(4)?,
                revoked: r.get::<_, i64>(5)? != 0,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

/// Revoca (o re-habilita) un dispositivo.
#[tauri::command]
pub fn agent_revoke(
    state: tauri::State<'_, AppState>,
    device_id: String,
    revoked: bool,
) -> Result<(), String> {
    let guard = state.catalog.lock().unwrap();
    let cat = guard.as_ref().ok_or("no hay catálogo abierto")?;
    cat.conn
        .execute(
            "UPDATE devices SET revoked = ?1 WHERE id = ?2",
            rusqlite::params![revoked as i64, device_id],
        )
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// M1: importa un archivo `.dcmf` a un catálogo `.dccat` (lo crea/abre) y deja
/// el catálogo abierto en el estado. Devuelve un resumen.
#[tauri::command(async)]
pub fn import_dcmf(
    state: tauri::State<'_, AppState>,
    dcmf_path: String,
    catalog_path: String,
) -> Result<ImportSummary, String> {
    let t0 = now_ms();
    let bytes = std::fs::read(&dcmf_path).map_err(|e| format!("no se pudo leer {dcmf_path}: {e}"))?;
    let disks = dcmf::import_dcmf(&bytes);
    if disks.is_empty() {
        return Err("el archivo .dcmf no contiene discos reconocibles".into());
    }

    let cat_path = PathBuf::from(&catalog_path);
    let mut conn = db::open(&cat_path).map_err(|e| format!("error abriendo catálogo: {e}"))?;
    let entries = db::ingest_disks(&mut conn, &disks).map_err(|e| format!("error en ingesta: {e}"))?;
    let disk_count = disks.len();

    *state.catalog.lock().unwrap() = Some(Catalog { path: cat_path, conn });

    Ok(ImportSummary {
        catalog_path,
        disks: disk_count,
        entries,
        elapsed_ms: now_ms().saturating_sub(t0),
    })
}

/// Abre un catálogo `.dccat` existente.
#[tauri::command]
pub fn open_catalog(state: tauri::State<'_, AppState>, catalog_path: String) -> Result<(), String> {
    let cat_path = PathBuf::from(&catalog_path);
    let conn = db::open(&cat_path).map_err(|e| format!("error abriendo catálogo: {e}"))?;
    *state.catalog.lock().unwrap() = Some(Catalog { path: cat_path, conn });
    Ok(())
}

/// M2: hijos directos de un nodo (raíz del disco si `parent_id` es None).
#[tauri::command]
pub fn list_children(
    state: tauri::State<'_, AppState>,
    disk_id: i64,
    parent_id: Option<i64>,
) -> Result<Vec<db::EntryRow>, String> {
    let guard = state.catalog.lock().unwrap();
    let cat = guard.as_ref().ok_or("no hay catálogo abierto")?;
    db::list_children(&cat.conn, disk_id, parent_id).map_err(|e| e.to_string())
}

/// M2: ruta completa de una entrada.
#[tauri::command]
pub fn entry_path(state: tauri::State<'_, AppState>, entry_id: i64) -> Result<String, String> {
    let guard = state.catalog.lock().unwrap();
    let cat = guard.as_ref().ok_or("no hay catálogo abierto")?;
    db::entry_path(&cat.conn, entry_id).map_err(|e| e.to_string())
}

/// M2: una entrada por id (para el inspector).
#[tauri::command]
pub fn get_entry(
    state: tauri::State<'_, AppState>,
    entry_id: i64,
) -> Result<Option<db::EntryRow>, String> {
    let guard = state.catalog.lock().unwrap();
    let cat = guard.as_ref().ok_or("no hay catálogo abierto")?;
    db::get_entry(&cat.conn, entry_id).map_err(|e| e.to_string())
}

/// M7: edita el comentario de una entrada.
#[tauri::command]
pub fn set_entry_comment(
    state: tauri::State<'_, AppState>,
    entry_id: i64,
    comment: Option<String>,
) -> Result<(), String> {
    let guard = state.catalog.lock().unwrap();
    let cat = guard.as_ref().ok_or("no hay catálogo abierto")?;
    db::set_entry_comment(&cat.conn, entry_id, comment.as_deref()).map_err(|e| e.to_string())
}

/// M7: edita ubicación / categoría / comentario de un disco.
#[tauri::command]
pub fn set_disk_meta(
    state: tauri::State<'_, AppState>,
    disk_id: i64,
    location: Option<String>,
    category: Option<String>,
    comment: Option<String>,
) -> Result<(), String> {
    let guard = state.catalog.lock().unwrap();
    let cat = guard.as_ref().ok_or("no hay catálogo abierto")?;
    db::set_disk_meta(
        &cat.conn,
        disk_id,
        location.as_deref(),
        category.as_deref(),
        comment.as_deref(),
    )
    .map_err(|e| e.to_string())
}

/// M8: estadísticas del catálogo (o de un disco si se pasa `disk_id`).
#[tauri::command]
pub fn catalog_stats(
    state: tauri::State<'_, AppState>,
    disk_id: Option<i64>,
) -> Result<db::Stats, String> {
    let guard = state.catalog.lock().unwrap();
    let cat = guard.as_ref().ok_or("no hay catálogo abierto")?;
    db::stats(&cat.conn, disk_id).map_err(|e| e.to_string())
}

/// M8: archivos duplicados (por nombre+tamaño), ordenados por espacio desperdiciado.
#[tauri::command]
pub fn find_duplicates(
    state: tauri::State<'_, AppState>,
    min_size: Option<i64>,
    limit: Option<i64>,
) -> Result<Vec<db::DupGroup>, String> {
    let guard = state.catalog.lock().unwrap();
    let cat = guard.as_ref().ok_or("no hay catálogo abierto")?;
    db::duplicates(&cat.conn, min_size.unwrap_or(1_048_576), limit.unwrap_or(500))
        .map_err(|e| e.to_string())
}

/// M7: escribe un archivo de texto (export CSV/TSV/JSON/HTML generado por la UI).
#[tauri::command]
pub fn write_text_file(path: String, contents: String) -> Result<(), String> {
    std::fs::write(&path, contents).map_err(|e| format!("no se pudo escribir {path}: {e}"))
}

/// M4: búsqueda por atributos / booleana (ext, tamaño, fecha, tipo, disco).
#[tauri::command]
pub fn search_advanced(
    state: tauri::State<'_, AppState>,
    filters: db::SearchFilters,
    limit: Option<i64>,
) -> Result<db::SearchResult, String> {
    let guard = state.catalog.lock().unwrap();
    let cat = guard.as_ref().ok_or("no hay catálogo abierto")?;
    db::search_advanced(&cat.conn, &filters, limit.unwrap_or(2000)).map_err(|e| e.to_string())
}

/// Resuelve la ruta real en disco de una entrada (si su volumen está montado).
/// Helper compartido por `resolve_fs_path` (M6) y `get_thumbnail` (P2).
fn resolve_real_path(conn: &Connection, entry_id: i64) -> Result<PathBuf, String> {
    let disk_id: i64 = conn
        .query_row("SELECT disk_id FROM entries WHERE id = ?1", [entry_id], |r| r.get(0))
        .map_err(|_| "la entrada no existe".to_string())?;
    let (mount_path, is_online, dname): (Option<String>, i64, String) = conn
        .query_row(
            "SELECT mount_path, is_online, name FROM disks WHERE id = ?1",
            [disk_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .map_err(|e| e.to_string())?;

    let mount = match (is_online, mount_path) {
        (1, Some(m)) => m,
        _ => {
            return Err(format!(
                "El disco «{dname}» está offline. Conectá el volumen y actualizá el estado."
            ))
        }
    };

    let cat_path = db::entry_path(conn, entry_id).map_err(|e| e.to_string())?;
    let rel: PathBuf = cat_path.split('/').filter(|s| !s.is_empty()).skip(1).collect();
    let real = std::path::Path::new(&mount).join(&rel);
    if !real.exists() {
        return Err(format!("No se encontró el original en {}. ¿Es el disco correcto?", real.display()));
    }
    Ok(real)
}

/// M6: resuelve la ruta real en el filesystem de una entrada, si su disco está montado.
#[tauri::command]
pub fn resolve_fs_path(state: tauri::State<'_, AppState>, entry_id: i64) -> Result<String, String> {
    let guard = state.catalog.lock().unwrap();
    let cat = guard.as_ref().ok_or("no hay catálogo abierto")?;
    Ok(resolve_real_path(&cat.conn, entry_id)?.to_string_lossy().to_string())
}

/// Extensiones de imagen estándar (decodificadas por el crate `image`).
const THUMB_EXTS: &[&str] = &["jpg", "jpeg", "png", "gif", "webp", "bmp", "tif", "tiff"];
/// RAW de cámara — en macOS se decodifican con `sips` (ImageIO de Apple).
const RAW_EXTS: &[&str] = &[
    "dng", "arw", "cr2", "cr3", "crw", "nef", "nrw", "raf", "orf", "rw2", "pef", "srw", "3fr",
    "iiq", "dcr", "mrw", "mos", "erf", "rwl",
];
/// Lado máximo del thumbnail cacheado. Compacto pero nítido en el inspector/grilla.
const THUMB_CACHE_MAX: u32 = 320;

/// Lista combinada (imagen estándar + RAW) para buscar pendientes de thumbnail.
fn thumb_exts() -> Vec<&'static str> {
    THUMB_EXTS.iter().chain(RAW_EXTS.iter()).copied().collect()
}

/// Renderiza un PNG de thumbnail desde una ruta real. Devuelve (bytes, w, h).
/// Despacha RAW de cámara a `sips` (macOS) y el resto al crate `image`.
fn render_image_thumb(path: &std::path::Path, max: u32) -> Result<(Vec<u8>, u32, u32), String> {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    if RAW_EXTS.contains(&ext.as_str()) {
        render_raw_thumbnail(path, max)
    } else {
        render_thumbnail_png(path, max)
    }
}

fn render_thumbnail_png(path: &std::path::Path, max: u32) -> Result<(Vec<u8>, u32, u32), String> {
    let img = image::open(path)
        .map_err(|e| format!("no se pudo generar preview (formato no soportado): {e}"))?;
    let thumb = img.thumbnail(max, max);
    let (w, h) = (thumb.width(), thumb.height());
    let mut buf = std::io::Cursor::new(Vec::new());
    thumb
        .write_to(&mut buf, image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok((buf.into_inner(), w, h))
}

/// Decodifica un RAW de cámara a PNG usando `sips` (ImageIO de macOS), que
/// soporta ARW/DNG/CR2/CR3/NEF/RAF/ORF/RW2/etc. de fábrica.
#[cfg(target_os = "macos")]
fn render_raw_thumbnail(path: &std::path::Path, max: u32) -> Result<(Vec<u8>, u32, u32), String> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let tmp = std::env::temp_dir().join(format!("diskdex_raw_{}_{}.png", std::process::id(), n));
    let out = std::process::Command::new("sips")
        .args(["-Z", &max.to_string(), "-s", "format", "png"])
        .arg(path)
        .arg("--out")
        .arg(&tmp)
        .output()
        .map_err(|e| format!("no se pudo ejecutar sips: {e}"))?;
    if !out.status.success() {
        let _ = std::fs::remove_file(&tmp);
        return Err("sips no pudo decodificar el RAW".into());
    }
    let bytes = std::fs::read(&tmp).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(&tmp);
    Ok((bytes, 0, 0))
}

#[cfg(not(target_os = "macos"))]
fn render_raw_thumbnail(_path: &std::path::Path, _max: u32) -> Result<(Vec<u8>, u32, u32), String> {
    Err("preview de RAW no disponible en esta plataforma".into())
}

/// Thumbnail/preview de una imagen. Primero busca en el cache del catálogo (lo
/// que permite verlo con el disco DESCONECTADO); si no está y el disco está
/// montado, lo genera on-demand y lo cachea. Devuelve un data URL PNG.
#[tauri::command(async)]
pub fn get_thumbnail(
    state: tauri::State<'_, AppState>,
    entry_id: i64,
    max: Option<u32>,
) -> Result<String, String> {
    use base64::{engine::general_purpose::STANDARD, Engine};

    // 1) Cache (offline-friendly).
    {
        let guard = state.catalog.lock().unwrap();
        let cat = guard.as_ref().ok_or("no hay catálogo abierto")?;
        if let Some(png) = db::get_cached_thumbnail(&cat.conn, entry_id).map_err(|e| e.to_string())? {
            return Ok(format!("data:image/png;base64,{}", STANDARD.encode(png)));
        }
    }

    // 2) Generar on-demand desde el original (requiere disco montado) y cachear.
    let path = {
        let guard = state.catalog.lock().unwrap();
        let cat = guard.as_ref().ok_or("no hay catálogo abierto")?;
        resolve_real_path(&cat.conn, entry_id)?
    };
    let max = max.unwrap_or(THUMB_CACHE_MAX).clamp(32, 1024);
    let (png, w, h) = render_image_thumb(&path, max)?;
    {
        let guard = state.catalog.lock().unwrap();
        if let Some(cat) = guard.as_ref() {
            let _ = db::store_thumbnail(&cat.conn, entry_id, &png, w, h);
        }
    }
    Ok(format!("data:image/png;base64,{}", STANDARD.encode(png)))
}

#[derive(Serialize)]
pub struct ThumbCacheSummary {
    pub total: i64,
    pub generated: i64,
    pub failed: i64,
}

/// Genera y cachea en el catálogo los thumbnails faltantes de un disco (Fase A).
/// Pensado para correr justo después de un escaneo, mientras el disco sigue montado,
/// así las miniaturas quedan disponibles aunque luego se desconecte.
#[tauri::command(async)]
pub fn cache_disk_thumbnails(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    disk_id: i64,
) -> Result<ThumbCacheSummary, String> {
    // Recolectar (id, ruta_real) bajo lock; el trabajo pesado de imagen va fuera.
    let jobs: Vec<(i64, PathBuf)> = {
        let guard = state.catalog.lock().unwrap();
        let cat = guard.as_ref().ok_or("no hay catálogo abierto")?;
        let ids = db::image_entries_without_thumb(&cat.conn, disk_id, &thumb_exts())
            .map_err(|e| e.to_string())?;
        let mut v = Vec::with_capacity(ids.len());
        for id in ids {
            if let Ok(p) = resolve_real_path(&cat.conn, id) {
                v.push((id, p));
            }
        }
        v
    };

    let total = jobs.len() as i64;
    let mut generated = 0i64;
    let mut failed = 0i64;
    let mut done = 0i64;
    if total > 0 {
        emit_index(&app, "thumbnails", 0, total);
    }
    for (id, path) in jobs {
        match render_image_thumb(&path, THUMB_CACHE_MAX) {
            Ok((png, w, h)) => {
                let guard = state.catalog.lock().unwrap();
                match guard.as_ref() {
                    Some(cat) if db::store_thumbnail(&cat.conn, id, &png, w, h).is_ok() => {
                        generated += 1
                    }
                    _ => failed += 1,
                }
            }
            Err(_) => failed += 1,
        }
        done += 1;
        if done % 8 == 0 || done == total {
            emit_index(&app, "thumbnails", done, total);
        }
    }
    Ok(ThumbCacheSummary { total, generated, failed })
}

// ---------- Video + archivos (Fase B) ----------

/// Extensiones de video que intentamos indexar con ffprobe/ffmpeg.
const VIDEO_EXTS: &[&str] = &[
    "mp4", "mov", "m4v", "avi", "mkv", "mxf", "mts", "m2ts", "wmv", "webm", "mpg", "mpeg", "3gp",
    "flv", "ogv", "vob", "m2v",
];
/// Extensiones de archivos comprimidos cuyo contenido sabemos indexar.
const ARCHIVE_EXTS: &[&str] = &["zip", "7z", "rar", "cbz", "cbr"];
/// Frames de la tira por video y ancho de cada uno.
const VIDEO_STRIP_FRAMES: usize = 5;
const VIDEO_FRAME_W: u32 = 320;

fn png_data_url(png: &[u8]) -> String {
    use base64::{engine::general_purpose::STANDARD, Engine};
    format!("data:image/png;base64,{}", STANDARD.encode(png))
}

/// ¿Están disponibles ffprobe/ffmpeg? La UI lo usa para mostrar/ocultar features.
#[tauri::command]
pub fn media_tools_available() -> bool {
    video::tools_available()
}

#[derive(Serialize)]
pub struct VideoIndexSummary {
    pub total: i64,
    pub indexed: i64,
    pub failed: i64,
    pub frames: i64,
    pub tools_ok: bool,
}

/// Indexa metadata + tira de frames de los videos de un disco (post-escaneo,
/// con el disco montado). Guarda también un frame póster como thumbnail, así los
/// videos se previsualizan offline igual que las imágenes.
#[tauri::command(async)]
pub fn index_disk_videos(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    disk_id: i64,
) -> Result<VideoIndexSummary, String> {
    if !video::tools_available() {
        return Ok(VideoIndexSummary { total: 0, indexed: 0, failed: 0, frames: 0, tools_ok: false });
    }
    let jobs: Vec<(i64, PathBuf)> = {
        let guard = state.catalog.lock().unwrap();
        let cat = guard.as_ref().ok_or("no hay catálogo abierto")?;
        let ids = db::video_entries_without_meta(&cat.conn, disk_id, VIDEO_EXTS)
            .map_err(|e| e.to_string())?;
        ids.into_iter()
            .filter_map(|id| resolve_real_path(&cat.conn, id).ok().map(|p| (id, p)))
            .collect()
    };

    let total = jobs.len() as i64;
    let (mut indexed, mut failed, mut frames) = (0i64, 0i64, 0i64);
    let mut done = 0i64;
    if total > 0 {
        emit_index(&app, "videos", 0, total);
    }
    for (id, path) in jobs {
        done += 1;
        emit_index(&app, "videos", done, total);
        let meta = match video::probe_video(&path) {
            Ok(m) => m,
            Err(_) => {
                failed += 1;
                continue;
            }
        };
        let row = db::VideoMetaRow {
            duration_ms: meta.duration_ms,
            width: meta.width,
            height: meta.height,
            fps: meta.fps,
            vcodec: meta.vcodec.clone(),
            acodec: meta.acodec.clone(),
            bitrate: meta.bitrate,
        };

        // Tira de frames (trabajo pesado, fuera del lock).
        let ts = video::strip_timestamps(meta.duration_ms, VIDEO_STRIP_FRAMES);
        let mut strip: Vec<(i64, Vec<u8>)> = Vec::new();
        for t in &ts {
            if let Ok(png) = video::extract_frame(&path, *t, VIDEO_FRAME_W) {
                strip.push(((t * 1000.0) as i64, png));
            }
        }
        let poster = strip.get(strip.len() / 2).map(|(_, p)| p.clone());

        let guard = state.catalog.lock().unwrap();
        if let Some(cat) = guard.as_ref() {
            let _ = db::store_video_meta(&cat.conn, id, &row);
            if !strip.is_empty() {
                frames += strip.len() as i64;
                let _ = db::replace_video_frames(&cat.conn, id, &strip);
            }
            if let Some(p) = &poster {
                let _ = db::store_thumbnail(&cat.conn, id, p, 0, 0);
            }
            indexed += 1;
        }
    }
    Ok(VideoIndexSummary { total, indexed, failed, frames, tools_ok: true })
}

/// Metadata técnica de un video (si fue indexada).
#[tauri::command]
pub fn get_video_meta(
    state: tauri::State<'_, AppState>,
    entry_id: i64,
) -> Result<Option<db::VideoMetaRow>, String> {
    let guard = state.catalog.lock().unwrap();
    let cat = guard.as_ref().ok_or("no hay catálogo abierto")?;
    db::get_video_meta(&cat.conn, entry_id).map_err(|e| e.to_string())
}

/// Tira de frames cacheada de un video, como data URLs PNG.
#[tauri::command]
pub fn get_video_frames(
    state: tauri::State<'_, AppState>,
    entry_id: i64,
) -> Result<Vec<String>, String> {
    let guard = state.catalog.lock().unwrap();
    let cat = guard.as_ref().ok_or("no hay catálogo abierto")?;
    let frames = db::get_video_frames(&cat.conn, entry_id).map_err(|e| e.to_string())?;
    Ok(frames.iter().map(|p| png_data_url(p)).collect())
}

/// Detección de escenas on-demand de un video (requiere disco montado). Devuelve
/// los segundos de los cortes detectados.
#[tauri::command(async)]
pub fn detect_video_scenes(
    state: tauri::State<'_, AppState>,
    entry_id: i64,
    threshold: Option<f64>,
) -> Result<Vec<f64>, String> {
    let path = {
        let guard = state.catalog.lock().unwrap();
        let cat = guard.as_ref().ok_or("no hay catálogo abierto")?;
        resolve_real_path(&cat.conn, entry_id)?
    };
    video::detect_scenes(&path, threshold.unwrap_or(0.4), 200)
}

#[derive(Serialize)]
pub struct ArchiveIndexSummary {
    pub total: i64,
    pub indexed: i64,
    pub failed: i64,
    pub items: i64,
}

/// Indexa el contenido (nombres/tamaños/fechas) de los archivos comprimidos de un
/// disco (post-escaneo, con el disco montado).
#[tauri::command(async)]
pub fn index_disk_archives(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    disk_id: i64,
) -> Result<ArchiveIndexSummary, String> {
    let jobs: Vec<(i64, PathBuf)> = {
        let guard = state.catalog.lock().unwrap();
        let cat = guard.as_ref().ok_or("no hay catálogo abierto")?;
        let ids = db::archive_files_without_index(&cat.conn, disk_id, ARCHIVE_EXTS)
            .map_err(|e| e.to_string())?;
        ids.into_iter()
            .filter_map(|id| resolve_real_path(&cat.conn, id).ok().map(|p| (id, p)))
            .collect()
    };

    let total = jobs.len() as i64;
    let (mut indexed, mut failed, mut items) = (0i64, 0i64, 0i64);
    let mut done = 0i64;
    if total > 0 {
        emit_index(&app, "archives", 0, total);
    }
    for (id, path) in jobs {
        done += 1;
        emit_index(&app, "archives", done, total);
        match archive::list_archive(&path) {
            Ok(list) => {
                items += list.len() as i64;
                let mut guard = state.catalog.lock().unwrap();
                if let Some(cat) = guard.as_mut() {
                    match db::store_archive_entries(&mut cat.conn, id, &list) {
                        Ok(()) => indexed += 1,
                        Err(_) => failed += 1,
                    }
                }
            }
            Err(_) => failed += 1,
        }
    }
    Ok(ArchiveIndexSummary { total, indexed, failed, items })
}

/// Lista el contenido indexado de un archivo comprimido.
#[tauri::command]
pub fn list_archive_contents(
    state: tauri::State<'_, AppState>,
    entry_id: i64,
) -> Result<Vec<db::ArchiveEntryRow>, String> {
    let guard = state.catalog.lock().unwrap();
    let cat = guard.as_ref().ok_or("no hay catálogo abierto")?;
    db::list_archive_entries(&cat.conn, entry_id).map_err(|e| e.to_string())
}

// ---------- Tags / keywords (Fase A) ----------

/// Agrega un tag a una entrada y devuelve la lista actualizada.
#[tauri::command]
pub fn add_entry_tag(
    state: tauri::State<'_, AppState>,
    entry_id: i64,
    tag: String,
) -> Result<Vec<String>, String> {
    let guard = state.catalog.lock().unwrap();
    let cat = guard.as_ref().ok_or("no hay catálogo abierto")?;
    db::add_entry_tag(&cat.conn, entry_id, &tag).map_err(|e| e.to_string())?;
    db::entry_tags(&cat.conn, entry_id).map_err(|e| e.to_string())
}

/// Quita un tag de una entrada y devuelve la lista actualizada.
#[tauri::command]
pub fn remove_entry_tag(
    state: tauri::State<'_, AppState>,
    entry_id: i64,
    tag: String,
) -> Result<Vec<String>, String> {
    let guard = state.catalog.lock().unwrap();
    let cat = guard.as_ref().ok_or("no hay catálogo abierto")?;
    db::remove_entry_tag(&cat.conn, entry_id, &tag).map_err(|e| e.to_string())?;
    db::entry_tags(&cat.conn, entry_id).map_err(|e| e.to_string())
}

/// Tags de una entrada.
#[tauri::command]
pub fn get_entry_tags(
    state: tauri::State<'_, AppState>,
    entry_id: i64,
) -> Result<Vec<String>, String> {
    let guard = state.catalog.lock().unwrap();
    let cat = guard.as_ref().ok_or("no hay catálogo abierto")?;
    db::entry_tags(&cat.conn, entry_id).map_err(|e| e.to_string())
}

/// Todos los tags del catálogo con su conteo de uso.
#[tauri::command]
pub fn list_tags(state: tauri::State<'_, AppState>) -> Result<Vec<db::TagStat>, String> {
    let guard = state.catalog.lock().unwrap();
    let cat = guard.as_ref().ok_or("no hay catálogo abierto")?;
    db::list_tags(&cat.conn).map_err(|e| e.to_string())
}

/// M3: búsqueda full-text por nombre sobre todo el catálogo.
#[tauri::command]
pub fn search_entries(
    state: tauri::State<'_, AppState>,
    query: String,
    limit: Option<i64>,
) -> Result<db::SearchResult, String> {
    let guard = state.catalog.lock().unwrap();
    let cat = guard.as_ref().ok_or("no hay catálogo abierto")?;
    db::search(&cat.conn, &query, limit.unwrap_or(1000)).map_err(|e| e.to_string())
}

#[derive(Serialize)]
pub struct ScanSummary {
    pub disk_id: i64,
    pub name: String,
    pub entries: u64,
    pub files: i64,
    pub folders: i64,
    pub replaced: bool,
    pub volume_uuid: Option<String>,
    pub elapsed_ms: u128,
}

/// M5: lista los volúmenes montados (para elegir cuál escanear / detectar nuevos).
#[tauri::command]
pub fn list_volumes() -> Vec<VolumeInfo> {
    scan::list_volumes()
}

/// M5: escanea un volumen/carpeta montado y lo guarda como disco del catálogo.
/// Re-escanea (reemplaza) si ya existe un disco con el mismo fingerprint.
/// Requiere un catálogo abierto.
#[tauri::command(async)]
pub fn scan_disk(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    mount_path: String,
    name: Option<String>,
    options: Option<ScanOptions>,
) -> Result<ScanSummary, String> {
    let t0 = now_ms();
    let root = PathBuf::from(&mount_path);
    if !root.exists() {
        return Err(format!("la ruta {mount_path} no existe o el disco no está montado"));
    }
    let volume_name = name.unwrap_or_else(|| {
        root.file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| mount_path.clone())
    });
    let opts = options.unwrap_or_default();

    // Fingerprint + capacidad/tipo desde el volumen (si coincide con un mount conocido).
    let fingerprint = scan::volume_fingerprint(&root);
    let (capacity, kind) = volume_caps(&mount_path);

    let disk = {
        let app = app.clone();
        scan::scan_volume_cb(&root, &volume_name, &opts, &mut |count| {
            let _ = app.emit("scan-progress", ScanProgress { count });
        })
        .map_err(|e| format!("error escaneando: {e}"))?
    };
    // Señal de fin de fase de escaneo (la UI esconde el contador).
    let _ = app.emit("scan-progress", ScanProgress { count: disk.entries.len() as u64 });

    let mut guard = state.catalog.lock().unwrap();
    let cat = guard.as_mut().ok_or("no hay catálogo abierto: creá o abrí uno antes de escanear")?;
    let ingest = db::ingest_scanned(
        &mut cat.conn,
        &disk,
        fingerprint.as_deref(),
        &kind,
        capacity,
        &mount_path,
    )
    .map_err(|e| format!("error guardando el escaneo: {e}"))?;

    Ok(ScanSummary {
        disk_id: ingest.disk_id,
        name: volume_name,
        entries: ingest.entries,
        files: ingest.files,
        folders: ingest.folders,
        replaced: ingest.replaced,
        volume_uuid: fingerprint,
        elapsed_ms: now_ms().saturating_sub(t0),
    })
}

/// Capacidad y tipo de un volumen por su mount path (best-effort vía sysinfo).
fn volume_caps(mount_path: &str) -> (Option<i64>, String) {
    for v in scan::list_volumes() {
        if v.mount_path == mount_path {
            return (Some(v.total_space as i64), v.kind);
        }
    }
    (None, "disk".into())
}

/// M5: arranca el watcher que detecta discos conectados/desconectados y emite
/// eventos `volume-added` / `volume-removed` con el `VolumeInfo`. Idempotente.
#[tauri::command]
pub fn start_volume_watch(app: tauri::AppHandle, state: tauri::State<'_, AppState>) {
    if state.watch_started.swap(true, Ordering::SeqCst) {
        return; // ya está corriendo
    }
    std::thread::spawn(move || {
        let mut known: Vec<VolumeInfo> = scan::list_volumes();
        // Emitir el estado inicial una vez.
        let _ = app.emit("volumes-initial", &known);
        loop {
            std::thread::sleep(std::time::Duration::from_millis(2500));
            let current = scan::list_volumes();
            // Nuevos: en current pero no en known (por mount_path).
            for v in &current {
                if !known.iter().any(|k| k.mount_path == v.mount_path) {
                    let _ = app.emit("volume-added", v);
                }
            }
            // Quitados: en known pero no en current.
            for k in &known {
                if !current.iter().any(|v| v.mount_path == k.mount_path) {
                    let _ = app.emit("volume-removed", k);
                }
            }
            known = current;
        }
    });
}

/// Marca discos online/offline comparando `volume_uuid`/`mount_path` con los
/// volúmenes montados ahora (M6, base). Devuelve la lista actualizada.
#[tauri::command]
pub fn refresh_online_status(state: tauri::State<'_, AppState>) -> Result<Vec<DiskRow>, String> {
    let vols = scan::list_volumes();
    let guard = state.catalog.lock().unwrap();
    let cat = guard.as_ref().ok_or("no hay catálogo abierto")?;

    // Resetear y volver a marcar.
    cat.conn
        .execute("UPDATE disks SET is_online = 0, mount_path = NULL", [])
        .map_err(|e| e.to_string())?;
    for v in &vols {
        // Por fingerprint si está, si no por nombre del volumen.
        if let Some(fp) = &v.fingerprint {
            let _ = cat.conn.execute(
                "UPDATE disks SET is_online = 1, mount_path = ?1 WHERE volume_uuid = ?2",
                rusqlite::params![v.mount_path, fp],
            );
        }
        let _ = cat.conn.execute(
            "UPDATE disks SET is_online = 1, mount_path = ?1 WHERE volume_uuid IS NULL AND name = ?2",
            rusqlite::params![v.mount_path, v.name],
        );
    }
    drop(guard);
    list_disks(state)
}

/// Lista los discos del catálogo abierto.
#[tauri::command]
pub fn list_disks(state: tauri::State<'_, AppState>) -> Result<Vec<DiskRow>, String> {
    let guard = state.catalog.lock().unwrap();
    let cat = guard.as_ref().ok_or("no hay catálogo abierto")?;
    let mut stmt = cat
        .conn
        .prepare(
            "SELECT id, name, total_size, file_count, folder_count, is_online, location, category, comment \
             FROM disks ORDER BY name",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok(DiskRow {
                id: r.get(0)?,
                name: r.get(1)?,
                total_size: r.get(2)?,
                file_count: r.get(3)?,
                folder_count: r.get(4)?,
                is_online: r.get::<_, i64>(5)? != 0,
                location: r.get(6)?,
                category: r.get(7)?,
                comment: r.get(8)?,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}
