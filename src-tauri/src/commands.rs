use crate::contest::def::ExchField;
use crate::contest::engine::{self, QsoResult, Score};
use crate::contest::{self, ContestDef};
use crate::dxcc::{self, Country, Cty};
use crate::export::{adif, cabrillo, scoresheet};
use crate::import::{self, ImportSummary};
use crate::model::{LogSetup, Qso, QsoInput};
use crate::qrz::{Qrz, QrzConfig, QrzInfo};
use crate::rig::hamlib::{self, RigModel, SerialPort};
use crate::rig::{Rig, RigConfig, RigKind, RigStatus, flex, tci};
use crate::scp::{self, Scp, ScpMatches};
use crate::settings::Settings;
use crate::store::Store;
use crate::{callsign, geo, modes};
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::{AppHandle, Manager, State};

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
    pub scp: Mutex<Scp>,
    pub scp_path: PathBuf,
    pub cty: Mutex<Cty>,
    pub cty_path: PathBuf,
    pub qrz: Qrz,
}

impl AppState {
    pub fn new(config_dir: &Path) -> Self {
        let contest_dir = config_dir.join("contests");
        let (defs, errors) = contest::load_all(Some(&contest_dir));
        let settings_path = config_dir.join("settings.json");
        let scp_path = config_dir.join("MASTER.SCP");
        let cty_path = config_dir.join("cty.csv");
        AppState {
            log: Mutex::new(None),
            contests: Mutex::new(defs),
            contest_errors: Mutex::new(errors),
            contest_dir,
            settings: Mutex::new(Settings::load(&settings_path)),
            settings_path,
            rig: Rig::default(),
            scp: Mutex::new(Scp::load(&scp_path)),
            scp_path,
            cty: Mutex::new(Cty::load(&cty_path)),
            cty_path,
            qrz: Qrz::default(),
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
    /// In general logs: the station's country, if the country file knows it.
    #[serde(flatten)]
    country: Option<Country>,
    /// In general logs: the first contact with this country.
    new_dxcc: bool,
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
pub struct ScpStatus {
    /// Calls in MASTER.SCP; 0 when there is none.
    count: usize,
    path: String,
    /// When MASTER.SCP was last downloaded or imported.
    updated: Option<chrono::DateTime<Utc>>,
}

#[derive(Serialize)]
pub struct CtyStatus {
    /// DXCC entities in the country file; 0 when there is none.
    count: usize,
    path: String,
    updated: Option<DateTime<Utc>>,
}

/// Whether a country has been worked before, on a band or in a mode class.
#[derive(Serialize, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Need {
    New,
    Worked,
    Confirmed,
}

#[derive(Serialize)]
pub struct DxCheck {
    country: Country,
    /// The country on any band and mode.
    any: Need,
    /// The country on this band.
    band: Need,
    /// The country in this mode class (phone, CW or data).
    mode: Need,
    /// The country on each band and mode class it has been worked on.
    slots: Vec<Slot>,
}

#[derive(Serialize)]
pub struct Slot {
    band: String,
    /// PH, CW or DG.
    class: &'static str,
    need: Need,
}

#[derive(Serialize)]
pub struct ImportResult {
    summary: ImportSummary,
    view: LogView,
}

#[derive(Serialize)]
pub struct CheckResult {
    result: QsoResult,
    /// Sent serial this contact would use.
    sent_nr: String,
    /// Locator last received from this station.
    known_locator: Option<String>,
    /// `number` field (e.g. branch) last received from this station.
    known_exch: Option<String>,
    /// Name last logged for this station, in general logs.
    known_name: Option<String>,
    /// Country and whether it's needed, in general logs.
    dx: Option<DxCheck>,
    previous: Vec<QsoRow>,
}

/// The country of a contact: the DXCC number it was logged or imported with, else
/// the country file's answer for the call.
fn country_of(cty: &Cty, q: &Qso) -> Option<Country> {
    match q.details.dxcc {
        Some(n) => cty.by_number(n).or_else(|| {
            Some(Country {
                dxcc: n,
                country: format!("DXCC {n}"),
                ..Default::default()
            })
        }),
        None => cty.lookup(&q.call),
    }
}

fn confirmed(q: &Qso) -> bool {
    q.details.qsl_rcvd == "Y" || q.details.lotw_rcvd == "Y"
}

/// Rows for the log, with countries in general logs.
fn rows(log: &OpenLog, cty: &Cty, results: Vec<QsoResult>) -> Vec<QsoRow> {
    let mut seen = std::collections::HashSet::new();
    log.qsos
        .iter()
        .cloned()
        .zip(results)
        .map(|(qso, result)| {
            let country = if log.def.general { country_of(cty, &qso) } else { None };
            let new_dxcc = country.as_ref().is_some_and(|c| seen.insert(c.dxcc));
            QsoRow {
                qso,
                result,
                country,
                new_dxcc,
            }
        })
        .collect()
}

fn dx_check(log: &OpenLog, cty: &Cty, draft: &Qso) -> Option<DxCheck> {
    let country = cty.lookup(&draft.call)?;
    let class = modes::class(&draft.mode);
    let (mut any, mut band, mut mode) = (Need::New, Need::New, Need::New);
    let mut slots: std::collections::BTreeMap<(String, &'static str), Need> = Default::default();
    for q in &log.qsos {
        if country_of(cty, q).is_none_or(|c| c.dxcc != country.dxcc) {
            continue;
        }
        let need = if confirmed(q) { Need::Confirmed } else { Need::Worked };
        any = any.max(need);
        if q.band == draft.band {
            band = band.max(need);
        }
        if modes::class(&q.mode) == class {
            mode = mode.max(need);
        }
        let slot = slots.entry((q.band.clone(), modes::class(&q.mode))).or_insert(need);
        *slot = (*slot).max(need);
    }
    Some(DxCheck {
        country,
        any,
        band,
        mode,
        slots: slots
            .into_iter()
            .map(|((band, class), need)| Slot { band, class, need })
            .collect(),
    })
}

fn view(log: &OpenLog, cty: &Cty) -> LogView {
    let eval = engine::evaluate(&log.def, &log.setup, &log.qsos);
    let qsos = rows(log, cty, eval.results);
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
        let per_band = log.setup.serial_per_band || log.def.serial_per_band;
        let max = log
            .qsos
            .iter()
            .filter(|q| !per_band || q.band == band)
            .filter_map(|q| q.sent_nr.parse::<u32>().ok())
            .max()
            .unwrap_or(0);
        format!("{:03}", max + 1)
    } else {
        String::new()
    }
}

fn draft_qso(log: &OpenLog, input: &QsoInput) -> Qso {
    let rst = |s: &str| {
        if s.trim().is_empty() {
            modes::default_rst(&input.mode).to_string()
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
        band: crate::bands::canonical(&input.band),
        mode: input.mode.clone(),
        freq_khz: input.freq_khz,
        sent_rst: rst(&input.sent_rst),
        sent_nr: next_sent_nr(log, &input.band),
        sent_locator,
        rcvd_rst: rst(&input.rcvd_rst),
        rcvd_nr: input.rcvd_nr.trim().to_string(),
        rcvd_locator: geo::normalize_locator(&input.rcvd_locator),
        operator: String::new(),
        sent_exch: if log.def.has_field(ExchField::Number) {
            log.setup.sent_number.trim().to_string()
        } else {
            String::new()
        },
        rcvd_exch: input.rcvd_exch.trim().to_string(),
        details: crate::model::QsoDetails {
            name: input.name.trim().to_string(),
            qth: input.qth.trim().to_string(),
            comment: input.comment.trim().to_string(),
            ..Default::default()
        },
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
    let v = view(&log, &state.cty.lock().unwrap());
    *state.log.lock().unwrap() = Some(log);
    state.remember_log(&path);
    Ok(v)
}

#[tauri::command]
pub fn open_log(state: State<AppState>, path: String) -> CmdResult<LogView> {
    let path = PathBuf::from(path);
    let (store, setup) = Store::open(&path)?;
    let def = state.find_contest(&setup.contest_id)?;
    if store.upgraded_from.is_some_and(|v| v < 2)
        && def.has_field(ExchField::Number)
        && !def.has_field(ExchField::Serial)
    {
        store.move_numbers_to_exch()?;
    }
    let qsos = store.qsos()?;
    let log = OpenLog {
        path: path.clone(),
        store,
        setup,
        def,
        qsos,
    };
    let v = view(&log, &state.cty.lock().unwrap());
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
    state.log.lock().unwrap().as_ref().map(|l| view(l, &state.cty.lock().unwrap()))
}

#[tauri::command]
pub fn update_setup(state: State<AppState>, setup: LogSetup) -> CmdResult<LogView> {
    let def = state.find_contest(&setup.contest_id)?;
    with_log(&state, |log| {
        log.store.save_setup(&setup)?;
        log.setup = setup;
        log.def = def;
        Ok(view(log, &state.cty.lock().unwrap()))
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
        let known_exch = log
            .qsos
            .iter()
            .rev()
            .find(|q| q.call == call && !q.rcvd_exch.is_empty())
            .map(|q| q.rcvd_exch.clone());
        if draft.rcvd_exch.is_empty()
            && let Some(x) = &known_exch
        {
            draft.rcvd_exch = x.clone();
        }
        let known_name = log
            .qsos
            .iter()
            .rev()
            .find(|q| q.call == call && !q.details.name.is_empty())
            .map(|q| q.details.name.clone());
        let cty = state.cty.lock().unwrap();
        let (result, results, dx) = if log.def.general {
            // Nothing to score: skip evaluating what could be a very long log.
            let dx = (!call.is_empty()).then(|| dx_check(log, &cty, &draft)).flatten();
            (QsoResult::default(), vec![QsoResult::default(); log.qsos.len()], dx)
        } else {
            let result = engine::check(&log.def, &log.setup, &log.qsos, &draft);
            (result, engine::evaluate(&log.def, &log.setup, &log.qsos).results, None)
        };
        let previous = log
            .qsos
            .iter()
            .zip(results)
            .filter(|(q, _)| !call.is_empty() && q.call == call)
            .map(|(qso, result)| QsoRow {
                country: log.def.general.then(|| country_of(&cty, qso)).flatten(),
                qso: qso.clone(),
                result,
                new_dxcc: false,
            })
            .collect();
        Ok(CheckResult {
            result,
            sent_nr: draft.sent_nr,
            known_locator,
            known_exch,
            known_name,
            dx,
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
        if log.def.general {
            q.details.dxcc = state.cty.lock().unwrap().lookup(&q.call).map(|c| c.dxcc);
        }
        q.id = log.store.insert(&q)?;
        let rig = state.settings.lock().unwrap().rig.clone();
        if rig.spots
            && let Some(khz) = q.freq_khz
        {
            let hz = (khz * 1000.0).round() as u64;
            let text = format!("worked {} {}", q.band, q.time.format("%H%MZ"));
            let mode = tci::spot_mode(modes::contest(&q.mode), hz);
            match rig.kind {
                RigKind::Tci => state.rig.send(tci::spot(&q.call, mode, hz, &text)),
                RigKind::Flex => state.rig.send(flex::spot(&q.call, &mode.to_ascii_uppercase(), hz, &text)),
                _ => {}
            }
        }
        log.qsos.push(q);
        log.qsos.sort_by_key(|q| (q.time, q.id));
        Ok(view(log, &state.cty.lock().unwrap()))
    })
}

#[tauri::command]
pub fn update_qso(state: State<AppState>, qso: Qso) -> CmdResult<LogView> {
    with_log(&state, |log| {
        let mut qso = qso;
        qso.call = callsign::normalize(&qso.call);
        qso.rcvd_locator = geo::normalize_locator(&qso.rcvd_locator);
        qso.sent_locator = geo::normalize_locator(&qso.sent_locator);
        let old_call = log.qsos.iter().find(|q| q.id == qso.id).map(|q| q.call.clone());
        if old_call.is_some_and(|c| c != qso.call) {
            // A new call may be in another country: forget the old one's.
            for k in ["DXCC", "COUNTRY", "CONT", "CQZ", "ITUZ"] {
                qso.details.adif.remove(k);
            }
            qso.details.dxcc = if log.def.general {
                state.cty.lock().unwrap().lookup(&qso.call).map(|c| c.dxcc)
            } else {
                None
            };
        }
        log.store.update(&qso)?;
        let slot = log
            .qsos
            .iter_mut()
            .find(|q| q.id == qso.id)
            .ok_or("QSO not found")?;
        *slot = qso;
        log.qsos.sort_by_key(|q| (q.time, q.id));
        Ok(view(log, &state.cty.lock().unwrap()))
    })
}

#[tauri::command]
pub fn delete_qso(state: State<AppState>, id: i64) -> CmdResult<LogView> {
    with_log(&state, |log| {
        log.store.delete(id)?;
        log.qsos.retain(|q| q.id != id);
        Ok(view(log, &state.cty.lock().unwrap()))
    })
}

#[tauri::command]
pub fn cabrillo_text(state: State<AppState>) -> CmdResult<String> {
    with_log(&state, |log| {
        if log.def.general {
            return Err("General logs have no contest entry to export".into());
        }
        let eval = engine::evaluate(&log.def, &log.setup, &log.qsos);
        Ok(cabrillo::render(&log.def, &log.setup, &log.qsos, &eval.score, VERSION))
    })
}

#[tauri::command]
pub fn export_cabrillo(state: State<AppState>, path: String) -> CmdResult<()> {
    let text = cabrillo_text(state)?;
    std::fs::write(&path, text).map_err(|e| format!("{path}: {e}"))
}

fn in_range(t: DateTime<Utc>, from: Option<DateTime<Utc>>, to: Option<DateTime<Utc>>) -> bool {
    from.is_none_or(|f| t >= f) && to.is_none_or(|x| t <= x)
}

/// Export contacts from `from` to `to` (inclusive; either may be open), returning how many.
#[tauri::command]
pub fn export_adif(
    state: State<AppState>,
    path: String,
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
) -> CmdResult<usize> {
    let (text, n) = with_log(&state, |log| {
        let qsos: Vec<Qso> = log.qsos.iter().filter(|q| in_range(q.time, from, to)).cloned().collect();
        let cty = state.cty.lock().unwrap();
        Ok((adif::render(&log.def, &log.setup, &qsos, &cty, VERSION), qsos.len()))
    })?;
    std::fs::write(&path, text).map_err(|e| format!("{path}: {e}"))?;
    Ok(n)
}

/// Add the contacts in an ADIF file to the open log, skipping ones already in it.
#[tauri::command]
pub fn import_adif(
    state: State<AppState>,
    path: String,
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
) -> CmdResult<ImportResult> {
    let bytes = std::fs::read(&path).map_err(|e| format!("{path}: {e}"))?;
    let records = import::parse(&bytes);
    if records.is_empty() {
        return Err("No contacts found: is this an ADIF (.adi) file?".into());
    }
    with_log(&state, |log| {
        let (mut qsos, summary) = import::select(&records, &log.def, &log.setup, &log.qsos, from, to);
        if log.def.general {
            let cty = state.cty.lock().unwrap();
            for q in qsos.iter_mut().filter(|q| q.details.dxcc.is_none()) {
                q.details.dxcc = cty.lookup(&q.call).map(|c| c.dxcc);
            }
        }
        log.store.insert_all(&mut qsos)?;
        log.qsos.extend(qsos);
        log.qsos.sort_by_key(|q| (q.time, q.id));
        Ok(ImportResult {
            summary,
            view: view(log, &state.cty.lock().unwrap()),
        })
    })
}

#[tauri::command]
pub fn export_score_sheet(state: State<AppState>, path: String) -> CmdResult<()> {
    let text = with_log(&state, |log| {
        if log.def.general {
            return Err("General logs have no contest entry to export".into());
        }
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
pub async fn rig_models(app: AppHandle) -> CmdResult<Vec<RigModel>> {
    hamlib::models(app.path().resource_dir().ok().as_deref()).await
}

/// FlexRadios announcing themselves on the local network.
#[tauri::command]
pub async fn flex_discover() -> CmdResult<Vec<flex::Found>> {
    flex::discover(std::time::Duration::from_secs(3)).await
}

#[tauri::command]
pub async fn serial_ports() -> Vec<SerialPort> {
    hamlib::serial_ports()
}

#[tauri::command]
pub fn app_version() -> &'static str {
    VERSION
}

fn scp_status_of(state: &AppState) -> ScpStatus {
    ScpStatus {
        count: state.scp.lock().unwrap().len(),
        path: state.scp_path.display().to_string(),
        updated: std::fs::metadata(&state.scp_path)
            .and_then(|m| m.modified())
            .ok()
            .map(chrono::DateTime::<Utc>::from),
    }
}

/// Check `text` looks like MASTER.SCP, save it as ours and start using it.
fn install_scp(state: &AppState, text: &str) -> CmdResult<ScpStatus> {
    let db = Scp::parse(text);
    if db.len() < 10 || !db.calls().iter().take(100).all(|c| callsign::is_plausible(c)) {
        return Err("That doesn't look like a MASTER.SCP file (one callsign per line)".into());
    }
    if let Some(dir) = state.scp_path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(&state.scp_path, text).map_err(|e| format!("{}: {e}", state.scp_path.display()))?;
    *state.scp.lock().unwrap() = db;
    Ok(scp_status_of(state))
}

#[tauri::command]
pub fn scp_status(state: State<AppState>) -> ScpStatus {
    scp_status_of(&state)
}

#[tauri::command]
pub async fn scp_download(state: State<'_, AppState>) -> CmdResult<ScpStatus> {
    let fail = |e: reqwest::Error| format!("Downloading MASTER.SCP failed: {e}");
    let text = reqwest::Client::new()
        .get(scp::DOWNLOAD_URL)
        .timeout(std::time::Duration::from_secs(60))
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(fail)?
        .text()
        .await
        .map_err(fail)?;
    install_scp(&state, &text)
}

#[tauri::command]
pub fn scp_import(state: State<AppState>, path: String) -> CmdResult<ScpStatus> {
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"))?;
    install_scp(&state, &text)
}

/// Calls from MASTER.SCP and the open log that match what has been typed so far.
#[tauri::command]
pub fn scp_lookup(state: State<AppState>, partial: String) -> ScpMatches {
    let log_calls: Vec<String> = state
        .log
        .lock()
        .unwrap()
        .as_ref()
        .map(|l| l.qsos.iter().map(|q| q.call.clone()).collect())
        .unwrap_or_default();
    let db = state.scp.lock().unwrap();
    let sources = db.calls().iter().chain(log_calls.iter()).map(String::as_str);
    scp::lookup(&partial, sources, 30)
}

fn cty_status_of(state: &AppState) -> CtyStatus {
    CtyStatus {
        count: state.cty.lock().unwrap().len(),
        path: state.cty_path.display().to_string(),
        updated: std::fs::metadata(&state.cty_path)
            .and_then(|m| m.modified())
            .ok()
            .map(DateTime::<Utc>::from),
    }
}

/// Check `text` looks like cty.csv, save it as ours and start using it.
fn install_cty(state: &AppState, text: &str) -> CmdResult<CtyStatus> {
    let cty = Cty::parse(text);
    if cty.len() < 300 {
        return Err("That doesn't look like a cty.csv country file".into());
    }
    if let Some(dir) = state.cty_path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(&state.cty_path, text).map_err(|e| format!("{}: {e}", state.cty_path.display()))?;
    *state.cty.lock().unwrap() = cty;
    Ok(cty_status_of(state))
}

#[tauri::command]
pub fn cty_status(state: State<AppState>) -> CtyStatus {
    cty_status_of(&state)
}

#[tauri::command]
pub async fn cty_download(state: State<'_, AppState>) -> CmdResult<CtyStatus> {
    let fail = |e: reqwest::Error| format!("Downloading the country file failed: {e}");
    let text = reqwest::Client::new()
        .get(dxcc::DOWNLOAD_URL)
        .timeout(std::time::Duration::from_secs(60))
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(fail)?
        .text()
        .await
        .map_err(fail)?;
    install_cty(&state, &text)
}

#[tauri::command]
pub fn cty_import(state: State<AppState>, path: String) -> CmdResult<CtyStatus> {
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"))?;
    install_cty(&state, &text)
}

fn qrz_agent() -> String {
    format!("OpenVKCL/{VERSION}")
}

/// Log in with `config` (which may not be saved yet) to check it works.
#[tauri::command]
pub async fn qrz_test(state: State<'_, AppState>, config: QrzConfig) -> CmdResult<String> {
    state.qrz.test(&config, &qrz_agent()).await
}

#[tauri::command]
pub async fn qrz_lookup(state: State<'_, AppState>, call: String) -> CmdResult<Option<QrzInfo>> {
    let cfg = state.settings.lock().unwrap().qrz.clone();
    if !cfg.enabled || !callsign::is_plausible(&call) {
        return Ok(None);
    }
    state.qrz.lookup(&cfg, &qrz_agent(), &call).await
}
