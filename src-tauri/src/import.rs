//! Reading ADIF (.adi) files into a log.

use crate::bands;
use crate::contest::ContestDef;
use crate::contest::def::ExchField;
use crate::model::{LogSetup, Qso, QsoDetails};
use crate::{callsign, geo, modes};
use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
use serde::Serialize;
use std::collections::{BTreeMap, HashSet};

/// One ADIF record: field names in upper case, and their values.
pub type Record = BTreeMap<String, String>;

/// Fields read into the log's own columns; everything else is kept as-is.
const MAPPED: &[&str] = &[
    "CALL", "QSO_DATE", "TIME_ON", "BAND", "FREQ", "MODE", "SUBMODE", "RST_SENT", "RST_RCVD", "STX", "SRX",
    "STX_STRING", "SRX_STRING", "GRIDSQUARE", "MY_GRIDSQUARE", "OPERATOR", "NAME", "QTH", "COMMENT", "DXCC",
    "QSL_SENT", "QSL_RCVD", "LOTW_QSL_SENT", "LOTW_QSL_RCVD", "EQSL_QSL_SENT", "EQSL_QSL_RCVD", "CONTEST_ID",
];

/// Latin-1 if the bytes aren't UTF-8, as older loggers write.
fn decode(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => bytes.iter().map(|&b| b as char).collect(),
    }
}

fn find_ci(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w.eq_ignore_ascii_case(needle))
}

/// Parse ADIF records. Text before `<EOH>` is the header and is skipped.
pub fn parse(bytes: &[u8]) -> Vec<Record> {
    let mut pos = find_ci(bytes, b"<EOH>").map_or(0, |i| i + 5);
    let mut records = Vec::new();
    let mut rec = Record::new();
    while let Some(lt) = bytes[pos..].iter().position(|&b| b == b'<') {
        let start = pos + lt + 1;
        let Some(gt) = bytes[start..].iter().position(|&b| b == b'>') else { break };
        let tag = decode(&bytes[start..start + gt]);
        pos = start + gt + 1;
        let mut parts = tag.split(':');
        let name = parts.next().unwrap_or("").trim().to_ascii_uppercase();
        if name == "EOR" {
            if !rec.is_empty() {
                records.push(std::mem::take(&mut rec));
            }
            continue;
        }
        let Some(len) = parts.next().and_then(|l| l.trim().parse::<usize>().ok()) else {
            continue;
        };
        let end = (pos + len).min(bytes.len());
        let value = decode(&bytes[pos..end]).trim().to_string();
        pos = end;
        if !name.is_empty() && !value.is_empty() {
            rec.insert(name, value);
        }
    }
    records
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ImportSummary {
    pub imported: usize,
    /// Already in the log, or repeated in the file.
    pub duplicates: usize,
    /// Outside the chosen dates.
    pub out_of_range: usize,
    /// Outside the contest period, or on a band or mode the contest doesn't use.
    pub not_in_contest: usize,
    /// Missing a callsign, date, time or band.
    pub unreadable: usize,
}

enum Skip {
    Unreadable,
    NotInContest,
}

fn time_of(rec: &Record) -> Option<DateTime<Utc>> {
    let date = NaiveDate::parse_from_str(rec.get("QSO_DATE")?, "%Y%m%d").ok()?;
    let t = rec.get("TIME_ON")?;
    let time = match t.len() {
        4 => NaiveTime::parse_from_str(t, "%H%M").ok()?,
        6 => NaiveTime::parse_from_str(t, "%H%M%S").ok()?,
        _ => return None,
    };
    Some(date.and_time(time).and_utc())
}

fn get(rec: &Record, name: &str) -> String {
    rec.get(name).cloned().unwrap_or_default()
}

/// A contact from an ADIF record, made to fit the log's contest (or general log).
fn to_qso(rec: &Record, def: &ContestDef, setup: &LogSetup) -> Result<Qso, Skip> {
    let call = callsign::normalize(&get(rec, "CALL"));
    let time = time_of(rec).ok_or(Skip::Unreadable)?;
    if !callsign::is_plausible(&call) {
        return Err(Skip::Unreadable);
    }
    let freq_mhz = rec.get("FREQ").and_then(|f| f.trim().parse::<f64>().ok());
    let freq_khz = freq_mhz.map(|f| f * 1000.0);
    let band = rec
        .get("BAND")
        .and_then(|b| bands::by_name(b))
        .or_else(|| freq_khz.and_then(bands::from_khz))
        .ok_or(Skip::Unreadable)?
        .name
        .to_string();
    let mode = modes::from_adif(&get(rec, "MODE"), &get(rec, "SUBMODE"));
    if mode.is_empty() {
        return Err(Skip::Unreadable);
    }
    let mode = if def.general { mode } else { modes::contest(&mode).to_string() };
    if !def.general
        && (!def.bands.contains(&band) || !def.modes.contains(&mode) || time < setup.start || time >= def.end(setup.start))
    {
        return Err(Skip::NotInContest);
    }

    let mut sent_exch = get(rec, "STX_STRING");
    let mut rcvd_exch = get(rec, "SRX_STRING");
    if !def.general && !def.has_field(ExchField::Number) {
        sent_exch.clear();
        rcvd_exch.clear();
    }
    if def.has_field(ExchField::Number) && sent_exch.is_empty() {
        sent_exch = setup.sent_number.clone();
    }
    let mut sent_locator = geo::normalize_locator(&get(rec, "MY_GRIDSQUARE"));
    if def.has_field(ExchField::Locator) && sent_locator.is_empty() {
        sent_locator = geo::normalize_locator(&setup.station.locator);
    }
    let adif: BTreeMap<String, String> = rec
        .iter()
        .filter(|(k, _)| !MAPPED.contains(&k.as_str()))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    Ok(Qso {
        id: 0,
        time,
        call,
        band,
        mode,
        freq_khz,
        sent_rst: get(rec, "RST_SENT"),
        sent_nr: get(rec, "STX"),
        sent_locator,
        rcvd_rst: get(rec, "RST_RCVD"),
        rcvd_nr: get(rec, "SRX"),
        rcvd_locator: geo::normalize_locator(&get(rec, "GRIDSQUARE")),
        operator: get(rec, "OPERATOR").to_ascii_uppercase(),
        sent_exch,
        rcvd_exch,
        details: QsoDetails {
            name: get(rec, "NAME"),
            qth: get(rec, "QTH"),
            comment: get(rec, "COMMENT"),
            dxcc: rec.get("DXCC").and_then(|d| d.trim().parse().ok()).filter(|&d| d > 0),
            qsl_sent: get(rec, "QSL_SENT").to_ascii_uppercase(),
            qsl_rcvd: get(rec, "QSL_RCVD").to_ascii_uppercase(),
            lotw_sent: get(rec, "LOTW_QSL_SENT").to_ascii_uppercase(),
            lotw_rcvd: get(rec, "LOTW_QSL_RCVD").to_ascii_uppercase(),
            eqsl_sent: get(rec, "EQSL_QSL_SENT").to_ascii_uppercase(),
            eqsl_rcvd: get(rec, "EQSL_QSL_RCVD").to_ascii_uppercase(),
            adif,
        },
    })
}

/// Same station, minute, band and mode: the same contact.
fn dupe_key(q: &Qso) -> (String, i64, String, String) {
    (q.call.clone(), q.time.timestamp() / 60, q.band.clone(), q.mode.clone())
}

/// The contacts in `records` to add to a log already holding `existing`, between
/// `from` and `to` (inclusive, either open-ended), with counts of what was skipped.
pub fn select(
    records: &[Record],
    def: &ContestDef,
    setup: &LogSetup,
    existing: &[Qso],
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
) -> (Vec<Qso>, ImportSummary) {
    let mut seen: HashSet<_> = existing.iter().map(dupe_key).collect();
    let mut summary = ImportSummary::default();
    let mut out = Vec::new();
    for rec in records {
        match to_qso(rec, def, setup) {
            Err(Skip::Unreadable) => summary.unreadable += 1,
            Err(Skip::NotInContest) => summary.not_in_contest += 1,
            Ok(q) if from.is_some_and(|f| q.time < f) || to.is_some_and(|t| q.time > t) => {
                summary.out_of_range += 1
            }
            Ok(q) => {
                if seen.insert(dupe_key(&q)) {
                    out.push(q);
                } else {
                    summary.duplicates += 1;
                }
            }
        }
    }
    summary.imported = out.len();
    (out, summary)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contest::load_all;
    use crate::model::Station;
    use chrono::TimeZone;

    const FILE: &str = "Exported by some logger
<ADIF_VER:5>3.1.4 <PROGRAMID:4>Test
<EOH>
<CALL:6>JA1ABC <QSO_DATE:8>20260105 <TIME_ON:6>083015 <BAND:3>20m <FREQ:9>14.074000
<MODE:4>MFSK <SUBMODE:3>FT4 <RST_SENT:3>-10 <RST_RCVD:3>-05 <NAME:4>Taro <LOTW_QSL_RCVD:1>Y
<DXCC:3>339 <TX_PWR:3>100 <STATION_CALLSIGN:6>VK3AAA <eor>
<call:6>JA1ABC <qso_date:8>20260105 <time_on:4>0830 <band:3>20M <mode:4>MFSK <submode:3>FT4 <eor>
<CALL:5>VK2AB <QSO_DATE:8>19991231 <TIME_ON:4>2359 <FREQ:5>7.090 <MODE:3>SSB <EOR>
<CALL:5>VK2AB <QSO_DATE:8>20260815 <TIME_ON:4>0310 <BAND:3>40m <MODE:3>SSB <RST_SENT:2>59
<RST_RCVD:2>57 <SRX_STRING:2>30 <STX_STRING:2>12 <EOR>
<CALL:5>VK2CD <QSO_DATE:8>20260815 <TIME_ON:4>0311 <BAND:3>30m <MODE:3>SSB <EOR>
<CALL:5>VK2EF <QSO_DATE:8>20260815 <TIME_ON:4>0312 <BAND:3>40m <MODE:3>FT8 <EOR>
<CALL:2>?? <QSO_DATE:8>20260815 <TIME_ON:4>0312 <BAND:3>40m <MODE:3>SSB <EOR>
<CALL:5>VK2GH <QSO_DATE:8>20260815 <BAND:3>40m <MODE:3>SSB <EOR>
";

    fn def(id: &str) -> ContestDef {
        load_all(None).0.into_iter().find(|d| d.id == id).unwrap()
    }

    fn setup(id: &str) -> LogSetup {
        LogSetup {
            contest_id: id.into(),
            start: Utc.with_ymd_and_hms(2026, 8, 15, 3, 0, 0).unwrap(),
            station: Station {
                callsign: "VK3AAA".into(),
                ..Default::default()
            },
            sent_number: "12".into(),
            serial_per_band: false,
            utc_offset_minutes: 600,
            location: String::new(),
            categories: Default::default(),
            bonus_bands: Default::default(),
            soapbox: String::new(),
            generic: Default::default(),
        }
    }

    #[test]
    fn general_log() {
        let records = parse(FILE.as_bytes());
        assert_eq!(records.len(), 8);
        let d = def("general");
        let (qsos, s) = select(&records, &d, &setup("general"), &[], None, None);
        assert_eq!((s.imported, s.duplicates, s.unreadable, s.not_in_contest), (5, 1, 2, 0));
        let ja = &qsos[0];
        assert_eq!(ja.time, Utc.with_ymd_and_hms(2026, 1, 5, 8, 30, 15).unwrap());
        assert_eq!((ja.band.as_str(), ja.mode.as_str()), ("20m", "FT4"));
        assert_eq!(ja.freq_khz, Some(14074.0));
        assert_eq!((ja.sent_rst.as_str(), ja.rcvd_rst.as_str()), ("-10", "-05"));
        assert_eq!(ja.details.name, "Taro");
        assert_eq!(ja.details.lotw_rcvd, "Y");
        assert_eq!(ja.details.dxcc, Some(339));
        assert_eq!(ja.details.adif.get("TX_PWR").map(String::as_str), Some("100"));
        assert!(!ja.details.adif.contains_key("SUBMODE"));
        // Band from frequency.
        assert_eq!(qsos[1].band, "40m");

        // Only from 1 January 2026; the repeat of an existing contact is a duplicate.
        let from = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let (again, s) = select(&records, &d, &setup("general"), &qsos[..1], Some(from), None);
        assert_eq!((s.imported, s.duplicates, s.out_of_range), (3, 2, 1));
        assert!(again.iter().all(|q| q.time >= from));
    }

    #[test]
    fn contest_log() {
        let records = parse(FILE.as_bytes());
        let (qsos, s) = select(&records, &def("wia-rd"), &setup("wia-rd"), &[], None, None);
        // VK2AB in the contest; VK2EF's FT8 counts as DG, which RD doesn't use.
        assert_eq!((s.imported, s.not_in_contest, s.unreadable), (1, 5, 2));
        let q = &qsos[0];
        assert_eq!((q.mode.as_str(), q.rcvd_exch.as_str(), q.sent_exch.as_str()), ("PH", "30", "12"));
    }
}
