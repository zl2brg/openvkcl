pub struct Band {
    pub name: &'static str,
    pub lower_khz: f64,
    pub upper_khz: f64,
    /// Cabrillo frequency for bands above 30 MHz, which are logged by band rather than frequency.
    pub cabrillo: &'static str,
}

impl Band {
    pub fn is_hf(&self) -> bool {
        self.lower_khz < 30_000.0
    }
}

const fn band(name: &'static str, lower_khz: f64, upper_khz: f64, cabrillo: &'static str) -> Band {
    Band {
        name,
        lower_khz,
        upper_khz,
        cabrillo,
    }
}

pub const BANDS: &[Band] = &[
    band("160m", 1_800.0, 2_000.0, "1800"),
    band("80m", 3_500.0, 4_000.0, "3500"),
    band("60m", 5_250.0, 5_450.0, "5300"),
    band("40m", 7_000.0, 7_300.0, "7000"),
    band("30m", 10_100.0, 10_150.0, "10100"),
    band("20m", 14_000.0, 14_350.0, "14000"),
    band("17m", 18_068.0, 18_168.0, "18068"),
    band("15m", 21_000.0, 21_450.0, "21000"),
    band("12m", 24_890.0, 24_990.0, "24890"),
    band("10m", 28_000.0, 29_700.0, "28000"),
    band("6m", 50_000.0, 54_000.0, "50"),
    band("2m", 144_000.0, 148_000.0, "144"),
    band("70cm", 420_000.0, 450_000.0, "432"),
    band("23cm", 1_240_000.0, 1_300_000.0, "1.2G"),
    band("13cm", 2_300_000.0, 2_450_000.0, "2.3G"),
    band("9cm", 3_300_000.0, 3_600_000.0, "3.4G"),
    band("6cm", 5_650_000.0, 5_925_000.0, "5.7G"),
    band("3cm", 10_000_000.0, 10_500_000.0, "10G"),
    band("1.25cm", 24_000_000.0, 24_250_000.0, "24G"),
    band("6mm", 47_000_000.0, 47_200_000.0, "47G"),
    band("4mm", 75_500_000.0, 81_000_000.0, "75G"),
    band("2.5mm", 119_980_000.0, 123_000_000.0, "122G"),
    band("2mm", 134_000_000.0, 149_000_000.0, "134G"),
    band("1mm", 241_000_000.0, 250_000_000.0, "241G"),
];

pub fn by_name(name: &str) -> Option<&'static Band> {
    BANDS.iter().find(|b| b.name.eq_ignore_ascii_case(name))
}

pub fn from_khz(khz: f64) -> Option<&'static Band> {
    BANDS
        .iter()
        .find(|b| khz >= b.lower_khz && khz <= b.upper_khz)
}

/// Frequency column for a Cabrillo QSO line: kHz on HF, band designator above.
pub fn cabrillo_freq(name: &str, freq_khz: Option<f64>) -> String {
    let Some(b) = by_name(name) else {
        return freq_khz.map(|f| format!("{f:.0}")).unwrap_or_default();
    };
    if !b.is_hf() {
        return b.cabrillo.to_string();
    }
    match freq_khz {
        Some(f) if f >= b.lower_khz && f <= b.upper_khz => format!("{f:.0}"),
        _ => b.cabrillo.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup() {
        assert_eq!(from_khz(7_090.0).unwrap().name, "40m");
        assert_eq!(from_khz(144_150.0).unwrap().name, "2m");
        assert!(from_khz(9_000.0).is_none());
        assert_eq!(cabrillo_freq("40m", Some(7_090.4)), "7090");
        assert_eq!(cabrillo_freq("40m", None), "7000");
        assert_eq!(cabrillo_freq("2m", Some(144_150.0)), "144");
        assert_eq!(cabrillo_freq("23cm", None), "1.2G");
    }
}
