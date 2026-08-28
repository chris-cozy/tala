//! Typed IPC boundary and native lifecycle. File dialogs grant purpose-scoped tokens;
//! database work runs off the UI thread under the shared store mutex.

use crate::{
    error::{AppError, Result},
    models::*,
    store::Store,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
use tauri::{Emitter, Manager};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(
    tag = "action",
    content = "payload",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
#[ts(export, export_to = "../../src/bindings/")]
pub enum Command {
    Bootstrap,
    EditorDirty {
        dirty: bool,
    },
    QuitApp,
    SaveDeck(DeckInput),
    DeleteDeck {
        deck_id: String,
        move_to: Option<String>,
    },
    SaveNote(NoteInput),
    GetCard {
        card_id: String,
    },
    Browse(BrowseQuery),
    Bulk(BulkInput),
    EditTag {
        from: String,
        to: Option<String>,
    },
    StartStudy {
        deck_id: Option<String>,
    },
    Study,
    Review(ReviewInput),
    Undo,
    History {
        card_id: String,
        offset: u32,
    },
    Statistics {
        range: String,
    },
    SavePreferences(Preferences),
    AttachFile {
        token: String,
    },
    AttachBytes {
        bytes: Vec<u8>,
    },
    PreviewImport(ImportConfig),
    CommitImport(ImportConfig),
    ExportDelimited {
        token: String,
        format: String,
        deck_id: Option<String>,
        ids: Vec<String>,
    },
    ExportNative {
        token: String,
    },
    Backups,
    CreateBackup,
    RestoreBackup {
        name: String,
    },
    RestoreFile {
        token: String,
    },
    Integrity,
    RepairIndexes,
    RecoverCollection,
    CleanupMedia,
    ExportDiagnostics {
        token: String,
    },
    OpenDataDirectory,
    #[cfg(feature = "e2e")]
    TestClock {
        #[ts(type = "number")]
        timestamp: i64,
        zone: String,
    },
    #[cfg(feature = "e2e")]
    TestGrant {
        path: String,
        purpose: String,
    },
}

pub struct Backend {
    pub store: Mutex<Option<Store>>,
    pub startup_error: Option<AppError>,
    pub root: PathBuf,
    pub editor_dirty: AtomicBool,
}
pub type SharedBackend = Arc<Backend>;

fn encode<T: Serialize>(value: T) -> Result<Value> {
    Ok(serde_json::to_value(value)?)
}
pub fn execute(store: &mut Store, request: Command) -> Result<Value> {
    match request {
        Command::EditorDirty { .. } | Command::QuitApp => Err(AppError::invalid(
            "This action requires the application window.",
        )),
        Command::Bootstrap => encode(store.bootstrap()?),
        Command::SaveDeck(input) => encode(store.save_deck(input)?),
        Command::DeleteDeck { deck_id, move_to } => encode(store.delete_deck(&deck_id, move_to)?),
        Command::SaveNote(input) => encode(store.save_note(input)?),
        Command::GetCard { card_id } => encode(store.card(&card_id)?),
        Command::Browse(query) => encode(store.browse(query)?),
        Command::Bulk(input) => encode(store.bulk(input)?),
        Command::EditTag { from, to } => encode(store.edit_tag(&from, to)?),
        Command::StartStudy { deck_id } => encode(store.start_session(deck_id)?),
        Command::Study => encode(store.study_view()?),
        Command::Review(input) => encode(store.review(input)?),
        Command::Undo => encode(store.undo()?),
        Command::History { card_id, offset } => encode(store.review_history(&card_id, offset)?),
        Command::Statistics { range } => encode(store.statistics(&range)?),
        Command::SavePreferences(input) => encode(store.save_preferences(input)?),
        Command::AttachFile { token } => encode(store.attach_image_file(&token)?),
        Command::AttachBytes { bytes } => encode(store.attach_image(&bytes)?),
        Command::PreviewImport(config) => encode(store.preview_import(&config)?),
        Command::CommitImport(config) => encode(store.commit_import(config)?),
        Command::ExportDelimited {
            token,
            format,
            deck_id,
            ids,
        } => encode(store.export_delimited(&token, &format, deck_id, ids)?),
        Command::ExportNative { token } => encode(store.export_native(&token)?),
        Command::Backups => encode(store.backups()?),
        Command::CreateBackup => encode(store.create_backup(false)?),
        Command::RestoreBackup { name } => encode(store.restore_named(&name)?),
        Command::RestoreFile { token } => encode(store.restore_token(&token)?),
        Command::Integrity => encode(store.integrity()?),
        Command::RepairIndexes => encode(store.repair_indexes()?),
        Command::RecoverCollection => Err(AppError::invalid(
            "Recovery requires the native file chooser.",
        )),
        Command::CleanupMedia => encode(store.cleanup_media()?),
        Command::ExportDiagnostics { token } => encode(store.export_diagnostics(&token)?),
        Command::OpenDataDirectory => Err(AppError::invalid(
            "This action requires the application window.",
        )),
        #[cfg(feature = "e2e")]
        Command::TestClock { timestamp, zone } => {
            store.clock_override = Some(crate::clock::Clock::at(
                timestamp,
                zone.parse()
                    .map_err(|_| AppError::invalid("Unknown test timezone."))?,
            ));
            encode(())
        }
        #[cfg(feature = "e2e")]
        Command::TestGrant { path, purpose } => encode(store.grant(path.into(), purpose)),
    }
}

#[tauri::command]
pub async fn dispatch(
    app: tauri::AppHandle,
    backend: tauri::State<'_, SharedBackend>,
    request: Command,
) -> Result<Value> {
    if let Command::EditorDirty { dirty } = &request {
        backend.editor_dirty.store(*dirty, Ordering::SeqCst);
        return encode(());
    }
    if matches!(request, Command::QuitApp) {
        app.exit(0);
        return encode(());
    }
    if matches!(request, Command::OpenDataDirectory) {
        app.opener()
            .open_path(backend.root.to_string_lossy().as_ref(), None::<&str>)
            .map_err(|_| AppError::storage("The data folder could not be opened."))?;
        return encode(());
    }
    let backend = backend.inner().clone();
    if matches!(request, Command::RecoverCollection) {
        return tauri::async_runtime::spawn_blocking(move || {
            let Some(file) = app.dialog().file().add_filter("Tala collection", &["tala"]).blocking_pick_file() else {
                return encode(false);
            };
            let path = file.into_path().map_err(|_| AppError::invalid("Select a local Tala backup."))?;
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            let approved = app
                .dialog()
                .message(format!(
                    "Restore {name}? This replaces the active collection. The current files will be preserved in the data folder under recovery-preserved. Nothing will be deleted from that recovery copy."
                ))
                .title("Recover Tala collection")
                .kind(tauri_plugin_dialog::MessageDialogKind::Warning)
                .buttons(tauri_plugin_dialog::MessageDialogButtons::OkCancel)
                .blocking_show();
            if !approved {
                return encode(false);
            }
            let _ = app.emit(
                "tala:job",
                serde_json::json!({"label":"Recovering collection","running":true}),
            );
            let mut guard = backend.store.lock().map_err(|_| AppError::storage("The collection is busy. Restart Tala before recovering it."))?;
            // Close SQLite before preserving the current database, WAL, and media together.
            drop(guard.take());
            let result = Store::recover_archive(&backend.root, &path);
            *guard = Store::open(&backend.root).ok();
            if let Some(store) = guard.as_ref() {
                app.asset_protocol_scope().allow_directory(store.media_dir(), false).map_err(|_| AppError::storage("The recovered media folder could not be opened."))?;
            }
            let _ = app.emit(
                "tala:job",
                serde_json::json!({"label":"Recovering collection","running":false}),
            );
            result?;
            encode(true)
        })
        .await
        .map_err(|_| AppError::storage("Recovery was interrupted. The preserved files remain in the data folder."))?;
    }
    let label = match &request {
        Command::CommitImport(_) => Some("Importing notes"),
        Command::ExportNative { .. } | Command::ExportDelimited { .. } => {
            Some("Exporting collection")
        }
        Command::CreateBackup => Some("Creating backup"),
        Command::RestoreBackup { .. } | Command::RestoreFile { .. } => Some("Restoring collection"),
        Command::Integrity => Some("Checking collection"),
        Command::RepairIndexes => Some("Rebuilding derived indexes"),
        _ => None,
    };
    if let Some(label) = label {
        let _ = app.emit(
            "tala:job",
            serde_json::json!({"label":label,"running":true}),
        );
    }
    let result = tauri::async_runtime::spawn_blocking(move || {
        let mut guard = backend
            .store
            .lock()
            .map_err(|_| AppError::storage("The collection is unavailable. Restart Tala."))?;
        let store = guard.as_mut().ok_or_else(|| {
            backend
                .startup_error
                .clone()
                .unwrap_or_else(|| AppError::storage("The collection could not be opened."))
        })?;
        let result = execute(store, request);
        if let Err(ref error) = result {
            log::error!("{}: {}", error.code, error.message);
        }
        result
    })
    .await
    .map_err(|_| {
        AppError::storage(
            "The operation was interrupted. Your saved data remains in the collection.",
        )
    })?;
    if let Some(label) = label {
        let _ = app.emit(
            "tala:job",
            serde_json::json!({"label":label,"running":false}),
        );
    }
    result
}

fn file_kind(purpose: &str, saving: bool) -> Result<(&'static str, Vec<&'static str>)> {
    match (purpose, saving) {
        ("image", false) => Ok(("Images", vec!["png", "jpg", "jpeg", "webp", "gif"])),
        ("import", false) => Ok(("Flashcards", vec!["csv", "tsv"])),
        ("restore", false) | ("native", true) => Ok(("Tala collection", vec!["tala"])),
        ("delimited", true) => Ok(("Flashcards", vec!["csv", "tsv"])),
        ("diagnostics", true) => Ok(("Diagnostic report", vec!["json"])),
        _ => Err(AppError::invalid("Unsupported file selection.")),
    }
}
#[cfg(feature = "e2e")]
fn take_test_selection(
    backend: &SharedBackend,
    purpose: &str,
    saving: bool,
) -> Result<Option<FileSelection>> {
    let mut guard = backend
        .store
        .lock()
        .map_err(|_| AppError::storage("Test collection is unavailable."))?;
    let store = guard
        .as_mut()
        .ok_or_else(|| AppError::storage("Test collection is unavailable."))?;
    let marker = format!("test-{}:{purpose}", if saving { "save" } else { "pick" });
    let token = store
        .grants
        .iter()
        .find(|(_, grant)| grant.purpose == marker)
        .map(|(token, _)| token.clone());
    Ok(token
        .and_then(|token| store.grants.remove(&token))
        .map(|grant| store.grant(grant.path, purpose.to_string())))
}
#[tauri::command]
pub async fn pick_file(
    app: tauri::AppHandle,
    backend: tauri::State<'_, SharedBackend>,
    purpose: String,
) -> Result<Option<FileSelection>> {
    let (label, extensions) = file_kind(&purpose, false)?;
    #[cfg(feature = "e2e")]
    if let Some(file) = take_test_selection(backend.inner(), &purpose, false)? {
        return Ok(Some(file));
    }
    let backend = backend.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let file = app
            .dialog()
            .file()
            .add_filter(label, &extensions)
            .blocking_pick_file();
        if let Some(file) = file {
            let path = file
                .into_path()
                .map_err(|_| AppError::invalid("Select a local file."))?;
            let mut guard = backend
                .store
                .lock()
                .map_err(|_| AppError::storage("Collection is unavailable."))?;
            let store = guard
                .as_mut()
                .ok_or_else(|| AppError::storage("Collection is unavailable."))?;
            Ok(Some(store.grant(path, purpose)))
        } else {
            Ok(None)
        }
    })
    .await
    .map_err(|_| AppError::storage("The file chooser was interrupted."))?
}
#[tauri::command]
pub async fn save_file(
    app: tauri::AppHandle,
    backend: tauri::State<'_, SharedBackend>,
    purpose: String,
    name: String,
) -> Result<Option<FileSelection>> {
    let (label, extensions) = file_kind(&purpose, true)?;
    #[cfg(feature = "e2e")]
    if let Some(file) = take_test_selection(backend.inner(), &purpose, true)? {
        return Ok(Some(file));
    }
    let backend = backend.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let file = app
            .dialog()
            .file()
            .add_filter(label, &extensions)
            .set_file_name(name)
            .blocking_save_file();
        if let Some(file) = file {
            let path = file
                .into_path()
                .map_err(|_| AppError::invalid("Select a local file."))?;
            let mut guard = backend
                .store
                .lock()
                .map_err(|_| AppError::storage("Collection is unavailable."))?;
            let store = guard
                .as_mut()
                .ok_or_else(|| AppError::storage("Collection is unavailable."))?;
            Ok(Some(store.grant(path, purpose)))
        } else {
            Ok(None)
        }
    })
    .await
    .map_err(|_| AppError::storage("The file chooser was interrupted."))?
}
#[tauri::command]
pub fn open_external(app: tauri::AppHandle, url: String) -> Result<()> {
    let parsed = tauri::Url::parse(&url).map_err(|_| AppError::invalid("This link is invalid."))?;
    if !["http", "https", "mailto"].contains(&parsed.scheme()) {
        return Err(AppError::invalid("Only web and email links can be opened."));
    }
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|_| AppError::storage("The link could not be opened."))
}

pub fn initialize(app: &mut tauri::App) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let root = app.path().app_data_dir()?;
    #[cfg(feature = "e2e")]
    let root = std::env::var_os("TALA_TEST_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or(root);
    crate::logging::initialize(&root);
    crate::archive::cleanup_abandoned_snapshots(&root);
    let result = Store::open(&root);
    let (store, startup_error) = match result {
        Ok(store) => (Some(store), None),
        Err(error) => {
            log::error!(
                "Collection startup failed: {}: {}",
                error.code,
                error.message
            );
            (None, Some(error))
        }
    };
    if let Some(ref store) = store {
        app.asset_protocol_scope()
            .allow_directory(store.media_dir(), false)?;
    }
    let backend = Arc::new(Backend {
        store: Mutex::new(store),
        startup_error,
        root,
        editor_dirty: AtomicBool::new(false),
    });
    app.manage(backend.clone());
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(std::time::Duration::from_secs(30));
            let job = {
                let Ok(mut guard) = backend.store.lock() else {
                    continue;
                };
                match guard
                    .as_mut()
                    .map(Store::prepare_automatic_backup)
                    .transpose()
                {
                    Ok(value) => value.flatten(),
                    Err(error) => {
                        log::error!("Automatic snapshot failed: {}", error.message);
                        None
                    }
                }
            };
            if let Some(job) = job {
                // Compression and hashing run with no collection mutex held.
                let result = job.write();
                if let Ok(mut guard) = backend.store.lock()
                    && let Some(store) = guard.as_mut()
                    && let Err(error) = store.finish_automatic_backup(job, result)
                {
                    log::error!("Automatic backup failed: {}", error.message);
                }
            }
        }
    });
    Ok(())
}
