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
    /// Fuerza un escaneo COMPLETO aunque exista un árbol catalogado reutilizable
    /// (desactiva el re-escaneo incremental por mtime). Útil cuando se sospecha
    /// de ediciones in-place que no tocan el mtime de la carpeta contenedora.
    pub force_full: bool,
    /// Si está activo, además de `exclude_names` se saltan los nombres de basura
    /// de `default_excludes()` (node_modules, caches, papeleras…). OPT-IN: por
    /// defecto NO se excluye nada, para que el catálogo sea completo (p.ej. poder
    /// buscar carpetas "Caches" y vaciarlas cuando el disco se llena).
    pub exclude_junk: bool,
}

/// Basura típica que infla/ralentiza el escaneo (dependencias, control de
/// versiones, papeleras y cachés; macOS + Windows/exFAT). Solo se aplica si el
/// usuario activa "Excluir basura" — NO es default.
pub fn default_excludes() -> Vec<String> {
    [
        "node_modules", ".git", ".svn", ".hg",
        ".Trash", ".Trashes", "$RECYCLE.BIN", "System Volume Information",
        ".cache", "Caches", ".npm", ".gradle", "DerivedData", "__pycache__",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

impl Default for ScanOptions {
    fn default() -> Self {
        ScanOptions {
            follow_symlinks: false,
            skip_hidden: false,
            skip_time_machine: true,
            exclude_names: Vec::new(),
            force_full: false,
            exclude_junk: false,
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
    scan_volume_cb(root, volume_name, opts, &mut |_, _| {}, &|| false)
}

/// Igual que `scan_volume` pero invoca `progress(entradas, bytes_lógicos)` de
/// forma periódica durante el recorrido (para reportar avance a la UI) y consulta
/// `cancel()` para abortar a pedido. Si se cancela, devuelve `ErrorKind::Interrupted`
/// y NO se ingesta nada (escaneo descartado).
pub fn scan_volume_cb(
    root: &Path,
    volume_name: &str,
    opts: &ScanOptions,
    progress: &mut dyn FnMut(u64, u64),
    cancel: &dyn Fn() -> bool,
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
        // Cancelación a pedido (entre carpetas): abortar sin ingestar nada.
        if cancel() {
            return Err(std::io::Error::new(std::io::ErrorKind::Interrupted, "escaneo cancelado"));
        }
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
                // También consultar cancelación acá, para responder rápido dentro
                // de carpetas enormes (no solo entre carpetas).
                if cancel() {
                    return Err(std::io::Error::new(std::io::ErrorKind::Interrupted, "escaneo cancelado"));
                }
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

/// Ruta relativa (con `/`) de cada entrada del árbol viejo respecto de la raíz.
/// La raíz (volumen) queda como cadena vacía. Asume orden padre-antes-que-hijo
/// (como devuelve `db::load_disk_tree`).
fn rel_paths(disk: &DcmfDisk) -> Vec<String> {
    let mut paths = vec![String::new(); disk.entries.len()];
    for (i, e) in disk.entries.iter().enumerate() {
        if e.parent < 0 {
            continue; // raíz → ""
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

/// Adyacencia hijos-directos del árbol viejo (índice → índices de sus hijos).
fn children_adjacency(disk: &DcmfDisk) -> Vec<Vec<usize>> {
    let mut kids: Vec<Vec<usize>> = vec![Vec::new(); disk.entries.len()];
    for (i, e) in disk.entries.iter().enumerate() {
        if e.parent >= 0 {
            kids[e.parent as usize].push(i);
        }
    }
    kids
}

/// Copia el subárbol viejo COLGANDO DE `old_folder` (sus descendientes, no el
/// nodo carpeta en sí) bajo `new_parent` en `out`, remapeando los índices de
/// padre a la posición nueva. Devuelve los bytes lógicos de archivos agregados.
fn splice_children(
    old: &DcmfDisk,
    kids: &[Vec<usize>],
    old_folder: usize,
    new_parent: i32,
    out: &mut Vec<DcmfEntry>,
) -> u64 {
    let mut bytes = 0u64;
    // BFS: el padre se inserta (y obtiene índice nuevo) antes que sus hijos.
    let mut queue: std::collections::VecDeque<(usize, i32)> = std::collections::VecDeque::new();
    for &c in &kids[old_folder] {
        queue.push_back((c, new_parent));
    }
    while let Some((oid, np)) = queue.pop_front() {
        let oe = &old.entries[oid];
        let ni = out.len() as i32;
        out.push(DcmfEntry {
            name: oe.name.clone(),
            parent: np,
            is_folder: oe.is_folder,
            is_volume: false,
            size_logical: oe.size_logical,
            size_physical: oe.size_physical,
            created: oe.created,
            modified: oe.modified,
        });
        if !oe.is_folder {
            bytes = bytes.saturating_add(oe.size_logical);
        }
        for &c in &kids[oid] {
            queue.push_back((c, ni));
        }
    }
    bytes
}

/// Re-escaneo INCREMENTAL: reutiliza el subárbol catalogado de toda carpeta cuyo
/// mtime no cambió (== el catalogado) sin descender el filesystem, y escanea
/// fresco el resto. Produce un `DcmfDisk` COMPLETO (apto para ingesta
/// full-replace, igual que `scan_volume_cb`). Devuelve también cuántas carpetas
/// se reutilizaron (señal de avance / ahorro).
///
/// CAVEAT: confiar en el mtime de la carpeta detecta archivos agregados/quitados
/// y carpetas tocadas, pero NO ediciones in-place de archivos profundos que no
/// alteran el mtime del directorio contenedor (raro en flujos de media). Para una
/// verificación exhaustiva usar `scan_volume_cb` (o `ScanOptions.force_full`).
pub fn scan_volume_incremental(
    root: &Path,
    volume_name: &str,
    opts: &ScanOptions,
    progress: &mut dyn FnMut(u64, u64),
    cancel: &dyn Fn() -> bool,
    old: &DcmfDisk,
) -> std::io::Result<(DcmfDisk, u64)> {
    let old_paths = rel_paths(old);
    let kids = children_adjacency(old);
    // Carpetas viejas indexadas por ruta relativa (para matchear contra el FS).
    let mut old_folder_by_path: std::collections::HashMap<&str, usize> =
        std::collections::HashMap::with_capacity(old.entries.len());
    for (i, e) in old.entries.iter().enumerate() {
        if e.is_folder && !e.is_volume {
            old_folder_by_path.insert(old_paths[i].as_str(), i);
        }
    }

    let root_meta = fs::metadata(root)?;
    let mut bytes_acc: u64 = 0;
    let mut reused_dirs: u64 = 0;
    let mut last_report: usize = 0;
    let mut entries: Vec<DcmfEntry> = Vec::new();

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

    // Pila: (ruta abs, índice del padre en `entries`, ruta relativa).
    let mut stack: Vec<(PathBuf, usize, String)> = vec![(root.to_path_buf(), 0, String::new())];

    while let Some((dir, parent_idx, dir_rel)) = stack.pop() {
        if cancel() {
            return Err(std::io::Error::new(std::io::ErrorKind::Interrupted, "escaneo cancelado"));
        }
        let rd = match fs::read_dir(&dir) {
            Ok(rd) => rd,
            Err(_) => continue,
        };
        for entry in rd.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();

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
            let child_rel = if dir_rel.is_empty() {
                name.clone()
            } else {
                format!("{dir_rel}/{name}")
            };

            if is_dir {
                let live_mtime = st_to_unix(meta.modified());
                // Reutilizar si hay carpeta vieja en esa ruta y su mtime coincide
                // (y es un mtime confiable, != 0).
                let reuse = old_folder_by_path
                    .get(child_rel.as_str())
                    .copied()
                    .filter(|&oid| {
                        let om = old.entries[oid].modified;
                        om != 0 && live_mtime != 0 && om == live_mtime
                    });

                let idx = entries.len();
                entries.push(DcmfEntry {
                    name,
                    parent: parent_idx as i32,
                    is_folder: true,
                    is_volume: false,
                    size_logical: 0,
                    size_physical: 0,
                    created: st_to_unix(meta.created()),
                    modified: live_mtime,
                });

                if let Some(oid) = reuse {
                    bytes_acc = bytes_acc
                        .saturating_add(splice_children(old, &kids, oid, idx as i32, &mut entries));
                    reused_dirs += 1;
                    // No se apila → no se desciende el FS para este subárbol.
                } else {
                    stack.push((path, idx, child_rel));
                }
            } else {
                let (size_logical, size_physical) = if is_symlink {
                    (0, 0)
                } else {
                    let logical = meta.len();
                    (logical, physical_size_path(&path, physical_size(&meta)))
                };
                entries.push(DcmfEntry {
                    name,
                    parent: parent_idx as i32,
                    is_folder: false,
                    is_volume: false,
                    size_logical,
                    size_physical,
                    created: st_to_unix(meta.created()),
                    modified: st_to_unix(meta.modified()),
                });
                bytes_acc = bytes_acc.saturating_add(size_logical);
            }

            // Reportar avance cada ~4096 entradas nuevas (los splices pueden saltar
            // de a muchas, así que comparamos contra un umbral, no el módulo).
            if entries.len() - last_report >= 4096 {
                last_report = entries.len();
                progress(entries.len() as u64, bytes_acc);
                if cancel() {
                    return Err(std::io::Error::new(std::io::ErrorKind::Interrupted, "escaneo cancelado"));
                }
            }
        }
    }

    progress(entries.len() as u64, bytes_acc);
    Ok((
        DcmfDisk {
            name: volume_name.to_string(),
            entries,
        },
        reused_dirs,
    ))
}

/// Fingerprint estable del volumen para reconocerlo al re-montar.
///
/// `diskutil info` puede COLGARSE con discos a medio desmontar; como esto se
/// llama desde comandos (en el pool async de Tauri), un cuelgue largo agotaría
/// los hilos del pool y congelaría operaciones no relacionadas (listar carpetas,
/// buscar). Por eso lo corremos en un hilo aparte con timeout: si `diskutil` no
/// responde a tiempo, devolvemos None (sin fingerprint) y seguimos.
#[cfg(target_os = "macos")]
pub fn volume_fingerprint(mount: &Path) -> Option<String> {
    use std::sync::mpsc;
    use std::time::Duration;
    let mount = mount.to_path_buf();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(diskutil_uuid(&mount)); // si el receptor expiró, se ignora
    });
    // No bloquear al llamador más de 3 s esperando a diskutil.
    rx.recv_timeout(Duration::from_secs(3)).ok().flatten()
}

/// Llamada real a `diskutil info` (puede bloquear): extrae el Volume UUID.
#[cfg(target_os = "macos")]
fn diskutil_uuid(mount: &Path) -> Option<String> {
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
    // sysinfo NO enumera montajes de red (SMB/AFP/NFS). Sumar los que falten
    // desde /Volumes para que aparezcan como "conectados" y escaneables.
    #[cfg(target_os = "macos")]
    out.extend(network_volumes(&out));
    out
}

/// Tamaño total/disponible (bytes) y tipo de filesystem de una ruta montada.
#[cfg(target_os = "macos")]
fn statfs_info(path: &Path) -> Option<(u64, u64, String)> {
    use std::os::unix::ffi::OsStrExt;
    let c = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut s: libc::statfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statfs(c.as_ptr(), &mut s) } != 0 {
        return None;
    }
    let bsize = s.f_bsize as u64;
    let fstype = unsafe { std::ffi::CStr::from_ptr(s.f_fstypename.as_ptr()) }
        .to_string_lossy()
        .to_string();
    Some((s.f_blocks * bsize, s.f_bavail * bsize, fstype))
}

/// Volúmenes de RED montados en /Volumes que sysinfo no lista. Solo agrega tipos
/// de FS de red (los locales/externos ya vienen por sysinfo, evita duplicados).
#[cfg(target_os = "macos")]
fn network_volumes(existing: &[VolumeInfo]) -> Vec<VolumeInfo> {
    let mut out = Vec::new();
    let rd = match fs::read_dir("/Volumes") {
        Ok(r) => r,
        Err(_) => return out,
    };
    for e in rd.flatten() {
        let path = e.path();
        let mount = path.to_string_lossy().to_string();
        if existing.iter().any(|v| v.mount_path == mount) {
            continue;
        }
        let (total, avail, fstype) = match statfs_info(&path) {
            Some(x) => x,
            None => continue,
        };
        let is_network = matches!(
            fstype.as_str(),
            "smbfs" | "afpfs" | "nfs" | "webdav" | "ftp" | "cifs"
        );
        if !is_network {
            continue;
        }
        let name = path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| mount.clone());
        out.push(VolumeInfo {
            name,
            mount_path: mount,
            fingerprint: volume_fingerprint(&path),
            total_space: total,
            available_space: avail,
            kind: "disk".into(),
            is_removable: true, // tratarlo como removible/externo en la UI
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

    #[test]
    fn incremental_reuses_unchanged_and_rescans_changed() {
        let base = std::env::temp_dir().join(format!("diskdex_incr_{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(base.join("A")).unwrap();
        fs::create_dir_all(base.join("B")).unwrap();
        fs::write(base.join("A").join("a.txt"), vec![0u8; 10]).unwrap();
        fs::write(base.join("B").join("b.txt"), vec![0u8; 20]).unwrap();

        // Árbol "catalogado" de referencia.
        let old = scan_volume(&base, "V", &ScanOptions::default()).unwrap();

        // 1) Sin cambios en el FS: A y B deben reutilizarse (mtime intacto) y el
        //    árbol resultante ser equivalente al viejo.
        let (fresh, reused) = scan_volume_incremental(
            &base, "V", &ScanOptions::default(), &mut |_, _| {}, &|| false, &old,
        )
        .unwrap();
        assert_eq!(reused, 2, "A y B sin cambios deben reutilizarse");
        assert_eq!(fresh.entries.len(), old.entries.len());
        assert!(fresh.entries.iter().any(|e| e.name == "a.txt" && e.size_logical == 10));
        assert!(fresh.entries.iter().any(|e| e.name == "b.txt" && e.size_logical == 20));

        // 2) Agregar una carpeta nueva C: A y B se reutilizan, C se escanea fresco.
        fs::create_dir_all(base.join("C")).unwrap();
        fs::write(base.join("C").join("c.txt"), vec![0u8; 30]).unwrap();
        let (fresh2, reused2) = scan_volume_incremental(
            &base, "V", &ScanOptions::default(), &mut |_, _| {}, &|| false, &old,
        )
        .unwrap();
        assert_eq!(reused2, 2, "solo A y B (preexistentes) se reutilizan; C es nueva");
        assert!(fresh2.entries.iter().any(|e| e.name == "c.txt" && e.size_logical == 30));

        // 3) Falsear el mtime catalogado de B fuerza un re-escaneo de B (no reuse),
        //    pero su contenido sigue presente vía escaneo fresco del FS.
        let mut tampered = old.clone();
        for e in tampered.entries.iter_mut() {
            if e.name == "B" {
                e.modified += 999_999;
            }
        }
        let (fresh3, reused3) = scan_volume_incremental(
            &base, "V", &ScanOptions::default(), &mut |_, _| {}, &|| false, &tampered,
        )
        .unwrap();
        assert_eq!(reused3, 1, "B cambió de mtime → solo A se reutiliza");
        assert!(fresh3.entries.iter().any(|e| e.name == "b.txt" && e.size_logical == 20));

        let _ = fs::remove_dir_all(&base);
    }
}
