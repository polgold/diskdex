// DiskDex — app de catálogo de discos (compatible DiskCatalogMaker).
// Toda la lógica de FS/parsing vive en el lado nativo (Rust); la UI consume IPC.

// BLAS de Apple (feature `accel`): fuerza al linker a incluir Accelerate para
// que candle lo use en el matmul (CPU rápido) — ver módulo `ai`.
#[cfg(feature = "accel")]
extern crate accelerate_src;

#[cfg(feature = "ai")]
pub mod ai;
pub mod agent;
pub mod archive;
mod commands;
pub mod db;
pub mod dcmf;
pub mod geo;
pub mod scan;
pub mod video;

use commands::AppState;
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
    Emitter, Manager, WindowEvent,
};

/// Muestra y enfoca la ventana principal (desde el tray o al detectar un disco).
fn show_main(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .manage(AppState::default())
        .setup(|app| {
            // Icono residente en la barra de menú (tray): abrir / escanear / buscar / salir.
            let show = MenuItem::with_id(app, "show", "Abrir DiskDex", true, None::<&str>)?;
            let scan = MenuItem::with_id(app, "scan", "Escanear…", true, None::<&str>)?;
            let search = MenuItem::with_id(app, "search", "Buscar…", true, None::<&str>)?;
            let sep = PredefinedMenuItem::separator(app)?;
            let quit = MenuItem::with_id(app, "quit", "Salir de DiskDex", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &scan, &search, &sep, &quit])?;
            let mut builder = TrayIconBuilder::new()
                .menu(&menu)
                .tooltip("DiskDex")
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => show_main(app),
                    "scan" => {
                        show_main(app);
                        let _ = app.emit("tray://scan", ());
                    }
                    "search" => {
                        show_main(app);
                        let _ = app.emit("tray://search", ());
                    }
                    "quit" => app.exit(0),
                    _ => {}
                });
            if let Some(icon) = app.default_window_icon() {
                builder = builder.icon(icon.clone());
            }
            builder.build(app)?;
            Ok(())
        })
        // Cerrar la ventana NO cierra la app: la oculta y queda residente en el
        // tray (para que el watcher de discos siga vivo y aparezca el popup).
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::ping,
            commands::import_dcmf,
            commands::dcmf_disk_names,
            commands::import_dcmf_merge,
            commands::open_catalog,
            commands::close_catalog,
            commands::list_disks,
            commands::disk_detail,
            commands::list_children,
            commands::entry_path,
            commands::get_entry,
            commands::get_entry_meta,
            commands::search_entries,
            commands::search_advanced,
            commands::resolve_fs_path,
            commands::move_to_trash,
            commands::move_entries_to_trash,
            commands::get_thumbnail,
            commands::cache_disk_thumbnails,
            commands::media_tools_available,
            commands::index_disk_videos,
            commands::get_video_meta,
            commands::get_video_frames,
            commands::detect_video_scenes,
            commands::index_disk_archives,
            commands::list_archive_contents,
            commands::add_entry_tag,
            commands::remove_entry_tag,
            commands::get_entry_tags,
            commands::list_tags,
            commands::set_entry_comment,
            commands::set_disk_meta,
            commands::delete_disk,
            commands::catalog_stats,
            commands::find_duplicates,
            commands::cancel_copy,
            commands::gather_plan,
            commands::gather_copy,
            commands::cancel_gather,
            commands::compare_disks,
            commands::missing_tree,
            commands::copy_missing,
            commands::write_text_file,
            commands::save_session,
            commands::load_session,
            commands::agent_start,
            commands::agent_stop,
            commands::agent_status,
            commands::agent_pair_code,
            commands::agent_devices,
            commands::agent_revoke,
            commands::list_volumes,
            commands::scan_disk,
            commands::cancel_scan,
            commands::start_volume_watch,
            commands::refresh_online_status,
            commands::ai_available,
            commands::ai_status,
            commands::ai_index,
            commands::ai_search,
            commands::ai_index_videos,
            commands::ai_similar,
            commands::ai_visual_duplicates,
            commands::ai_transcribe_disk,
            commands::ai_search_transcripts,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            // Al salir de verdad (no al cerrar la ventana, que solo la esconde en
            // el tray) hay que consolidar el WAL dentro del .dccat. Si no, el
            // catálogo queda repartido en dos archivos y Dropbox los sincroniza
            // por separado, que es como se corrompe en la otra máquina.
            if let tauri::RunEvent::Exit = event {
                let state = app.state::<commands::AppState>();
                let guard = state.catalog.lock().unwrap();
                if let Some(cat) = guard.as_ref() {
                    commands::checkpoint_quietly(&cat.conn, &cat.path);
                }
            }
        });
}
