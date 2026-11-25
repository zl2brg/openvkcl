//! Modes: the Cabrillo modes contests use (PH, CW, RY, DG), the ADIF modes a
//! general log uses, and the mapping between them and ADIF MODE/SUBMODE.

/// Modes offered in a general log, as shown and stored. Each maps to an ADIF
/// MODE and, for some, a SUBMODE.
pub const GENERAL: &[(&str, &str, &str)] = &[
    ("SSB", "SSB", ""),
    ("CW", "CW", ""),
    ("FT8", "FT8", ""),
    ("FT4", "MFSK", "FT4"),
    ("RTTY", "RTTY", ""),
    ("AM", "AM", ""),
    ("FM", "FM", ""),
    ("DIGITALVOICE", "DIGITALVOICE", ""),
    ("PSK31", "PSK", "PSK31"),
    ("JS8", "MFSK", "JS8"),
    ("Q65", "MFSK", "Q65"),
    ("JT65", "JT65", ""),
    ("JT9", "JT9", ""),
    ("MSK144", "MSK144", ""),
    ("OLIVIA", "OLIVIA", ""),
    ("SSTV", "SSTV", ""),
];

/// ADIF MODE and SUBMODE for a mode stored in a log.
pub fn to_adif(mode: &str) -> (&str, &str) {
    match mode {
        "PH" => ("SSB", ""),
        "RY" => ("RTTY", ""),
        "DG" => ("MFSK", ""),
        _ => GENERAL
            .iter()
            .find(|(m, _, _)| *m == mode)
            .map_or((mode, ""), |(_, adif, sub)| (*adif, *sub)),
    }
}

/// The general-log mode for an ADIF MODE and SUBMODE, e.g. MFSK + FT4 -> FT4.
pub fn from_adif(mode: &str, submode: &str) -> String {
    let mode = mode.trim().to_ascii_uppercase();
    let submode = submode.trim().to_ascii_uppercase();
    if let Some((m, _, _)) = GENERAL.iter().find(|(m, _, _)| !submode.is_empty() && *m == submode) {
        return (*m).to_string();
    }
    match mode.as_str() {
        "USB" | "LSB" => "SSB".into(),
        _ => mode,
    }
}

/// Phone, CW or data, the three mode classes DXCC awards count by.
pub fn class(mode: &str) -> &'static str {
    match mode {
        "PH" | "SSB" | "USB" | "LSB" | "AM" | "FM" | "DIGITALVOICE" => "PH",
        "CW" => "CW",
        _ => "DG",
    }
}

/// The Cabrillo mode for any mode, as contests score it.
pub fn contest(mode: &str) -> &'static str {
    match mode {
        "RTTY" | "RY" => "RY",
        _ => class(mode),
    }
}

/// Signal report to assume when none is entered, or "" for modes that send dB reports.
pub fn default_rst(mode: &str) -> &'static str {
    match class(mode) {
        "PH" => "59",
        "CW" => "599",
        _ if ["RY", "RTTY", "PSK31", "OLIVIA", "SSTV", "DG"].contains(&mode) => "599",
        _ => "",
    }
}

/// The kind of signal report a mode uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Report {
    /// Readability 1-5 and strength 1-9, e.g. 59.
    Rs,
    /// Readability, strength and tone 1-9, e.g. 599.
    Rst,
    /// A signal-to-noise figure in dB from WSJT-X and similar, e.g. -10 or +05.
    Db,
    /// A contest's DG mode covers both kinds of digital mode.
    RstOrDb,
}

pub fn report(mode: &str) -> Report {
    match mode {
        "DG" => Report::RstOrDb,
        "RY" | "RTTY" | "PSK31" | "OLIVIA" | "SSTV" => Report::Rst,
        _ => match class(mode) {
            "PH" => Report::Rs,
            "CW" => Report::Rst,
            _ => Report::Db,
        },
    }
}

fn is_rs(r: &str) -> bool {
    let b = r.as_bytes();
    b.len() == 2 && (b'1'..=b'5').contains(&b[0]) && (b'1'..=b'9').contains(&b[1])
}

fn is_rst(r: &str) -> bool {
    r.len() == 3 && is_rs(&r[..2]) && (b'1'..=b'9').contains(&r.as_bytes()[2])
}

/// RS or RST, with an optional trailing plus (59, 599, 59+).
fn is_signal(r: &str) -> bool {
    let r = r.strip_suffix('+').unwrap_or(r);
    is_rs(r) || is_rst(r)
}

fn is_db(r: &str) -> bool {
    let b = r.as_bytes();
    (2..=3).contains(&b.len()) && (b[0] == b'+' || b[0] == b'-') && b[1..].iter().all(u8::is_ascii_digit)
}

/// Why `rst` isn't a valid report in `mode`, or None if it is. Empty is fine:
/// the mode's default is used.
pub fn check_report(mode: &str, rst: &str) -> Option<&'static str> {
    let rst = rst.trim();
    if rst.is_empty() {
        return None;
    }
    let ok = match report(mode) {
        Report::Rs | Report::Rst => is_signal(rst),
        Report::Db => is_db(rst),
        Report::RstOrDb => is_signal(rst) || is_db(rst),
    };
    (!ok).then_some(match report(mode) {
        Report::Rs | Report::Rst => "must be like 59 or 599",
        Report::Db => "must be a signal report in dB like -10 or +05",
        Report::RstOrDb => "must be like 59 or 599, or a report in dB like -10",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports() {
        // Phone, CW and RTTY take either RS or RST.
        for ok in ["59", "57", "599", "579", "59+", "599+"] {
            assert_eq!(check_report("SSB", ok), None, "{ok}");
            assert_eq!(check_report("PH", ok), None, "{ok}");
            assert_eq!(check_report("CW", ok), None, "{ok}");
            assert_eq!(check_report("RY", ok), None, "{ok}");
        }
        for bad in ["69", "50", "5", "5x", "abc", "-10", "5990", "609", "5nn"] {
            assert!(check_report("SSB", bad).is_some(), "{bad}");
            assert!(check_report("CW", bad).is_some(), "{bad}");
        }
        // FT8 and friends: dB.
        for ok in ["-10", "+05", "-7", "+0", "-24"] {
            assert_eq!(check_report("FT8", ok), None, "{ok}");
        }
        for bad in ["10", "599", "-", "-100", "--5", "+a"] {
            assert!(check_report("FT4", bad).is_some(), "{bad}");
        }
        // A contest's DG takes either.
        assert_eq!(check_report("DG", "599"), None);
        assert_eq!(check_report("DG", "59"), None);
        assert_eq!(check_report("DG", "-12"), None);
        assert!(check_report("DG", "5nn").is_some());
        // Blank uses the default.
        assert_eq!(check_report("SSB", "  "), None);
    }

    #[test]
    fn adif_round_trip() {
        for (m, _, _) in GENERAL {
            let (mode, sub) = to_adif(m);
            assert_eq!(from_adif(mode, sub), *m);
        }
        assert_eq!(to_adif("PH"), ("SSB", ""));
        assert_eq!(from_adif("ssb", "USB"), "SSB");
        assert_eq!(from_adif("USB", ""), "SSB");
        assert_eq!(from_adif("MFSK", "FST4"), "MFSK");
        assert_eq!(contest("FT8"), "DG");
        assert_eq!(contest("RTTY"), "RY");
        assert_eq!(contest("FM"), "PH");
        assert_eq!(default_rst("FT8"), "");
        assert_eq!(default_rst("SSB"), "59");
        assert_eq!(default_rst("RY"), "599");
    }
}
