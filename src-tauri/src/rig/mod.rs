mod flrig;
mod rigctld;
pub mod tci;

use crate::bands;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::async_runtime::JoinHandle;
use tokio::sync::mpsc;
use tauri::{AppHandle, Emitter};

pub const STATUS_EVENT: &str = "rig-status";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RigKind {
    #[default]
    None,
    Rigctld,
    Flrig,
    /// ExpertSDR / cudaSDR Transceiver Control Interface.
    Tci,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RigConfig {
    pub kind: RigKind,
    pub host: String,
    pub port: u16,
    pub poll_ms: u64,
    /// TCI: mark logged contacts on the SDR's band display.
    pub spots: bool,
}

impl Default for RigConfig {
    fn default() -> Self {
        RigConfig {
            kind: RigKind::None,
            host: "127.0.0.1".into(),
            port: 4532,
            poll_ms: 500,
            spots: true,
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
    Rigctld(rigctld::Client),
    Flrig(flrig::Client),
    Tci(Box<tci::Client>),
}

impl Client {
    async fn connect(cfg: &RigConfig) -> Result<Self, String> {
        match cfg.kind {
            RigKind::Rigctld => rigctld::Client::connect(&cfg.host, cfg.port)
                .await
                .map(Client::Rigctld),
            RigKind::Flrig => flrig::Client::connect(&cfg.host, cfg.port)
                .await
                .map(Client::Flrig),
            RigKind::Tci => tci::Client::connect(&cfg.host, cfg.port)
                .await
                .map(|c| Client::Tci(Box::new(c))),
            RigKind::None => Err("No rig configured".into()),
        }
    }

    /// The radio's frequency and mode, at most `interval` after the last call.
    async fn poll(&mut self, interval: Duration) -> Result<(u64, String), String> {
        match self {
            Client::Rigctld(c) => {
                tokio::time::sleep(interval).await;
                c.poll().await
            }
            Client::Flrig(c) => {
                tokio::time::sleep(interval).await;
                c.poll().await
            }
            Client::Tci(c) => c.poll(interval).await,
        }
    }

    /// Send a raw command to the radio's control server; only TCI accepts any.
    async fn send(&mut self, text: &str) -> Result<(), String> {
        match self {
            Client::Tci(c) => c.send(text).await,
            _ => Ok(()),
        }
    }
}

#[derive(Default)]
pub struct Rig {
    task: Mutex<Option<JoinHandle<()>>>,
    status: Arc<Mutex<RigStatus>>,
    /// Commands queued for the radio's control server (TCI spots).
    outbox: Mutex<Option<mpsc::UnboundedSender<String>>>,
}

fn publish(app: &AppHandle, shared: &Mutex<RigStatus>, status: RigStatus) {
    let mut cur = shared.lock().unwrap();
    if *cur != status {
        *cur = status.clone();
        let _ = app.emit(STATUS_EVENT, status);
    }
}

async fn run(
    app: AppHandle,
    cfg: RigConfig,
    shared: Arc<Mutex<RigStatus>>,
    mut outbox: mpsc::UnboundedReceiver<String>,
) {
    let poll = Duration::from_millis(cfg.poll_ms.clamp(100, 5000));
    loop {
        let error = match Client::connect(&cfg).await {
            Ok(mut client) => loop {
                let mut sent = Ok(());
                while let Ok(cmd) = outbox.try_recv() {
                    sent = sent.and(client.send(&cmd).await);
                }
                if let Err(e) = sent {
                    break e;
                }
                let first = !shared.lock().unwrap().connected;
                let wait = if first { Duration::ZERO } else { poll };
                match client.poll(wait).await {
                    Ok((hz, rig_mode)) => {
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
        if cfg.kind == RigKind::None {
            return;
        }
        let shared = self.status.clone();
        let (tx, rx) = mpsc::unbounded_channel();
        *self.outbox.lock().unwrap() = Some(tx);
        let handle = tauri::async_runtime::spawn(run(app, cfg, shared, rx));
        *self.task.lock().unwrap() = Some(handle);
    }

    /// Queue a command for the radio's control server; dropped if none is running.
    pub fn send(&self, cmd: String) {
        if let Some(tx) = self.outbox.lock().unwrap().as_ref() {
            let _ = tx.send(cmd);
        }
    }

    pub fn stop(&self, app: &AppHandle) {
        if let Some(h) = self.task.lock().unwrap().take() {
            h.abort();
        }
        *self.outbox.lock().unwrap() = None;
        publish(app, &self.status, RigStatus::default());
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
        assert_eq!(contest_mode("???"), None);
    }
}
