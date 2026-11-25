use crate::bands;
use crate::contest::ContestDef;
use crate::contest::def::ExchField;
use crate::dxcc::Cty;
use crate::model::{LogSetup, Qso};
use crate::modes;
use std::collections::HashSet;
use std::fmt::Write;

/// Writes `<NAME:len>value` fields, each name at most once per record.
struct Record<'a> {
    out: &'a mut String,
    written: HashSet<String>,
}

impl Record<'_> {
    fn field(&mut self, name: &str, value: &str) {
        if !value.is_empty() && self.written.insert(name.to_string()) {
            let _ = write!(self.out, "<{name}:{}>{value} ", value.len());
        }
    }
}

fn header(out: &mut String, name: &str, value: &str) {
    let _ = write!(out, "<{name}:{}>{value} ", value.len());
}

pub fn render(def: &ContestDef, setup: &LogSetup, qsos: &[Qso], cty: &Cty, version: &str) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "OpenVKCL export: {}", def.name);
    header(&mut out, "ADIF_VER", "3.1.4");
    header(&mut out, "PROGRAMID", "OpenVKCL");
    header(&mut out, "PROGRAMVERSION", version);
    out.push_str("<EOH>\n");

    let mycall = setup.station.callsign.to_ascii_uppercase();
    let serial = def.has_field(ExchField::Serial);
    for q in qsos {
        let d = &q.details;
        let mut r = Record {
            out: &mut out,
            written: HashSet::new(),
        };
        r.field("CALL", &q.call.to_ascii_uppercase());
        r.field("QSO_DATE", &q.time.format("%Y%m%d").to_string());
        r.field("TIME_ON", &q.time.format("%H%M%S").to_string());
        r.field("BAND", &bands::adif_band(&q.band));
        let (mode, submode) = modes::to_adif(&q.mode);
        r.field("MODE", mode);
        r.field("SUBMODE", submode);
        if let Some(f) = q.freq_khz {
            r.field("FREQ", &format!("{:.4}", f / 1000.0));
        }
        r.field("RST_SENT", &q.sent_rst);
        r.field("RST_RCVD", &q.rcvd_rst);
        if serial || def.general {
            r.field("STX", &q.sent_nr);
            r.field("SRX", &q.rcvd_nr);
        }
        if def.general {
            r.field("STX_STRING", &q.sent_exch);
            r.field("SRX_STRING", &q.rcvd_exch);
        } else {
            let info = |nr: &str, exch: &str| if exch.is_empty() { nr.to_string() } else { exch.to_string() };
            r.field("STX_STRING", &info(&q.sent_nr, &q.sent_exch));
            r.field("SRX_STRING", &info(&q.rcvd_nr, &q.rcvd_exch));
        }
        r.field("GRIDSQUARE", &q.rcvd_locator.to_ascii_uppercase());
        r.field("MY_GRIDSQUARE", &q.sent_locator.to_ascii_uppercase());
        r.field("OPERATOR", &q.operator.to_ascii_uppercase());
        if !def.general {
            r.field("CONTEST_ID", &def.cabrillo_name);
        }
        r.field("NAME", &d.name);
        r.field("QTH", &d.qth);
        r.field("COMMENT", &d.comment);
        r.field("QSL_SENT", &d.qsl_sent);
        r.field("QSL_RCVD", &d.qsl_rcvd);
        r.field("LOTW_QSL_SENT", &d.lotw_sent);
        r.field("LOTW_QSL_RCVD", &d.lotw_rcvd);
        r.field("EQSL_QSL_SENT", &d.eqsl_sent);
        r.field("EQSL_QSL_RCVD", &d.eqsl_rcvd);
        // Imported fields, including any STATION_CALLSIGN or zones the file gave.
        for (k, v) in &d.adif {
            r.field(k, v);
        }
        r.field("STATION_CALLSIGN", &mycall);
        let country = d.dxcc.and_then(|n| cty.by_number(n)).or_else(|| cty.lookup(&q.call));
        if let Some(n) = d.dxcc.or(country.as_ref().map(|c| c.dxcc)) {
            r.field("DXCC", &n.to_string());
        }
        if let Some(c) = country {
            r.field("COUNTRY", &c.country);
            r.field("CONT", &c.cont);
            if c.cqz > 0 {
                r.field("CQZ", &c.cqz.to_string());
            }
            if c.ituz > 0 {
                r.field("ITUZ", &c.ituz.to_string());
            }
        }
        out.push_str("<EOR>\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contest::load_all;
    use crate::import;
    use crate::model::Station;
    use chrono::{TimeZone, Utc};

    #[test]
    fn round_trip() {
        let def = load_all(None).0.into_iter().find(|d| d.id == "general").unwrap();
        let setup = LogSetup {
            contest_id: "general".into(),
            start: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
            station: Station {
                callsign: "VK3AAA".into(),
                ..Default::default()
            },
            sent_number: String::new(),
            serial_per_band: false,
            utc_offset_minutes: 600,
            location: String::new(),
            categories: Default::default(),
            bonus_bands: Default::default(),
            soapbox: String::new(),
            generic: Default::default(),
        };
        let file = "<EOH><CALL:6>JA1ABC <QSO_DATE:8>20260105 <TIME_ON:6>083015 <BAND:3>20m <MODE:4>MFSK \
                    <SUBMODE:3>FT4 <NAME:4>Taro <LOTW_QSL_RCVD:1>Y <TX_PWR:3>100 \
                    <STATION_CALLSIGN:8>VK3AAA/P <DXCC:3>339 <EOR>";
        let (qsos, _) = import::select(&import::parse(file.as_bytes()), &def, &setup, &[], None, None);
        let cty = Cty::parse("JA,Japan,339,AS,25,45,36.40,-138.38,-9.0,JA;\n");
        let text = render(&def, &setup, &qsos, &cty, "1.0");
        for f in [
            "<CALL:6>JA1ABC ",
            "<MODE:4>MFSK <SUBMODE:3>FT4 ",
            "<NAME:4>Taro ",
            "<LOTW_QSL_RCVD:1>Y ",
            "<TX_PWR:3>100 ",
            "<STATION_CALLSIGN:8>VK3AAA/P ",
            "<DXCC:3>339 <COUNTRY:5>Japan <CONT:2>AS <CQZ:2>25 <ITUZ:2>45 ",
        ] {
            assert!(text.contains(f), "{f} missing from {text}");
        }
        assert!(!text.contains("CONTEST_ID"));
        assert_eq!(text.matches("STATION_CALLSIGN").count(), 1);
        let (again, _) = import::select(&import::parse(text.as_bytes()), &def, &setup, &[], None, None);
        assert_eq!(again[0].mode, "FT4");
        assert_eq!(again[0].details.name, "Taro");
        assert_eq!(again[0].details.dxcc, Some(339));
    }
}
