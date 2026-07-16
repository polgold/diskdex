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
use tauri::{Emitter, Manager};

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

/// Detalle de un disco para el panel de info (sección 11): fecha del último
/// escaneo, capacidad cataloga y —si está montado— espacio total/libre en vivo.
#[derive(Serialize)]
pub struct DiskDetail {
    pub id: i64,
    pub name: String,
    /// Suma de tamaños lógicos cataloga (lo que ocupan los archivos indexados).
    pub total_size: i64,
    pub file_count: i64,
    pub folder_count: i64,
    pub is_online: bool,
    pub kind: Option<String>,
    /// Capacidad del volumen guardada al escanear (puede faltar en catálogos viejos).
    pub capacity: Option<i64>,
    /// Unix (segundos) del último escaneo de este disco.
    pub scanned_at: Option<i64>,
    /// Capacidad real del volumen montado ahora (solo si está online).
    pub live_total: Option<i64>,
    /// Espacio libre real del volumen montado ahora (solo si está online).
    pub live_free: Option<i64>,
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

/// Mount paths con cancelación pendiente. Registro global (no hace falta plomería
/// por `AppState`): el escaneo consulta acá y `cancel_scan` agrega. Soporta varios
/// escaneos simultáneos.
static SCAN_CANCELS: Mutex<Vec<String>> = Mutex::new(Vec::new());

fn cancel_requested(mount_path: &str) -> bool {
    SCAN_CANCELS
        .lock()
        .map(|v| v.iter().any(|m| m == mount_path))
        .unwrap_or(false)
}

fn clear_cancel(mount_path: &str) {
    if let Ok(mut v) = SCAN_CANCELS.lock() {
        v.retain(|m| m != mount_path);
    }
}

/// Pide cancelar el escaneo en curso de `mount_path`. El escaneo aborta en el
/// próximo chequeo (entre carpetas o cada ~4096 entradas) y no ingesta nada.
#[tauri::command(async)]
pub fn cancel_scan(mount_path: String) {
    if let Ok(mut v) = SCAN_CANCELS.lock() {
        if !v.iter().any(|m| m == &mount_path) {
            v.push(mount_path);
        }
    }
}

/// M0: sanity check de la IPC.
#[tauri::command(async)]
pub fn ping() -> String {
    "pong".into()
}

// ---------- Progreso (eventos a la UI) ----------

#[derive(Clone, Serialize)]
struct ScanProgress {
    /// Mount path del disco que se está escaneando (para enrutar el progreso por disco).
    mount: String,
    count: u64,
    /// % estimado (bytes recorridos / usados del volumen). -1 si se desconoce.
    pct: i32,
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
#[tauri::command(async)]
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
#[tauri::command(async)]
pub fn agent_stop(state: tauri::State<'_, AppState>) -> Result<(), String> {
    if let Some(handle) = state.agent.lock().unwrap().take() {
        handle.stop();
    }
    Ok(())
}

#[tauri::command(async)]
pub fn agent_status(state: tauri::State<'_, AppState>) -> AgentStatus {
    let guard = state.agent.lock().unwrap();
    match guard.as_ref() {
        Some(h) => AgentStatus { running: true, addr: Some(h.addr.clone()) },
        None => AgentStatus { running: false, addr: None },
    }
}

/// Genera un código de emparejamiento (válido 5 min) para enrolar un dispositivo.
#[tauri::command(async)]
pub fn agent_pair_code(state: tauri::State<'_, AppState>) -> Result<String, String> {
    let guard = state.agent.lock().unwrap();
    let h = guard.as_ref().ok_or("el conector no está activo")?;
    Ok(h.new_pairing_code())
}

/// Lista los dispositivos enrolados.
#[tauri::command(async)]
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
#[tauri::command(async)]
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
    let bytes = read_dcmf_bytes(&dcmf_path)?;
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

/// Lee los bytes del `.dcmf`. DiskCatalogMaker a veces guarda el catálogo como
/// PAQUETE (una carpeta `.dcmf` con un `Catalog.dcmf` adentro) en vez de archivo
/// plano. Si el path es un directorio, busca el dato real adentro.
fn read_dcmf_bytes(dcmf_path: &str) -> Result<Vec<u8>, String> {
    let p = PathBuf::from(dcmf_path);
    let file = if p.is_dir() {
        let inner = p.join("Catalog.dcmf");
        if inner.is_file() {
            inner
        } else {
            // Fallback: el .dcmf/.dcmd más grande dentro del paquete.
            std::fs::read_dir(&p)
                .ok()
                .and_then(|rd| {
                    rd.filter_map(|e| e.ok().map(|e| e.path()))
                        .filter(|x| {
                            x.is_file()
                                && x.extension()
                                    .map(|e| e == "dcmf" || e == "dcmd")
                                    .unwrap_or(false)
                        })
                        .max_by_key(|x| std::fs::metadata(x).map(|m| m.len()).unwrap_or(0))
                })
                .ok_or_else(|| "el paquete .dcmf no contiene un Catalog.dcmf".to_string())?
        }
    } else {
        p
    };
    std::fs::read(&file).map_err(|e| format!("no se pudo leer {}: {e}", file.display()))
}

/// Nombres de los discos contenidos en un `.dcmf` (para previsualizar conflictos
/// antes de importar al catálogo abierto).
#[tauri::command(async)]
pub fn dcmf_disk_names(dcmf_path: String) -> Result<Vec<String>, String> {
    let bytes = read_dcmf_bytes(&dcmf_path)?;
    Ok(dcmf::import_dcmf(&bytes).into_iter().map(|d| d.name).collect())
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportMergeArgs {
    pub dcmf_path: String,
    pub replace: bool,
}

/// Importa los discos de un `.dcmf` DENTRO del catálogo ABIERTO (no crea uno
/// nuevo). Si un disco ya existe (por nombre): `replace=true` lo reemplaza,
/// `replace=false` lo saltea (mantiene el actual). Devuelve cuántos importó.
/// (Args en un struct único para evitar el borde de Tauri con varios args +
/// State en comandos async.)
#[tauri::command(async)]
pub fn import_dcmf_merge(
    state: tauri::State<'_, AppState>,
    args: ImportMergeArgs,
) -> Result<ImportSummary, String> {
    let ImportMergeArgs { dcmf_path, replace } = args;
    let t0 = now_ms();
    let bytes = read_dcmf_bytes(&dcmf_path)?;
    let disks = dcmf::import_dcmf(&bytes);
    if disks.is_empty() {
        return Err("el archivo .dcmf no contiene discos reconocibles".into());
    }

    let mut guard = state.catalog.lock().unwrap();
    let cat = guard.as_mut().ok_or("no hay catálogo abierto")?;

    // Nombres existentes → id.
    let existing: std::collections::HashMap<String, i64> = {
        let mut stmt = cat.conn.prepare("SELECT name, id FROM disks").map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))
            .map_err(|e| e.to_string())?;
        rows.filter_map(|r| r.ok()).collect()
    };

    let mut to_import: Vec<dcmf::DcmfDisk> = Vec::new();
    for d in disks {
        match existing.get(&d.name) {
            Some(&id) if replace => {
                let _ = db::delete_disk(&mut cat.conn, id);
                to_import.push(d);
            }
            Some(_) => { /* mantener el actual, saltar */ }
            None => to_import.push(d),
        }
    }

    let disk_count = to_import.len();
    let entries = if to_import.is_empty() {
        0
    } else {
        db::ingest_disks(&mut cat.conn, &to_import).map_err(|e| format!("error en ingesta: {e}"))?
    };

    Ok(ImportSummary {
        catalog_path: cat.path.to_string_lossy().to_string(),
        disks: disk_count,
        entries,
        elapsed_ms: now_ms().saturating_sub(t0),
    })
}

/// Abre un catálogo `.dccat` existente.
#[tauri::command(async)]
pub fn open_catalog(state: tauri::State<'_, AppState>, catalog_path: String) -> Result<(), String> {
    let cat_path = PathBuf::from(&catalog_path);
    let conn = db::open(&cat_path).map_err(|e| format!("error abriendo catálogo: {e}"))?;
    *state.catalog.lock().unwrap() = Some(Catalog { path: cat_path, conn });
    Ok(())
}

/// M2: hijos directos de un nodo (raíz del disco si `parent_id` es None).
#[tauri::command(async)]
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
#[tauri::command(async)]
pub fn entry_path(state: tauri::State<'_, AppState>, entry_id: i64) -> Result<String, String> {
    let guard = state.catalog.lock().unwrap();
    let cat = guard.as_ref().ok_or("no hay catálogo abierto")?;
    db::entry_path(&cat.conn, entry_id).map_err(|e| e.to_string())
}

/// M2: una entrada por id (para el inspector).
#[tauri::command(async)]
pub fn get_entry(
    state: tauri::State<'_, AppState>,
    entry_id: i64,
) -> Result<Option<db::EntryRow>, String> {
    let guard = state.catalog.lock().unwrap();
    let cat = guard.as_ref().ok_or("no hay catálogo abierto")?;
    db::get_entry(&cat.conn, entry_id).map_err(|e| e.to_string())
}

/// M7: edita el comentario de una entrada.
#[tauri::command(async)]
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
#[tauri::command(async)]
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
#[tauri::command(async)]
pub fn catalog_stats(
    state: tauri::State<'_, AppState>,
    disk_id: Option<i64>,
) -> Result<db::Stats, String> {
    let guard = state.catalog.lock().unwrap();
    let cat = guard.as_ref().ok_or("no hay catálogo abierto")?;
    db::stats(&cat.conn, disk_id).map_err(|e| e.to_string())
}

/// M8: archivos duplicados (por nombre+tamaño), ordenados por espacio desperdiciado.
#[tauri::command(async)]
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

// ---------- Comparación de discos / mirror de backup (M9) ----------

/// M9: compara dos discos del catálogo por ruta relativa + tamaño. Solo lee del
/// catálogo, así que funciona aunque los discos estén desconectados.
#[tauri::command(async)]
pub fn compare_disks(
    state: tauri::State<'_, AppState>,
    src_disk_id: i64,
    dst_disk_id: i64,
    src_root_id: Option<i64>,
    dst_root_id: Option<i64>,
    limit: Option<i64>,
) -> Result<db::DiskDiff, String> {
    // Mismo disco solo se permite si se comparan carpetas distintas.
    if src_disk_id == dst_disk_id && src_root_id == dst_root_id {
        return Err("El origen y el destino no pueden ser la misma carpeta.".into());
    }
    let guard = state.catalog.lock().unwrap();
    let cat = guard.as_ref().ok_or("no hay catálogo abierto")?;
    db::compare_disks(
        &cat.conn,
        src_disk_id,
        dst_disk_id,
        src_root_id,
        dst_root_id,
        limit.unwrap_or(5000).max(0) as usize,
    )
    .map_err(|e| e.to_string())
}

/// Progreso del mirror, emitido por el evento `compare-copy-progress`.
#[derive(Debug, Clone, Serialize)]
pub struct CopyProgress {
    pub done: i64,
    pub total: i64,
    pub bytes_done: i64,
    pub bytes_total: i64,
    pub current: String,
}

/// Resumen final del mirror.
#[derive(Debug, Clone, Serialize)]
pub struct CopySummary {
    pub copied: i64,
    pub failed: i64,
    pub bytes_copied: i64,
    pub errors: Vec<String>,
    pub cancelled: bool,
    /// El catálogo del destino quedó desactualizado: conviene re-escanear.
    pub needs_rescan: bool,
}

/// Flag global de cancelación del mirror en curso (solo uno a la vez).
static COPY_CANCEL: AtomicBool = AtomicBool::new(false);

/// M9: pide cancelar el mirror en curso. Aborta antes del próximo archivo.
#[tauri::command(async)]
pub fn cancel_copy() {
    COPY_CANCEL.store(true, Ordering::SeqCst);
}

/// Lee mount_path + online + nombre de un disco.
fn disk_mount(conn: &Connection, disk_id: i64) -> Result<(String, String), String> {
    let (mount, online, name): (Option<String>, i64, String) = conn
        .query_row(
            "SELECT mount_path, is_online, name FROM disks WHERE id = ?1",
            [disk_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .map_err(|_| "el disco no existe".to_string())?;
    match (online, mount) {
        (1, Some(m)) => Ok((m, name)),
        _ => Err(format!(
            "El disco «{name}» está offline. Conectalo y actualizá el estado antes de copiar."
        )),
    }
}

/// M9: copia al destino los archivos que faltan (y opcionalmente los de tamaño
/// distinto), reproduciendo la estructura de carpetas. Requiere ambos discos
/// online. Emite `compare-copy-progress` y devuelve un resumen.
#[tauri::command(async)]
pub async fn copy_missing(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    src_disk_id: i64,
    dst_disk_id: i64,
    src_root_id: Option<i64>,
    dst_root_id: Option<i64>,
    include_mismatch: bool,
) -> Result<CopySummary, String> {
    if src_disk_id == dst_disk_id && src_root_id == dst_root_id {
        return Err("El origen y el destino no pueden ser la misma carpeta.".into());
    }

    // Bajo el lock: validar discos, resolver el prefijo de cada carpeta raíz
    // (para reconstruir la ruta real bajo el mount) y calcular el plan completo.
    // Después soltamos el lock (la copia no toca la base) para no bloquear la UI.
    let (src_mount, dst_mount, plan) = {
        let guard = state.catalog.lock().unwrap();
        let cat = guard.as_ref().ok_or("no hay catálogo abierto")?;
        let (src_disk_mount, _sn) = disk_mount(&cat.conn, src_disk_id)?;
        let (dst_disk_mount, _dn) = disk_mount(&cat.conn, dst_disk_id)?;
        // La ruta real de un subárbol = mount + (ruta de la carpeta relativa al disco).
        let join_root = |mount: &str, root: Option<i64>| -> Result<PathBuf, String> {
            let base = PathBuf::from(mount);
            match root {
                None => Ok(base),
                Some(id) => {
                    let rel = db::disk_rel_path_of(&cat.conn, id).map_err(|e| e.to_string())?;
                    let rel_pb: PathBuf = rel.split('/').filter(|s| !s.is_empty()).collect();
                    Ok(base.join(rel_pb))
                }
            }
        };
        let src_mount = join_root(&src_disk_mount, src_root_id)?;
        let dst_mount = join_root(&dst_disk_mount, dst_root_id)?;
        let plan = db::copy_plan(&cat.conn, src_disk_id, dst_disk_id, src_root_id, dst_root_id, include_mismatch)
            .map_err(|e| e.to_string())?;
        (src_mount, dst_mount, plan)
    };

    let total = plan.len() as i64;
    let bytes_total: i64 = plan.iter().map(|c| c.size).sum();
    if total == 0 {
        return Ok(CopySummary {
            copied: 0,
            failed: 0,
            bytes_copied: 0,
            errors: Vec::new(),
            cancelled: false,
            needs_rescan: false,
        });
    }

    COPY_CANCEL.store(false, Ordering::SeqCst);
    let src_root = src_mount;
    let dst_root = dst_mount;

    let summary = tauri::async_runtime::spawn_blocking(move || {
        run_copy(
            &plan,
            &src_root,
            &dst_root,
            total,
            bytes_total,
            || COPY_CANCEL.load(Ordering::SeqCst),
            |done, bytes_done, current| {
                let _ = app.emit(
                    "compare-copy-progress",
                    CopyProgress { done, total, bytes_done, bytes_total, current: current.to_string() },
                );
            },
        )
    })
    .await
    .map_err(|e| e.to_string())?;

    COPY_CANCEL.store(false, Ordering::SeqCst);
    Ok(summary)
}

/// Copia física del plan: reconstruye carpetas y copia cada archivo del árbol
/// origen al destino. Puro (sin Tauri) para poder testearlo contra tmp dirs.
/// `is_cancelled` se consulta antes de cada archivo; `on_progress` recibe
/// (hechos, bytes copiados, ruta actual).
fn run_copy(
    plan: &[db::CopyItem],
    src_root: &std::path::Path,
    dst_root: &std::path::Path,
    total: i64,
    _bytes_total: i64,
    is_cancelled: impl Fn() -> bool,
    mut on_progress: impl FnMut(i64, i64, &str),
) -> CopySummary {
    let mut copied = 0i64;
    let mut failed = 0i64;
    let mut bytes_copied = 0i64;
    let mut errors: Vec<String> = Vec::new();
    let mut cancelled = false;

    for (i, item) in plan.iter().enumerate() {
        if is_cancelled() {
            cancelled = true;
            break;
        }
        // La ruta relativa usa '/'; en el FS la reconstruimos por componentes.
        let rel: PathBuf = item.rel_path.split('/').filter(|s| !s.is_empty()).collect();
        let src_path = src_root.join(&rel);
        let dst_path = dst_root.join(&rel);

        on_progress(i as i64, bytes_copied, &item.rel_path);

        if let Some(parent) = dst_path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                failed += 1;
                if errors.len() < 50 {
                    errors.push(format!("{}: {e}", item.rel_path));
                }
                continue;
            }
        }
        match std::fs::copy(&src_path, &dst_path) {
            Ok(n) => {
                copied += 1;
                bytes_copied += n as i64;
            }
            Err(e) => {
                failed += 1;
                if errors.len() < 50 {
                    errors.push(format!("{}: {e}", item.rel_path));
                }
            }
        }
    }

    on_progress(total, bytes_copied, "");

    CopySummary {
        copied,
        failed,
        bytes_copied,
        errors,
        cancelled,
        needs_rescan: copied > 0,
    }
}

#[cfg(test)]
mod copy_tests {
    use super::*;

    #[test]
    fn run_copy_reconstructs_tree_and_bytes() {
        // Directorios temporales aislados (sin colisiones entre corridas).
        let base = std::env::temp_dir().join(format!("diskdex_copytest_{}", std::process::id()));
        let src = base.join("src");
        let dst = base.join("dst");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(src.join("CLIP")).unwrap();
        std::fs::write(src.join("CLIP/A.MP4"), b"hello world").unwrap(); // 11 bytes
        std::fs::write(src.join("README.txt"), b"doc").unwrap(); // 3 bytes
        std::fs::create_dir_all(&dst).unwrap();

        let plan = vec![
            db::CopyItem { rel_path: "CLIP/A.MP4".into(), size: 11 },
            db::CopyItem { rel_path: "README.txt".into(), size: 3 },
        ];
        let summary = run_copy(&plan, &src, &dst, 2, 14, || false, |_, _, _| {});

        assert_eq!(summary.copied, 2);
        assert_eq!(summary.failed, 0);
        assert_eq!(summary.bytes_copied, 14);
        assert!(summary.needs_rescan);
        // El árbol se reprodujo en el destino con el contenido correcto.
        assert_eq!(std::fs::read(dst.join("CLIP/A.MP4")).unwrap(), b"hello world");
        assert_eq!(std::fs::read(dst.join("README.txt")).unwrap(), b"doc");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn run_copy_honors_cancellation() {
        let base = std::env::temp_dir().join(format!("diskdex_canceltest_{}", std::process::id()));
        let src = base.join("src");
        let dst = base.join("dst");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("A.bin"), b"x").unwrap();
        std::fs::create_dir_all(&dst).unwrap();

        let plan = vec![db::CopyItem { rel_path: "A.bin".into(), size: 1 }];
        // Cancelado desde el arranque: no copia nada.
        let summary = run_copy(&plan, &src, &dst, 1, 1, || true, |_, _, _| {});
        assert!(summary.cancelled);
        assert_eq!(summary.copied, 0);
        assert!(!dst.join("A.bin").exists());
        let _ = std::fs::remove_dir_all(&base);
    }
}

/// M7: escribe un archivo de texto (export CSV/TSV/JSON/HTML generado por la UI).
#[tauri::command(async)]
pub fn write_text_file(path: String, contents: String) -> Result<(), String> {
    std::fs::write(&path, contents).map_err(|e| format!("no se pudo escribir {path}: {e}"))
}

/// Ruta del archivo de sesión (último catálogo abierto), en el dir de config de
/// la app. Persistente y durable (no depende del localStorage del webview).
fn session_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|e| format!("no se pudo resolver el dir de config: {e}"))?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("no se pudo crear {}: {e}", dir.display()))?;
    Ok(dir.join("session.json"))
}

/// Guarda la sesión (JSON con catálogos abiertos + activo) en disco. Durable
/// frente a cierres forzados; se llama en cada cambio de catálogo.
#[tauri::command(async)]
pub fn save_session(app: tauri::AppHandle, contents: String) -> Result<(), String> {
    let path = session_path(&app)?;
    std::fs::write(&path, contents).map_err(|e| format!("no se pudo guardar la sesión: {e}"))
}

/// Lee la sesión guardada (o None si no existe). Nunca borra nada.
#[tauri::command(async)]
pub fn load_session(app: tauri::AppHandle) -> Result<Option<String>, String> {
    let path = session_path(&app)?;
    match std::fs::read_to_string(&path) {
        Ok(s) => Ok(Some(s)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("no se pudo leer la sesión: {e}")),
    }
}

/// M4: búsqueda por atributos / booleana (ext, tamaño, fecha, tipo, disco).
#[tauri::command(async)]
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
#[tauri::command(async)]
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
/// Extensiones de video para preview on-demand (extrae un frame con ffmpeg).
const VIDEO_THUMB_EXTS: &[&str] = &[
    "mp4", "mov", "m4v", "avi", "mkv", "mxf", "mts", "m2ts", "wmv", "webm", "mpg", "mpeg", "3gp",
    "flv", "ogv", "vob", "m2v",
];

/// Lado máximo del thumbnail cacheado. Compacto pero nítido en el inspector/grilla.
const THUMB_CACHE_MAX: u32 = 320;

/// Lista combinada (imagen estándar + RAW) para buscar pendientes de thumbnail.
fn thumb_exts() -> Vec<&'static str> {
    THUMB_EXTS.iter().chain(RAW_EXTS.iter()).copied().collect()
}

/// Renderiza un JPEG de thumbnail desde una ruta real. Devuelve (bytes, w, h).
/// Despacha RAW de cámara a `sips` (macOS) y el resto al crate `image`.
/// JPEG (no PNG) para que el catálogo no se infle: ~5-10× más chico por miniatura.
fn render_image_thumb(path: &std::path::Path, max: u32) -> Result<(Vec<u8>, u32, u32), String> {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    if RAW_EXTS.contains(&ext.as_str()) {
        render_raw_thumbnail(path, max)
    } else {
        render_thumbnail_jpeg(path, max)
    }
}

fn render_thumbnail_jpeg(path: &std::path::Path, max: u32) -> Result<(Vec<u8>, u32, u32), String> {
    let img = image::open(path)
        .map_err(|e| format!("no se pudo generar preview (formato no soportado): {e}"))?;
    let thumb = img.thumbnail(max, max);
    let (w, h) = (thumb.width(), thumb.height());
    // A RGB8 (JPEG no soporta alfa) y encode JPEG (calidad por defecto ~75).
    let mut buf = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(thumb.to_rgb8())
        .write_to(&mut buf, image::ImageFormat::Jpeg)
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
    let tmp = std::env::temp_dir().join(format!("diskdex_raw_{}_{}.jpg", std::process::id(), n));
    let out = std::process::Command::new("sips")
        .args(["-Z", &max.to_string(), "-s", "format", "jpeg"])
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
    // 1) Cache (offline-friendly).
    {
        let guard = state.catalog.lock().unwrap();
        let cat = guard.as_ref().ok_or("no hay catálogo abierto")?;
        if let Some(bytes) = db::get_cached_thumbnail(&cat.conn, entry_id).map_err(|e| e.to_string())? {
            return Ok(img_data_url(&bytes));
        }
    }

    // 2) Generar on-demand desde el original (requiere disco montado) y cachear.
    let path = {
        let guard = state.catalog.lock().unwrap();
        let cat = guard.as_ref().ok_or("no hay catálogo abierto")?;
        resolve_real_path(&cat.conn, entry_id)?
    };
    let max = max.unwrap_or(THUMB_CACHE_MAX).clamp(32, 1024);

    // Video: extraer un frame con ffmpeg en el momento (rápido, seek por keyframe).
    // Probar ~1s y, si el clip es muy corto, caer a 0s. Se cachea el frame.
    let is_video = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| VIDEO_THUMB_EXTS.contains(&e.to_lowercase().as_str()))
        .unwrap_or(false);
    if is_video {
        let bytes = video::extract_frame(&path, 1.0, max)
            .or_else(|_| video::extract_frame(&path, 0.0, max))?;
        let guard = state.catalog.lock().unwrap();
        if let Some(cat) = guard.as_ref() {
            let _ = db::store_thumbnail(&cat.conn, entry_id, &bytes, max, 0);
        }
        return Ok(img_data_url(&bytes));
    }

    let (bytes, w, h) = render_image_thumb(&path, max)?;
    {
        let guard = state.catalog.lock().unwrap();
        if let Some(cat) = guard.as_ref() {
            let _ = db::store_thumbnail(&cat.conn, entry_id, &bytes, w, h);
        }
    }
    Ok(img_data_url(&bytes))
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

fn img_data_url(bytes: &[u8]) -> String {
    use base64::{engine::general_purpose::STANDARD, Engine};
    format!("data:image/jpeg;base64,{}", STANDARD.encode(bytes))
}

/// ¿Están disponibles ffprobe/ffmpeg? La UI lo usa para mostrar/ocultar features.
#[tauri::command(async)]
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
#[tauri::command(async)]
pub fn get_video_meta(
    state: tauri::State<'_, AppState>,
    entry_id: i64,
) -> Result<Option<db::VideoMetaRow>, String> {
    let guard = state.catalog.lock().unwrap();
    let cat = guard.as_ref().ok_or("no hay catálogo abierto")?;
    db::get_video_meta(&cat.conn, entry_id).map_err(|e| e.to_string())
}

/// Tira de frames cacheada de un video, como data URLs PNG.
#[tauri::command(async)]
pub fn get_video_frames(
    state: tauri::State<'_, AppState>,
    entry_id: i64,
) -> Result<Vec<String>, String> {
    let guard = state.catalog.lock().unwrap();
    let cat = guard.as_ref().ok_or("no hay catálogo abierto")?;
    let frames = db::get_video_frames(&cat.conn, entry_id).map_err(|e| e.to_string())?;
    Ok(frames.iter().map(|p| img_data_url(p)).collect())
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
#[tauri::command(async)]
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
#[tauri::command(async)]
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
#[tauri::command(async)]
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
#[tauri::command(async)]
pub fn get_entry_tags(
    state: tauri::State<'_, AppState>,
    entry_id: i64,
) -> Result<Vec<String>, String> {
    let guard = state.catalog.lock().unwrap();
    let cat = guard.as_ref().ok_or("no hay catálogo abierto")?;
    db::entry_tags(&cat.conn, entry_id).map_err(|e| e.to_string())
}

/// Todos los tags del catálogo con su conteo de uso.
#[tauri::command(async)]
pub fn list_tags(state: tauri::State<'_, AppState>) -> Result<Vec<db::TagStat>, String> {
    let guard = state.catalog.lock().unwrap();
    let cat = guard.as_ref().ok_or("no hay catálogo abierto")?;
    db::list_tags(&cat.conn).map_err(|e| e.to_string())
}

/// Limpieza: mueve el ORIGINAL a la papelera del sistema (requiere disco montado)
/// y elimina la entrada (y su subárbol, si es carpeta) del catálogo. Devuelve la
/// ruta movida.
#[tauri::command(async)]
pub fn move_to_trash(state: tauri::State<'_, AppState>, entry_id: i64) -> Result<String, String> {
    let path = {
        let guard = state.catalog.lock().unwrap();
        let cat = guard.as_ref().ok_or("no hay catálogo abierto")?;
        resolve_real_path(&cat.conn, entry_id)?
    };
    trash::delete(&path).map_err(|e| format!("no se pudo mover a la papelera: {e}"))?;
    let mut guard = state.catalog.lock().unwrap();
    if let Some(cat) = guard.as_mut() {
        let _ = db::delete_subtree(&mut cat.conn, entry_id);
    }
    Ok(path.to_string_lossy().to_string())
}

#[derive(Serialize)]
pub struct TrashFailure {
    pub id: i64,
    pub name: String,
    pub error: String,
}

#[derive(Serialize)]
pub struct TrashSummary {
    /// Cantidad de ítems efectivamente enviados a la papelera (o ya cubiertos por
    /// una carpeta padre borrada en el mismo lote).
    pub moved: i64,
    pub failed: Vec<TrashFailure>,
}

/// Limpieza en lote: mueve varios originales a la papelera y limpia el catálogo.
/// Procesa de menos a más profundo y saltea descendientes de carpetas ya borradas
/// en el mismo lote (evita errores de "ya no existe"). Tolerante a fallos parciales.
#[tauri::command(async)]
pub fn move_entries_to_trash(
    state: tauri::State<'_, AppState>,
    entry_ids: Vec<i64>,
) -> Result<TrashSummary, String> {
    // 1) Resolver nombres y rutas reales con un único lock de lectura.
    let mut resolved: Vec<(i64, String, Result<PathBuf, String>)> = Vec::new();
    {
        let guard = state.catalog.lock().unwrap();
        let cat = guard.as_ref().ok_or("no hay catálogo abierto")?;
        for id in entry_ids {
            let name = db::get_entry(&cat.conn, id)
                .ok()
                .flatten()
                .map(|e| e.name)
                .unwrap_or_else(|| format!("#{id}"));
            let path = resolve_real_path(&cat.conn, id);
            resolved.push((id, name, path));
        }
    }

    // 2) Más superficial primero, para que las carpetas se borren antes que sus hijos.
    resolved.sort_by_key(|(_, _, p)| p.as_ref().map(|p| p.components().count()).unwrap_or(usize::MAX));

    let mut moved = 0i64;
    let mut failed = Vec::new();
    let mut trashed_dirs: Vec<PathBuf> = Vec::new();
    let mut ok_ids: Vec<i64> = Vec::new();

    for (id, name, path) in resolved {
        let path = match path {
            Ok(p) => p,
            Err(e) => {
                failed.push(TrashFailure { id, name, error: e });
                continue;
            }
        };
        // ¿Descendiente de una carpeta ya enviada a la papelera? Ya se fue con el padre.
        if trashed_dirs.iter().any(|d| path.starts_with(d)) {
            ok_ids.push(id);
            moved += 1;
            continue;
        }
        let is_dir = path.is_dir();
        match trash::delete(&path) {
            Ok(()) => {
                if is_dir {
                    trashed_dirs.push(path.clone());
                }
                ok_ids.push(id);
                moved += 1;
            }
            Err(e) => failed.push(TrashFailure {
                id,
                name,
                error: format!("no se pudo mover a la papelera: {e}"),
            }),
        }
    }

    // 3) Limpiar el catálogo (subárbol por cada id exitoso) con un lock de escritura.
    {
        let mut guard = state.catalog.lock().unwrap();
        if let Some(cat) = guard.as_mut() {
            for id in ok_ids {
                let _ = db::delete_subtree(&mut cat.conn, id);
            }
        }
    }

    Ok(TrashSummary { moved, failed })
}

/// Quita un disco entero del catálogo (no toca el original en disco). Para
/// discos que ya no existen o que se quieren sacar del listado.
#[tauri::command(async)]
pub fn delete_disk(state: tauri::State<'_, AppState>, disk_id: i64) -> Result<(), String> {
    let mut guard = state.catalog.lock().unwrap();
    let cat = guard.as_mut().ok_or("no hay catálogo abierto")?;
    db::delete_disk(&mut cat.conn, disk_id).map_err(|e| e.to_string())
}

/// M3: búsqueda full-text por nombre sobre todo el catálogo.
#[tauri::command(async)]
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
    /// Carpetas reutilizadas sin descender el FS (re-escaneo incremental). 0 en
    /// un escaneo completo (disco nuevo o `force_full`).
    pub reused_dirs: u64,
}

/// M5: lista los volúmenes montados (para elegir cuál escanear / detectar nuevos).
#[tauri::command(async)]
pub fn list_volumes() -> Vec<VolumeInfo> {
    scan::list_volumes()
}

/// M5: escanea un volumen/carpeta montado y lo guarda como disco del catálogo.
/// Re-escanea (reemplaza) si ya existe un disco con el mismo fingerprint.
/// Requiere un catálogo abierto.
#[tauri::command(async)]
pub async fn scan_disk(
    app: tauri::AppHandle,
    mount_path: String,
    name: Option<String>,
    options: Option<ScanOptions>,
) -> Result<ScanSummary, String> {
    // El recorrido es 100% bloqueante (FS). Lo corremos en el pool de bloqueo
    // para que varios discos se escaneen EN PARALELO sin trabar el runtime ni la
    // UI (antes, como comando async con cuerpo bloqueante, podían serializarse).
    tauri::async_runtime::spawn_blocking(move || scan_disk_blocking(app, mount_path, name, options))
        .await
        .map_err(|e| format!("error en la tarea de escaneo: {e}"))?
}

fn scan_disk_blocking(
    app: tauri::AppHandle,
    mount_path: String,
    name: Option<String>,
    options: Option<ScanOptions>,
) -> Result<ScanSummary, String> {
    let state = app.state::<AppState>();
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
    let mut opts = options.unwrap_or_default();
    // "Excluir basura" (opt-in): suma la lista de basura conocida a las exclusiones.
    if opts.exclude_junk {
        opts.exclude_names.extend(scan::default_excludes());
    }

    // Fingerprint + capacidad/tipo desde el volumen (si coincide con un mount conocido).
    let fingerprint = scan::volume_fingerprint(&root);
    let (capacity, kind) = volume_caps(&mount_path);
    // Bytes usados del volumen (para estimar % de avance). 0 = desconocido
    // (p.ej. al escanear una subcarpeta, no un volumen completo).
    let used_bytes: u64 = scan::list_volumes()
        .into_iter()
        .find(|v| v.mount_path == mount_path)
        .map(|v| v.total_space.saturating_sub(v.available_space))
        .unwrap_or(0);

    // Ruta del catálogo abierto (la necesitamos antes para cargar el árbol viejo).
    let cat_path = {
        let guard = state.catalog.lock().unwrap();
        guard
            .as_ref()
            .ok_or("no hay catálogo abierto: creá o abrí uno antes de escanear")?
            .path
            .clone()
    };

    // Re-escaneo incremental: cargar el árbol catalogado (por fingerprint, o por
    // nombre si el disco no expone Volume UUID) para reutilizar subárboles cuyo
    // mtime no cambió. `force_full` lo desactiva (escaneo completo).
    let old_tree = if opts.force_full {
        None
    } else {
        db::open(&cat_path)
            .ok()
            .and_then(|c| db::load_disk_tree(&c, fingerprint.as_deref(), &volume_name).ok().flatten())
    };

    // Empezar "limpio": descartar cualquier cancelación vieja de este mount.
    clear_cancel(&mount_path);
    let (disk, reused_dirs) = {
        let app = app.clone();
        let mp = mount_path.clone();
        let cancel_mp = mount_path.clone();
        let mut on_progress = |count: u64, bytes: u64| {
            let pct = if used_bytes > 0 {
                ((bytes.min(used_bytes)) * 100 / used_bytes).min(99) as i32
            } else {
                -1
            };
            let _ = app.emit("scan-progress", ScanProgress { mount: mp.clone(), count, pct });
        };
        let cancel = || cancel_requested(&cancel_mp);
        let res = match &old_tree {
            Some(old) => scan::scan_volume_incremental(
                &root, &volume_name, &opts, &mut on_progress, &cancel, old,
            ),
            None => scan::scan_volume_cb(&root, &volume_name, &opts, &mut on_progress, &cancel)
                .map(|d| (d, 0)),
        };
        match res {
            Ok(d) => d,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {
                clear_cancel(&mount_path);
                // Avisar a la UI que el escaneo terminó (sin barra colgada).
                let _ = app.emit(
                    "scan-progress",
                    ScanProgress { mount: mount_path.clone(), count: 0, pct: 100 },
                );
                return Err("escaneo cancelado".into());
            }
            Err(e) => return Err(format!("error escaneando: {e}")),
        }
    };
    clear_cancel(&mount_path);
    // Fin del recorrido → fase de GUARDADO (pct = -2). Ingestar millones de filas
    // tarda y no emite progreso; este sentinel hace que la UI muestre "Guardando…"
    // en vez de un "Scanning" congelado que parece colgado.
    let _ = app.emit(
        "scan-progress",
        ScanProgress { mount: mount_path.clone(), count: disk.entries.len() as u64, pct: -2 },
    );

    // Ingesta en una conexión PROPIA (WAL): así insertar millones de filas NO
    // bloquea las lecturas de la UI (clickear discos/carpetas) ni a otros
    // escaneos. La conexión compartida queda libre; otro escritor espera por
    // busy_timeout en vez de fallar.
    let mut conn = db::open(&cat_path).map_err(|e| format!("error abriendo catálogo: {e}"))?;
    let ingest = db::ingest_scanned(
        &mut conn,
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
        reused_dirs,
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
#[tauri::command(async)]
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
                    // Traer la ventana al frente (aunque esté oculta en el tray)
                    // para que el popup "disco detectado" sea visible.
                    if let Some(w) = app.get_webview_window("main") {
                        let _ = w.show();
                        let _ = w.set_focus();
                    }
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
#[tauri::command(async)]
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
#[tauri::command(async)]
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

/// Detalle de un disco para el panel de info. Si el disco está montado ahora,
/// agrega el espacio total/libre en vivo del volumen (más preciso que lo cataloga).
#[tauri::command(async)]
pub fn disk_detail(state: tauri::State<'_, AppState>, disk_id: i64) -> Result<DiskDetail, String> {
    let guard = state.catalog.lock().unwrap();
    let cat = guard.as_ref().ok_or("no hay catálogo abierto")?;
    let (
        id,
        name,
        total_size,
        file_count,
        folder_count,
        is_online,
        kind,
        capacity,
        scanned_at,
        volume_uuid,
        mount_path,
    ) = cat
        .conn
        .query_row(
            "SELECT id, name, total_size, file_count, folder_count, is_online, kind, capacity, scanned_at, volume_uuid, mount_path \
             FROM disks WHERE id = ?1",
            rusqlite::params![disk_id],
            |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, i64>(3)?,
                    r.get::<_, i64>(4)?,
                    r.get::<_, i64>(5)? != 0,
                    r.get::<_, Option<String>>(6)?,
                    r.get::<_, Option<i64>>(7)?,
                    r.get::<_, Option<i64>>(8)?,
                    r.get::<_, Option<String>>(9)?,
                    r.get::<_, Option<String>>(10)?,
                ))
            },
        )
        .map_err(|e| e.to_string())?;

    // Espacio en vivo si el disco está montado: priorizar fingerprint, luego
    // mount_path, luego nombre del volumen.
    let (mut live_total, mut live_free) = (None, None);
    if is_online {
        let vols = scan::list_volumes();
        let matched = vols
            .iter()
            .find(|v| v.fingerprint.is_some() && v.fingerprint == volume_uuid)
            .or_else(|| {
                vols.iter()
                    .find(|v| mount_path.as_deref() == Some(v.mount_path.as_str()))
            })
            .or_else(|| vols.iter().find(|v| v.name == name));
        if let Some(v) = matched {
            live_total = Some(v.total_space as i64);
            live_free = Some(v.available_space as i64);
        }
    }

    Ok(DiskDetail {
        id,
        name,
        total_size,
        file_count,
        folder_count,
        is_online,
        kind,
        capacity,
        scanned_at,
        live_total,
        live_free,
    })
}
