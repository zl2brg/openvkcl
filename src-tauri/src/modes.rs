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

#[cfg(test)]
mod tests {
    use super::*;

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
