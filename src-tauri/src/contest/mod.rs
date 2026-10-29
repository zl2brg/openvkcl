pub mod def;
pub mod engine;

pub use def::ContestDef;

use std::path::Path;

const BUILTIN: &[(&str, &str)] = &[
    ("wia-rd.toml", include_str!("../../contests/wia-rd.toml")),
    ("wia-tt.toml", include_str!("../../contests/wia-tt.toml")),
    ("wia-ha.toml", include_str!("../../contests/wia-ha.toml")),
    ("wia-jmmfd.toml", include_str!("../../contests/wia-jmmfd.toml")),
    ("wia-vhfuhf.toml", include_str!("../../contests/wia-vhfuhf.toml")),
    ("nzart-vhf-dx.toml", include_str!("../../contests/nzart-vhf-dx.toml")),
    ("nzart-vhf-lowband.toml", include_str!("../../contests/nzart-vhf-lowband.toml")),
    ("nzart-vhf-microwave.toml", include_str!("../../contests/nzart-vhf-microwave.toml")),
    ("nzart-vhf-fd.toml", include_str!("../../contests/nzart-vhf-fd.toml")),
    ("nzart-jwmfd.toml", include_str!("../../contests/nzart-jwmfd.toml")),
    ("nzart-memorial.toml", include_str!("../../contests/nzart-memorial.toml")),
    ("nzart-sangster.toml", include_str!("../../contests/nzart-sangster.toml")),
];

pub fn parse(name: &str, text: &str) -> Result<ContestDef, String> {
    let def: ContestDef = toml::from_str(text).map_err(|e| format!("{name}: {e}"))?;
    def.validate()?;
    Ok(def)
}

/// Built-in contests, overridden or extended by `*.toml` files in `user_dir`.
/// Returns the definitions sorted by name, plus any load errors.
pub fn load_all(user_dir: Option<&Path>) -> (Vec<ContestDef>, Vec<String>) {
    let mut defs = Vec::new();
    let mut errors = Vec::new();
    for (name, text) in BUILTIN {
        match parse(name, text) {
            Ok(d) => defs.push(d),
            Err(e) => errors.push(e),
        }
    }
    if let Some(entries) = user_dir.and_then(|d| std::fs::read_dir(d).ok()) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|e| e != "toml") {
                continue;
            }
            let name = path.display().to_string();
            match std::fs::read_to_string(&path)
                .map_err(|e| format!("{name}: {e}"))
                .and_then(|t| parse(&name, &t))
            {
                Ok(d) => {
                    defs.retain(|x: &ContestDef| x.id != d.id);
                    defs.push(d);
                }
                Err(e) => errors.push(e),
            }
        }
    }
    defs.sort_by(|a, b| a.name.cmp(&b.name));
    (defs, errors)
}

#[cfg(test)]
mod tests {
    use super::engine::evaluate;
    use super::*;
    use crate::model::{LogSetup, Qso, Station};
    use chrono::{DateTime, Duration, TimeZone, Utc};

    fn def(id: &str) -> ContestDef {
        let (defs, errors) = load_all(None);
        assert!(errors.is_empty(), "{errors:?}");
        defs.into_iter().find(|d| d.id == id).unwrap()
    }

    fn setup(id: &str, start: DateTime<Utc>) -> LogSetup {
        LogSetup {
            contest_id: id.into(),
            start,
            station: Station {
                callsign: "VK3AAA".into(),
                ..Default::default()
            },
            sent_number: "10".into(),
            serial_per_band: false,
            utc_offset_minutes: 600,
            location: String::new(),
            categories: Default::default(),
            soapbox: String::new(),
        }
    }

    fn qso(id: i64, t: DateTime<Utc>, call: &str, band: &str, mode: &str) -> Qso {
        Qso {
            id,
            time: t,
            call: call.into(),
            band: band.into(),
            mode: mode.into(),
            freq_khz: None,
            sent_rst: "59".into(),
            sent_nr: id.to_string(),
            sent_locator: String::new(),
            rcvd_rst: "59".into(),
            rcvd_nr: "1".into(),
            rcvd_locator: String::new(),
            operator: String::new(),
            sent_exch: String::new(),
            rcvd_exch: "1".into(),
        }
    }

    #[test]
    fn builtins_load() {
        let (defs, errors) = load_all(None);
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(defs.len(), BUILTIN.len());
    }

    #[test]
    fn remembrance_day() {
        let d = def("wia-rd");
        // 0300 UTC = 1300 AEST.
        let start = Utc.with_ymd_and_hms(2026, 8, 15, 3, 0, 0).unwrap();
        let s = setup("wia-rd", start);
        let m = |n| start + Duration::minutes(n);
        let qsos = vec![
            qso(1, m(5), "VK2ABC", "40m", "PH"),   // 1
            qso(2, m(10), "VK2ABC", "40m", "PH"),  // dupe
            qso(3, m(15), "VK2ABC", "40m", "CW"),  // 2
            qso(4, m(20), "VK2ABC", "40m", "RY"),  // dupe: RY counts as CW
            qso(5, m(200), "VK2ABC", "40m", "PH"), // 1, 3h later
            qso(6, m(25), "VK4XYZ", "160m", "CW"), // 2 * 2 = 4
            // 1500 UTC = 0100 AEST, inside the triple-points window.
            qso(7, m(12 * 60), "VK5DEF", "80m", "PH"), // 3
            qso(8, m(30), "JA1ABC", "20m", "PH"),      // not workable
        ];
        let e = evaluate(&d, &s, &qsos);
        let pts: Vec<f64> = e.results.iter().map(|r| r.points).collect();
        assert_eq!(pts, vec![1.0, 0.0, 2.0, 0.0, 1.0, 4.0, 3.0, 0.0]);
        assert!(e.results[1].dupe && e.results[3].dupe);
        assert_eq!(e.results[1].next_ok, Some(m(5 + 180)));
        assert!(e.results[7].invalid.is_some());
        assert_eq!(e.score.total, 11.0);
        assert_eq!(e.score.dupes, 2);
    }

    #[test]
    fn trans_tasman_blocks() {
        let d = def("wia-tt");
        let start = Utc.with_ymd_and_hms(2026, 7, 18, 8, 0, 0).unwrap();
        let s = setup("wia-tt", start);
        let m = |n| start + Duration::minutes(n);
        let qsos = vec![
            qso(1, m(1), "VK2ABC", "80m", "PH"),
            qso(2, m(2), "VK2XYZ", "80m", "PH"),
            qso(3, m(3), "ZL1AA", "80m", "PH"),
            qso(4, m(4), "VK2ABC", "80m", "PH"),   // dupe in block 0
            qso(5, m(5), "VK2ABC", "80m", "CW"),   // other mode OK
            qso(6, m(121), "VK2ABC", "80m", "PH"), // block 1 OK
        ];
        let e = evaluate(&d, &s, &qsos);
        assert!(e.results[3].dupe);
        assert_eq!(e.results[3].next_ok, Some(m(120)));
        assert!(!e.results[4].dupe && !e.results[5].dupe);
        // Block 0: 4 QSOs x {VK2, ZL1} = 8. Block 1: 1 x {VK2} = 1.
        assert_eq!(e.score.total, 9.0);
    }

    #[test]
    fn harry_angel() {
        let d = def("wia-ha");
        let start = Utc.with_ymd_and_hms(2027, 5, 1, 10, 0, 0).unwrap();
        let s = setup("wia-ha", start);
        let m = |n| start + Duration::minutes(n);
        let qsos = vec![
            qso(1, m(1), "VK4ABC", "80m", "PH"),
            qso(2, m(2), "VK4ABC", "80m", "CW"),
            qso(3, m(90), "VK4ABC", "80m", "PH"),
            qso(4, m(3), "VK4ABC", "40m", "PH"),
        ];
        let e = evaluate(&d, &s, &qsos);
        assert!(e.results[2].dupe);
        assert!(e.results[3].invalid.is_some());
        assert_eq!(e.score.total, 3.0);
    }

    #[test]
    fn john_moyle_groups() {
        let d = def("wia-jmmfd");
        let start = Utc.with_ymd_and_hms(2027, 3, 20, 1, 0, 0).unwrap();
        let s = setup("wia-jmmfd", start);
        let m = |n| start + Duration::minutes(n);
        let qsos = vec![
            qso(1, m(1), "VK2ABC", "40m", "PH"),
            qso(2, m(2), "VK2XYZ", "40m", "CW"),
            qso(3, m(3), "JA1ABC", "40m", "PH"), // points, no mult
            qso(4, m(181), "VK2ABC", "40m", "PH"),
            qso(5, m(5), "VK3XX", "2m", "PH"),
        ];
        let e = evaluate(&d, &s, &qsos);
        assert!(e.results.iter().all(|r| !r.dupe && r.invalid.is_none()));
        let hf = &e.score.groups[0];
        let vhf = &e.score.groups[1];
        // HF: points 1 + 2 + 1 + 1 = 5; mults VK2|PH|0, VK2|CW|0, VK2|PH|1 = 3.
        assert_eq!((hf.points, hf.mults, hf.score), (5.0, 3, 15.0));
        assert_eq!((vhf.points, vhf.mults, vhf.score), (1.0, 1, 1.0));
        assert_eq!(e.score.total, 16.0);
    }

    #[test]
    fn vhf_distance_and_squares() {
        let d = def("wia-vhfuhf");
        let start = Utc.with_ymd_and_hms(2026, 9, 19, 1, 0, 0).unwrap();
        let s = setup("wia-vhfuhf", start);
        let m = |n| start + Duration::minutes(n);
        let q = |id, t, call: &str, band: &str, mine: &str, theirs: &str| {
            let mut q = qso(id, t, call, band, "PH");
            q.sent_locator = mine.into();
            q.rcvd_locator = theirs.into();
            q
        };
        let qsos = vec![
            q(1, m(1), "VK3ABC", "2m", "QF22le", "QF22le"),  // same sub-square: 0
            q(2, m(2), "VK2XYZ", "70cm", "QF22le", "QF56od"), // ~710 km x 2.7
            q(3, m(30), "VK2XYZ", "70cm", "QF22le", "QF56od"), // dupe
            q(4, m(31), "VK2XYZ", "70cm", "QF32aa", "QF56od"), // I moved square: OK
            q(5, m(32), "VK2XYZ", "2m", "QF22le", "QF56od"),  // other band OK
        ];
        let e = evaluate(&d, &s, &qsos);
        assert_eq!(e.results[0].points, 0.0);
        assert!(e.results[2].dupe);
        assert!(!e.results[3].dupe && !e.results[4].dupe);
        let km = e.results[1].distance_km.unwrap();
        let tapered = if km > 700.0 { 700.0 + (km - 700.0) / 100.0 } else { km };
        let expected = tapered * 2.7;
        assert!((e.results[1].points - expected).abs() < 1e-9);
    }

    fn nz_qso(id: i64, t: DateTime<Utc>, call: &str, band: &str, mode: &str, theirs: &str) -> Qso {
        let mut q = qso(id, t, call, band, mode);
        q.sent_locator = "RF73hc".into();
        q.rcvd_locator = theirs.into();
        q
    }

    #[test]
    fn nzart_vhf_table_periods_and_bonuses() {
        let d = def("nzart-vhf-dx");
        // 0400 UTC = 1700 NZDT.
        let start = Utc.with_ymd_and_hms(2027, 2, 13, 4, 0, 0).unwrap();
        let mut s = setup("nzart-vhf-dx", start);
        let m = |n| start + Duration::minutes(n);
        // From RF73hc: RF73jb 15.5 km, RF72lc 115 km, RE78ir 487 km, RE66hq 733 km.
        let qsos = vec![
            nz_qso(1, m(1), "ZL1AAA", "2m", "PH", "RF73jb"),    // 1
            nz_qso(2, m(5), "ZL1AAA", "2m", "PH", "RF73jb"),    // dupe in this hour
            nz_qso(3, m(61), "ZL1AAA", "2m", "PH", "RF73jb"),   // next hour: 1
            nz_qso(4, m(62), "ZL1AAA", "70cm", "CW", "RF72lc"), // 10
            nz_qso(5, m(63), "ZL2BBB", "6m", "DG", "RE66hq"),   // 25 x 1.5 = 37.5 -> 38
            nz_qso(6, m(64), "ZL3CCC", "2m", "RY", "RE78ir"),   // 20 x 1.5 = 30
            nz_qso(7, m(400), "ZL4DDD", "2m", "PH", "RE78ir"),  // between periods: 20, warned
            nz_qso(8, m(841), "ZL4DDD", "23cm", "PH", "RF73jb"), // 15.5 x 0.2 = 3.1 -> 3
        ];
        let e = evaluate(&d, &s, &qsos);
        let pts: Vec<f64> = e.results.iter().map(|r| r.points).collect();
        assert_eq!(pts, vec![1.0, 0.0, 1.0, 10.0, 38.0, 30.0, 20.0, 3.0]);
        assert!(e.results[1].dupe);
        assert_eq!(e.results[1].next_ok, Some(m(60)));
        assert!(e.results[6].warning.as_deref().unwrap().contains("operating periods"));
        assert!(e.results[7].warning.is_none());
        let groups: Vec<(&str, f64)> = e
            .score
            .groups
            .iter()
            .filter(|g| g.qsos > 0)
            .map(|g| (g.name.as_str(), g.score))
            .collect();
        assert_eq!(groups, vec![("6m", 38.0), ("2m", 52.0), ("70cm", 10.0), ("23cm", 3.0)]);
        assert_eq!(e.score.total, 103.0);

        // QRP x1.5 then field station x1.2, applied to the rounded basic score.
        s.categories.insert("CATEGORY-POWER".into(), "QRP".into());
        s.categories.insert("CATEGORY-STATION".into(), "PORTABLE".into());
        let e = evaluate(&d, &s, &qsos);
        let pts: Vec<f64> = e.results.iter().map(|r| r.points).collect();
        assert_eq!(pts, vec![2.0, 0.0, 2.0, 18.0, 68.0, 54.0, 36.0, 5.0]);

        let sheet = crate::export::scoresheet::render(&d, &s, &qsos, &e);
        assert!(sheet.contains("Bonuses applied,QRP x1.5 PORTABLE x1.2\r\n"));
        assert!(sheet.contains("2027-02-13,0405,1,ZL1AAA,PH,59,2,59,1,RF73JB,,0,2,Dupe\r\n"));
        assert!(sheet.contains("Band total,2m,94\r\n"));
        assert!(sheet.ends_with("Claimed score,185\r\n"));
    }

    fn hf(id: i64, t: DateTime<Utc>, call: &str, band: &str, mode: &str, branch: &str) -> Qso {
        let mut q = qso(id, t, call, band, mode);
        q.sent_exch = "10".into();
        q.rcvd_exch = branch.into();
        q
    }

    #[test]
    fn jock_white_periods_and_branches() {
        let d = def("nzart-jwmfd");
        assert!(d.serial_per_band);
        // 0200 UTC = 1500 NZDT.
        let start = Utc.with_ymd_and_hms(2027, 2, 27, 2, 0, 0).unwrap();
        let mut s = setup("nzart-jwmfd", start);
        s.station.callsign = "ZL2ZZZ".into();
        let m = |n| start + Duration::minutes(n);
        let qsos = vec![
            hf(1, m(1), "ZL2AAA", "80m", "PH", "25"),  // 3, mult 25 80m PH
            hf(2, m(2), "ZL2AAA", "80m", "PH", "25"),  // dupe this hour
            hf(3, m(3), "ZL2AAA", "80m", "CW", "25"),  // 5, mult 25 80m CW
            hf(4, m(4), "ZL2AAA", "40m", "PH", "25"),  // 3, mult 25 40m PH
            hf(5, m(61), "ZL2AAA", "80m", "PH", "25"), // next hour: 3
            hf(6, m(62), "ZL2BBB", "80m", "PH", "25"), // same branch: 3, no mult
            hf(7, m(63), "ZL3CCC", "80m", "PH", "10"), // own branch: 3, no mult
            hf(8, m(64), "ZL4DDD", "80m", "PH", "00"), // home station: 3, no mult
            hf(9, m(65), "VK2XYZ", "80m", "CW", ""),   // overseas, no branch: 5
            hf(10, m(66), "ZL1EEE", "80m", "CW", ""),  // branch missing: 5, warned
            hf(11, m(600), "ZL1FFF", "80m", "PH", "030"), // between periods: 3, mult 30
        ];
        let e = evaluate(&d, &s, &qsos);
        let pts: Vec<f64> = e.results.iter().map(|r| r.points).collect();
        assert_eq!(pts, vec![3.0, 0.0, 5.0, 3.0, 3.0, 3.0, 3.0, 3.0, 5.0, 5.0, 3.0]);
        let mults: Vec<bool> = e.results.iter().map(|r| r.new_mult).collect();
        assert_eq!(mults, vec![true, false, true, true, false, false, false, false, false, false, true]);
        assert_eq!(e.results[0].mult, "25");
        assert_eq!(e.results[10].mult, "30");
        assert!(e.results[1].dupe);
        assert_eq!(e.results[1].next_ok, Some(m(60)));
        assert!(e.results[8].warning.is_none());
        assert!(e.results[9].warning.as_deref().unwrap().contains("branch missing"));
        assert!(e.results[10].warning.as_deref().unwrap().contains("operating periods"));
        // 36 points x 4 branch multipliers.
        assert_eq!(e.score.total, 144.0);

        let sheet = crate::export::scoresheet::render(&d, &s, &qsos, &e);
        assert!(sheet.contains(
            "Date,Time UTC,Period,Call,Mode,Sent RST,Sent Nr,Sent Branch,Rcvd RST,Rcvd Nr,Rcvd Branch,Points,Running total,New mult,Note\r\n"
        ));
        assert!(sheet.contains("2027-02-27,0302,2,ZL2BBB,PH,59,6,10,59,1,25,3,14,,\r\n"));
        assert!(sheet.contains("80m,CW,3,15,1\r\n"));
    }

    #[test]
    fn sangster_half_hours_and_overseas_points() {
        let d = def("nzart-sangster");
        let start = Utc.with_ymd_and_hms(2027, 5, 15, 8, 0, 0).unwrap();
        let mut s = setup("nzart-sangster", start);
        s.station.callsign = "ZL2ZZZ".into();
        let m = |n| start + Duration::minutes(n);
        let qsos = vec![
            hf(1, m(1), "ZL1AAA", "80m", "CW", "05"), // 5, mult 5
            hf(2, m(20), "ZL1AAA", "80m", "CW", "5"), // dupe this half hour
            hf(3, m(31), "ZL1AAA", "80m", "CW", "5"), // 5
            hf(4, m(32), "VK2XYZ", "80m", "CW", ""),  // overseas: 10
            hf(5, m(33), "ZL3BBB", "80m", "CW", "10"), // own branch: 5, no mult
            hf(6, m(34), "ZL4CCC", "80m", "CW", "7"), // 5, mult 7
        ];
        let e = evaluate(&d, &s, &qsos);
        let pts: Vec<f64> = e.results.iter().map(|r| r.points).collect();
        assert_eq!(pts, vec![5.0, 0.0, 5.0, 10.0, 5.0, 5.0]);
        assert_eq!(e.results[1].next_ok, Some(m(30)));
        assert!(e.results[3].warning.is_none());
        assert_eq!(e.score.total, 30.0 * 2.0);
    }

    #[test]
    fn memorial_modes_and_oceania_mults() {
        let d = def("nzart-memorial");
        let start = Utc.with_ymd_and_hms(2027, 7, 3, 8, 0, 0).unwrap();
        let mut s = setup("nzart-memorial", start);
        s.station.callsign = "ZL2ZZZ".into();
        let m = |n| start + Duration::minutes(n);
        let qsos = vec![
            qso(1, m(1), "ZL1AAA", "80m", "PH"),  // 1, mult ZL1 PH
            qso(2, m(5), "ZL1AAA", "80m", "CW"),  // 2, mult ZL1 CW
            qso(3, m(10), "ZL1AAA", "80m", "PH"), // dupe
            qso(4, m(61), "ZL1AAA", "80m", "PH"), // next hour: 1
            qso(5, m(62), "VK2ABC", "80m", "CW"), // 2, mult VK2 CW
            qso(6, m(63), "JA1ABC", "80m", "PH"), // 1, not Oceania
            qso(7, m(64), "FK8XX", "80m", "PH"),  // 1, mult FK8 PH
        ];
        let e = evaluate(&d, &s, &qsos);
        let pts: Vec<f64> = e.results.iter().map(|r| r.points).collect();
        assert_eq!(pts, vec![1.0, 2.0, 0.0, 1.0, 2.0, 1.0, 1.0]);
        assert!(!e.results[5].new_mult && e.results[6].new_mult);
        assert_eq!(e.score.total, 8.0 * 4.0);
    }

    #[test]
    fn nzart_band_sets_and_sessions() {
        let start = Utc.with_ymd_and_hms(2027, 4, 3, 4, 0, 0).unwrap();
        let m = |n| start + Duration::minutes(n);

        let low = def("nzart-vhf-lowband");
        let s = setup("nzart-vhf-lowband", start);
        let qsos = vec![
            nz_qso(1, m(1), "ZL1AAA", "23cm", "PH", "RF73jb"),
            // 1250 minutes in: 1254 NZST after daylight saving ends.
            nz_qso(2, m(1254), "ZL1AAA", "2m", "PH", "RF73jb"),
        ];
        let e = evaluate(&low, &s, &qsos);
        assert!(e.results[0].invalid.is_some());
        assert!(e.results[1].invalid.is_none() && e.results[1].warning.is_none());

        let mw = def("nzart-vhf-microwave");
        let s = setup("nzart-vhf-microwave", start);
        let mut q = nz_qso(1, m(1), "ZL1AAA", "3cm", "PH", "RF73jb12");
        q.sent_locator = "RF73hc34".into();
        let e = evaluate(&mw, &s, &[q.clone(), nz_qso(2, m(2), "ZL1AAA", "2m", "PH", "RF73jb")]);
        assert!(e.results[1].invalid.is_some());
        let km = e.results[0].distance_km.unwrap();
        assert_eq!(e.results[0].points, km.round());
        assert!((km - 15.52).abs() > 0.01, "8-character locators should be used");
    }
}
