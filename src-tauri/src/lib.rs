mod bands;
mod callsign;
mod cluster;
mod commands;
mod contest;
mod dxcc;
mod export;
mod geo;
mod import;
mod model;
mod modes;
mod qrz;
mod rig;
mod scp;
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
            // Log in to QRZ.com now, so the first call typed only waits for its lookup.
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let state = handle.state::<AppState>();
                let cfg = state.settings.lock().unwrap().qrz.clone();
                state.qrz.warm_up(&cfg, &commands::qrz_agent()).await;
            });
            let (cluster, login) = {
                let s = app.state::<AppState>().settings.lock().unwrap().clone();
                (s.cluster.clone(), commands::cluster_login(&s))
            };
            app.state::<AppState>().cluster.start(app.handle().clone(), cluster, login);
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
            commands::import_adif,
            commands::export_score_sheet,
            commands::rig_connect,
            commands::rig_disconnect,
            commands::rig_status,
            commands::rig_models,
            commands::flex_discover,
            commands::serial_ports,
            commands::app_version,
            commands::scp_status,
            commands::scp_download,
            commands::scp_import,
            commands::scp_lookup,
            commands::cty_status,
            commands::cty_download,
            commands::cty_import,
            commands::code_lists,
            commands::code_list_download,
            commands::code_list_import,
            commands::qrz_test,
            commands::qrz_lookup,
            commands::cluster_connect,
            commands::cluster_status,
            commands::cluster_spots,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                app.state::<AppState>().rig.shutdown();
            }
        });
}
