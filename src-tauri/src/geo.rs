const EARTH_RADIUS_KM: f64 = 6371.0;

/// Uppercase field and square letters, lowercase sub-square letters: QG62lm.
pub fn normalize_locator(s: &str) -> String {
    let s = s.trim();
    s.chars()
        .enumerate()
        .map(|(i, c)| {
            if i < 4 {
                c.to_ascii_uppercase()
            } else {
                c.to_ascii_lowercase()
            }
        })
        .collect()
}

/// A 4 or 6 character Maidenhead locator.
pub fn is_valid(s: &str) -> bool {
    let b = s.trim().as_bytes();
    if b.len() != 4 && b.len() != 6 {
        return false;
    }
    let letter = |c: u8, max: u8| (b'A'..=max).contains(&c.to_ascii_uppercase());
    letter(b[0], b'R')
        && letter(b[1], b'R')
        && b[2].is_ascii_digit()
        && b[3].is_ascii_digit()
        && (b.len() == 4 || (letter(b[4], b'X') && letter(b[5], b'X')))
}

/// The 4-character square of a locator.
pub fn square(s: &str) -> String {
    s.trim().chars().take(4).collect::<String>().to_ascii_uppercase()
}

/// Latitude and longitude of the centre of the locator's square or sub-square.
pub fn center(s: &str) -> Option<(f64, f64)> {
    if !is_valid(s) {
        return None;
    }
    let b = s.trim().to_ascii_uppercase().into_bytes();
    let mut lon = (b[0] - b'A') as f64 * 20.0 + (b[2] - b'0') as f64 * 2.0 - 180.0;
    let mut lat = (b[1] - b'A') as f64 * 10.0 + (b[3] - b'0') as f64 - 90.0;
    if b.len() == 6 {
        lon += (b[4] - b'A') as f64 * (2.0 / 24.0) + 1.0 / 24.0;
        lat += (b[5] - b'A') as f64 * (1.0 / 24.0) + 0.5 / 24.0;
    } else {
        lon += 1.0;
        lat += 0.5;
    }
    Some((lat, lon))
}

/// Great-circle distance between the centres of two locators.
pub fn distance_km(a: &str, b: &str) -> Option<f64> {
    let (lat1, lon1) = center(a)?;
    let (lat2, lon2) = center(b)?;
    let (p1, p2) = (lat1.to_radians(), lat2.to_radians());
    let dp = (lat2 - lat1).to_radians();
    let dl = (lon2 - lon1).to_radians();
    let h = (dp / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (dl / 2.0).sin().powi(2);
    Some(2.0 * EARTH_RADIUS_KM * h.sqrt().asin())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validity() {
        assert!(is_valid("QF22"));
        assert!(is_valid("qf22le"));
        assert!(!is_valid("QF2"));
        assert!(!is_valid("SF22le"));
        assert!(!is_valid("QF22lz"));
    }

    #[test]
    fn centres() {
        let (lat, lon) = center("QF22le").unwrap();
        assert!((lat - (-37.8125)).abs() < 1e-9, "{lat}");
        assert!((lon - 144.958333).abs() < 1e-5, "{lon}");
    }

    #[test]
    fn distances() {
        assert_eq!(distance_km("QF22le", "QF22le"), Some(0.0));
        // Melbourne to Sydney is roughly 710 km.
        let d = distance_km("QF22le", "QF56od").unwrap();
        assert!((690.0..730.0).contains(&d), "{d}");
    }
}
