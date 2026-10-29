const PORTABLE_SUFFIXES: &[&str] = &["P", "M", "MM", "AM", "QRP", "A", "B", "LH", "R"];

pub fn normalize(call: &str) -> String {
    call.trim().to_ascii_uppercase()
}

/// Rough sanity check for a callsign: letters and digits with optional slashes,
/// containing at least one of each.
pub fn is_plausible(call: &str) -> bool {
    let call = normalize(call);
    call.len() >= 3
        && call.chars().all(|c| c.is_ascii_alphanumeric() || c == '/')
        && call.chars().any(|c| c.is_ascii_digit())
        && call.chars().any(|c| c.is_ascii_alphabetic())
        && !call.starts_with('/')
        && !call.ends_with('/')
}

fn base_prefix(call: &str) -> String {
    match call.rfind(|c: char| c.is_ascii_digit()) {
        Some(i) => call[..=i].to_string(),
        None => format!("{}0", &call[..call.len().min(2)]),
    }
}

/// CQ WPX style prefix: VK4ABC -> VK4, VK2/VK4ABC -> VK2, VK4ABC/3 -> VK3, VK4ABC/P -> VK4.
pub fn wpx_prefix(call: &str) -> String {
    let call = normalize(call);
    let parts: Vec<&str> = call.split('/').filter(|p| !p.is_empty()).collect();
    match parts.len() {
        0 => return String::new(),
        1 => return base_prefix(parts[0]),
        _ => {}
    }
    let mut rest: Vec<&str> = parts
        .iter()
        .copied()
        .filter(|p| !PORTABLE_SUFFIXES.contains(p))
        .collect();
    if let Some(i) = rest
        .iter()
        .position(|p| p.len() == 1 && p.chars().all(|c| c.is_ascii_digit()))
    {
        let digit = rest.remove(i);
        let base = rest.first().map(|c| base_prefix(c)).unwrap_or_default();
        let letters = base.trim_end_matches(|c: char| c.is_ascii_digit());
        return format!("{letters}{digit}");
    }
    match rest.len() {
        0 => base_prefix(parts[0]),
        1 => base_prefix(rest[0]),
        _ => {
            let p = if rest[0].len() <= rest[1].len() {
                rest[0]
            } else {
                rest[1]
            };
            if p.chars().any(|c| c.is_ascii_digit()) {
                base_prefix(p)
            } else {
                format!("{p}0")
            }
        }
    }
}

/// True if the call's prefix starts with any of `prefixes` (e.g. "VK", "ZL"), or the list is empty.
pub fn matches_any(call: &str, prefixes: &[String]) -> bool {
    if prefixes.is_empty() {
        return true;
    }
    let p = wpx_prefix(call);
    prefixes.iter().any(|x| p.starts_with(x.as_str()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefixes() {
        assert_eq!(wpx_prefix("vk4abc"), "VK4");
        assert_eq!(wpx_prefix("VK4ABC/P"), "VK4");
        assert_eq!(wpx_prefix("VK4ABC/3"), "VK3");
        assert_eq!(wpx_prefix("VK2/VK4ABC"), "VK2");
        assert_eq!(wpx_prefix("ZL/VK4ABC"), "ZL0");
        assert_eq!(wpx_prefix("P29ZX"), "P29");
        assert_eq!(wpx_prefix("VK100ANZAC"), "VK100");
        assert_eq!(wpx_prefix("ZM1A"), "ZM1");
    }

    #[test]
    fn workable() {
        let vkzl = vec!["VK".to_string(), "ZL".to_string()];
        assert!(matches_any("VK3ABC", &vkzl));
        assert!(matches_any("ZL2XY", &vkzl));
        assert!(!matches_any("JA1ABC", &vkzl));
        assert!(matches_any("JA1ABC", &[]));
    }

    #[test]
    fn plausible() {
        assert!(is_plausible("VK4ABC"));
        assert!(is_plausible("VK2/VK4ABC"));
        assert!(!is_plausible("VK"));
        assert!(!is_plausible("VKABC"));
        assert!(!is_plausible("VK4 ABC"));
    }
}
