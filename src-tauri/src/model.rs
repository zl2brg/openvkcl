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
    /// Bands claimed for each per-band bonus, keyed by the bonus's category.
    #[serde(default)]
    pub bonus_bands: BTreeMap<String, Vec<String>>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub freq_khz: Option<f64>,
    pub sent_rst: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub sent_nr: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub sent_locator: String,
    pub rcvd_rst: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub rcvd_nr: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub rcvd_locator: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub operator: String,
    /// The contest's `number` field (e.g. NZART branch), sent and received.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub sent_exch: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub rcvd_exch: String,
    #[serde(flatten, default)]
    pub details: QsoDetails,
}

/// General logging fields: who the station was, the country and QSL status.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct QsoDetails {
    #[serde(skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub qth: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub comment: String,
    /// ADIF DXCC entity number, from an import or the country file when logged.
    /// Not sent to the UI, which gets the country with it; editing keeps it.
    #[serde(skip_serializing)]
    pub dxcc: Option<u32>,
    /// ADIF QSL status values: Y (yes), N (no), R (requested), Q (queued), I (ignore).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub qsl_sent: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub qsl_rcvd: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub lotw_sent: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub lotw_rcvd: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub eqsl_sent: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub eqsl_rcvd: String,
    /// Other ADIF fields from an import, written back out on export. Not sent
    /// to the UI, which has no use for them; editing a contact keeps them.
    #[serde(skip_serializing)]
    pub adif: BTreeMap<String, String>,
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
    pub name: String,
    pub qth: String,
    pub comment: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn sent_to_the_ui() {
        let q = Qso {
            id: 7,
            time: Utc.with_ymd_and_hms(2026, 11, 21, 8, 51, 0).unwrap(),
            call: "M8JJM".into(),
            band: "15m".into(),
            mode: "SSB".into(),
            freq_khz: None,
            sent_rst: "55".into(),
            sent_nr: String::new(),
            sent_locator: String::new(),
            rcvd_rst: "59".into(),
            rcvd_nr: String::new(),
            rcvd_locator: String::new(),
            operator: String::new(),
            sent_exch: String::new(),
            rcvd_exch: String::new(),
            details: QsoDetails {
                name: "John".into(),
                dxcc: Some(223),
                adif: [("APP_N1MM_ID".to_string(), "7877a8ea".to_string())].into(),
                ..Default::default()
            },
        };
        // Empty and default values, the stored DXCC number and the extra ADIF
        // fields stay behind.
        let json = serde_json::to_string(&q).unwrap();
        assert_eq!(
            json,
            r#"{"id":7,"time":"2026-11-21T08:51:00Z","call":"M8JJM","band":"15m","mode":"SSB","sent_rst":"55","rcvd_rst":"59","name":"John"}"#
        );
        // What the UI sends back reads with those left at their defaults.
        let back: Qso = serde_json::from_str(&json).unwrap();
        assert_eq!((back.details.name.as_str(), back.details.dxcc), ("John", None));
        assert!(back.details.adif.is_empty() && back.sent_nr.is_empty() && back.freq_khz.is_none());
    }
}
