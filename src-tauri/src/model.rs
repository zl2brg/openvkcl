use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Station {
    pub callsign: String,
    pub operators: String,
    pub name: String,
    pub email: String,
    pub address: String,
    pub city: String,
    pub state: String,
    pub postcode: String,
    pub country: String,
    pub club: String,
    pub locator: String,
}

/// Per-log contest setup, stored inside the log file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogSetup {
    pub contest_id: String,
    pub start: DateTime<Utc>,
    pub station: Station,
    /// Sent for `number` exchanges, e.g. years licensed or NZART branch.
    #[serde(default)]
    pub sent_number: String,
    /// Number contacts in a separate serial series on each band.
    #[serde(default)]
    pub serial_per_band: bool,
    /// Station's offset from UTC, used for local-time scoring bonuses.
    #[serde(default)]
    pub utc_offset_minutes: i32,
    #[serde(default)]
    pub location: String,
    #[serde(default)]
    pub categories: BTreeMap<String, String>,
    #[serde(default)]
    pub soapbox: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Qso {
    pub id: i64,
    pub time: DateTime<Utc>,
    pub call: String,
    pub band: String,
    /// Cabrillo mode: PH, CW, RY or DG.
    pub mode: String,
    pub freq_khz: Option<f64>,
    pub sent_rst: String,
    pub sent_nr: String,
    pub sent_locator: String,
    pub rcvd_rst: String,
    pub rcvd_nr: String,
    pub rcvd_locator: String,
    pub operator: String,
    /// The contest's `number` field (e.g. NZART branch), sent and received.
    #[serde(default)]
    pub sent_exch: String,
    #[serde(default)]
    pub rcvd_exch: String,
}

/// A contact as entered on the entry line; the sent exchange is filled in by the backend.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct QsoInput {
    pub time: Option<DateTime<Utc>>,
    pub call: String,
    pub band: String,
    pub mode: String,
    pub freq_khz: Option<f64>,
    pub sent_rst: String,
    pub rcvd_rst: String,
    pub rcvd_nr: String,
    pub rcvd_exch: String,
    pub rcvd_locator: String,
}
