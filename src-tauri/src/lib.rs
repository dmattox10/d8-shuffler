//! Thin Tauri layer. All the real work, and all the tests, live in `d8-core`.

use std::{
    fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

use d8_core::{
    audio::scan_source,
    plan::{preview, Preview},
    run::{
        check_ffmpeg, find_ffmpeg, inspect_card, run as run_shuffle, CardContents, Guard, RunOptions, RunReport,
    },
    volumes::{self, Volume, MAX_CARD_BYTES},
};
use serde::Serialize;
use tauri::{async_runtime::spawn_blocking, AppHandle, Emitter, Manager, State};

#[derive(Default)]
struct AppState {
    /// Set while a run is going; flipping it asks the run to stop.
    cancel: Mutex<Option<Arc<AtomicBool>>>,
}

fn blocking_err(e: tauri::Error) -> String {
    e.to_string()
}

/* ----------------------------------------------------------------- queries -- */

#[tauri::command]
fn default_source() -> String {
    let home = std::env::var("HOME").unwrap_or_default();
    PathBuf::from(home).join("Music").join("D8").display().to_string()
}

#[tauri::command]
async fn scan(source: String, native_formats: Vec<String>) -> Result<Preview, String> {
    spawn_blocking(move || {
        let files = scan_source(&PathBuf::from(&source)).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => format!("{source} does not exist."),
            _ => e.to_string(),
        })?;
        Ok(preview(&files, &native_formats))
    })
    .await
    .map_err(blocking_err)?
}

#[tauri::command]
async fn list_volumes() -> Result<Vec<Volume>, String> {
    spawn_blocking(volumes::list_volumes).await.map_err(blocking_err)
}

#[derive(Serialize)]
struct FfmpegStatus {
    path: Option<String>,
    ok: bool,
    message: Option<String>,
}

#[tauri::command]
async fn ffmpeg_status() -> FfmpegStatus {
    spawn_blocking(|| match find_ffmpeg() {
        None => FfmpegStatus {
            path: None,
            ok: false,
            message: Some("ffmpeg was not found.".into()),
        },
        Some(path) => match check_ffmpeg(&path) {
            Ok(()) => FfmpegStatus { path: Some(path.display().to_string()), ok: true, message: None },
            Err(m) => FfmpegStatus { path: Some(path.display().to_string()), ok: false, message: Some(m) },
        },
    })
    .await
    .unwrap_or(FfmpegStatus { path: None, ok: false, message: Some("could not check ffmpeg".into()) })
}

/// What is on the card, so the confirmation can say what is about to go.
#[tauri::command]
async fn card_contents(mount_point: String) -> Result<CardContents, String> {
    spawn_blocking(move || {
        volumes::volume_at(&mount_point).ok_or("That is not a removable card.")?;
        inspect_card(&PathBuf::from(mount_point))
    })
    .await
    .map_err(blocking_err)?
}

/* ----------------------------------------------------------------- actions -- */

#[tauri::command]
async fn format_card(whole_disk: String, label: String) -> Result<Volume, String> {
    spawn_blocking(move || volumes::erase_fat32(&whole_disk, &label))
        .await
        .map_err(blocking_err)?
}

#[tauri::command]
async fn eject_card(mount_point: String) -> Result<(), String> {
    spawn_blocking(move || volumes::eject(&mount_point))
        .await
        .map_err(blocking_err)?
}

#[derive(Serialize)]
struct Finished {
    #[serde(flatten)]
    report: RunReport,
    manifest_path: Option<String>,
}

#[tauri::command]
async fn start_run(
    app: AppHandle,
    state: State<'_, AppState>,
    options: RunOptions,
) -> Result<Finished, String> {
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut slot = state.cancel.lock().map_err(|_| "internal error")?;
        if slot.is_some() {
            return Err("A run is already in progress.".into());
        }
        *slot = Some(cancel.clone());
    }

    let emitter = app.clone();
    let flag = cancel.clone();
    let result = spawn_blocking(move || {
        // The front end names the card; this checks it really is one.
        let mount = options.target.to_string_lossy().into_owned();
        let vol = volumes::volume_at(&mount)
            .ok_or("That volume is not an external or removable card. Refusing.")?;
        if vol.too_large {
            return Err("That volume is larger than any card this tool will wipe.".to_string());
        }
        let guard = Guard { volumes_root: PathBuf::from("/Volumes"), max_card_bytes: MAX_CARD_BYTES };
        run_shuffle(&options, &guard, &flag, &mut |p| {
            let _ = emitter.emit("progress", p);
        })
    })
    .await;

    if let Ok(mut slot) = state.cancel.lock() {
        *slot = None;
    }

    let report = result.map_err(blocking_err)??;
    let manifest_path = write_manifest(&app, &report);
    Ok(Finished { report, manifest_path })
}

#[tauri::command]
fn cancel_run(state: State<'_, AppState>) {
    if let Ok(slot) = state.cancel.lock() {
        if let Some(flag) = slot.as_ref() {
            flag.store(true, Ordering::Relaxed);
        }
    }
}

/// Keeps a list of which numbered file is which song, for the last run.
fn write_manifest(app: &AppHandle, report: &RunReport) -> Option<String> {
    let dir = app.path().app_data_dir().ok()?;
    fs::create_dir_all(&dir).ok()?;
    let path = dir.join("last-run.txt");
    let body: String = report
        .manifest
        .iter()
        .map(|m| format!("{}\t{}\n", m.name, m.source))
        .collect();
    fs::write(&path, body).ok()?;
    Some(path.display().to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            default_source,
            scan,
            list_volumes,
            ffmpeg_status,
            card_contents,
            format_card,
            eject_card,
            start_run,
            cancel_run,
        ])
        .run(tauri::generate_context!())
        .expect("error while running D8 Shuffler");
}
