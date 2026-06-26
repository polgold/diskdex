//! Prueba end-to-end del conector (M9) sin GUI: escanea una carpeta real a un
//! catálogo (deja un disco online con archivos reales), arranca el agente y
//! escribe addr + código de emparejamiento + un entry_id chico para testear /v1/file.
//!
//! Uso: agent_probe <carpeta> <catalogo.dccat> <info.json>

use diskdex_lib::agent::{self, AgentConfig};
use diskdex_lib::db;
use diskdex_lib::scan::{self, ScanOptions};

fn main() {
    let folder = std::env::args().nth(1).expect("carpeta");
    let catalog = std::env::args().nth(2).expect("catalogo");
    let info = std::env::args().nth(3).expect("info.json");

    let root = std::path::PathBuf::from(&folder);
    let name = root.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| folder.clone());

    let _ = std::fs::remove_file(&catalog);
    let disk = scan::scan_volume(&root, &name, &ScanOptions::default()).expect("scan");
    let mut conn = db::open(std::path::Path::new(&catalog)).expect("open");
    let fp = scan::volume_fingerprint(&root);
    db::ingest_scanned(&mut conn, &disk, fp.as_deref(), "disk", None, &folder).expect("ingest");

    let entry_id: i64 = conn
        .query_row(
            "SELECT id FROM entries WHERE is_folder=0 ORDER BY size_logical ASC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .expect("entry");
    let disk_id: i64 = conn.query_row("SELECT id FROM disks LIMIT 1", [], |r| r.get(0)).unwrap();
    drop(conn);

    let handle = agent::start(
        std::path::PathBuf::from(&catalog),
        AgentConfig { bind: "127.0.0.1:8799".into(), default_scopes: "*".into(), name: "Probe".into() },
    )
    .expect("agent start");
    let code = handle.new_pairing_code();

    let payload = format!(
        "{{\"addr\":\"{}\",\"code\":\"{}\",\"entry_id\":{},\"disk_id\":{}}}",
        handle.addr, code, entry_id, disk_id
    );
    std::fs::write(&info, payload).expect("write info");
    eprintln!("agente escuchando en {} · code {} · entry {}", handle.addr, code, entry_id);

    std::thread::sleep(std::time::Duration::from_secs(180));
    handle.stop();
}
