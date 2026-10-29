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

/// Each pair of locator characters: (highest allowed character, degrees of longitude, degrees of latitude).
const PAIRS: [(u8, f64, f64); 5] = [
    (b'R', 20.0, 10.0),
    (b'9', 2.0, 1.0),
    (b'X', 2.0 / 24.0, 1.0 / 24.0),
    (b'9', 2.0 / 240.0, 1.0 / 240.0),
    (b'X', 2.0 / 5760.0, 1.0 / 5760.0),
];

fn pair_value(c: u8, max: u8) -> Option<f64> {
    let c = c.to_ascii_uppercase();
    let base = if max == b'9' { b'0' } else { b'A' };
    (base..=max).contains(&c).then(|| (c - base) as f64)
}

/// A 4, 6, 8 or 10 character Maidenhead locator.
pub fn is_valid(s: &str) -> bool {
    let b = s.trim().as_bytes();
    if !matches!(b.len(), 4 | 6 | 8 | 10) {
        return false;
    }
    b.chunks(2)
        .zip(PAIRS)
        .all(|(p, (max, _, _))| pair_value(p[0], max).is_some() && pair_value(p[1], max).is_some())
}

/// The 4-character square of a locator.
pub fn square(s: &str) -> String {
    s.trim().chars().take(4).collect::<String>().to_ascii_uppercase()
}

/// Latitude and longitude of the centre of the locator's smallest square.
pub fn center(s: &str) -> Option<(f64, f64)> {
    if !is_valid(s) {
        return None;
    }
    let b = s.trim().as_bytes();
    let (mut lon, mut lat) = (-180.0, -90.0);
    let (mut lon_size, mut lat_size) = (0.0, 0.0);
    for (p, (max, dlon, dlat)) in b.chunks(2).zip(PAIRS) {
        lon += pair_value(p[0], max)? * dlon;
        lat += pair_value(p[1], max)? * dlat;
        (lon_size, lat_size) = (dlon, dlat);
    }
    Some((lat + lat_size / 2.0, lon + lon_size / 2.0))
}

/// Great-circle distance between the centres of two locators, each truncated to
/// `max_chars` characters if given (e.g. 6 to measure between sub-square centres).
pub fn distance_km_limited(a: &str, b: &str, max_chars: Option<usize>) -> Option<f64> {
    let cut = |s: &str| -> String {
        let s = s.trim();
        match max_chars {
            Some(n) if s.len() > n => s[..n].to_string(),
            _ => s.to_string(),
        }
    };
    distance_km(&cut(a), &cut(b))
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
        assert!(is_valid("RE78ir45"));
        assert!(is_valid("RE78ir45bx"));
        assert!(!is_valid("RE78ir4"));
        assert!(!is_valid("RE78irab"));
        assert!(!is_valid("RE78ir45bz"));
    }

    #[test]
    fn centres() {
        let (lat, lon) = center("QF22le").unwrap();
        assert!((lat - (-37.8125)).abs() < 1e-9, "{lat}");
        assert!((lon - 144.958333).abs() < 1e-5, "{lon}");
        let (lat, lon) = center("QF22").unwrap();
        assert_eq!((lat, lon), (-37.5, 145.0));
        // An extended square lies inside its sub-square.
        let (lat, lon) = center("QF22le09").unwrap();
        assert!((-37.833333..-37.791666).contains(&lat), "{lat}");
        assert!((144.916666..145.0).contains(&lon), "{lon}");
    }

    #[test]
    fn distances() {
        assert_eq!(distance_km("QF22le", "QF22le"), Some(0.0));
        // Melbourne to Sydney is roughly 710 km.
        let d = distance_km("QF22le", "QF56od").unwrap();
        assert!((690.0..730.0).contains(&d), "{d}");
        // Two points 1 km apart in the same sub-square measure 0 when truncated to 6 characters.
        assert!(distance_km("RF73jc11", "RF73jc19").unwrap() > 0.5);
        assert_eq!(distance_km_limited("RF73jc11", "RF73jc19", Some(6)), Some(0.0));
    }
}
