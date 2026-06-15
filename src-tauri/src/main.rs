#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod drive;
mod email;
mod storage;
mod tracker;

use chrono::{Local, Timelike};
use config::{load_credentials, save_credentials, DriveConfig, LocalCredentials};
use rusqlite::Connection;
use serde::Serialize;
use std::sync::{Arc, Mutex};
use tauri::{
    AppHandle, CustomMenuItem, Manager, SystemTray, SystemTrayEvent, SystemTrayMenu,
    SystemTrayMenuItem,
};

// ── Shared state ──────────────────────────────────────────────────────────────

struct AppState {
    db: Mutex<Connection>,
    config: Mutex<DriveConfig>,
    creds: Mutex<LocalCredentials>,
}

type State = Arc<AppState>;

// ── Tauri commands (called from JS) ──────────────────────────────────────────

#[tauri::command]
async fn get_status(state: tauri::State<'_, State>) -> Result<StatusPayload, String> {
    let creds = state.creds.lock().unwrap().clone();
    let config = state.config.lock().unwrap().clone();
    let db = state.db.lock().unwrap();

    let limited_minutes = storage::get_today_limited_minutes(&db).map_err(|e| e.to_string())?;
    let limit_minutes = config.limit_minutes() as i64;
    let is_exempt = config.is_currently_exempt();

    Ok(StatusPayload {
        device_name: creds.device_name.clone(),
        date: Local::now().format("%Y-%m-%d").to_string(),
        used_minutes: limited_minutes,
        limit_minutes,
        is_exempt,
        idle_detection_enabled: config.idle_detection_enabled,
        limit_hours: config.limit_hours,
        is_setup_complete: creds.is_complete(),
    })
}

#[derive(Serialize)]
struct StatusPayload {
    device_name: String,
    date: String,
    used_minutes: i64,
    limit_minutes: i64,
    is_exempt: bool,
    idle_detection_enabled: bool,
    limit_hours: f64,
    is_setup_complete: bool,
}

#[tauri::command]
async fn save_setup(
    creds_input: LocalCredentials,
    state: tauri::State<'_, State>,
) -> Result<String, String> {
    let mut existing = state.creds.lock().unwrap().clone();
    // Only overwrite fields that are non-empty so partial saves don't wipe other fields
    if !creds_input.device_name.is_empty() { existing.device_name = creds_input.device_name; }
    if !creds_input.script_url.is_empty() { existing.script_url = creds_input.script_url; }
    if !creds_input.script_secret.is_empty() { existing.script_secret = creds_input.script_secret; }
    if !creds_input.gmail_address.is_empty() { existing.gmail_address = creds_input.gmail_address; }
    if !creds_input.gmail_app_password.is_empty() { existing.gmail_app_password = creds_input.gmail_app_password; }
    save_credentials(&existing).map_err(|e| e.to_string())?;
    *state.creds.lock().unwrap() = existing;
    Ok("Saved".into())
}

#[tauri::command]
async fn test_drive_connection(state: tauri::State<'_, State>) -> Result<String, String> {
    let creds = state.creds.lock().unwrap().clone();
    let default_cfg = DriveConfig::default();
    drive::ensure_config_exists(&creds.script_url, &creds.script_secret, &default_cfg)
        .await
        .map_err(|e| format!("Drive write failed: {e}"))?;
    Ok("Connected! config.json created in your Drive folder.".into())
}

#[tauri::command]
async fn test_email(state: tauri::State<'_, State>) -> Result<String, String> {
    let creds = state.creds.lock().unwrap().clone();
    let ecfg = email::EmailConfig {
        gmail_address: creds.gmail_address.clone(),
        gmail_app_password: creds.gmail_app_password.clone(),
        recipient: creds.gmail_address.clone(),
    };
    let dummy = storage::DailySummary {
        date: chrono::Local::now().format("%Y-%m-%d").to_string(),
        device_name: creds.device_name.clone(),
        total_minutes: 5,
        exempt_minutes: 0,
        limited_minutes: 5,
        app_breakdown: Default::default(),
    };
    email::send_warning_email(&ecfg, &dummy, 0.0)
        .await
        .map(|_| "Test email sent! Check your inbox.".to_string())
        .map_err(|e| format!("Email failed: {e}"))
}

// ── Background loop ───────────────────────────────────────────────────────────

async fn background_loop(state: State) {
    eprintln!("[loop] background_loop started");
    let mut tick: u64 = 0;
    loop {
        eprintln!("[loop] sleeping...");
        tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
        tick += 1;

        let creds = state.creds.lock().unwrap().clone();
        if !creds.is_complete() {
            eprintln!("[tick {tick}] creds incomplete, skipping");
            continue;
        }
        let config = state.config.lock().unwrap().clone();
        let is_exempt = config.is_currently_exempt();
        eprintln!("[tick {tick}] limit={:.2}h is_exempt={is_exempt}", config.limit_hours);

        // ── Track one minute (lock dropped before any await) ──────────────
        {
            let idle_mins = if config.idle_detection_enabled {
                tracker::idle_seconds().map(|s| s / 60).unwrap_or(0)
            } else {
                0
            };
            let is_idle = config.idle_detection_enabled && idle_mins >= config.idle_threshold_minutes;
            if !is_idle {
                let app_name = tracker::get_active_app().unwrap_or_else(|| "Unknown".into());
                let db = state.db.lock().unwrap();
                let _ = storage::record_minute(&db, &app_name, is_exempt);
            }
        }

        // ── Check report time (needs db lock, dropped before await) ─────
        let report_needed = {
            let db = state.db.lock().unwrap();
            let now = Local::now();
            now.hour() == config.daily_report_hour
                && !storage::was_report_sent_today(&db).unwrap_or(true)
        };

        // ── Drive sync: push data every 2 min, pull config every 60 min ─
        if tick % 2 == 0 {
            let summary_json = {
                let db = state.db.lock().unwrap();
                storage::today_summary(&db, &creds.device_name)
                    .ok()
                    .and_then(|s| serde_json::to_string_pretty(&s).ok())
            };
            if let Some(json) = summary_json {
                let _ = drive::push_daily_summary(
                    &creds.script_url,
                    &creds.script_secret,
                    &creds.device_name,
                    &json,
                )
                .await;
            }

            // ── Warning: check combined total across all devices ──────────
            if !is_exempt {
                let limit = config.limit_minutes() as i64;
                match drive::fetch_combined_today_minutes(&creds.script_url, &creds.script_secret).await {
                    Ok(combined) => {
                        eprintln!("[tick {tick}] combined={combined} limit={limit}");
                        if combined >= limit {
                            let today = Local::now().format("%Y-%m-%d").to_string();
                            let flag = format!("warning_sent_{today}.flag");
                            if !drive::check_flag(&creds.script_url, &creds.script_secret, &flag).await {
                                let _ = drive::set_flag(&creds.script_url, &creds.script_secret, &flag).await;
                                let summary = {
                                    let db = state.db.lock().unwrap();
                                    storage::today_summary(&db, &creds.device_name).unwrap_or_default()
                                };
                                let recipient = if config.report_email.is_empty() {
                                    creds.gmail_address.clone()
                                } else {
                                    config.report_email.clone()
                                };
                                let ecfg = email::EmailConfig {
                                    gmail_address: creds.gmail_address.clone(),
                                    gmail_app_password: creds.gmail_app_password.clone(),
                                    recipient,
                                };
                                eprintln!("[email] sending combined warning to {}", ecfg.recipient);
                                match email::send_warning_email(&ecfg, &summary, config.limit_hours).await {
                                    Ok(_) => eprintln!("[email] warning sent ok"),
                                    Err(e) => eprintln!("[email] warning failed: {e}"),
                                }
                            }
                        }
                    }
                    Err(e) => eprintln!("[tick {tick}] combined fetch error: {e}"),
                }
            }
        }

        // ── Daily report email ────────────────────────────────────────────
        if report_needed {
            let today = Local::now().format("%Y-%m-%d").to_string();
            let report_flag = format!("report_sent_{today}.flag");
            if !drive::check_flag(&creds.script_url, &creds.script_secret, &report_flag).await {
                let _ = drive::set_flag(&creds.script_url, &creds.script_secret, &report_flag).await;
                let local_summary = {
                    let db = state.db.lock().unwrap();
                    storage::today_summary(&db, &creds.device_name).unwrap_or_default()
                };
                let recipient = if config.report_email.is_empty() {
                    creds.gmail_address.clone()
                } else {
                    config.report_email.clone()
                };
                let ecfg = email::EmailConfig {
                    gmail_address: creds.gmail_address.clone(),
                    gmail_app_password: creds.gmail_app_password.clone(),
                    recipient,
                };
                if email::send_daily_report(&ecfg, &[local_summary], config.limit_hours).await.is_ok() {
                    let db = state.db.lock().unwrap();
                    let _ = storage::mark_report_sent(&db);
                }
            }
        }

        if tick % 60 == 0 {
            if let Ok(Some(new_cfg)) =
                drive::fetch_config(&creds.script_url, &creds.script_secret).await
            {
                *state.config.lock().unwrap() = new_cfg;
            }
        }
    }
}

// ── Tray setup ────────────────────────────────────────────────────────────────

fn build_tray() -> SystemTray {
    let open = CustomMenuItem::new("open".to_string(), "Open");
    let setup = CustomMenuItem::new("setup".to_string(), "Setup…");
    let quit = CustomMenuItem::new("quit".to_string(), "Quit");
    let menu = SystemTrayMenu::new()
        .add_item(open)
        .add_item(setup)
        .add_native_item(SystemTrayMenuItem::Separator)
        .add_item(quit);
    SystemTray::new().with_menu(menu)
}

fn handle_tray_event(app: &AppHandle, event: SystemTrayEvent) {
    match event {
        SystemTrayEvent::MenuItemClick { id, .. } => match id.as_str() {
            "open" => {
                let win = app.get_window("main").unwrap();
                let _ = win.show();
                let _ = win.set_focus();
            }
            "setup" => {
                let win = app.get_window("setup").unwrap();
                let _ = win.show();
                let _ = win.set_focus();
            }
            "quit" => {
                std::process::exit(0);
            }
            _ => {}
        },
        SystemTrayEvent::LeftClick { .. } => {
            let win = app.get_window("main").unwrap();
            if win.is_visible().unwrap_or(false) {
                let _ = win.hide();
            } else {
                let _ = win.show();
                let _ = win.set_focus();
            }
        }
        _ => {}
    }
}

// ── Entry point ───────────────────────────────────────────────────────────────

fn main() {
    env_logger::init();

    let creds = load_credentials().unwrap_or_default();
    eprintln!("[startup] creds complete: {}", creds.is_complete());
    eprintln!("[startup] script_url: '{}'", creds.script_url);
    eprintln!("[startup] script_secret empty: {}", creds.script_secret.is_empty());
    eprintln!("[startup] gmail_address empty: {}", creds.gmail_address.is_empty());
    eprintln!("[startup] gmail_app_password empty: {}", creds.gmail_app_password.is_empty());
    eprintln!("[startup] device_name empty: {}", creds.device_name.is_empty());
    let db = storage::open_db().expect("Failed to open database");

    // Try to load config from Drive immediately on startup
    let initial_config = if creds.is_complete() {
        eprintln!("[startup] fetching config from Drive...");
        match tauri::async_runtime::block_on(drive::fetch_config(&creds.script_url, &creds.script_secret)) {
            Ok(Some(cfg)) => {
                eprintln!("[startup] loaded config: limit_hours={}", cfg.limit_hours);
                cfg
            }
            Ok(None) => { eprintln!("[startup] config.json not found, using defaults"); DriveConfig::default() }
            Err(e) => { eprintln!("[startup] config fetch error: {e}"); DriveConfig::default() }
        }
    } else {
        eprintln!("[startup] creds incomplete, using default config");
        DriveConfig::default()
    };

    let state: State = Arc::new(AppState {
        db: Mutex::new(db),
        config: Mutex::new(initial_config),
        creds: Mutex::new(creds),
    });

    let state_bg = Arc::clone(&state);
    tauri::Builder::default()
        .manage(state)
        .setup(move |_app| {
            eprintln!("[setup] setup hook running, spawning background loop");
            tauri::async_runtime::spawn(async move {
                background_loop(state_bg).await;
            });
            eprintln!("[setup] spawn done");
            Ok(())
        })
        .system_tray(build_tray())
        .on_system_tray_event(handle_tray_event)
        .invoke_handler(tauri::generate_handler![
            get_status,
            save_setup,
            test_drive_connection,
            test_email,
        ])
        .on_window_event(|event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event.event() {
                // Hide instead of close so the app stays in the tray
                event.window().hide().unwrap();
                api.prevent_close();
            }
        })
        .run(tauri::generate_context!())
        .expect("Error running Tauri app");
}

// Make DailySummary have a Default impl for error paths
impl Default for storage::DailySummary {
    fn default() -> Self {
        Self {
            date: Local::now().format("%Y-%m-%d").to_string(),
            device_name: "Unknown".into(),
            total_minutes: 0,
            exempt_minutes: 0,
            limited_minutes: 0,
            app_breakdown: Default::default(),
        }
    }
}
