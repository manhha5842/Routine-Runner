//! Routine Runner - Main entry point

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use auto_open_lib::commands;
use tauri::{Manager, menu::{Menu, MenuItem}, tray::TrayIconBuilder};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

fn main() {
    // Initialize logging
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    tracing::info!("Starting Routine Runner...");

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // Initialize storage
            let app_data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&app_data_dir)?;
            tracing::info!("Data directory: {:?}", app_data_dir);

            // Initialize database
            if let Err(e) = commands::init_database(&app_data_dir) {
                tracing::error!("Failed to initialize database: {}", e);
            }

            // Setup tray menu
            let show_item = MenuItem::with_id(app, "show", "Mở Routine Runner", true, None::<&str>)?;
            let pause_item = MenuItem::with_id(app, "pause", "Tạm dừng", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "Thoát", true, None::<&str>)?;
            
            let menu = Menu::with_items(app, &[&show_item, &pause_item, &quit_item])?;

            let _tray = TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| {
                    match event.id.as_ref() {
                        "show" => {
                            if let Some(window) = app.get_webview_window("main") {
                                let _ = window.show();
                                let _ = window.set_focus();
                            }
                        }
                        "pause" => {
                            tracing::info!("Pause/Resume clicked");
                            // TODO: Toggle scheduler pause
                        }
                        "quit" => {
                            tracing::info!("Quit clicked");
                            app.exit(0);
                        }
                        _ => {}
                    }
                })
                .on_tray_icon_event(|tray, event| {
                    if let tauri::tray::TrayIconEvent::DoubleClick { .. } = event {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                })
                .build(app)?;

            // Handle window close -> hide to tray
            let main_window = app.get_webview_window("main").unwrap();
            let window_clone = main_window.clone();
            main_window.on_window_event(move |event| {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = window_clone.hide();
                    tracing::info!("Window hidden to tray");
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_tasks,
            commands::create_task,
            commands::update_task,
            commands::delete_task,
            commands::run_task_now,
            commands::get_logs,
            commands::get_settings,
            commands::update_settings,
            commands::get_autostart_status,
            commands::set_autostart,
            commands::save_config_file,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
