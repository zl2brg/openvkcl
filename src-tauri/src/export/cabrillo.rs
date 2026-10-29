use crate::bands;
use crate::contest::ContestDef;
use crate::contest::def::ExchField;
use crate::contest::engine::Score;
use crate::model::{LogSetup, Qso};

fn exch(def: &ContestDef, rst: &str, nr: &str, locator: &str) -> String {
    def.exchange
        .iter()
        .map(|f| match f {
            ExchField::Rst => format!("{rst:<3}"),
            ExchField::Serial | ExchField::Number => format!("{:<4}", if nr.is_empty() { "0" } else { nr }),
            ExchField::Locator => format!("{:<6}", locator.to_ascii_uppercase()),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn qso_line(def: &ContestDef, mycall: &str, q: &Qso) -> String {
    format!(
        "QSO: {:>5} {:<2} {} {:<13} {} {:<13} {}",
        bands::cabrillo_freq(&q.band, q.freq_khz),
        q.mode,
        q.time.format("%Y-%m-%d %H%M"),
        mycall,
        exch(def, &q.sent_rst, &q.sent_nr, &q.sent_locator),
        q.call.to_ascii_uppercase(),
        exch(def, &q.rcvd_rst, &q.rcvd_nr, &q.rcvd_locator),
    )
    .trim_end()
    .to_string()
}

/// A Cabrillo 3.0 log. Dupes are included; the log checker decides how to score them.
pub fn render(def: &ContestDef, setup: &LogSetup, qsos: &[Qso], score: &Score, version: &str) -> String {
    let st = &setup.station;
    let mycall = st.callsign.to_ascii_uppercase();
    let mut out = Vec::new();
    let mut header = |key: &str, value: &str| {
        let value = value.trim();
        if !value.is_empty() {
            out.push(format!("{key}: {value}"));
        }
    };
    header("START-OF-LOG", "3.0");
    header("CONTEST", &def.cabrillo_name);
    header("CALLSIGN", &mycall);
    header("LOCATION", &setup.location);
    for cat in &def.categories {
        let value = setup.categories.get(&cat.key).unwrap_or(&cat.default);
        header(&cat.key, value);
    }
    header("GRID-LOCATOR", &st.locator);
    header("CLAIMED-SCORE", &format!("{:.0}", score.total));
    header("CLUB", &st.club);
    header("OPERATORS", &st.operators.to_ascii_uppercase());
    header("NAME", &st.name);
    header("ADDRESS", &st.address);
    header("ADDRESS-CITY", &st.city);
    header("ADDRESS-STATE-PROVINCE", &st.state);
    header("ADDRESS-POSTALCODE", &st.postcode);
    header("ADDRESS-COUNTRY", &st.country);
    header("EMAIL", &st.email);
    header("CREATED-BY", &format!("OpenVKCL {version}"));
    for line in setup.soapbox.lines() {
        header("SOAPBOX", line);
    }
    for q in qsos {
        out.push(qso_line(def, &mycall, q));
    }
    out.push("END-OF-LOG:".into());
    let mut text = out.join("\r\n");
    text.push_str("\r\n");
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contest::load_all;
    use chrono::{TimeZone, Utc};

    fn sample(band: &str, freq: Option<f64>) -> Qso {
        Qso {
            id: 1,
            time: Utc.with_ymd_and_hms(2026, 9, 19, 1, 5, 0).unwrap(),
            call: "vk2xyz".into(),
            band: band.into(),
            mode: "PH".into(),
            freq_khz: freq,
            sent_rst: "59".into(),
            sent_nr: "1".into(),
            sent_locator: "QF22le".into(),
            rcvd_rst: "57".into(),
            rcvd_nr: "12".into(),
            rcvd_locator: "QF56od".into(),
            operator: String::new(),
        }
    }

    #[test]
    fn lines() {
        let (defs, _) = load_all(None);
        let vhf = defs.iter().find(|d| d.id == "wia-vhfuhf").unwrap();
        assert_eq!(
            qso_line(vhf, "VK3AAA", &sample("2m", Some(144_150.0))),
            "QSO:   144 PH 2026-09-19 0105 VK3AAA        59  1    QF22LE VK2XYZ        57  12   QF56OD"
        );
        let rd = defs.iter().find(|d| d.id == "wia-rd").unwrap();
        assert_eq!(
            qso_line(rd, "VK3AAA", &sample("40m", Some(7_090.2))),
            "QSO:  7090 PH 2026-09-19 0105 VK3AAA        59  1    VK2XYZ        57  12"
        );
    }
}
