//! DXCC countries from the country file (cty.csv from country-files.com): which
//! entity a callsign belongs to, with its continent and CQ and ITU zones.

use serde::Serialize;
use std::collections::HashMap;

/// Where cty.csv is published.
pub const DOWNLOAD_URL: &str = "https://www.country-files.com/bigcty/cty.csv";

#[derive(Debug, Clone)]
struct Entity {
    name: String,
    dxcc: u32,
    cont: String,
    cqz: u8,
    ituz: u8,
}

fn entity_of(f: &[&str], dxcc: u32) -> Entity {
    Entity {
        name: f[1].trim().to_string(),
        dxcc,
        cont: f[3].trim().to_string(),
        cqz: f[4].trim().parse().unwrap_or(0),
        ituz: f[5].trim().parse().unwrap_or(0),
    }
}

/// Where a prefix or exact call points: an entity, and its zones there.
#[derive(Debug, Clone)]
struct Hit {
    entity: usize,
    cont: String,
    cqz: u8,
    ituz: u8,
}

/// The country a callsign is in.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Country {
    pub dxcc: u32,
    pub country: String,
    pub cont: String,
    pub cqz: u8,
    pub ituz: u8,
}

#[derive(Debug, Default)]
pub struct Cty {
    entities: Vec<Entity>,
    /// Entity for each DXCC number.
    by_dxcc: HashMap<u32, usize>,
    exact: HashMap<String, Hit>,
    prefixes: HashMap<String, Hit>,
    longest: usize,
}

/// Portable and similar suffixes that don't change the country.
const IGNORED_PARTS: &[&str] = &["P", "M", "QRP", "QRPP", "A", "B", "LH", "J", "R", "T"];

/// Parse a prefix like "AA0(4)[7]" or "=N2NL/MM(7)" into the prefix and its zone overrides.
fn parse_prefix(raw: &str, base: &Hit) -> (String, bool, Hit) {
    let exact = raw.starts_with('=');
    let raw = raw.trim_start_matches('=');
    let end = raw.find(['(', '[', '<', '{', '~']).unwrap_or(raw.len());
    let mut hit = base.clone();
    let between = |open: char, close: char| {
        let i = raw.find(open)?;
        let j = raw[i + 1..].find(close)? + i + 1;
        Some(raw[i + 1..j].to_string())
    };
    if let Some(z) = between('(', ')').and_then(|z| z.parse().ok()) {
        hit.cqz = z;
    }
    if let Some(z) = between('[', ']').and_then(|z| z.parse().ok()) {
        hit.ituz = z;
    }
    if let Some(c) = between('{', '}') {
        hit.cont = c;
    }
    (raw[..end].to_ascii_uppercase(), exact, hit)
}

impl Cty {
    pub fn parse(text: &str) -> Cty {
        let mut cty = Cty::default();
        let rows: Vec<Vec<&str>> = text
            .lines()
            .map(|l| l.trim().trim_end_matches(';'))
            .filter(|l| !l.is_empty())
            .map(|l| l.splitn(10, ',').collect::<Vec<_>>())
            .filter(|f| f.len() == 10)
            .collect();
        // Entities first. Lines starting with '*' are only countries for the WAE
        // award (e.g. Shetland); for DXCC they belong to their parent entity.
        for f in rows.iter().filter(|f| !f[0].starts_with('*')) {
            let Ok(dxcc) = f[2].trim().parse() else { continue };
            cty.by_dxcc.entry(dxcc).or_insert(cty.entities.len());
            cty.entities.push(entity_of(f, dxcc));
        }
        for f in &rows {
            let Ok(dxcc) = f[2].trim().parse::<u32>() else { continue };
            let entity = match cty.by_dxcc.get(&dxcc) {
                Some(&i) => i,
                None => {
                    cty.entities.push(entity_of(f, dxcc));
                    cty.by_dxcc.insert(dxcc, cty.entities.len() - 1);
                    cty.entities.len() - 1
                }
            };
            let own = entity_of(f, dxcc);
            let base = Hit {
                entity,
                cont: own.cont,
                cqz: own.cqz,
                ituz: own.ituz,
            };
            for raw in f[9].split_whitespace() {
                let (prefix, exact, hit) = parse_prefix(raw, &base);
                if prefix.is_empty() {
                    continue;
                }
                if exact {
                    cty.exact.insert(prefix, hit);
                } else {
                    cty.longest = cty.longest.max(prefix.len());
                    // The parent entity's own line wins over a WAE-only one.
                    if !f[0].starts_with('*') || !cty.prefixes.contains_key(&prefix) {
                        cty.prefixes.insert(prefix, hit);
                    }
                }
            }
        }
        cty
    }

    pub fn load(path: &std::path::Path) -> Cty {
        std::fs::read_to_string(path)
            .map(|t| Cty::parse(&t))
            .unwrap_or_default()
    }

    /// Number of DXCC entities known.
    pub fn len(&self) -> usize {
        self.entities.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entities.is_empty()
    }

    fn country(&self, hit: &Hit) -> Country {
        let e = &self.entities[hit.entity];
        Country {
            dxcc: e.dxcc,
            country: e.name.clone(),
            cont: hit.cont.clone(),
            cqz: hit.cqz,
            ituz: hit.ituz,
        }
    }

    /// The entity with this DXCC number, e.g. from an imported contact.
    pub fn by_number(&self, dxcc: u32) -> Option<Country> {
        let &i = self.by_dxcc.get(&dxcc)?;
        let e = &self.entities[i];
        Some(Country {
            dxcc,
            country: e.name.clone(),
            cont: e.cont.clone(),
            cqz: e.cqz,
            ituz: e.ituz,
        })
    }

    fn longest_prefix(&self, s: &str) -> Option<&Hit> {
        (1..=s.len().min(self.longest))
            .rev()
            .find_map(|n| s.get(..n).and_then(|p| self.prefixes.get(p)))
    }

    /// The country for a callsign, e.g. VK9NA -> Norfolk Island, VK2/W1AW -> Australia.
    /// Maritime and aeronautical mobile stations are in no country.
    pub fn lookup(&self, call: &str) -> Option<Country> {
        let call = call.trim().to_ascii_uppercase();
        if call.is_empty() || self.is_empty() {
            return None;
        }
        if let Some(hit) = self.exact.get(&call) {
            return Some(self.country(hit));
        }
        let parts: Vec<&str> = call
            .split('/')
            .filter(|p| !p.is_empty() && !IGNORED_PARTS.contains(p))
            .collect();
        if call.split('/').any(|p| p == "MM" || p == "AM") {
            return None;
        }
        let target = match parts.as_slice() {
            [] => return None,
            [one] => one.to_string(),
            [a, b, ..] => {
                let digit = |p: &str| p.len() == 1 && p.chars().all(|c| c.is_ascii_digit());
                if digit(b) || digit(a) {
                    // W1AW/4: a US call in call area 4. Replace the call's digit.
                    let (base, d) = if digit(b) { (*a, *b) } else { (*b, *a) };
                    match base.rfind(|c: char| c.is_ascii_digit()) {
                        Some(i) => format!("{}{d}{}", &base[..i], &base[i + 1..]),
                        None => base.to_string(),
                    }
                } else if a.len() <= b.len() {
                    a.to_string()
                } else {
                    b.to_string()
                }
            }
        };
        if let Some(hit) = self.exact.get(&target) {
            return Some(self.country(hit));
        }
        self.longest_prefix(&target).map(|h| self.country(h))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
3D2,Fiji,176,OC,32,56,-17.78,-177.92,-12.0,3D2 =3D5X;
3D2/r,Rotuma Island,460,OC,32,56,-12.48,-177.08,-12.0,=3D2R =3D2RR;
K,United States,291,NA,5,8,37.60,91.87,5.0,AA K N W =N2NL/MM(7) AA0(4)[7] W4(5);
KH6,Hawaii,110,OC,31,61,21.12,157.48,10.0,AH6 KH6 NH6 WH6;
GM,Scotland,279,EU,14,27,56.82,4.18,0.0,GM MM 2M;
*GM/s,Shetland Islands,279,EU,14,27,60.50,1.50,0.0,GM8 =2M0BDR;
VK,Australia,150,OC,30,59,-23.70,-132.33,-10.0,AX VI VJ VK VL VK6(29)[58] =VK9MAV;
VK9N,Norfolk Island,189,OC,32,60,-29.03,-167.93,-11.5,AX9 VI9 VK9;
VK9X,Christmas Island,35,OC,29,54,-10.48,-105.62,-7.0,VK9X;
";

    fn name(cty: &Cty, call: &str) -> Option<String> {
        cty.lookup(call).map(|c| c.country)
    }

    #[test]
    fn lookups() {
        let cty = Cty::parse(SAMPLE);
        assert_eq!(cty.len(), 8);
        assert_eq!(name(&cty, "vk2abc").as_deref(), Some("Australia"));
        assert_eq!(name(&cty, "VK9NA").as_deref(), Some("Norfolk Island"));
        assert_eq!(name(&cty, "VK9XX").as_deref(), Some("Christmas Island"));
        assert_eq!(name(&cty, "VK9MAV").as_deref(), Some("Australia"));
        assert_eq!(name(&cty, "VK9MAV/P").as_deref(), Some("Australia"));
        assert_eq!(name(&cty, "3D2RR").as_deref(), Some("Rotuma Island"));
        assert_eq!(name(&cty, "3D2AB").as_deref(), Some("Fiji"));
        assert_eq!(name(&cty, "W1AW").as_deref(), Some("United States"));
        assert_eq!(name(&cty, "KH6/W1AW").as_deref(), Some("Hawaii"));
        assert_eq!(name(&cty, "W1AW/KH6").as_deref(), Some("Hawaii"));
        assert_eq!(name(&cty, "VK2/W1AW").as_deref(), Some("Australia"));
        assert_eq!(name(&cty, "W1AW/MM"), None);
        assert_eq!(name(&cty, "N2NL/MM").as_deref(), Some("United States"));
        assert_eq!(name(&cty, "JA1ABC"), None);
        // WAE-only countries count as their DXCC parent.
        let shetland = cty.lookup("GM8ABC").unwrap();
        assert_eq!((shetland.country.as_str(), shetland.dxcc), ("Scotland", 279));
        assert_eq!(name(&cty, "2M0BDR").as_deref(), Some("Scotland"));
        // Zone overrides on a prefix.
        let vk6 = cty.lookup("VK6ABC").unwrap();
        assert_eq!((vk6.cqz, vk6.ituz), (29, 58));
        let w4 = cty.lookup("W1AW/4").unwrap();
        assert_eq!(w4.cqz, 5);
        assert_eq!(cty.lookup("W4ABC").unwrap().cqz, 5);
        assert_eq!(cty.lookup("AA0AA").unwrap().ituz, 7);
        assert_eq!(cty.by_number(150).unwrap().country, "Australia");
        assert_eq!(cty.by_number(150).unwrap().cont, "OC");
        assert_eq!(cty.by_number(1), None);
    }
}
