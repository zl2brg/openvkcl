use crate::contest::ContestDef;
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

    for rows in by_band.values() {
        let band = &qsos[rows[0]].band;
        out += "\r\n";
        out += &csv(&["Band", band]);
        out += &csv(&[
            "Date", "Time UTC", "Call", "Mode", "Sent RST", "Sent Nr", "Rcvd RST", "Rcvd Nr",
            "Locator", "Distance km", "Points", "Running total", "Note",
        ]);
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
            out += &csv(&[
                &q.time.format("%Y-%m-%d").to_string(),
                &q.time.format("%H%M").to_string(),
                &q.call.to_ascii_uppercase(),
                &q.mode,
                &q.sent_rst,
                &q.sent_nr,
                &q.rcvd_rst,
                &q.rcvd_nr,
                &q.rcvd_locator.to_ascii_uppercase(),
                &r.distance_km.map(|d| format!("{d:.0}")).unwrap_or_default(),
                &num(r.points),
                &num(running),
                &note,
            ]);
        }
        out += &csv(&["Band total", band, &num(running)]);
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
