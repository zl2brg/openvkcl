use crate::contest::ContestDef;
use crate::contest::def::ExchField;
use crate::contest::engine::{self, Evaluation};
use crate::model::{LogSetup, Qso};
use std::collections::BTreeMap;

fn csv(fields: &[&str]) -> String {
    let cells: Vec<String> = fields
        .iter()
        .map(|f| {
            if f.contains([',', '"', '\n', '\r']) {
                format!("\"{}\"", f.replace('"', "\"\""))
            } else {
                f.to_string()
            }
        })
        .collect();
    cells.join(",") + "\r\n"
}

fn num(x: f64) -> String {
    if x.fract() == 0.0 { format!("{x:.0}") } else { format!("{x:.1}") }
}

/// A per-band score sheet: each contact with distance, claimed points and a running
/// total, as paper-style contests such as the NZART VHF series ask for.
pub fn render(def: &ContestDef, setup: &LogSetup, qsos: &[Qso], eval: &Evaluation) -> String {
    let mut out = String::new();
    let s = &setup.station;
    out += &csv(&["Contest", &def.name]);
    out += &csv(&["Date", &setup.start.format("%Y-%m-%d").to_string()]);
    out += &csv(&["Callsign", &s.callsign.to_ascii_uppercase()]);
    out += &csv(&["Operators", &s.operators]);
    out += &csv(&["Locator", &s.locator.to_ascii_uppercase()]);
    out += &csv(&["Location", &setup.location]);
    for c in &def.categories {
        let value = setup.categories.get(&c.key).unwrap_or(&c.default);
        let label = if c.label.is_empty() { &c.key } else { &c.label };
        out += &csv(&[label, value]);
    }
    let bonus: Vec<String> = engine::applied_bonuses(def, setup)
        .iter()
        .map(|b| format!("{} x{}", b.value, b.factor))
        .collect();
    if !bonus.is_empty() {
        out += &csv(&["Bonuses applied", &bonus.join(" ")]);
    }

    let mut by_band: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    let mut order: Vec<usize> = (0..qsos.len()).collect();
    order.sort_by_key(|&i| (qsos[i].time, qsos[i].id));
    for i in order {
        let band = def.bands.iter().position(|b| b == &qsos[i].band).unwrap_or(usize::MAX);
        by_band.entry(band).or_default().push(i);
    }

    let has = |f| def.has_field(f);
    let label = def.number_label.as_str();
    let mut header: Vec<String> = vec!["Date".into(), "Time UTC".into()];
    if def.blocks.is_some() {
        header.push("Period".into());
    }
    header.extend(["Call".into(), "Mode".into(), "Sent RST".into()]);
    if has(ExchField::Serial) {
        header.push("Sent Nr".into());
    }
    if has(ExchField::Number) {
        header.push(format!("Sent {label}"));
    }
    header.push("Rcvd RST".into());
    if has(ExchField::Serial) {
        header.push("Rcvd Nr".into());
    }
    if has(ExchField::Number) {
        header.push(format!("Rcvd {label}"));
    }
    if has(ExchField::Locator) {
        header.extend(["Locator".into(), "Distance km".into()]);
    }
    header.extend(["Points".into(), "Running total".into()]);
    if def.multiplier.is_some() {
        header.push("New mult".into());
    }
    header.push("Note".into());
    let header: Vec<&str> = header.iter().map(String::as_str).collect();

    // (band, mode) -> (QSOs, points, new mults)
    let mut summary: BTreeMap<(usize, String), (u32, f64, u32)> = BTreeMap::new();
    for (&band_idx, rows) in &by_band {
        let band = &qsos[rows[0]].band;
        out += "\r\n";
        out += &csv(&["Band", band]);
        out += &csv(&header);
        let mut running = 0.0;
        for &i in rows {
            let q = &qsos[i];
            let r = &eval.results[i];
            running += r.points;
            let note = if let Some(why) = &r.invalid {
                why.clone()
            } else if r.dupe {
                "Dupe".into()
            } else {
                r.warning.clone().unwrap_or_default()
            };
            if r.invalid.is_none() && !r.dupe {
                let e = summary.entry((band_idx, q.mode.clone())).or_default();
                e.0 += 1;
                e.1 += r.points;
                e.2 += r.new_mult as u32;
            }
            let mut row: Vec<String> = vec![q.time.format("%Y-%m-%d").to_string(), q.time.format("%H%M").to_string()];
            if def.blocks.is_some() {
                row.push((r.block + 1).to_string());
            }
            row.extend([q.call.to_ascii_uppercase(), q.mode.clone(), q.sent_rst.clone()]);
            if has(ExchField::Serial) {
                row.push(q.sent_nr.clone());
            }
            if has(ExchField::Number) {
                row.push(q.sent_exch.clone());
            }
            row.push(q.rcvd_rst.clone());
            if has(ExchField::Serial) {
                row.push(q.rcvd_nr.clone());
            }
            if has(ExchField::Number) {
                row.push(q.rcvd_exch.clone());
            }
            if has(ExchField::Locator) {
                row.push(q.rcvd_locator.to_ascii_uppercase());
                row.push(r.distance_km.map(|d| format!("{d:.0}")).unwrap_or_default());
            }
            row.extend([num(r.points), num(running)]);
            if def.multiplier.is_some() {
                row.push(if r.new_mult { r.mult.clone() } else { String::new() });
            }
            row.push(note);
            out += &csv(&row.iter().map(String::as_str).collect::<Vec<_>>());
        }
        out += &csv(&["Band total", band, &num(running)]);
    }

    out += "\r\n";
    out += &csv(&["Summary"]);
    let mut cols = vec!["Band", "Mode", "QSOs", "Points"];
    if def.multiplier.is_some() {
        cols.push("Mults");
    }
    out += &csv(&cols);
    for ((band_idx, mode), (n, pts, mults)) in &summary {
        let band = def.bands.get(*band_idx).map_or("?", String::as_str);
        let (n, pts, mults) = (n.to_string(), num(*pts), mults.to_string());
        let mut row = vec![band, mode.as_str(), n.as_str(), pts.as_str()];
        if def.multiplier.is_some() {
            row.push(mults.as_str());
        }
        out += &csv(&row);
    }
    out += "\r\n";
    out += &csv(&["Claimed score", &num(eval.score.total)]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoting() {
        assert_eq!(csv(&["a", "b,c", "say \"hi\""]), "a,\"b,c\",\"say \"\"hi\"\"\"\r\n");
        assert_eq!(num(3.0), "3");
        assert_eq!(num(2.5), "2.5");
    }
}
