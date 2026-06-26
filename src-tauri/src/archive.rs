//! Lectura del contenido de archivos comprimidos (Fase B): ZIP, 7z y RAR.
//!
//! Sólo se INDEXA el listado (ruta interna, tamaño descomprimido, fecha, si es
//! carpeta) — no se extrae nada al disco. Cada formato tiene su backend en Rust
//! puro o con fuente bundleada, así no dependemos de binarios del sistema.

use serde::Serialize;
use std::path::Path;

/// Una entrada dentro de un archivo comprimido.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ArchiveItem {
    /// Ruta interna completa (ej. "fotos/2023/img001.jpg").
    pub path: String,
    /// Tamaño lógico (descomprimido) en bytes; 0 para carpetas/desconocido.
    pub size: u64,
    /// Fecha de modificación (unix secs) si el formato la expone (0 = desconocida).
    pub modified: i64,
    pub is_dir: bool,
}

/// ¿La extensión corresponde a un contenedor que sabemos leer?
pub fn is_supported_archive(ext: &str) -> bool {
    matches!(ext.to_lowercase().as_str(), "zip" | "7z" | "rar" | "cbz" | "cbr")
}

/// Lista el contenido de un archivo comprimido, despachando por extensión.
/// Devuelve error con mensaje claro si el formato no se soporta o está dañado.
pub fn list_archive(path: &Path) -> Result<Vec<ArchiveItem>, String> {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "zip" | "cbz" => list_zip(path),
        "7z" => list_7z(path),
        "rar" | "cbr" => list_rar(path),
        other => Err(format!("formato de archivo no soportado: .{other}")),
    }
}

fn list_zip(path: &Path) -> Result<Vec<ArchiveItem>, String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| format!("ZIP inválido: {e}"))?;
    let mut out = Vec::with_capacity(zip.len());
    for i in 0..zip.len() {
        let f = zip.by_index(i).map_err(|e| e.to_string())?;
        let modified = f
            .last_modified()
            .and_then(|dt| {
                // zip::DateTime → unix secs (best-effort).
                let (y, mo, d, h, mi, s) = (
                    dt.year() as i64,
                    dt.month() as i64,
                    dt.day() as i64,
                    dt.hour() as i64,
                    dt.minute() as i64,
                    dt.second() as i64,
                );
                ymd_to_unix(y, mo, d, h, mi, s)
            })
            .unwrap_or(0);
        out.push(ArchiveItem {
            path: f.name().trim_end_matches('/').to_string(),
            size: f.size(),
            modified,
            is_dir: f.is_dir(),
        });
    }
    Ok(out)
}

fn list_7z(path: &Path) -> Result<Vec<ArchiveItem>, String> {
    use sevenz_rust::Archive;
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let len = file.metadata().map_err(|e| e.to_string())?.len();
    let archive = Archive::read(&mut file, len, &[]).map_err(|e| format!("7z inválido: {e}"))?;
    let mut out = Vec::with_capacity(archive.files.len());
    for f in &archive.files {
        out.push(ArchiveItem {
            path: f.name().trim_end_matches('/').to_string(),
            size: f.size(),
            modified: 0,
            is_dir: f.is_directory(),
        });
    }
    Ok(out)
}

fn list_rar(path: &Path) -> Result<Vec<ArchiveItem>, String> {
    use unrar::Archive;
    let archive = Archive::new(path)
        .open_for_listing()
        .map_err(|e| format!("RAR inválido: {e}"))?;
    let mut out = Vec::new();
    for entry in archive {
        let header = entry.map_err(|e| e.to_string())?;
        out.push(ArchiveItem {
            path: header.filename.to_string_lossy().replace('\\', "/"),
            size: header.unpacked_size,
            modified: 0,
            is_dir: header.is_directory(),
        });
    }
    Ok(out)
}

/// Conversión simple fecha civil → unix secs (UTC), sin dependencias.
fn ymd_to_unix(y: i64, mo: i64, d: i64, h: i64, mi: i64, s: i64) -> Option<i64> {
    if y < 1970 || !(1..=12).contains(&mo) || !(1..=31).contains(&d) {
        return None;
    }
    // Algoritmo de días desde época (Howard Hinnant).
    let y = if mo <= 2 { y - 1 } else { y };
    let era = y / 400;
    let yoe = y - era * 400;
    let doy = (153 * (if mo > 2 { mo - 3 } else { mo + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    Some(days * 86400 + h * 3600 + mi * 60 + s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn supported_extensions() {
        assert!(is_supported_archive("zip"));
        assert!(is_supported_archive("ZIP"));
        assert!(is_supported_archive("7z"));
        assert!(is_supported_archive("rar"));
        assert!(!is_supported_archive("mp4"));
    }

    #[test]
    fn lists_a_real_zip() {
        // Construir un ZIP real en un tempfile y leer su índice.
        let dir = std::env::temp_dir().join(format!("diskdex_zip_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let zip_path = dir.join("test.zip");
        {
            let file = std::fs::File::create(&zip_path).unwrap();
            let mut w = zip::ZipWriter::new(file);
            let opts: zip::write::FileOptions<()> = zip::write::FileOptions::default();
            w.start_file("hola.txt", opts).unwrap();
            w.write_all(b"contenido de prueba").unwrap();
            w.add_directory("carpeta/", opts).unwrap();
            w.start_file("carpeta/anidado.bin", opts).unwrap();
            w.write_all(&[0u8; 1234]).unwrap();
            w.finish().unwrap();
        }

        let items = list_archive(&zip_path).unwrap();
        let txt = items.iter().find(|i| i.path == "hola.txt").unwrap();
        assert_eq!(txt.size, 19);
        assert!(!txt.is_dir);
        let nested = items.iter().find(|i| i.path == "carpeta/anidado.bin").unwrap();
        assert_eq!(nested.size, 1234);
        assert!(items.iter().any(|i| i.is_dir && i.path == "carpeta"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unsupported_format_errors() {
        let p = std::env::temp_dir().join("nope.tar");
        assert!(list_archive(&p).is_err());
    }
}
