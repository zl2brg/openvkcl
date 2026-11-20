//! Super Check Partial: suggest callsigns from a MASTER.SCP database and the
//! current log as the operator types.

use serde::Serialize;
use std::path::Path;

/// Where MASTER.SCP is published.
pub const DOWNLOAD_URL: &str = "https://www.supercheckpartial.com/MASTER.SCP";

/// Partials shorter than this give too many matches to be useful.
pub const MIN_PARTIAL: usize = 2;

#[derive(Debug, Default)]
pub struct Scp {
    calls: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScpMatches {
    /// The whole entry is itself a known call.
    pub exact: bool,
    /// Calls containing the partial ('?' matches any one character).
    pub partial: Vec<String>,
    /// Calls that differ from the whole entry by one character (N+1).
    pub near: Vec<String>,
}

impl Scp {
    /// Parse MASTER.SCP text: one call per line, '#' starts a comment line and
    /// '!!' a directive line (current files open with `!!Order,1,1`).
    pub fn parse(text: &str) -> Scp {
        let mut calls: Vec<String> = text
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#') && !l.starts_with("!!"))
            .map(|l| l.to_ascii_uppercase())
            .collect();
        calls.sort();
        calls.dedup();
        Scp { calls }
    }

    pub fn load(path: &Path) -> Scp {
        std::fs::read_to_string(path)
            .map(|t| Scp::parse(&t))
            .unwrap_or_default()
    }

    pub fn len(&self) -> usize {
        self.calls.len()
    }

    pub fn calls(&self) -> &[String] {
        &self.calls
    }
}

fn contains_pattern(call: &[u8], pat: &[u8]) -> bool {
    pat.len() <= call.len()
        && call
            .windows(pat.len())
            .any(|w| w.iter().zip(pat).all(|(c, p)| *p == b'?' || c == p))
}

/// True if `a` and `b` differ by exactly one substitution, insertion or deletion.
fn one_edit(a: &[u8], b: &[u8]) -> bool {
    let (short, long) = if a.len() <= b.len() { (a, b) } else { (b, a) };
    match long.len() - short.len() {
        0 => a.iter().zip(b).filter(|(x, y)| x != y).count() == 1,
        1 => {
            let i = short.iter().zip(long).take_while(|(x, y)| x == y).count();
            short[i..] == long[i + 1..]
        }
        _ => false,
    }
}

/// Match `partial` against every source, de-duplicated, at most `limit` of each kind.
/// Calls starting with the partial are listed first.
pub fn lookup<'a>(
    partial: &str,
    sources: impl IntoIterator<Item = &'a str> + Clone,
    limit: usize,
) -> ScpMatches {
    let partial = partial.trim().to_ascii_uppercase();
    let pat = partial.as_bytes();
    if partial.chars().filter(|c| *c != '?').count() < MIN_PARTIAL {
        return ScpMatches { exact: false, partial: vec![], near: vec![] };
    }
    let exact = sources.clone().into_iter().any(|c| c == partial);
    let mut found: Vec<&str> = sources
        .clone()
        .into_iter()
        .filter(|c| *c != partial && contains_pattern(c.as_bytes(), pat))
        .collect();
    found.sort_by_key(|c| (!c.starts_with(partial.as_str()), c.len(), *c));
    found.dedup();
    found.truncate(limit);

    let mut near: Vec<&str> = if partial.len() >= 3 && !partial.contains('?') {
        sources
            .into_iter()
            .filter(|c| one_edit(c.as_bytes(), pat) && !found.contains(c))
            .collect()
    } else {
        vec![]
    };
    near.sort();
    near.dedup();
    near.truncate(limit);

    ScpMatches {
        exact,
        partial: found.into_iter().map(String::from).collect(),
        near: near.into_iter().map(String::from).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Scp {
        Scp::parse("# comment\nVK4ABC\nvk2abc\nVK3XYZ\nZL2ABC\nVK4ABD\nVK4AB\n")
    }

    fn calls(s: &Scp) -> impl Iterator<Item = &str> + Clone {
        s.calls().iter().map(String::as_str)
    }

    #[test]
    fn parses() {
        let s = db();
        assert_eq!(s.len(), 6);
        assert!(s.calls().contains(&"VK2ABC".to_string()));
    }

    #[test]
    fn skips_directives() {
        let s = Scp::parse("!!Order,1,1\n#\n# Release 2026.11.20\n#\nVK4ABC\nZL2ABC\n");
        assert_eq!(s.calls(), ["VK4ABC", "ZL2ABC"]);
    }

    #[test]
    fn partial_matches() {
        let s = db();
        let m = lookup("abc", calls(&s), 10);
        assert_eq!(m.partial, vec!["VK2ABC", "VK4ABC", "ZL2ABC"]);
        let m = lookup("VK4", calls(&s), 10);
        assert_eq!(m.partial, vec!["VK4AB", "VK4ABC", "VK4ABD"]);
        let m = lookup("VK?ABC", calls(&s), 10);
        assert_eq!(m.partial, vec!["VK2ABC", "VK4ABC"]);
        assert!(lookup("V", calls(&s), 10).partial.is_empty());
    }

    #[test]
    fn near_matches() {
        let s = db();
        let m = lookup("VK4ABE", calls(&s), 10);
        assert!(m.partial.is_empty());
        assert_eq!(m.near, vec!["VK4AB", "VK4ABC", "VK4ABD"]);
        let m = lookup("VK4ABC", calls(&s), 10);
        assert_eq!(m.near, vec!["VK2ABC", "VK4AB", "VK4ABD"]);
        assert!(m.exact);
    }

    #[test]
    fn edits() {
        assert!(one_edit(b"VK4ABC", b"VK4ABD"));
        assert!(one_edit(b"VK4ABC", b"VK4AB"));
        assert!(one_edit(b"VK4AB", b"VK4XAB"));
        assert!(!one_edit(b"VK4ABC", b"VK4ABC"));
        assert!(!one_edit(b"VK4ABC", b"VK4AXY"));
    }
}
