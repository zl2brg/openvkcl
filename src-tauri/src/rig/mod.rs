mod flrig;
mod rigctld;

use crate::bands;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::async_runtime::JoinHandle;
use tauri::{AppHandle, Emitter};

pub const STATUS_EVENT: &str = "rig-status";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RigKind {
    #[default]
    None,
    Rigctld,
    Flrig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RigConfig {
    pub kind: RigKind,
    pub host: String,
    pub port: u16,
    pub poll_ms: u64,
}

impl Default for RigConfig {
    fn default() -> Self {
        RigConfig {
            kind: RigKind::None,
            host: "127.0.0.1".into(),
            port: 4532,
            poll_ms: 500,
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
    } else if ["USB", "LSB", "AM", "FM", "WFM", "FMN", "SAM", "DSB", "SSB"]
        .iter()
        .any(|x| m == *x || m.starts_with(&format!("{x}-")))
    {
        if m.contains("-D") { Some("DG") } else { Some("PH") }
    } else if m.starts_with("PKT") || m.starts_with("DATA") || m.starts_with("DIG") || m.starts_with("PSK") {
        Some("DG")
    } else {
        None
    }
}

enum Client {
    Rigctld(rigctld::Client),
    Flrig(flrig::Client),
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
            RigKind::None => Err("No rig configured".into()),
        }
    }

    async fn poll(&mut self) -> Result<(u64, String), String> {
        match self {
            Client::Rigctld(c) => c.poll().await,
            Client::Flrig(c) => c.poll().await,
        }
    }
}

#[derive(Default)]
pub struct Rig {
    task: Mutex<Option<JoinHandle<()>>>,
    status: Arc<Mutex<RigStatus>>,
}

fn publish(app: &AppHandle, shared: &Mutex<RigStatus>, status: RigStatus) {
    let mut cur = shared.lock().unwrap();
    if *cur != status {
        *cur = status.clone();
        let _ = app.emit(STATUS_EVENT, status);
    }
}

async fn run(app: AppHandle, cfg: RigConfig, shared: Arc<Mutex<RigStatus>>) {
    let poll = Duration::from_millis(cfg.poll_ms.clamp(100, 5000));
    loop {
        let error = match Client::connect(&cfg).await {
            Ok(mut client) => loop {
                match client.poll().await {
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
                        tokio::time::sleep(poll).await;
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
        let handle = tauri::async_runtime::spawn(run(app, cfg, shared));
        *self.task.lock().unwrap() = Some(handle);
    }

    pub fn stop(&self, app: &AppHandle) {
        if let Some(h) = self.task.lock().unwrap().take() {
            h.abort();
        }
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
        assert_eq!(contest_mode("???"), None);
    }
}
