//! Validador offline del importador `.dcmf` contra un catálogo real.
//!
//! Uso:
//!   cargo run --release --bin validate_dcmf -- "/ruta/al/Catalog.dcmf"
//!
//! Verifica los criterios de aceptación de la sección 12 sin necesidad de la GUI:
//! cantidad de discos, total de entradas, reconstrucción de ruta de C0001.MP4 con
//! su tamaño (4.25 GB / 64-bit) y conteo de archivos `.mov`.

use diskdex_lib::db;
use diskdex_lib::dcmf::{self, DcmfDisk};
use std::time::Instant;

fn full_path(disk: &DcmfDisk, idx: usize) -> String {
    let mut parts = Vec::new();
    let mut cur: i32 = idx as i32;
    let mut guard = 0;
    while cur >= 0 {
        let e = &disk.entries[cur as usize];
        parts.push(e.name.clone());
        cur = e.parent;
        guard += 1;
        if guard > 4096 {
            break;
        }
    }
    parts.reverse();
    format!("/{}", parts.join("/"))
}

fn human(bytes: u64) -> String {
    let units = ["B", "KB", "MB", "GB", "TB", "PB"];
    if bytes == 0 {
        return "0 B".into();
    }
    let mut v = bytes as f64;
    let mut i = 0;
    while v >= 1024.0 && i < units.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    format!("{:.2} {}", v, units[i])
}

fn main() {
    let path = match std::env::args().nth(1) {
        Some(p) => p,
        None => {
            eprintln!("uso: validate_dcmf <ruta a Catalog.dcmf>");
            std::process::exit(2);
        }
    };

    let t0 = Instant::now();
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("error leyendo {path}: {e}");
            std::process::exit(1);
        }
    };
    if bytes.is_empty() {
        eprintln!(
            "El archivo está vacío (0 bytes). Si está en Dropbox, hacelo disponible offline primero."
        );
        std::process::exit(1);
    }
    println!("Archivo: {path} ({})", human(bytes.len() as u64));

    let disks = dcmf::import_dcmf(&bytes);
    let elapsed = t0.elapsed();

    let total_entries: usize = disks.iter().map(|d| d.entries.len()).sum();
    let total_files: usize = disks
        .iter()
        .flat_map(|d| d.entries.iter())
        .filter(|e| !e.is_folder)
        .count();
    let total_folders = total_entries - total_files;

    println!("──────────────────────────────────────────────");
    println!("Discos:           {}", disks.len());
    println!("Entradas totales: {total_entries}");
    println!("  archivos:       {total_files}");
    println!("  carpetas:       {total_folders}");
    println!("Tiempo de parseo: {:.2?}", elapsed);
    println!("──────────────────────────────────────────────");

    // Discos detectados.
    for d in &disks {
        let files = d.entries.iter().filter(|e| !e.is_folder).count();
        println!("  · {:<24} {} entradas ({} archivos)", d.name, d.entries.len(), files);
    }
    println!("──────────────────────────────────────────────");

    // Criterio: C0001.MP4 reconstruye ruta y muestra 4.25 GB.
    let mut found_c0001 = false;
    for d in &disks {
        for (i, e) in d.entries.iter().enumerate() {
            if e.name == "C0001.MP4" {
                println!(
                    "C0001.MP4  →  {}  ({})",
                    full_path(d, i),
                    human(e.size_logical)
                );
                found_c0001 = true;
            }
        }
    }
    if !found_c0001 {
        println!("(no se encontró C0001.MP4 — puede no existir en este catálogo)");
    }

    // Criterio: conteo de .mov (~256 k esperado).
    let mov_count: usize = disks
        .iter()
        .flat_map(|d| d.entries.iter())
        .filter(|e| !e.is_folder && e.name.to_lowercase().ends_with(".mov"))
        .count();
    println!("Archivos .mov:    {mov_count}");

    // Archivo más grande (sanity de 64-bit).
    if let Some((d, e)) = disks
        .iter()
        .flat_map(|d| d.entries.iter().map(move |e| (d, e)))
        .filter(|(_, e)| !e.is_folder)
        .max_by_key(|(_, e)| e.size_logical)
    {
        println!("Archivo más grande: {} en disco {} ({})", e.name, d.name, human(e.size_logical));
    }

    // Pipeline completo opcional: si se pasa un 2º arg, ingestar a un .dccat y
    // medir la búsqueda (criterio M3: .mov en <1 s sobre el catálogo importado).
    if let Some(out) = std::env::args().nth(2) {
        println!("──────────────────────────────────────────────");
        let _ = std::fs::remove_file(&out);
        let mut conn = match db::open(std::path::Path::new(&out)) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("error abriendo catálogo {out}: {e}");
                std::process::exit(1);
            }
        };
        let ti = Instant::now();
        let n = db::ingest_disks(&mut conn, &disks).expect("ingesta falló");
        println!("Ingesta a SQLite: {n} entradas en {:.2?}  →  {out}", ti.elapsed());

        let ts = Instant::now();
        let res = db::search(&conn, ".mov", 50).expect("search falló");
        let dt = ts.elapsed();
        println!(
            "Búsqueda \".mov\": {} coincidencias en {:.2?} (mostrando {})",
            res.total,
            dt,
            res.items.len()
        );
        if let Some(first) = res.items.first() {
            println!("  ej: {}  [{}]  {}", first.name, first.disk_name, first.path);
        }
        if dt.as_millis() < 1000 {
            println!("  ✓ criterio M3 cumplido (<1 s)");
        } else {
            println!("  ⚠ tardó >1 s");
        }
    }
}
