//! Extracción de metadata y frames de video (Fase B) vía `ffprobe`/`ffmpeg`.
//!
//! Se usa el binario del sistema si está disponible (en el release se puede
//! empaquetar como sidecar de Tauri). Si no está, las funciones devuelven error
//! y la app degrada con elegancia (cataloga el video como archivo opaco).

use serde::Serialize;
use std::path::Path;
use std::process::Command;

/// Metadata técnica de un clip.
#[derive(Debug, Clone, Serialize, Default, PartialEq)]
pub struct VideoMeta {
    pub duration_ms: i64,
    pub width: i64,
    pub height: i64,
    pub fps: f64,
    pub vcodec: Option<String>,
    pub acodec: Option<String>,
    pub bitrate: i64,
}

/// ¿Están disponibles ffprobe y ffmpeg?
pub fn tools_available() -> bool {
    ffprobe_ok() && which("ffmpeg").is_some()
}

fn ffprobe_ok() -> bool {
    which("ffprobe").is_some()
}

/// Resuelve `ffmpeg`/`ffprobe`. Prioridad:
///   1) sidecar empaquetado junto al ejecutable (app instalada, autocontenida),
///   2) el PATH del sistema,
///   3) rutas habituales (Homebrew / Linux).
/// Así el `.app` distribuido no depende de un ffmpeg instalado, pero en `dev`
/// (donde no hay sidecar al lado del binario de cargo) sigue usando el del sistema.
fn which(bin: &str) -> Option<String> {
    // 1) Sidecar junto al ejecutable. Tauri copia los `externalBin` al mismo
    //    directorio que el binario principal, sin el sufijo de target-triple
    //    (en Windows con `.exe`).
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            for name in [bin.to_string(), format!("{bin}.exe")] {
                let cand = dir.join(&name);
                if cand.exists() {
                    return Some(cand.to_string_lossy().into_owned());
                }
            }
        }
    }
    // 2) En el PATH: si `-version` corre, está disponible.
    if Command::new(bin).arg("-version").output().map(|o| o.status.success()).unwrap_or(false) {
        return Some(bin.to_string());
    }
    // 3) Rutas habituales (Homebrew Intel/ARM, Linux).
    for base in ["/usr/local/bin", "/opt/homebrew/bin", "/usr/bin"] {
        let p = format!("{base}/{bin}");
        if Path::new(&p).exists() {
            return Some(p);
        }
    }
    None
}

/// Parsea "30000/1001" o "25/1" → fps.
fn parse_rate(s: &str) -> f64 {
    if let Some((n, d)) = s.split_once('/') {
        let n: f64 = n.parse().unwrap_or(0.0);
        let d: f64 = d.parse().unwrap_or(0.0);
        if d != 0.0 {
            return n / d;
        }
    }
    s.parse().unwrap_or(0.0)
}

/// Extrae metadata con `ffprobe -print_format json`.
pub fn probe_video(path: &Path) -> Result<VideoMeta, String> {
    let bin = which("ffprobe").ok_or("ffprobe no está disponible")?;
    let out = Command::new(bin)
        .args(["-v", "quiet", "-print_format", "json", "-show_format", "-show_streams"])
        .arg(path)
        .output()
        .map_err(|e| format!("no se pudo ejecutar ffprobe: {e}"))?;
    if !out.status.success() {
        return Err("ffprobe falló al leer el archivo".into());
    }
    let json: serde_json::Value =
        serde_json::from_slice(&out.stdout).map_err(|e| format!("salida de ffprobe inválida: {e}"))?;

    let mut meta = VideoMeta::default();

    if let Some(fmt) = json.get("format") {
        if let Some(d) = fmt.get("duration").and_then(|v| v.as_str()) {
            meta.duration_ms = (d.parse::<f64>().unwrap_or(0.0) * 1000.0) as i64;
        }
        if let Some(b) = fmt.get("bit_rate").and_then(|v| v.as_str()) {
            meta.bitrate = b.parse().unwrap_or(0);
        }
    }

    if let Some(streams) = json.get("streams").and_then(|v| v.as_array()) {
        for s in streams {
            let kind = s.get("codec_type").and_then(|v| v.as_str()).unwrap_or("");
            let codec = s.get("codec_name").and_then(|v| v.as_str()).map(|x| x.to_string());
            match kind {
                "video" if meta.vcodec.is_none() => {
                    meta.vcodec = codec;
                    meta.width = s.get("width").and_then(|v| v.as_i64()).unwrap_or(0);
                    meta.height = s.get("height").and_then(|v| v.as_i64()).unwrap_or(0);
                    if let Some(r) = s.get("avg_frame_rate").and_then(|v| v.as_str()) {
                        meta.fps = parse_rate(r);
                    }
                    if meta.fps == 0.0 {
                        if let Some(r) = s.get("r_frame_rate").and_then(|v| v.as_str()) {
                            meta.fps = parse_rate(r);
                        }
                    }
                }
                "audio" if meta.acodec.is_none() => meta.acodec = codec,
                _ => {}
            }
        }
    }

    if meta.width == 0 && meta.vcodec.is_none() {
        return Err("el archivo no contiene un stream de video reconocible".into());
    }
    Ok(meta)
}

// ───────────────────────── Metadata de ubicación / cámara (A2-meta) ─────────────────────────

/// Extensiones de video que vale la pena sondear con ffprobe para GPS/cámara/fecha.
const LOCATION_VIDEO_EXTS: &[&str] = &[
    "mp4", "mov", "m4v", "avi", "mkv", "mxf", "mts", "m2ts", "wmv", "webm", "mpg", "mpeg", "3gp", "insv",
];

/// ¿La extensión (sin punto, cualquier caja) es un video que sondeamos por ubicación?
pub fn is_location_video_ext(ext: &str) -> bool {
    LOCATION_VIDEO_EXTS.contains(&ext.to_lowercase().as_str())
}

/// Metadata de cámara/ubicación extraída de un clip (A2-meta).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LocationMeta {
    pub gps_lat: Option<f64>,
    pub gps_lon: Option<f64>,
    pub captured_at: Option<i64>,
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
}

impl LocationMeta {
    pub fn is_empty(&self) -> bool {
        self.gps_lat.is_none()
            && self.gps_lon.is_none()
            && self.captured_at.is_none()
            && self.camera_make.is_none()
            && self.camera_model.is_none()
    }
}

/// Parsea ISO 6709 (`+34.0522-118.2437/`, con o sin altitud) → (lat, lon).
/// Sony/Canon/QuickTime guardan la ubicación así en el tag de formato.
fn parse_iso6709(s: &str) -> Option<(f64, f64)> {
    let s = s.trim().trim_end_matches('/');
    // Posiciones de los signos (+/-) que delimitan los campos lat/lon/alt.
    let signs: Vec<usize> = s
        .char_indices()
        .filter(|(_, c)| *c == '+' || *c == '-')
        .map(|(i, _)| i)
        .collect();
    if signs.len() < 2 {
        return None;
    }
    let lat: f64 = s[signs[0]..signs[1]].parse().ok()?;
    let lon_end = if signs.len() >= 3 { signs[2] } else { s.len() };
    let lon: f64 = s[signs[1]..lon_end].parse().ok()?;
    if !(-90.0..=90.0).contains(&lat) || !(-180.0..=180.0).contains(&lon) {
        return None;
    }
    Some((lat, lon))
}

/// Días civiles desde 1970-01-01 (algoritmo de Howard Hinnant).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = (if y >= 0 { y } else { y - 399 }) / 400;
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

/// Parsea un timestamp tipo `2023-06-01T12:34:56.000000Z` → unix secs (asume UTC).
/// Ignora la fracción y el offset (suele venir `Z`). Devuelve None si no matchea.
fn parse_iso8601_to_unix(s: &str) -> Option<i64> {
    let s = s.trim();
    let (date, time) = s.split_once(['T', ' '])?;
    let mut dp = date.split('-');
    let y: i64 = dp.next()?.parse().ok()?;
    let mo: i64 = dp.next()?.parse().ok()?;
    let d: i64 = dp.next()?.parse().ok()?;
    // Recortar fracción/zona del tiempo (quedarnos con HH:MM:SS).
    let time = &time[..time.len().min(8)];
    let mut tp = time.split(':');
    let hh: i64 = tp.next()?.parse().ok()?;
    let mm: i64 = tp.next()?.parse().ok()?;
    let ss: i64 = tp.next().unwrap_or("0").parse().unwrap_or(0);
    if mo < 1 || mo > 12 || d < 1 || d > 31 {
        return None;
    }
    Some(days_from_civil(y, mo, d) * 86400 + hh * 3600 + mm * 60 + ss)
}

/// Busca en un objeto de tags JSON el primer valor cuya clave (en minúsculas)
/// contenga alguno de los `needles`. ffprobe normaliza muchos tags de QuickTime.
fn tag_lookup<'a>(tags: &'a serde_json::Value, needles: &[&str]) -> Option<&'a str> {
    let obj = tags.as_object()?;
    for (k, v) in obj {
        let kl = k.to_lowercase();
        if needles.iter().any(|n| kl.contains(n)) {
            if let Some(s) = v.as_str() {
                if !s.is_empty() {
                    return Some(s);
                }
            }
        }
    }
    None
}

/// Extrae GPS / cámara / fecha de captura de un clip con `ffprobe` (tags de
/// `format` y de streams). Degrada con elegancia: si ffprobe no está o el clip no
/// trae estos tags, devuelve un `LocationMeta` vacío sin error fatal.
pub fn probe_location(path: &Path) -> Result<LocationMeta, String> {
    let bin = which("ffprobe").ok_or("ffprobe no está disponible")?;
    let out = Command::new(bin)
        .args(["-v", "quiet", "-print_format", "json", "-show_format", "-show_streams"])
        .arg(path)
        .output()
        .map_err(|e| format!("no se pudo ejecutar ffprobe: {e}"))?;
    if !out.status.success() {
        return Err("ffprobe falló al leer el archivo".into());
    }
    let json: serde_json::Value =
        serde_json::from_slice(&out.stdout).map_err(|e| format!("salida de ffprobe inválida: {e}"))?;

    // Recolectar todos los objetos de tags (format + cada stream).
    let mut tag_objs: Vec<&serde_json::Value> = Vec::new();
    if let Some(t) = json.get("format").and_then(|f| f.get("tags")) {
        tag_objs.push(t);
    }
    if let Some(streams) = json.get("streams").and_then(|v| v.as_array()) {
        for s in streams {
            if let Some(t) = s.get("tags") {
                tag_objs.push(t);
            }
        }
    }

    let mut meta = LocationMeta::default();
    for tags in &tag_objs {
        if meta.gps_lat.is_none() {
            if let Some(loc) = tag_lookup(tags, &["location", "iso6709", "gps"]) {
                if let Some((lat, lon)) = parse_iso6709(loc) {
                    meta.gps_lat = Some(lat);
                    meta.gps_lon = Some(lon);
                }
            }
        }
        if meta.camera_make.is_none() {
            if let Some(m) = tag_lookup(tags, &["make", "manufacturer"]) {
                meta.camera_make = Some(m.to_string());
            }
        }
        if meta.camera_model.is_none() {
            if let Some(m) = tag_lookup(tags, &["model"]) {
                meta.camera_model = Some(m.to_string());
            }
        }
        if meta.captured_at.is_none() {
            if let Some(c) = tag_lookup(tags, &["creation_time", "creationdate", "date"]) {
                meta.captured_at = parse_iso8601_to_unix(c);
            }
        }
    }
    Ok(meta)
}

/// Extrae un frame JPEG en el segundo `at_secs`, escalado a `max_w` de ancho.
pub fn extract_frame(path: &Path, at_secs: f64, max_w: u32) -> Result<Vec<u8>, String> {
    let bin = which("ffmpeg").ok_or("ffmpeg no está disponible")?;
    // `-ss` antes de `-i` = seek rápido por keyframe (suficiente para thumbnails).
    let out = Command::new(bin)
        .args(["-v", "quiet", "-ss"])
        .arg(format!("{at_secs:.3}"))
        .arg("-i")
        .arg(path)
        .args(["-frames:v", "1", "-vf"])
        .arg(format!("scale={max_w}:-2"))
        .args(["-f", "image2pipe", "-vcodec", "mjpeg", "-q:v", "4", "-"])
        .output()
        .map_err(|e| format!("no se pudo ejecutar ffmpeg: {e}"))?;
    if !out.status.success() || out.stdout.is_empty() {
        return Err("ffmpeg no pudo extraer el frame".into());
    }
    Ok(out.stdout)
}

/// Timestamps (segundos) de una tira de `n` frames repartidos en la duración.
/// Evita el segundo 0 (suele ser negro) y el final exacto.
pub fn strip_timestamps(duration_ms: i64, n: usize) -> Vec<f64> {
    if duration_ms <= 0 || n == 0 {
        return vec![0.0];
    }
    let dur = duration_ms as f64 / 1000.0;
    (0..n)
        .map(|i| dur * (i as f64 + 1.0) / (n as f64 + 1.0))
        .collect()
}

/// Detección de cambios de escena (on-demand; costosa). Devuelve los segundos
/// donde la métrica `scene` supera `threshold` (0.0–1.0). Parsea el `showinfo`.
pub fn detect_scenes(path: &Path, threshold: f64, max: usize) -> Result<Vec<f64>, String> {
    let bin = which("ffmpeg").ok_or("ffmpeg no está disponible")?;
    let out = Command::new(bin)
        .args(["-v", "info", "-i"])
        .arg(path)
        .args([
            "-vf",
            &format!("select='gt(scene,{threshold})',showinfo"),
            "-f",
            "null",
            "-",
        ])
        .output()
        .map_err(|e| format!("no se pudo ejecutar ffmpeg: {e}"))?;
    // `showinfo` escribe en stderr: líneas con "pts_time:NN.NN".
    let log = String::from_utf8_lossy(&out.stderr);
    let mut times = Vec::new();
    for line in log.lines() {
        if let Some(idx) = line.find("pts_time:") {
            let rest = &line[idx + "pts_time:".len()..];
            let num: String = rest.chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
            if let Ok(t) = num.parse::<f64>() {
                times.push(t);
                if times.len() >= max {
                    break;
                }
            }
        }
    }
    Ok(times)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_frame_rate() {
        assert!((parse_rate("30000/1001") - 29.97).abs() < 0.01);
        assert!((parse_rate("25/1") - 25.0).abs() < 0.001);
        assert_eq!(parse_rate("0/0"), 0.0);
        assert!((parse_rate("24") - 24.0).abs() < 0.001);
    }

    #[test]
    fn strip_spreads_frames() {
        let ts = strip_timestamps(10_000, 4); // 10s, 4 frames
        assert_eq!(ts.len(), 4);
        // Repartidos a 2,4,6,8 s aprox.
        assert!((ts[0] - 2.0).abs() < 0.01);
        assert!((ts[3] - 8.0).abs() < 0.01);
        // Nunca arranca en 0 ni termina en el final exacto.
        assert!(ts[0] > 0.0 && *ts.last().unwrap() < 10.0);
    }

    #[test]
    fn strip_handles_zero_duration() {
        assert_eq!(strip_timestamps(0, 5), vec![0.0]);
    }

    #[test]
    fn parses_iso6709() {
        let (lat, lon) = parse_iso6709("+34.0522-118.2437/").unwrap();
        assert!((lat - 34.0522).abs() < 1e-6);
        assert!((lon + 118.2437).abs() < 1e-6);
        // Con altitud.
        let (lat, lon) = parse_iso6709("+27.5916+086.5640+8850/").unwrap();
        assert!((lat - 27.5916).abs() < 1e-6);
        assert!((lon - 86.5640).abs() < 1e-6);
        // Basura / fuera de rango → None.
        assert!(parse_iso6709("nope").is_none());
        assert!(parse_iso6709("+999.0+000.0/").is_none());
    }

    #[test]
    fn parses_iso8601() {
        // 2023-06-01T00:00:00Z = 1685577600 (UTC).
        assert_eq!(parse_iso8601_to_unix("2023-06-01T00:00:00.000000Z"), Some(1_685_577_600));
        assert_eq!(parse_iso8601_to_unix("1970-01-01T00:00:00Z"), Some(0));
        assert!(parse_iso8601_to_unix("not-a-date").is_none());
    }

    #[test]
    fn location_video_ext_check() {
        assert!(is_location_video_ext("MOV"));
        assert!(is_location_video_ext("mp4"));
        assert!(!is_location_video_ext("jpg"));
    }
}
