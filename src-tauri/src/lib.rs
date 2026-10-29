mod bands;
mod callsign;
mod commands;
mod contest;
mod export;
mod geo;
mod model;
mod rig;
mod settings;
mod store;

use commands::AppState;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let config_dir = app.path().app_config_dir()?;
            let state = AppState::new(&config_dir);
            for e in state.contest_errors.lock().unwrap().iter() {
                eprintln!("contest definition error: {e}");
            }
            let rig = state.settings.lock().unwrap().rig.clone();
            app.manage(state);
            app.state::<AppState>().rig.start(app.handle().clone(), rig);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_contests,
            commands::get_settings,
            commands::save_settings,
            commands::new_log,
            commands::open_log,
            commands::close_log,
            commands::get_log,
            commands::update_setup,
            commands::check_qso,
            commands::log_qso,
            commands::update_qso,
            commands::delete_qso,
            commands::cabrillo_text,
            commands::export_cabrillo,
            commands::export_adif,
            commands::rig_connect,
            commands::rig_disconnect,
            commands::rig_status,
            commands::app_version,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
