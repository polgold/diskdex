//! Motor de escaneo de discos/carpetas montados (sección 7).
//!
//! - Recorrido recursivo iterativo (sin recursión → sin stack overflow en árboles
//!   profundos) capturando: nombre, es_carpeta, tamaño lógico, tamaño físico
//!   (asignado en disco), fechas de creación/modificación.
//! - Fingerprint del volumen (UUID/serial + label + capacidad) para reconocer el
//!   mismo disco al re-montarlo (`disks.volume_uuid`).
//! - Listado de volúmenes montados (para detectar discos conectados).
//!
//! Reusa `DcmfDisk`/`DcmfEntry` como representación de árbol: misma forma que el
//! importador, así la capa de DB ingesta ambos por igual.

use crate::dcmf::{DcmfDisk, DcmfEntry};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Opciones de escaneo (subconjunto de las de DiskCatalogMaker).
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(default)]
pub struct ScanOptions {
    pub follow_symlinks: bool,
    pub skip_hidden: bool,
    pub skip_time_machine: bool,
    pub exclude_names: Vec<String>,
}

impl Default for ScanOptions {
    fn default() -> Self {
        ScanOptions {
            follow_symlinks: false,
            skip_hidden: false,
            skip_time_machine: true,
            exclude_names: Vec::new(),
        }
    }
}

/// Metadatos del volumen escaneado, para la tabla `disks`.
#[derive(Debug, Clone, Serialize)]
pub struct ScanMeta {
    pub volume_uuid: Option<String>,
    pub kind: String,
    pub capacity: Option<i64>,
    pub mount_path: String,
}

/// Información de un volumen montado (para la UI / detección de conexión).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct VolumeInfo {
    pub name: String,
    pub mount_path: String,
    pub fingerprint: Option<String>,
    pub total_space: u64,
    pub available_space: u64,
    pub kind: String, // hdd | ssd | disk
    pub is_removable: bool,
}

fn st_to_unix(t: std::io::Result<SystemTime>) -> i64 {
    t.ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Tamaño físico (bytes asignados en disco) a partir de la metadata.
#[cfg(unix)]
fn physical_size(meta: &fs::Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;
    // `blocks()` cuenta bloques de 512 bytes (convención st_blocks).
    meta.blocks() * 512
}

#[cfg(windows)]
fn physical_size(meta: &fs::Metadata) -> u64 {
    // En Windows la metadata estándar no expone el tamaño asignado; se consulta
    // por ruta con GetCompressedFileSizeW (ver physical_size_path). Acá devolvemos
    // el lógico como fallback razonable.
    meta.len()
}

/// En Windows, tamaño comprimido/asignado real por ruta.
#[cfg(windows)]
fn physical_size_path(path: &Path, logical: u64) -> u64 {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::GetCompressedFileSizeW;
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    let mut high: u32 = 0;
    let low = unsafe { GetCompressedFileSizeW(PCWSTR(wide.as_ptr()), Some(&mut high)) };
    if low == u32::MAX {
        // INVALID_FILE_SIZE posible incluso sin error; usar lógico como fallback.
        return logical;
    }
    ((high as u64) << 32) | (low as u64)
}

#[cfg(not(windows))]
#[allow(unused_variables)]
fn physical_size_path(path: &Path, logical: u64) -> u64 {
    logical
}

fn is_time_machine_artifact(name: &str) -> bool {
    matches!(
        name,
        ".HFS+ Private Directory Data\r" | ".HFS+ Private Directory Data" | ".Spotlight-V100" | ".fseventsd" | ".DocumentRevisions-V100" | ".TemporaryItems" | ".Trashes" | ".MobileBackups"
    )
}

/// Escanea un volumen/carpeta montado y devuelve su árbol como `DcmfDisk`.
/// `volume_name` define el nombre del nodo raíz (label del volumen).
pub fn scan_volume(root: &Path, volume_name: &str, opts: &ScanOptions) -> std::io::Result<DcmfDisk> {
    scan_volume_cb(root, volume_name, opts, &mut |_, _| {})
}

/// Igual que `scan_volume` pero invoca `progress(entradas, bytes_lógicos)` de
/// forma periódica durante el recorrido (para reportar avance a la UI).
pub fn scan_volume_cb(
    root: &Path,
    volume_name: &str,
    opts: &ScanOptions,
    progress: &mut dyn FnMut(u64, u64),
) -> std::io::Result<DcmfDisk> {
    let root_meta = fs::metadata(root)?;
    let mut bytes_acc: u64 = 0;
    let mut entries: Vec<DcmfEntry> = Vec::new();

    // Nodo raíz = volumen.
    entries.push(DcmfEntry {
        name: volume_name.to_string(),
        parent: -1,
        is_folder: true,
        is_volume: true,
        size_logical: 0,
        size_physical: 0,
        created: st_to_unix(root_meta.created()),
        modified: st_to_unix(root_meta.modified()),
    });

    // Pila de carpetas por visitar: (ruta, índice del nodo padre).
    let mut stack: Vec<(PathBuf, usize)> = vec![(root.to_path_buf(), 0)];

    while let Some((dir, parent_idx)) = stack.pop() {
        let rd = match fs::read_dir(&dir) {
            Ok(rd) => rd,
            Err(_) => continue, // permisos / desmontado: saltear esa carpeta
        };
        for entry in rd.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();

            // Exclusiones.
            // Basura de macOS: AppleDouble (`._*`) y `.DS_Store` nunca aportan al
            // catálogo y duplican el conteo en discos externos/exFAT — se saltan siempre.
            if name == ".DS_Store" || name.starts_with("._") {
                continue;
            }
            if opts.skip_hidden && name.starts_with('.') {
                continue;
            }
            if opts.skip_time_machine && is_time_machine_artifact(&name) {
                continue;
            }
            if opts.exclude_names.iter().any(|x| x == &name) {
                continue;
            }

            let path = entry.path();
            let meta = match fs::symlink_metadata(&path) {
                Ok(m) => m,
                Err(_) => continue,
            };
            let is_symlink = meta.file_type().is_symlink();
            let is_dir = meta.is_dir() && !is_symlink;

            let (size_logical, size_physical) = if is_dir || is_symlink {
                (0, 0)
            } else {
                let logical = meta.len();
                (logical, physical_size_path(&path, physical_size(&meta)))
            };

            let idx = entries.len();
            entries.push(DcmfEntry {
                name,
                parent: parent_idx as i32,
                is_folder: is_dir,
                is_volume: false,
                size_logical,
                size_physical,
                created: st_to_unix(meta.created()),
                modified: st_to_unix(meta.modified()),
            });

            bytes_acc = bytes_acc.saturating_add(size_logical);

            // Reportar avance periódicamente (sin saturar el canal de eventos).
            if entries.len() % 4096 == 0 {
                progress(entries.len() as u64, bytes_acc);
            }

            if is_dir && (!is_symlink || opts.follow_symlinks) {
                stack.push((path, idx));
            }
        }
    }

    progress(entries.len() as u64, bytes_acc);
    Ok(DcmfDisk {
        name: volume_name.to_string(),
        entries,
    })
}

/// Fingerprint estable del volumen para reconocerlo al re-montar.
#[cfg(target_os = "macos")]
pub fn volume_fingerprint(mount: &Path) -> Option<String> {
    let out = std::process::Command::new("diskutil")
        .arg("info")
        .arg(mount)
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    for key in ["Volume UUID", "Disk / Partition UUID"] {
        for line in text.lines() {
            if let Some(rest) = line.trim().strip_prefix(key) {
                if let Some(val) = rest.trim_start_matches(':').trim().split_whitespace().next() {
                    if !val.is_empty() {
                        return Some(val.to_string());
                    }
                }
            }
        }
    }
    None
}

/// Fingerprint en Windows: número de serie del volumen (GetVolumeInformationW).
#[cfg(windows)]
pub fn volume_fingerprint(mount: &Path) -> Option<String> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::GetVolumeInformationW;
    // El path debe terminar en `\` (ej: "E:\").
    let mut root = mount.as_os_str().to_os_string();
    let s = root.to_string_lossy().to_string();
    if !s.ends_with('\\') {
        root.push("\\");
    }
    let wide: Vec<u16> = root.encode_wide().chain(std::iter::once(0)).collect();
    let mut serial: u32 = 0;
    let ok = unsafe {
        GetVolumeInformationW(
            PCWSTR(wide.as_ptr()),
            None,
            Some(&mut serial),
            None,
            None,
            None,
        )
    };
    if ok.is_ok() {
        Some(format!("{serial:08X}"))
    } else {
        None
    }
}

#[cfg(not(any(target_os = "macos", windows)))]
pub fn volume_fingerprint(_mount: &Path) -> Option<String> {
    None
}

/// Lista los volúmenes montados con capacidad, tipo y si es removible.
pub fn list_volumes() -> Vec<VolumeInfo> {
    use sysinfo::Disks;
    let disks = Disks::new_with_refreshed_list();
    let mut out = Vec::new();
    for d in &disks {
        let mount = d.mount_point().to_string_lossy().to_string();
        // En macOS ignorar mounts de sistema ruidosos.
        #[cfg(target_os = "macos")]
        if mount.starts_with("/System/Volumes") || mount == "/private/var/vm" {
            continue;
        }
        let name = {
            let n = d.name().to_string_lossy().to_string();
            if n.is_empty() {
                d.mount_point()
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| mount.clone())
            } else {
                n
            }
        };
        let kind = match d.kind() {
            sysinfo::DiskKind::HDD => "hdd",
            sysinfo::DiskKind::SSD => "ssd",
            _ => "disk",
        }
        .to_string();
        out.push(VolumeInfo {
            name,
            mount_path: mount.clone(),
            fingerprint: volume_fingerprint(d.mount_point()),
            total_space: d.total_space(),
            available_space: d.available_space(),
            kind,
            is_removable: d.is_removable(),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn scans_a_temp_tree() {
        // Construir un arbolito real en un tempdir y escanearlo.
        let base = std::env::temp_dir().join(format!("diskdex_scan_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(base.join("CLIP")).unwrap();
        fs::write(base.join("CLIP").join("a.mov"), vec![0u8; 1234]).unwrap();
        fs::write(base.join("readme.txt"), b"hello").unwrap();

        let disk = scan_volume(&base, "TESTVOL", &ScanOptions::default()).unwrap();
        assert_eq!(disk.name, "TESTVOL");
        // raíz + CLIP + a.mov + readme.txt = 4
        assert_eq!(disk.entries.len(), 4);
        assert!(disk.entries[0].is_volume);

        let mov = disk.entries.iter().find(|e| e.name == "a.mov").unwrap();
        assert!(!mov.is_folder);
        assert_eq!(mov.size_logical, 1234);
        // físico >= lógico (o igual en fallback).
        assert!(mov.size_physical >= mov.size_logical || mov.size_physical == 0);

        let clip = disk.entries.iter().find(|e| e.name == "CLIP").unwrap();
        assert!(clip.is_folder);

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn skip_hidden_excludes_dotfiles() {
        let base = std::env::temp_dir().join(format!("diskdex_scan_hidden_{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        fs::write(base.join(".secret"), b"x").unwrap();
        fs::write(base.join("visible.txt"), b"x").unwrap();

        let opts = ScanOptions { skip_hidden: true, ..Default::default() };
        let disk = scan_volume(&base, "V", &opts).unwrap();
        assert!(disk.entries.iter().all(|e| e.name != ".secret"));
        assert!(disk.entries.iter().any(|e| e.name == "visible.txt"));

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn list_volumes_returns_something() {
        // En cualquier máquina debería haber al menos el volumen raíz.
        let vols = list_volumes();
        assert!(!vols.is_empty());
    }
}
