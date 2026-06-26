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

/// Resuelve un binario en el PATH (o devuelve el nombre tal cual si existe en
/// ubicaciones comunes). Mantiene la lógica simple y multiplataforma.
fn which(bin: &str) -> Option<String> {
    // Probar directamente: si `--version` corre, está en PATH.
    if Command::new(bin).arg("-version").output().map(|o| o.status.success()).unwrap_or(false) {
        return Some(bin.to_string());
    }
    // Rutas habituales (Homebrew / Linux).
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

/// Extrae un frame PNG en el segundo `at_secs`, escalado a `max_w` de ancho.
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
        .args(["-f", "image2pipe", "-vcodec", "png", "-"])
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
}
