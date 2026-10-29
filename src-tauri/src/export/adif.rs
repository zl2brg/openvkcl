use crate::contest::ContestDef;
use crate::contest::def::ExchField;
use crate::model::{LogSetup, Qso};
use std::fmt::Write;

fn field(out: &mut String, name: &str, value: &str) {
    if !value.is_empty() {
        let _ = write!(out, "<{name}:{}>{value} ", value.len());
    }
}

fn adif_mode(mode: &str) -> &str {
    match mode {
        "PH" => "SSB",
        "CW" => "CW",
        "RY" => "RTTY",
        "DG" => "MFSK",
        other => other,
    }
}

pub fn render(def: &ContestDef, setup: &LogSetup, qsos: &[Qso], version: &str) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "OpenVKCL export: {}", def.name);
    field(&mut out, "ADIF_VER", "3.1.4");
    field(&mut out, "PROGRAMID", "OpenVKCL");
    field(&mut out, "PROGRAMVERSION", version);
    out.push_str("<EOH>\n");

    let mycall = setup.station.callsign.to_ascii_uppercase();
    let serial = def.has_field(ExchField::Serial);
    for q in qsos {
        field(&mut out, "CALL", &q.call.to_ascii_uppercase());
        field(&mut out, "QSO_DATE", &q.time.format("%Y%m%d").to_string());
        field(&mut out, "TIME_ON", &q.time.format("%H%M%S").to_string());
        field(&mut out, "BAND", &q.band);
        field(&mut out, "MODE", adif_mode(&q.mode));
        if let Some(f) = q.freq_khz {
            field(&mut out, "FREQ", &format!("{:.4}", f / 1000.0));
        }
        field(&mut out, "RST_SENT", &q.sent_rst);
        field(&mut out, "RST_RCVD", &q.rcvd_rst);
        if serial {
            field(&mut out, "STX", &q.sent_nr);
            field(&mut out, "SRX", &q.rcvd_nr);
        }
        field(&mut out, "STX_STRING", &q.sent_nr);
        field(&mut out, "SRX_STRING", &q.rcvd_nr);
        field(&mut out, "GRIDSQUARE", &q.rcvd_locator.to_ascii_uppercase());
        field(&mut out, "MY_GRIDSQUARE", &q.sent_locator.to_ascii_uppercase());
        field(&mut out, "STATION_CALLSIGN", &mycall);
        field(&mut out, "OPERATOR", &q.operator.to_ascii_uppercase());
        field(&mut out, "CONTEST_ID", &def.cabrillo_name);
        out.push_str("<EOR>\n");
    }
    out
}
