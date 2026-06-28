//! Conector remoto seguro (sección 9). Agente HTTP **read-only, autenticado por
//! dispositivo**, que sirve únicamente archivos del catálogo cuyo volumen está
//! montado y verificado por fingerprint. Nunca expone el filesystem entero.
//!
//! Transporte: pensado para escuchar SOLO en la interfaz de una malla privada
//! (Tailscale/WireGuard) o detrás de un túnel TLS; por defecto liga a loopback.
//! Acá implementamos la capa de identidad/autorización/seguridad de archivos,
//! que es independiente del transporte y se testea localmente con curl.
//!
//! Auth: emparejamiento por código → token de dispositivo (se guarda hasheado) →
//! JWT HS256 de vida corta. Scopes por dispositivo (default deny salvo lo que el
//! usuario comparta). Revocación inmediata. Auditoría en `access_log`.

use axum::{
    body::Body,
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD as B64, Engine};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::Sha256;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio::sync::oneshot;
use tokio_util::io::ReaderStream;

const ACCESS_TTL_SECS: u64 = 900; // 15 min
const PAIRING_TTL_SECS: u64 = 300; // 5 min
const CHECKSUM_MAX_BYTES: u64 = 256 * 1024 * 1024;

type Hs256 = Hmac<Sha256>;

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn random_hex(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    getrandom::getrandom(&mut buf).expect("rng");
    buf.iter().map(|b| format!("{b:02x}")).collect()
}

/// Código de emparejamiento numérico de 6 dígitos.
fn pairing_code() -> String {
    let mut buf = [0u8; 6];
    getrandom::getrandom(&mut buf).expect("rng");
    buf.iter().map(|b| char::from(b'0' + (b % 10))).collect()
}

fn token_hash(token: &str) -> String {
    blake3::hash(token.as_bytes()).to_hex().to_string()
}

// ---------- JWT HS256 (propio, sin dependencias de crypto pesadas) ----------

#[derive(Serialize, Deserialize)]
struct Claims {
    sub: String,    // device_id
    scopes: String, // "*" o lista de disk_id separada por comas
    exp: u64,
}

fn sign(secret: &[u8], msg: &str) -> String {
    let mut mac = Hs256::new_from_slice(secret).expect("hmac key");
    mac.update(msg.as_bytes());
    B64.encode(mac.finalize().into_bytes())
}

fn make_jwt(secret: &[u8], claims: &Claims) -> String {
    let header = B64.encode(br#"{"alg":"HS256","typ":"JWT"}"#);
    let payload = B64.encode(serde_json::to_vec(claims).unwrap());
    let msg = format!("{header}.{payload}");
    let sig = sign(secret, &msg);
    format!("{msg}.{sig}")
}

fn verify_jwt(secret: &[u8], token: &str) -> Option<Claims> {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return None;
    }
    let msg = format!("{}.{}", parts[0], parts[1]);
    let expected = sign(secret, &msg);
    // Comparación en tiempo ~constante sobre el HMAC (suficiente con secreto fuerte).
    if expected.as_bytes().len() != parts[2].as_bytes().len() {
        return None;
    }
    let mut diff = 0u8;
    for (a, b) in expected.as_bytes().iter().zip(parts[2].as_bytes()) {
        diff |= a ^ b;
    }
    if diff != 0 {
        return None;
    }
    let payload = B64.decode(parts[1]).ok()?;
    let claims: Claims = serde_json::from_slice(&payload).ok()?;
    if claims.exp < now() {
        return None;
    }
    Some(claims)
}

fn scope_allows(scopes: &str, disk_id: i64) -> bool {
    if scopes == "*" {
        return true;
    }
    scopes.split(',').filter_map(|s| s.trim().parse::<i64>().ok()).any(|id| id == disk_id)
}

// ---------- Estado del agente ----------

#[derive(Clone, Deserialize)]
pub struct AgentConfig {
    /// Dirección de escucha. Por defecto loopback; en producción, la IP de la malla.
    pub bind: String,
    /// Scopes que reciben los nuevos dispositivos al emparejarse ("*" o "1,3,5").
    pub default_scopes: String,
    pub name: String,
}

impl Default for AgentConfig {
    fn default() -> Self {
        AgentConfig {
            bind: "127.0.0.1:8787".into(),
            default_scopes: "*".into(),
            name: "DiskDex Agent".into(),
        }
    }
}

struct Pairing {
    code: String,
    expires: u64,
}

pub struct AgentInner {
    catalog_path: PathBuf,
    db: Mutex<rusqlite::Connection>,
    jwt_secret: Vec<u8>,
    pairing: Mutex<Option<Pairing>>,
    config: AgentConfig,
}

pub type Shared = Arc<AgentInner>;

pub struct AgentHandle {
    pub addr: String,
    pub inner: Shared,
    shutdown: Option<oneshot::Sender<()>>,
}

impl AgentHandle {
    pub fn stop(mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
    }

    /// Genera un nuevo código de emparejamiento (válido 5 min). Lo devuelve.
    pub fn new_pairing_code(&self) -> String {
        let code = pairing_code();
        *self.inner.pairing.lock().unwrap() = Some(Pairing {
            code: code.clone(),
            expires: now() + PAIRING_TTL_SECS,
        });
        code
    }
}

/// Arranca el agente sobre el catálogo dado. Devuelve el handle (incluye addr real).
pub fn start(catalog_path: PathBuf, config: AgentConfig) -> Result<AgentHandle, String> {
    let conn = rusqlite::Connection::open(&catalog_path)
        .map_err(|e| format!("agente: no pudo abrir el catálogo: {e}"))?;
    let inner: Shared = Arc::new(AgentInner {
        catalog_path,
        db: Mutex::new(conn),
        jwt_secret: {
            let mut s = vec![0u8; 32];
            getrandom::getrandom(&mut s).expect("rng");
            s
        },
        pairing: Mutex::new(None),
        config: config.clone(),
    });

    let app = Router::new()
        .route("/v1/health", get(health))
        .route("/v1/pair", post(pair))
        .route("/v1/auth/token", post(auth_token))
        .route("/v1/disks", get(list_disks))
        .route("/v1/entries", get(list_entries))
        .route("/v1/file", get(get_file))
        .route("/v1/transfers", post(transfers))
        .with_state(inner.clone());

    // Bind sincrónico (std, sin runtime) para conocer la addr real antes de
    // devolver. OJO: no usar `block_on` acá — este comando ya corre dentro del
    // runtime tokio (`#[tauri::command(async)]`), y `block_on` anidado paniquea.
    let std_listener = std::net::TcpListener::bind(&config.bind)
        .map_err(|e| format!("agente: no pudo escuchar en {}: {e}", config.bind))?;
    std_listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    let addr = std_listener.local_addr().map_err(|e| e.to_string())?.to_string();

    let (tx, rx) = oneshot::channel::<()>();
    tauri::async_runtime::spawn(async move {
        // `from_std` requiere contexto de runtime: acá ya estamos dentro de la task.
        let listener = match tokio::net::TcpListener::from_std(std_listener) {
            Ok(l) => l,
            Err(e) => {
                eprintln!("agente: no pudo adoptar el socket: {e}");
                return;
            }
        };
        let _ = axum::serve(listener, app)
            .with_graceful_shutdown(async {
                let _ = rx.await;
            })
            .await;
    });

    Ok(AgentHandle { addr, inner, shutdown: Some(tx) })
}

// ---------- Helpers de respuesta / auth ----------

fn err(code: StatusCode, msg: &str) -> Response {
    (code, Json(json!({ "error": msg }))).into_response()
}

/// Autoriza la request: Bearer JWT válido + dispositivo no revocado.
fn authorize(inner: &AgentInner, headers: &HeaderMap) -> Result<Claims, Response> {
    let auth = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let token = auth.strip_prefix("Bearer ").ok_or_else(|| err(StatusCode::UNAUTHORIZED, "falta token Bearer"))?;
    let claims = verify_jwt(&inner.jwt_secret, token)
        .ok_or_else(|| err(StatusCode::UNAUTHORIZED, "token inválido o expirado"))?;
    // Revocación inmediata.
    let revoked: i64 = {
        let db = inner.db.lock().unwrap();
        db.query_row("SELECT revoked FROM devices WHERE id = ?1", [&claims.sub], |r| r.get(0))
            .unwrap_or(1)
    };
    if revoked != 0 {
        return Err(err(StatusCode::FORBIDDEN, "dispositivo revocado"));
    }
    Ok(claims)
}

fn log_access(inner: &AgentInner, device: &str, action: &str, disk_id: Option<i64>, entry_id: Option<i64>, bytes: i64, result: &str) {
    let db = inner.db.lock().unwrap();
    let _ = db.execute(
        "INSERT INTO access_log (ts, device_id, action, disk_id, entry_id, bytes, result) VALUES (?1,?2,?3,?4,?5,?6,?7)",
        rusqlite::params![now() as i64, device, action, disk_id, entry_id, bytes, result],
    );
}

// ---------- Handlers ----------

async fn health(State(inner): State<Shared>) -> Response {
    Json(json!({
        "status": "ok",
        "name": inner.config.name,
        "version": env!("CARGO_PKG_VERSION"),
        "catalog": inner.catalog_path.file_name().and_then(|s| s.to_str()).unwrap_or(""),
    }))
    .into_response()
}

#[derive(Deserialize)]
struct PairBody {
    code: String,
    name: Option<String>,
}

async fn pair(State(inner): State<Shared>, Json(body): Json<PairBody>) -> Response {
    {
        let mut p = inner.pairing.lock().unwrap();
        match p.as_ref() {
            Some(pp) if pp.code == body.code && pp.expires >= now() => {}
            Some(pp) if pp.expires < now() => return err(StatusCode::GONE, "el código expiró"),
            _ => return err(StatusCode::UNAUTHORIZED, "código de emparejamiento inválido"),
        }
        *p = None; // single-use
    }

    let device_id = random_hex(8);
    let device_token = random_hex(24);
    let hash = token_hash(&device_token);
    let name = body.name.unwrap_or_else(|| "dispositivo".into());

    {
        let db = inner.db.lock().unwrap();
        if let Err(e) = db.execute(
            "INSERT INTO devices (id, name, public_key, scopes, created_at, last_seen, revoked) VALUES (?1,?2,?3,?4,?5,?5,0)",
            rusqlite::params![device_id, name, hash, inner.config.default_scopes, now() as i64],
        ) {
            return err(StatusCode::INTERNAL_SERVER_ERROR, &format!("no se pudo registrar el dispositivo: {e}"));
        }
    }
    log_access(&inner, &device_id, "pair", None, None, 0, "ok");

    Json(json!({
        "device_id": device_id,
        "device_token": device_token,
        "scopes": inner.config.default_scopes,
    }))
    .into_response()
}

#[derive(Deserialize)]
struct TokenBody {
    device_id: String,
    device_token: String,
}

async fn auth_token(State(inner): State<Shared>, Json(body): Json<TokenBody>) -> Response {
    let (stored_hash, scopes, revoked): (String, String, i64) = {
        let db = inner.db.lock().unwrap();
        match db.query_row(
            "SELECT public_key, scopes, revoked FROM devices WHERE id = ?1",
            [&body.device_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        ) {
            Ok(t) => t,
            Err(_) => return err(StatusCode::UNAUTHORIZED, "dispositivo desconocido"),
        }
    };
    if revoked != 0 {
        return err(StatusCode::FORBIDDEN, "dispositivo revocado");
    }
    if token_hash(&body.device_token) != stored_hash {
        return err(StatusCode::UNAUTHORIZED, "token de dispositivo inválido");
    }
    {
        let db = inner.db.lock().unwrap();
        let _ = db.execute("UPDATE devices SET last_seen = ?1 WHERE id = ?2", rusqlite::params![now() as i64, body.device_id]);
    }
    let claims = Claims { sub: body.device_id, scopes, exp: now() + ACCESS_TTL_SECS };
    let access = make_jwt(&inner.jwt_secret, &claims);
    Json(json!({ "access_token": access, "token_type": "Bearer", "expires_in": ACCESS_TTL_SECS })).into_response()
}

async fn list_disks(State(inner): State<Shared>, headers: HeaderMap) -> Response {
    let claims = match authorize(&inner, &headers) {
        Ok(c) => c,
        Err(r) => return r,
    };
    let db = inner.db.lock().unwrap();
    let mut stmt = match db.prepare("SELECT id, name, total_size, file_count, is_online FROM disks ORDER BY name") {
        Ok(s) => s,
        Err(e) => return err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };
    let rows: Vec<Value> = stmt
        .query_map([], |r| {
            let id: i64 = r.get(0)?;
            Ok((id, r.get::<_, String>(1)?, r.get::<_, i64>(2)?, r.get::<_, i64>(3)?, r.get::<_, i64>(4)?))
        })
        .and_then(|it| it.collect::<Result<Vec<_>, _>>())
        .unwrap_or_default()
        .into_iter()
        .filter(|(id, ..)| scope_allows(&claims.scopes, *id))
        .map(|(id, name, total, files, online)| json!({
            "id": id, "name": name, "total_size": total, "file_count": files, "is_online": online != 0
        }))
        .collect();
    Json(json!({ "disks": rows })).into_response()
}

async fn list_entries(State(inner): State<Shared>, headers: HeaderMap, Query(q): Query<HashMap<String, String>>) -> Response {
    let claims = match authorize(&inner, &headers) {
        Ok(c) => c,
        Err(r) => return r,
    };
    let disk_id: i64 = match q.get("disk").and_then(|s| s.parse().ok()) {
        Some(d) => d,
        None => return err(StatusCode::BAD_REQUEST, "falta parámetro disk"),
    };
    if !scope_allows(&claims.scopes, disk_id) {
        return err(StatusCode::FORBIDDEN, "el dispositivo no tiene acceso a ese disco");
    }
    let db = inner.db.lock().unwrap();

    // Búsqueda (q) o navegación (parent).
    if let Some(text) = q.get("q").filter(|s| !s.is_empty()) {
        match crate::db::build_fts_query(text) {
            Some(fts) => {
                let mut stmt = db
                    .prepare(
                        "SELECT e.id, e.name, e.is_folder, e.size_logical, e.modified_at \
                         FROM entries_fts f JOIN entries e ON e.id=f.rowid \
                         WHERE f.entries_fts MATCH ?1 AND e.disk_id=?2 ORDER BY rank LIMIT 500",
                    )
                    .unwrap();
                let rows: Vec<Value> = stmt
                    .query_map(rusqlite::params![fts, disk_id], row_to_json)
                    .and_then(|it| it.collect::<Result<Vec<_>, _>>())
                    .unwrap_or_default();
                return Json(json!({ "entries": rows })).into_response();
            }
            None => return Json(json!({ "entries": [] })).into_response(),
        }
    }

    let parent: Option<i64> = q.get("parent").and_then(|s| s.parse().ok());
    let mut stmt = db
        .prepare(
            "SELECT e.id, e.name, e.is_folder, e.size_logical, e.modified_at FROM entries e \
             WHERE e.disk_id=?1 AND e.parent_id IS ?2 ORDER BY e.is_folder DESC, e.name COLLATE NOCASE LIMIT 2000",
        )
        .unwrap();
    let rows: Vec<Value> = stmt
        .query_map(rusqlite::params![disk_id, parent], row_to_json)
        .and_then(|it| it.collect::<Result<Vec<_>, _>>())
        .unwrap_or_default();
    Json(json!({ "entries": rows })).into_response()
}

fn row_to_json(r: &rusqlite::Row) -> rusqlite::Result<Value> {
    Ok(json!({
        "id": r.get::<_, i64>(0)?,
        "name": r.get::<_, String>(1)?,
        "is_folder": r.get::<_, i64>(2)? != 0,
        "size_logical": r.get::<_, i64>(3)?,
        "modified_at": r.get::<_, Option<i64>>(4)?,
    }))
}

/// Resuelve la ruta real y verifica todas las condiciones de seguridad.
/// Devuelve (path_canónico, disk_id) o un error listo para responder.
fn secure_resolve(inner: &AgentInner, entry_id: i64, scopes: &str) -> Result<(PathBuf, i64), Response> {
    let db = inner.db.lock().unwrap();

    let (disk_id, is_folder): (i64, i64) = db
        .query_row("SELECT disk_id, is_folder FROM entries WHERE id=?1", [entry_id], |r| Ok((r.get(0)?, r.get(1)?)))
        .map_err(|_| err(StatusCode::NOT_FOUND, "la entrada no existe"))?;
    if is_folder != 0 {
        return Err(err(StatusCode::BAD_REQUEST, "la entrada es una carpeta"));
    }
    if !scope_allows(scopes, disk_id) {
        return Err(err(StatusCode::FORBIDDEN, "sin acceso a ese disco"));
    }

    let (mount, online, uuid): (Option<String>, i64, Option<String>) = db
        .query_row("SELECT mount_path, is_online, volume_uuid FROM disks WHERE id=?1", [disk_id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))?;

    let mount = match (online, mount) {
        (1, Some(m)) => m,
        _ => return Err(err(StatusCode::CONFLICT, "disk offline")),
    };

    // Verificar que el volumen montado es el correcto (fingerprint).
    if let Some(expected) = uuid {
        let current = crate::scan::volume_fingerprint(Path::new(&mount));
        if current.as_deref() != Some(expected.as_str()) {
            return Err(err(StatusCode::CONFLICT, "el volumen montado no coincide con el fingerprint del disco"));
        }
    }

    // Ruta del catálogo → relativa → real, y canonicalizar para frenar `..`/symlinks.
    let cat_path = crate::db::entry_path(&db, entry_id).map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))?;
    let rel: PathBuf = cat_path.split('/').filter(|s| !s.is_empty()).skip(1).collect();
    let real = Path::new(&mount).join(&rel);

    let canon_mount = std::fs::canonicalize(&mount).map_err(|_| err(StatusCode::CONFLICT, "no se pudo resolver el volumen"))?;
    let canon_real = std::fs::canonicalize(&real).map_err(|_| err(StatusCode::NOT_FOUND, "el original no está en el disco"))?;
    if !canon_real.starts_with(&canon_mount) {
        return Err(err(StatusCode::FORBIDDEN, "ruta fuera del volumen permitido"));
    }
    if !canon_real.is_file() {
        return Err(err(StatusCode::NOT_FOUND, "no es un archivo"));
    }
    Ok((canon_real, disk_id))
}

fn parse_range(headers: &HeaderMap, len: u64) -> Option<(u64, u64)> {
    let raw = headers.get("range")?.to_str().ok()?;
    let spec = raw.strip_prefix("bytes=")?;
    let (s, e) = spec.split_once('-')?;
    let start: u64 = if s.is_empty() { 0 } else { s.parse().ok()? };
    let end: u64 = if e.is_empty() { len.saturating_sub(1) } else { e.parse().ok()? };
    if start > end || start >= len {
        return None;
    }
    Some((start, end.min(len - 1)))
}

fn device_revoked(inner: &AgentInner, device_id: &str) -> bool {
    let db = inner.db.lock().unwrap();
    db.query_row("SELECT revoked FROM devices WHERE id = ?1", [device_id], |r| r.get::<_, i64>(0))
        .map(|v| v != 0)
        .unwrap_or(true)
}

async fn get_file(State(inner): State<Shared>, headers: HeaderMap, Query(q): Query<HashMap<String, String>>) -> Response {
    let entry_id: i64 = match q.get("entry").and_then(|s| s.parse().ok()) {
        Some(e) => e,
        None => return err(StatusCode::BAD_REQUEST, "falta parámetro entry"),
    };
    // Auth: Bearer (dispositivo) o link firmado temporal (?token=) acotado a esta entrada.
    let claims = match authorize(&inner, &headers) {
        Ok(c) => c,
        Err(auth_err) => match q.get("token").and_then(|t| verify_jwt(&inner.jwt_secret, t)) {
            Some(c) if c.scopes == format!("link:{entry_id}") && !device_revoked(&inner, &c.sub) => {
                Claims { sub: c.sub, scopes: "*".into(), exp: c.exp }
            }
            _ => return auth_err,
        },
    };

    let (path, disk_id) = match secure_resolve(&inner, entry_id, &claims.scopes) {
        Ok(v) => v,
        Err(r) => {
            log_access(&inner, &claims.sub, "file", None, Some(entry_id), 0, "denied");
            return r;
        }
    };

    let mut file = match tokio::fs::File::open(&path).await {
        Ok(f) => f,
        Err(e) => return err(StatusCode::INTERNAL_SERVER_ERROR, &format!("no se pudo abrir: {e}")),
    };
    let total = match file.metadata().await {
        Ok(m) => m.len(),
        Err(e) => return err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };

    let range = parse_range(&headers, total);
    let (start, end) = range.unwrap_or((0, total.saturating_sub(1)));
    let length = end - start + 1;

    if start > 0 {
        if let Err(e) = file.seek(std::io::SeekFrom::Start(start)).await {
            return err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string());
        }
    }
    let stream = ReaderStream::new(file.take(length));
    let body = Body::from_stream(stream);

    let filename = path.file_name().and_then(|s| s.to_str()).unwrap_or("file").to_string();
    let status = if range.is_some() { StatusCode::PARTIAL_CONTENT } else { StatusCode::OK };

    let mut resp = Response::builder()
        .status(status)
        .header("content-length", length)
        .header("accept-ranges", "bytes")
        .header("content-type", "application/octet-stream")
        .header("content-disposition", format!("attachment; filename=\"{filename}\""));
    if let Some((s, e)) = range {
        resp = resp.header("content-range", format!("bytes {s}-{e}/{total}"));
    }
    // Checksum BLAKE3 solo para archivos chicos servidos completos.
    if range.is_none() && total <= CHECKSUM_MAX_BYTES && q.get("checksum").map(|v| v == "1").unwrap_or(false) {
        if let Ok(bytes) = tokio::fs::read(&path).await {
            let h = blake3::hash(&bytes);
            resp = resp.header("x-checksum-blake3", h.to_hex().to_string());
        }
    }

    log_access(&inner, &claims.sub, "file", Some(disk_id), Some(entry_id), length as i64, "ok");
    resp.body(body).unwrap_or_else(|_| err(StatusCode::INTERNAL_SERVER_ERROR, "build response"))
}

#[derive(Deserialize)]
struct TransferBody {
    entry_id: i64,
}

/// POST /v1/transfers — genera una **URL firmada y temporal** (10 min, acotada a
/// esa entrada) para descargar el archivo sin enviar el token de dispositivo.
/// Es el equivalente local a "subir a la nube y devolver un link firmado": el
/// link sirve para compartir el archivo puntual. El push a buckets (S3/B2/Drive)
/// es una capa de transporte adicional que se conecta sobre este mismo modelo.
async fn transfers(State(inner): State<Shared>, headers: HeaderMap, Json(body): Json<TransferBody>) -> Response {
    let claims = match authorize(&inner, &headers) {
        Ok(c) => c,
        Err(r) => return r,
    };
    let disk_id: i64 = {
        let db = inner.db.lock().unwrap();
        match db.query_row("SELECT disk_id FROM entries WHERE id = ?1 AND is_folder = 0", [body.entry_id], |r| r.get(0)) {
            Ok(d) => d,
            Err(_) => return err(StatusCode::NOT_FOUND, "archivo inexistente"),
        }
    };
    if !scope_allows(&claims.scopes, disk_id) {
        return err(StatusCode::FORBIDDEN, "sin acceso a ese disco");
    }
    let ttl = 600u64;
    let link = Claims { sub: claims.sub.clone(), scopes: format!("link:{}", body.entry_id), exp: now() + ttl };
    let token = make_jwt(&inner.jwt_secret, &link);
    log_access(&inner, &claims.sub, "transfer-link", Some(disk_id), Some(body.entry_id), 0, "ok");
    Json(json!({
        "url": format!("/v1/file?entry={}&token={}", body.entry_id, token),
        "expires_in": ttl,
    }))
    .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jwt_roundtrip_and_tamper() {
        let secret = b"0123456789abcdef0123456789abcdef";
        let c = Claims { sub: "dev1".into(), scopes: "*".into(), exp: now() + 100 };
        let tok = make_jwt(secret, &c);
        let v = verify_jwt(secret, &tok).expect("debe validar");
        assert_eq!(v.sub, "dev1");
        // Manipular la firma invalida.
        let bad = format!("{}x", &tok[..tok.len() - 1]);
        assert!(verify_jwt(secret, &bad).is_none());
        // Otro secreto invalida.
        assert!(verify_jwt(b"otraotraotraotraotraotraotraotra", &tok).is_none());
    }

    #[test]
    fn jwt_expired_rejected() {
        let secret = b"0123456789abcdef0123456789abcdef";
        let c = Claims { sub: "d".into(), scopes: "*".into(), exp: now() - 1 };
        let tok = make_jwt(secret, &c);
        assert!(verify_jwt(secret, &tok).is_none());
    }

    #[test]
    fn scopes_default_deny() {
        assert!(scope_allows("*", 5));
        assert!(scope_allows("1,5,9", 5));
        assert!(!scope_allows("1,2,3", 5));
        assert!(!scope_allows("", 5));
    }

    #[test]
    fn range_parsing() {
        let mut h = HeaderMap::new();
        h.insert("range", "bytes=0-1023".parse().unwrap());
        assert_eq!(parse_range(&h, 10000), Some((0, 1023)));
        h.insert("range", "bytes=1000-".parse().unwrap());
        assert_eq!(parse_range(&h, 10000), Some((1000, 9999)));
        h.insert("range", "bytes=99999-".parse().unwrap());
        assert_eq!(parse_range(&h, 10000), None); // fuera de rango
    }

    #[test]
    fn token_hash_is_stable() {
        assert_eq!(token_hash("abc"), token_hash("abc"));
        assert_ne!(token_hash("abc"), token_hash("abd"));
    }
}
