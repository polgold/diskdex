//! Prueba real del motor de escaneo sobre una carpeta/volumen del sistema.
//! Uso: cargo run --release --bin scan_probe -- <ruta> [catalogo.dccat]

use diskdex_lib::db;
use diskdex_lib::scan::{self, ScanOptions};
use std::time::Instant;

fn human(b: u64) -> String {
    let u = ["B", "KB", "MB", "GB", "TB"];
    if b == 0 {
        return "0 B".into();
    }
    let mut v = b as f64;
    let mut i = 0;
    while v >= 1024.0 && i < u.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    format!("{v:.2} {}", u[i])
}

fn main() {
    let path = std::env::args().nth(1).expect("uso: scan_probe <ruta> [catalogo]");
    let root = std::path::PathBuf::from(&path);
    let name = root
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| path.clone());

    println!("Volúmenes montados detectados:");
    for v in scan::list_volumes() {
        println!(
            "  · {:<22} {:<8} removible={} {}",
            v.name,
            v.kind,
            v.is_removable,
            v.mount_path
        );
    }
    println!("Fingerprint de {path}: {:?}", scan::volume_fingerprint(&root));
    println!("──────────────────────────────────────────");

    let t0 = Instant::now();
    let disk = scan::scan_volume(&root, &name, &ScanOptions::default()).expect("scan falló");
    let dt = t0.elapsed();

    let files = disk.entries.iter().filter(|e| !e.is_folder).count();
    let folders = disk.entries.len() - files;
    let logical: u64 = disk.entries.iter().filter(|e| !e.is_folder).map(|e| e.size_logical).sum();
    let physical: u64 = disk.entries.iter().filter(|e| !e.is_folder).map(|e| e.size_physical).sum();
    println!("Escaneado '{name}' en {dt:.2?}");
    println!("  entradas: {} ({files} archivos, {folders} carpetas)", disk.entries.len());
    println!("  lógico:   {}", human(logical));
    println!("  físico:   {}", human(physical));

    if let Some((i, biggest)) = disk
        .entries
        .iter()
        .enumerate()
        .filter(|(_, e)| !e.is_folder)
        .max_by_key(|(_, e)| e.size_logical)
    {
        // ruta
        let mut parts = Vec::new();
        let mut cur = i as i32;
        while cur >= 0 {
            parts.push(disk.entries[cur as usize].name.clone());
            cur = disk.entries[cur as usize].parent;
        }
        parts.reverse();
        println!("  más grande: /{} ({})", parts.join("/"), human(biggest.size_logical));
    }

    if let Some(out) = std::env::args().nth(2) {
        let mut conn = db::open(std::path::Path::new(&out)).expect("open catalog");
        let fp = scan::volume_fingerprint(&root);
        let ti = Instant::now();
        let r = db::ingest_scanned(&mut conn, &disk, fp.as_deref(), "disk", None, &path).expect("ingest");
        println!(
            "Ingestado a {out}: disk_id={} entries={} replaced={} en {:.2?}",
            r.disk_id, r.entries, r.replaced, ti.elapsed()
        );
    }
}
