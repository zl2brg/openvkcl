use crate::bands;
use crate::contest::ContestDef;
use crate::contest::def::ExchField;
use crate::contest::engine::Score;
use crate::model::{LogSetup, Qso};

fn exch(def: &ContestDef, rst: &str, nr: &str, number: &str, locator: &str) -> String {
    let num = |n: &str| format!("{:<4}", if n.is_empty() { "0" } else { n });
    def.exchange
        .iter()
        .map(|f| match f {
            ExchField::Rst => format!("{rst:<3}"),
            ExchField::Serial => num(nr),
            ExchField::Number => num(number),
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
        exch(def, &q.sent_rst, &q.sent_nr, &q.sent_exch, &q.sent_locator),
        q.call.to_ascii_uppercase(),
        exch(def, &q.rcvd_rst, &q.rcvd_nr, &q.rcvd_exch, &q.rcvd_locator),
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
    use crate::contest::engine::Score;
    use crate::contest::load_all;
    use crate::model::Station;
    use chrono::{TimeZone, Utc};
    use std::collections::BTreeMap;

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
            sent_exch: "30".into(),
            rcvd_exch: "45".into(),
            details: Default::default(),
        }
    }

    fn setup(def: &ContestDef) -> LogSetup {
        LogSetup {
            contest_id: def.id.clone(),
            start: Utc.with_ymd_and_hms(2026, 8, 15, 8, 0, 0).unwrap(),
            station: Station {
                callsign: "vk3aaa".into(),
                operators: "vk3aaa vk3bbb".into(),
                name: "Test Op".into(),
                email: "op@example.com".into(),
                address: "1 Test St".into(),
                city: "Melbourne".into(),
                state: "VIC".into(),
                postcode: "3000".into(),
                country: "Australia".into(),
                club: "Test ARC".into(),
                locator: "QF22le".into(),
            },
            sent_number: "10".into(),
            serial_per_band: false,
            utc_offset_minutes: 600,
            location: "VIC".into(),
            categories: def
                .categories
                .iter()
                .map(|c| (c.key.clone(), c.default.clone()))
                .collect(),
            bonus_bands: BTreeMap::new(),
            soapbox: "Line one\nLine two".into(),
        }
    }

    fn score(total: f64) -> Score {
        Score {
            total,
            qsos: 1,
            ..Default::default()
        }
    }

    /// WWROF Cabrillo 3.0: CONTEST is A–Z, 0–9 and hyphen, max 32 characters.
    fn contest_name_ok(name: &str) -> bool {
        !name.is_empty()
            && name.len() <= 32
            && name
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '-')
    }

    fn tags(text: &str) -> Vec<(&str, &str)> {
        text.trim_end_matches(['\r', '\n'])
            .split("\r\n")
            .map(|line| {
                line.split_once(": ")
                    .map(|(k, v)| (k, v))
                    .unwrap_or((line.trim_end_matches(':'), ""))
            })
            .collect()
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
            "QSO:  7090 PH 2026-09-19 0105 VK3AAA        59  30   VK2XYZ        57  45"
        );
    }

    #[test]
    fn header_follows_wwrof_v3() {
        let (defs, _) = load_all(None);
        let def = defs.iter().find(|d| d.id == "wia-rd").unwrap();
        let s = setup(def);
        let text = render(def, &s, &[sample("40m", Some(7_090.0))], &score(12.0), "0.4.0");

        assert!(text.ends_with("\r\n"), "log must end with CRLF");
        assert!(!text.contains('\n') || text.contains("\r\n"));
        let lines: Vec<&str> = text.trim_end_matches(['\r', '\n']).split("\r\n").collect();
        assert_eq!(lines[0], "START-OF-LOG: 3.0");
        assert_eq!(*lines.last().unwrap(), "END-OF-LOG:");

        let parsed = tags(&text);
        let get = |k: &str| parsed.iter().find(|(t, _)| *t == k).map(|(_, v)| *v);

        assert_eq!(get("CONTEST"), Some(def.cabrillo_name.as_str()));
        assert!(contest_name_ok(def.cabrillo_name.as_str()), "{}", def.cabrillo_name);
        assert_eq!(get("CALLSIGN"), Some("VK3AAA"));
        assert_eq!(get("LOCATION"), Some("VIC"));
        assert_eq!(get("GRID-LOCATOR"), Some("QF22le"));
        assert_eq!(get("CLAIMED-SCORE"), Some("12"));
        assert!(
            get("CLAIMED-SCORE").unwrap().chars().all(|c| c.is_ascii_digit()),
            "CLAIMED-SCORE must be an integer with no commas or decimals"
        );
        assert_eq!(get("CLUB"), Some("Test ARC"));
        assert_eq!(get("OPERATORS"), Some("VK3AAA VK3BBB"));
        assert!(get("OPERATORS").unwrap().len() <= 75);
        assert_eq!(get("NAME"), Some("Test Op"));
        assert!(get("NAME").unwrap().len() <= 75);
        assert_eq!(get("ADDRESS"), Some("1 Test St"));
        assert!(get("ADDRESS").unwrap().len() <= 45);
        assert_eq!(get("ADDRESS-CITY"), Some("Melbourne"));
        assert_eq!(get("ADDRESS-STATE-PROVINCE"), Some("VIC"));
        assert_eq!(get("ADDRESS-POSTALCODE"), Some("3000"));
        assert_eq!(get("ADDRESS-COUNTRY"), Some("Australia"));
        assert_eq!(get("EMAIL"), Some("op@example.com"));
        assert_eq!(get("CREATED-BY"), Some("OpenVKCL 0.4.0"));

        let soap: Vec<_> = parsed.iter().filter(|(t, _)| *t == "SOAPBOX").map(|(_, v)| *v).collect();
        assert_eq!(soap, ["Line one", "Line two"]);
        for line in soap {
            assert!(line.len() <= 75);
        }

        for cat in &def.categories {
            let value = s.categories.get(&cat.key).unwrap();
            if value.is_empty() {
                assert!(get(&cat.key).is_none(), "{} should be omitted when empty", cat.key);
            } else {
                assert_eq!(get(&cat.key), Some(value.as_str()));
                assert!(cat.options.iter().any(|o| o == value));
            }
        }

        // Optional tags we do not emit.
        assert!(get("CERTIFICATE").is_none());
        assert!(get("OFFTIME").is_none());

        let qso_at = lines.iter().position(|l| l.starts_with("QSO:")).unwrap();
        let end_at = lines.len() - 1;
        assert!(qso_at > 0 && qso_at < end_at);
        assert!(lines[1..qso_at].iter().all(|l| !l.starts_with("QSO:")));
        assert!(lines[qso_at..end_at].iter().all(|l| l.starts_with("QSO:")));
    }

    #[test]
    fn omits_blank_optional_headers() {
        let (defs, _) = load_all(None);
        let def = defs.iter().find(|d| d.id == "wia-rd").unwrap();
        let mut s = setup(def);
        s.station.club.clear();
        s.station.email.clear();
        s.station.address.clear();
        s.station.city.clear();
        s.station.state.clear();
        s.station.postcode.clear();
        s.station.country.clear();
        s.station.operators.clear();
        s.station.name.clear();
        s.location.clear();
        s.soapbox.clear();
        s.categories.insert("CATEGORY-OVERLAY".into(), String::new());

        let text = render(def, &s, &[], &score(0.0), "1.0.0");
        let parsed = tags(&text);
        for absent in [
            "LOCATION",
            "CLUB",
            "EMAIL",
            "ADDRESS",
            "ADDRESS-CITY",
            "ADDRESS-STATE-PROVINCE",
            "ADDRESS-POSTALCODE",
            "ADDRESS-COUNTRY",
            "OPERATORS",
            "NAME",
            "SOAPBOX",
            "CATEGORY-OVERLAY",
        ] {
            assert!(
                parsed.iter().all(|(t, _)| *t != absent),
                "{absent} should be omitted when blank"
            );
        }
        assert!(parsed.iter().any(|(t, v)| *t == "CREATED-BY" && *v == "OpenVKCL 1.0.0"));
        assert!(parsed.iter().any(|(t, v)| *t == "CLAIMED-SCORE" && *v == "0"));
    }

    #[test]
    fn claimed_score_rounds_to_integer() {
        let (defs, _) = load_all(None);
        let def = defs.iter().find(|d| d.id == "wia-rd").unwrap();
        let text = render(def, &setup(def), &[], &score(12.6), "0.4.0");
        let score_line = text.lines().find(|l| l.starts_with("CLAIMED-SCORE:")).unwrap();
        // `{:.0}` rounds; WWROF forbids a decimal point in the tag value.
        assert_eq!(score_line, "CLAIMED-SCORE: 13");
        assert!(!score_line.contains('.'));
        assert!(!score_line.contains(','));
    }

    #[test]
    fn qsos_stay_in_given_order_before_end() {
        let (defs, _) = load_all(None);
        let def = defs.iter().find(|d| d.id == "wia-rd").unwrap();
        let mut a = sample("80m", Some(3_600.0));
        a.time = Utc.with_ymd_and_hms(2026, 8, 15, 9, 0, 0).unwrap();
        a.call = "vk1aaa".into();
        let mut b = sample("40m", Some(7_090.0));
        b.id = 2;
        b.time = Utc.with_ymd_and_hms(2026, 8, 15, 8, 0, 0).unwrap(); // earlier, but listed second
        b.call = "vk2bbb".into();
        let text = render(def, &setup(def), &[a, b], &score(2.0), "0.4.0");
        let qsos: Vec<_> = text.lines().filter(|l| l.starts_with("QSO:")).collect();
        assert!(qsos[0].contains("VK1AAA"));
        assert!(qsos[1].contains("VK2BBB"));
        assert!(text.trim_end().ends_with("END-OF-LOG:"));
    }

    #[test]
    fn builtin_contest_names_vs_wwrof_charset() {
        let (defs, errs) = load_all(None);
        assert!(errs.is_empty(), "{errs:?}");
        // Known exceptions: underscore / space are outside A–Z, 0–9, hyphen.
        let exceptions = ["WIA_VHFUHF", "WIA-HARRY ANGEL", "WIA-TRANS TASMAN"];
        let mut bad = Vec::new();
        for d in defs.iter().filter(|d| !d.general) {
            if !contest_name_ok(&d.cabrillo_name) && !exceptions.contains(&d.cabrillo_name.as_str())
            {
                bad.push(format!("{} -> {}", d.id, d.cabrillo_name));
            }
        }
        assert!(bad.is_empty(), "unexpected CONTEST name charset issues: {bad:?}");
        for name in exceptions {
            assert!(
                defs.iter().any(|d| d.cabrillo_name == name),
                "expected known exception {name} still present"
            );
            assert!(!contest_name_ok(name));
        }
        assert!(contest_name_ok("WIA-REMEMBRANCE"));
        assert!(contest_name_ok("NZART-VHF-UHF-SHF-EHF"));
        assert!(!contest_name_ok(""));
        assert!(!contest_name_ok(&"A".repeat(33)));
    }
}
