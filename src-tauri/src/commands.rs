use crate::contest::def::ExchField;
use crate::contest::engine::{self, QsoResult, Score};
use crate::contest::{self, ContestDef};
use crate::export::{adif, cabrillo, scoresheet};
use crate::model::{LogSetup, Qso, QsoInput};
use crate::rig::{Rig, RigConfig, RigStatus};
use crate::settings::Settings;
use crate::store::Store;
use crate::{callsign, geo};
use chrono::Utc;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::{AppHandle, State};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

type CmdResult<T> = Result<T, String>;

pub struct OpenLog {
    path: PathBuf,
    store: Store,
    setup: LogSetup,
    def: ContestDef,
    qsos: Vec<Qso>,
}

pub struct AppState {
    pub log: Mutex<Option<OpenLog>>,
    pub contests: Mutex<Vec<ContestDef>>,
    pub contest_errors: Mutex<Vec<String>>,
    pub contest_dir: PathBuf,
    pub settings: Mutex<Settings>,
    pub settings_path: PathBuf,
    pub rig: Rig,
}

impl AppState {
    pub fn new(config_dir: &Path) -> Self {
        let contest_dir = config_dir.join("contests");
        let (defs, errors) = contest::load_all(Some(&contest_dir));
        let settings_path = config_dir.join("settings.json");
        AppState {
            log: Mutex::new(None),
            contests: Mutex::new(defs),
            contest_errors: Mutex::new(errors),
            contest_dir,
            settings: Mutex::new(Settings::load(&settings_path)),
            settings_path,
            rig: Rig::default(),
        }
    }

    fn find_contest(&self, id: &str) -> CmdResult<ContestDef> {
        self.contests
            .lock()
            .unwrap()
            .iter()
            .find(|d| d.id == id)
            .cloned()
            .ok_or_else(|| format!("Unknown contest '{id}'"))
    }

    fn remember_log(&self, path: &Path) {
        let mut s = self.settings.lock().unwrap();
        s.last_log = Some(path.display().to_string());
        let _ = s.save(&self.settings_path);
    }
}

#[derive(Serialize)]
pub struct QsoRow {
    #[serde(flatten)]
    qso: Qso,
    #[serde(flatten)]
    result: QsoResult,
}

#[derive(Serialize)]
pub struct LogView {
    path: String,
    setup: LogSetup,
    contest: ContestDef,
    qsos: Vec<QsoRow>,
    score: Score,
}

#[derive(Serialize)]
pub struct ContestList {
    contests: Vec<ContestDef>,
    errors: Vec<String>,
    user_dir: String,
}

#[derive(Serialize)]
pub struct CheckResult {
    result: QsoResult,
    /// Sent serial or number this contact would use.
    sent_nr: String,
    /// Locator last received from this station.
    known_locator: Option<String>,
    previous: Vec<QsoRow>,
}

fn view(log: &OpenLog) -> LogView {
    let eval = engine::evaluate(&log.def, &log.setup, &log.qsos);
    let qsos = log
        .qsos
        .iter()
        .cloned()
        .zip(eval.results)
        .map(|(qso, result)| QsoRow { qso, result })
        .collect();
    LogView {
        path: log.path.display().to_string(),
        setup: log.setup.clone(),
        contest: log.def.clone(),
        qsos,
        score: eval.score,
    }
}

fn next_sent_nr(log: &OpenLog, band: &str) -> String {
    if log.def.has_field(ExchField::Serial) {
        let max = log
            .qsos
            .iter()
            .filter(|q| !log.setup.serial_per_band || q.band == band)
            .filter_map(|q| q.sent_nr.parse::<u32>().ok())
            .max()
            .unwrap_or(0);
        (max + 1).to_string()
    } else if log.def.has_field(ExchField::Number) {
        log.setup.sent_number.clone()
    } else {
        String::new()
    }
}

fn default_rst(mode: &str) -> &'static str {
    if mode == "PH" { "59" } else { "599" }
}

fn draft_qso(log: &OpenLog, input: &QsoInput) -> Qso {
    let rst = |s: &str| {
        if s.trim().is_empty() {
            default_rst(&input.mode).to_string()
        } else {
            s.trim().to_string()
        }
    };
    let sent_locator = if log.def.has_field(ExchField::Locator) {
        geo::normalize_locator(&log.setup.station.locator)
    } else {
        String::new()
    };
    Qso {
        id: -1,
        time: input.time.unwrap_or_else(Utc::now),
        call: callsign::normalize(&input.call),
        band: input.band.clone(),
        mode: input.mode.clone(),
        freq_khz: input.freq_khz,
        sent_rst: rst(&input.sent_rst),
        sent_nr: next_sent_nr(log, &input.band),
        sent_locator,
        rcvd_rst: rst(&input.rcvd_rst),
        rcvd_nr: input.rcvd_nr.trim().to_string(),
        rcvd_locator: geo::normalize_locator(&input.rcvd_locator),
        operator: String::new(),
    }
}

fn with_log<T>(state: &AppState, f: impl FnOnce(&mut OpenLog) -> CmdResult<T>) -> CmdResult<T> {
    let mut guard = state.log.lock().unwrap();
    let log = guard.as_mut().ok_or("No log is open")?;
    f(log)
}

#[tauri::command]
pub fn list_contests(state: State<AppState>) -> ContestList {
    let (defs, errors) = contest::load_all(Some(&state.contest_dir));
    *state.contests.lock().unwrap() = defs.clone();
    *state.contest_errors.lock().unwrap() = errors.clone();
    ContestList {
        contests: defs,
        errors,
        user_dir: state.contest_dir.display().to_string(),
    }
}

#[tauri::command]
pub fn get_settings(state: State<AppState>) -> Settings {
    state.settings.lock().unwrap().clone()
}

#[tauri::command]
pub fn save_settings(state: State<AppState>, settings: Settings) -> CmdResult<()> {
    settings.save(&state.settings_path)?;
    *state.settings.lock().unwrap() = settings;
    Ok(())
}

#[tauri::command]
pub fn new_log(state: State<AppState>, path: String, setup: LogSetup) -> CmdResult<LogView> {
    let def = state.find_contest(&setup.contest_id)?;
    let path = PathBuf::from(path);
    let store = Store::create(&path, &setup)?;
    let log = OpenLog {
        path: path.clone(),
        store,
        setup,
        def,
        qsos: Vec::new(),
    };
    let v = view(&log);
    *state.log.lock().unwrap() = Some(log);
    state.remember_log(&path);
    Ok(v)
}

#[tauri::command]
pub fn open_log(state: State<AppState>, path: String) -> CmdResult<LogView> {
    let path = PathBuf::from(path);
    let (store, setup) = Store::open(&path)?;
    let def = state.find_contest(&setup.contest_id)?;
    let qsos = store.qsos()?;
    let log = OpenLog {
        path: path.clone(),
        store,
        setup,
        def,
        qsos,
    };
    let v = view(&log);
    *state.log.lock().unwrap() = Some(log);
    state.remember_log(&path);
    Ok(v)
}

#[tauri::command]
pub fn close_log(state: State<AppState>) {
    *state.log.lock().unwrap() = None;
}

#[tauri::command]
pub fn get_log(state: State<AppState>) -> Option<LogView> {
    state.log.lock().unwrap().as_ref().map(view)
}

#[tauri::command]
pub fn update_setup(state: State<AppState>, setup: LogSetup) -> CmdResult<LogView> {
    let def = state.find_contest(&setup.contest_id)?;
    with_log(&state, |log| {
        log.store.save_setup(&setup)?;
        log.setup = setup;
        log.def = def;
        Ok(view(log))
    })
}

#[tauri::command]
pub fn check_qso(state: State<AppState>, input: QsoInput) -> CmdResult<CheckResult> {
    with_log(&state, |log| {
        let mut draft = draft_qso(log, &input);
        let call = draft.call.clone();
        let known_locator = log
            .qsos
            .iter()
            .rev()
            .find(|q| q.call == call && !q.rcvd_locator.is_empty())
            .map(|q| q.rcvd_locator.clone());
        if draft.rcvd_locator.is_empty()
            && let Some(loc) = &known_locator
        {
            draft.rcvd_locator = loc.clone();
        }
        let result = engine::check(&log.def, &log.setup, &log.qsos, &draft);
        let eval = engine::evaluate(&log.def, &log.setup, &log.qsos);
        let previous = log
            .qsos
            .iter()
            .cloned()
            .zip(eval.results)
            .filter(|(q, _)| !call.is_empty() && q.call == call)
            .map(|(qso, result)| QsoRow { qso, result })
            .collect();
        Ok(CheckResult {
            result,
            sent_nr: draft.sent_nr,
            known_locator,
            previous,
        })
    })
}

#[tauri::command]
pub fn log_qso(state: State<AppState>, input: QsoInput) -> CmdResult<LogView> {
    with_log(&state, |log| {
        if !callsign::is_plausible(&input.call) {
            return Err(format!("'{}' is not a valid callsign", input.call.trim()));
        }
        if input.band.is_empty() || input.mode.is_empty() {
            return Err("Choose a band and mode".into());
        }
        let mut q = draft_qso(log, &input);
        q.id = log.store.insert(&q)?;
        log.qsos.push(q);
        log.qsos.sort_by_key(|q| (q.time, q.id));
        Ok(view(log))
    })
}

#[tauri::command]
pub fn update_qso(state: State<AppState>, qso: Qso) -> CmdResult<LogView> {
    with_log(&state, |log| {
        let mut qso = qso;
        qso.call = callsign::normalize(&qso.call);
        qso.rcvd_locator = geo::normalize_locator(&qso.rcvd_locator);
        qso.sent_locator = geo::normalize_locator(&qso.sent_locator);
        log.store.update(&qso)?;
        let slot = log
            .qsos
            .iter_mut()
            .find(|q| q.id == qso.id)
            .ok_or("QSO not found")?;
        *slot = qso;
        log.qsos.sort_by_key(|q| (q.time, q.id));
        Ok(view(log))
    })
}

#[tauri::command]
pub fn delete_qso(state: State<AppState>, id: i64) -> CmdResult<LogView> {
    with_log(&state, |log| {
        log.store.delete(id)?;
        log.qsos.retain(|q| q.id != id);
        Ok(view(log))
    })
}

#[tauri::command]
pub fn cabrillo_text(state: State<AppState>) -> CmdResult<String> {
    with_log(&state, |log| {
        let eval = engine::evaluate(&log.def, &log.setup, &log.qsos);
        Ok(cabrillo::render(&log.def, &log.setup, &log.qsos, &eval.score, VERSION))
    })
}

#[tauri::command]
pub fn export_cabrillo(state: State<AppState>, path: String) -> CmdResult<()> {
    let text = cabrillo_text(state)?;
    std::fs::write(&path, text).map_err(|e| format!("{path}: {e}"))
}

#[tauri::command]
pub fn export_adif(state: State<AppState>, path: String) -> CmdResult<()> {
    let text = with_log(&state, |log| {
        Ok(adif::render(&log.def, &log.setup, &log.qsos, VERSION))
    })?;
    std::fs::write(&path, text).map_err(|e| format!("{path}: {e}"))
}

#[tauri::command]
pub fn export_score_sheet(state: State<AppState>, path: String) -> CmdResult<()> {
    let text = with_log(&state, |log| {
        let eval = engine::evaluate(&log.def, &log.setup, &log.qsos);
        Ok(scoresheet::render(&log.def, &log.setup, &log.qsos, &eval))
    })?;
    std::fs::write(&path, text).map_err(|e| format!("{path}: {e}"))
}

#[tauri::command]
pub fn rig_connect(app: AppHandle, state: State<AppState>, config: RigConfig) -> CmdResult<()> {
    {
        let mut s = state.settings.lock().unwrap();
        s.rig = config.clone();
        s.save(&state.settings_path)?;
    }
    state.rig.start(app, config);
    Ok(())
}

#[tauri::command]
pub fn rig_disconnect(app: AppHandle, state: State<AppState>) {
    state.rig.stop(&app);
}

#[tauri::command]
pub fn rig_status(state: State<AppState>) -> RigStatus {
    state.rig.status()
}

#[tauri::command]
pub fn app_version() -> &'static str {
    VERSION
}
