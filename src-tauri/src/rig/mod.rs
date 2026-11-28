pub mod flex;
mod flrig;
pub mod hamlib;
mod rigctld;
pub mod tci;

use crate::bands;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::async_runtime::JoinHandle;
use tokio::sync::mpsc;
use tauri::{AppHandle, Emitter, Manager};

pub const STATUS_EVENT: &str = "rig-status";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RigKind {
    #[default]
    None,
    /// A radio on a serial or USB port, through a rigctld we start ourselves.
    Hamlib,
    Rigctld,
    Flrig,
    /// ExpertSDR / cudaSDR Transceiver Control Interface.
    Tci,
    /// FlexRadio 6000, 8000 and Aurora series over the SmartSDR API.
    Flex,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RigConfig {
    pub kind: RigKind,
    pub host: String,
    pub port: u16,
    pub poll_ms: u64,
    /// TCI and FlexRadio: mark logged contacts on the SDR's band display.
    pub spots: bool,
    /// Hamlib: rig model number, serial port and baud rate (0 for the radio's default).
    pub model: u32,
    pub serial_port: String,
    pub baud: u32,
    /// Hamlib: extra rigctld options, e.g. `--set-conf=civaddr=0xA2`.
    pub rigctld_args: String,
}

impl Default for RigConfig {
    fn default() -> Self {
        RigConfig {
            kind: RigKind::None,
            host: "127.0.0.1".into(),
            port: 4532,
            poll_ms: 500,
            spots: true,
            model: 0,
            serial_port: String::new(),
            baud: 0,
            rigctld_args: String::new(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct RigStatus {
    pub connected: bool,
    pub freq_hz: Option<u64>,
    pub band: Option<String>,
    /// Contest mode (PH, CW, RY, DG) derived from the radio's mode.
    pub mode: Option<String>,
    pub rig_mode: Option<String>,
    pub error: Option<String>,
}

/// Map a Hamlib or FLRig mode name to a Cabrillo contest mode.
pub fn contest_mode(rig_mode: &str) -> Option<&'static str> {
    let m = rig_mode.trim().to_ascii_uppercase();
    if m.starts_with("CW") {
        Some("CW")
    } else if m.starts_with("RTTY") || m == "FSK" || m == "FSK-R" {
        Some("RY")
    } else if ["USB", "LSB", "AM", "FM", "WFM", "FMN", "NFM", "SAM", "DSB", "SSB"]
        .iter()
        .any(|x| m == *x || m.starts_with(&format!("{x}-")))
    {
        if m.contains("-D") { Some("DG") } else { Some("PH") }
    } else if ["D-STAR", "DSTAR", "DV", "FDV", "FREEDV"].contains(&m.as_str()) {
        // Digital voice is a phone mode for contest purposes.
        Some("PH")
    } else if m.starts_with("PKT") || m.starts_with("DATA") || m.starts_with("DIG") || m.starts_with("PSK") {
        Some("DG")
    } else {
        None
    }
}

enum Client {
    Hamlib(hamlib::Client),
    Rigctld(rigctld::Client),
    Flrig(flrig::Client),
    Tci(Box<tci::Client>),
    Flex(Box<flex::Client>),
}

impl Client {
    async fn connect(cfg: &RigConfig, app: &AppHandle, process: &hamlib::Process) -> Result<Self, String> {
        match cfg.kind {
            RigKind::Hamlib => {
                let resources = app.path().resource_dir().ok();
                hamlib::Client::start(cfg, resources.as_deref(), process.clone())
                    .await
                    .map(Client::Hamlib)
            }
            RigKind::Rigctld => rigctld::Client::connect(&cfg.host, cfg.port)
                .await
                .map(Client::Rigctld),
            RigKind::Flrig => flrig::Client::connect(&cfg.host, cfg.port)
                .await
                .map(Client::Flrig),
            RigKind::Tci => tci::Client::connect(&cfg.host, cfg.port)
                .await
                .map(|c| Client::Tci(Box::new(c))),
            RigKind::Flex => flex::Client::connect(&cfg.host, cfg.port)
                .await
                .map(|c| Client::Flex(Box::new(c))),
            RigKind::None => Err("No rig configured".into()),
        }
    }

    /// The radio's frequency and mode, at most `interval` after the last call.
    /// The radio's frequency and mode, or None when it's connected but has none
    /// to report (a FlexRadio with no slice open).
    async fn poll(&mut self, interval: Duration) -> Result<Option<(u64, String)>, String> {
        match self {
            Client::Hamlib(c) => {
                tokio::time::sleep(interval).await;
                c.poll().await.map(Some)
            }
            Client::Rigctld(c) => {
                tokio::time::sleep(interval).await;
                c.poll().await.map(Some)
            }
            Client::Flrig(c) => {
                tokio::time::sleep(interval).await;
                c.poll().await.map(Some)
            }
            Client::Tci(c) => c.poll(interval).await.map(Some),
            Client::Flex(c) => c.poll(interval).await,
        }
    }

    /// Send a raw command to the radio's control server; only TCI and FlexRadio accept any.
    async fn send(&mut self, text: &str) -> Result<(), String> {
        match self {
            Client::Tci(c) => c.send(text).await,
            Client::Flex(c) => c.send(text).await,
            _ => Ok(()),
        }
    }

    /// Whether the radio pushes changes (TCI, FlexRadio) rather than being polled.
    fn pushes(&self) -> bool {
        matches!(self, Client::Tci(_) | Client::Flex(_))
    }

    async fn run(&mut self, cmd: Command) -> Result<(), String> {
        match cmd {
            Command::Raw(text) => self.send(&text).await,
        }
    }
}

/// Something for the radio connection to do between polls.
enum Command {
    /// Text for the radio's control server (TCI and FlexRadio spots).
    Raw(String),
}

#[derive(Default)]
pub struct Rig {
    task: Mutex<Option<JoinHandle<()>>>,
    status: Arc<Mutex<RigStatus>>,
    /// Commands queued for the radio's control server (TCI and FlexRadio spots).
    outbox: Mutex<Option<mpsc::UnboundedSender<Command>>>,
    /// The rigctld started for the current connection, if any.
    rigctld: Mutex<hamlib::Process>,
    /// How the radio is connected.
    kind: Mutex<RigKind>,
}

fn publish(app: &AppHandle, shared: &Mutex<RigStatus>, status: RigStatus) {
    {
        let mut cur = shared.lock().unwrap();
        if *cur == status {
            return;
        }
        *cur = status.clone();
    }
    // Emit after the lock is released. The window asks for this status on the
    // UI thread; holding the lock across the emit deadlocks that thread and
    // the clock and buttons stop.
    let _ = app.emit(STATUS_EVENT, status);
}

async fn run(
    app: AppHandle,
    cfg: RigConfig,
    shared: Arc<Mutex<RigStatus>>,
    mut outbox: mpsc::UnboundedReceiver<Command>,
    process: hamlib::Process,
) {
    let poll = Duration::from_millis(cfg.poll_ms.clamp(100, 5000));
    loop {
        let error = match Client::connect(&cfg, &app, &process).await {
            Ok(mut client) => loop {
                let first = !shared.lock().unwrap().connected;
                let wait = if first { Duration::ZERO } else { poll };
                // Commands (PTT especially) go out as soon as they arrive, not
                // at the next poll. Radios that push changes are waited on
                // directly; polled ones only during the pause between polls,
                // so a question and its answer are never cut apart.
                let polled = if client.pushes() {
                    tokio::select! {
                        Some(cmd) = outbox.recv() => Err(cmd),
                        result = client.poll(wait) => Ok(result),
                    }
                } else {
                    tokio::select! {
                        Some(cmd) = outbox.recv() => Err(cmd),
                        _ = tokio::time::sleep(wait) => Ok(client.poll(Duration::ZERO).await),
                    }
                };
                let polled = match polled {
                    Ok(result) => result,
                    Err(cmd) => match client.run(cmd).await {
                        Ok(()) => continue,
                        Err(e) => break e,
                    },
                };
                match polled {
                    Ok(None) => publish(
                        &app,
                        &shared,
                        RigStatus {
                            connected: true,
                            ..Default::default()
                        },
                    ),
                    Ok(Some((hz, rig_mode))) => {
                        let status = RigStatus {
                            connected: true,
                            freq_hz: Some(hz),
                            band: bands::from_khz(hz as f64 / 1000.0).map(|b| b.name.to_string()),
                            mode: contest_mode(&rig_mode).map(str::to_string),
                            rig_mode: Some(rig_mode),
                            error: None,
                        };
                        publish(&app, &shared, status);
                    }
                    Err(e) => break e,
                }
            },
            Err(e) => e,
        };
        publish(
            &app,
            &shared,
            RigStatus {
                error: Some(error),
                ..Default::default()
            },
        );
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

impl Rig {
    pub fn start(&self, app: AppHandle, cfg: RigConfig) {
        self.stop(&app);
        *self.kind.lock().unwrap() = cfg.kind;
        if cfg.kind == RigKind::None {
            return;
        }
        let shared = self.status.clone();
        let (tx, rx) = mpsc::unbounded_channel();
        *self.outbox.lock().unwrap() = Some(tx);
        let process = hamlib::Process::default();
        *self.rigctld.lock().unwrap() = process.clone();
        let handle = tauri::async_runtime::spawn(run(app, cfg, shared, rx, process));
        *self.task.lock().unwrap() = Some(handle);
    }

    /// Queue a command for the radio's control server; dropped if none is running.
    pub fn send(&self, cmd: String) {
        if let Some(tx) = self.outbox.lock().unwrap().as_ref() {
            let _ = tx.send(Command::Raw(cmd));
        }
    }

    pub fn stop(&self, app: &AppHandle) {
        if let Some(h) = self.task.lock().unwrap().take() {
            h.abort();
        }
        *self.outbox.lock().unwrap() = None;
        self.shutdown();
        publish(app, &self.status, RigStatus::default());
    }

    /// Stop any rigctld we started; called when the app exits.
    pub fn shutdown(&self) {
        self.rigctld.lock().unwrap().kill();
    }

    pub fn status(&self) -> RigStatus {
        self.status.lock().unwrap().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::contest_mode;

    #[test]
    fn modes() {
        assert_eq!(contest_mode("USB"), Some("PH"));
        assert_eq!(contest_mode("FM"), Some("PH"));
        assert_eq!(contest_mode("CWR"), Some("CW"));
        assert_eq!(contest_mode("RTTY"), Some("RY"));
        assert_eq!(contest_mode("PKTUSB"), Some("DG"));
        assert_eq!(contest_mode("USB-D"), Some("DG"));
        assert_eq!(contest_mode("D-STAR"), Some("PH"));
        assert_eq!(contest_mode("DIGU"), Some("DG"));
        assert_eq!(contest_mode("FDV"), Some("PH"));
        assert_eq!(contest_mode("???"), None);
    }
}
