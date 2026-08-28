//! Tala desktop entry point. Rust owns persistence, scheduling, and OS integration;
//! the bundled webview presents typed collection commands.

pub mod api;
mod archive;
pub mod clock;
pub mod content;
pub mod error;
mod files;
mod import_export;
mod integrity;
mod logging;
pub mod models;
pub mod scheduler;
mod statistics;
pub mod store;
mod study;

pub fn run() {
    use std::sync::atomic::Ordering;
    use tauri::{Emitter, Manager};
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init());
    #[cfg(feature = "e2e")]
    let builder = builder
        .plugin(tauri_plugin_wdio::init())
        .plugin(tauri_plugin_wdio_webdriver::init());
    builder
        .setup(api::initialize)
        .invoke_handler(tauri::generate_handler![
            api::dispatch,
            api::pick_file,
            api::save_file,
            api::open_external
        ])
        .build(tauri::generate_context!())
        .expect("Tala could not start")
        .run(|app, event| {
            if let tauri::RunEvent::ExitRequested { api, .. } = event
                && app
                    .state::<api::SharedBackend>()
                    .editor_dirty
                    .load(Ordering::SeqCst)
            {
                api.prevent_exit();
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
                let _ = app.emit("tala:quit-requested", ());
            }
        });
}
