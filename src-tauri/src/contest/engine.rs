use super::def::{Bonus, ContestDef, Dim, ExchField, Formula, MultKind, Points, ReworkKind};
use crate::model::{LogSetup, Qso};
use crate::{callsign, geo};
use chrono::{DateTime, Duration, NaiveTime, Utc};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap, HashSet};

#[derive(Debug, Clone, Default, Serialize)]
pub struct QsoResult {
    pub dupe: bool,
    /// Why the contact can't count at all (wrong band, unworkable station...).
    pub invalid: Option<String>,
    /// Problems with the exchange; the contact still counts as worked.
    pub warning: Option<String>,
    pub points: f64,
    pub new_mult: bool,
    /// The multiplier this contact counts towards (prefix or branch), if any.
    pub mult: String,
    pub prefix: String,
    pub block: i64,
    pub distance_km: Option<f64>,
    /// For dupes, when this station can next be worked on this band and mode.
    pub next_ok: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct GroupScore {
    pub name: String,
    pub qsos: u32,
    pub points: f64,
    pub mults: u32,
    pub score: f64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Score {
    pub groups: Vec<GroupScore>,
    pub total: f64,
    pub qsos: u32,
    pub dupes: u32,
    pub invalid: u32,
    pub by_band: Vec<(String, u32)>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Evaluation {
    /// Same order as the QSOs passed to `evaluate`.
    pub results: Vec<QsoResult>,
    pub score: Score,
}

#[derive(Default)]
struct Tally {
    qsos: u32,
    points: f64,
    mults: HashSet<String>,
    blocks: BTreeMap<i64, (f64, HashSet<String>)>,
}

fn group_of(def: &ContestDef, band: &str) -> usize {
    def.score_groups
        .iter()
        .position(|g| g.bands.iter().any(|b| b == band))
        .unwrap_or(0)
}

fn station_invalid(def: &ContestDef, q: &Qso) -> Option<String> {
    if !callsign::is_plausible(&q.call) {
        return Some("Not a valid callsign".into());
    }
    if !def.bands.iter().any(|b| b == &q.band) {
        return Some(format!("{} is not a contest band", q.band));
    }
    if !def.modes.iter().any(|m| m == &q.mode) {
        return Some(format!("{} is not a contest mode", q.mode));
    }
    if !callsign::matches_any(&q.call, &def.workable) {
        return Some("Station is not workable in this contest".into());
    }
    None
}

fn exchange_warning(def: &ContestDef, setup: &LogSetup, q: &Qso) -> Option<String> {
    let mut problems = Vec::new();
    let numeric = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit());
    if def.has_field(ExchField::Serial) && !numeric(&q.rcvd_nr) {
        problems.push("received serial number missing".to_string());
    }
    if def.has_field(ExchField::Number)
        && callsign::matches_any(&q.call, &def.number_from)
        && !numeric(&q.rcvd_exch)
    {
        problems.push(format!("received {} missing", def.number_label.to_lowercase()));
    }
    if def.has_field(ExchField::Locator) {
        if q.rcvd_locator.len() < 6 || !geo::is_valid(&q.rcvd_locator) {
            problems.push("received locator must be at least 6 characters".to_string());
        }
        if q.sent_locator.len() < 6 || !geo::is_valid(&q.sent_locator) {
            problems.push("own locator must be at least 6 characters".to_string());
        }
    }
    if !def.in_session(setup.start, q.time) {
        problems.push("outside the contest operating periods".to_string());
    }
    (!problems.is_empty()).then(|| {
        let mut s = problems.join(", ");
        s[..1].make_ascii_uppercase();
        s
    })
}

fn local_time_factor(setup: &LogSetup, t: DateTime<Utc>, from: &str, to: &str, factor: f64) -> f64 {
    let parse = |s: &str| NaiveTime::parse_from_str(s, "%H:%M").ok();
    let (Some(from), Some(to)) = (parse(from), parse(to)) else {
        return 1.0;
    };
    let local = (t + Duration::minutes(setup.utc_offset_minutes as i64)).time();
    let inside = if from <= to {
        local >= from && local < to
    } else {
        local >= from || local < to
    };
    if inside { factor } else { 1.0 }
}

/// Bonuses that apply to this log, from its setup categories (or their defaults).
pub fn applied_bonuses<'a>(def: &'a ContestDef, setup: &LogSetup) -> Vec<&'a Bonus> {
    def.scoring
        .bonus
        .iter()
        .filter(|b| {
            let chosen = setup.categories.get(&b.category).cloned().or_else(|| {
                def.categories
                    .iter()
                    .find(|c| c.key == b.category)
                    .map(|c| c.default.clone())
            });
            chosen.as_deref() == Some(b.value.as_str())
        })
        .collect()
}

fn bonus_factor(def: &ContestDef, setup: &LogSetup) -> f64 {
    applied_bonuses(def, setup).iter().map(|b| b.factor).product()
}

/// Points for a valid, non-dupe contact, plus distance for distance-scored contests.
fn points(def: &ContestDef, setup: &LogSetup, q: &Qso) -> (f64, Option<f64>) {
    let (p, dist) = basic_points(def, setup, q);
    (def.scoring.round.apply(p * bonus_factor(def, setup)), dist)
}

fn basic_points(def: &ContestDef, setup: &LogSetup, q: &Qso) -> (f64, Option<f64>) {
    match &def.scoring.points {
        Points::Fixed {
            base,
            band,
            mode_points,
            foreign,
            mode,
            local_time,
        } => {
            let by_mode = mode_points
                .get(&q.mode)
                .or_else(|| mode_points.get(&def.mode_group(&q.mode)));
            let mut p = band.get(&q.band).or(by_mode).copied().unwrap_or(*base);
            if let Some(f) = foreign
                && callsign::matches_any(&setup.station.callsign, &f.home)
                    != callsign::matches_any(&q.call, &f.home)
            {
                p = f.points;
            }
            p *= mode
                .get(&q.mode)
                .or_else(|| mode.get(&def.mode_group(&q.mode)))
                .copied()
                .unwrap_or(1.0);
            if let Some(lt) = local_time {
                p *= local_time_factor(setup, q.time, &lt.from, &lt.to, lt.factor);
            }
            (p, None)
        }
        Points::Distance {
            band_factor,
            band_table,
            mode,
            taper_km,
            taper_divisor,
            taper_bands,
            max_locator_chars,
        } => {
            if q.rcvd_locator.len() < 6 || q.sent_locator.len() < 6 {
                return (0.0, None);
            }
            let Some(d) = geo::distance_km_limited(&q.sent_locator, &q.rcvd_locator, *max_locator_chars)
            else {
                return (0.0, None);
            };
            let basic = if let Some(rows) = band_table.get(&q.band) {
                rows.iter()
                    .find(|(upto, _)| d < *upto)
                    .or(rows.last())
                    .map_or(0.0, |(_, p)| *p)
            } else {
                let mut km = d;
                if let Some(t) = taper_km
                    && taper_bands.iter().any(|b| b == &q.band)
                    && km > *t
                {
                    km = t + (km - t) / taper_divisor;
                }
                km * band_factor.get(&q.band).copied().unwrap_or(0.0)
            };
            let basic = def.scoring.round.apply(basic);
            let mode_factor = mode
                .get(&q.mode)
                .or_else(|| mode.get(&def.mode_group(&q.mode)))
                .copied()
                .unwrap_or(1.0);
            (basic * mode_factor, Some(d))
        }
    }
}

fn dupe_key(def: &ContestDef, q: &Qso, block: i64) -> String {
    let mut key = callsign::normalize(&q.call);
    for dim in &def.rework.per {
        key.push('|');
        match dim {
            Dim::Band => key.push_str(&q.band),
            Dim::Mode => key.push_str(&def.mode_group(&q.mode)),
            Dim::Block => key.push_str(&block.to_string()),
        }
    }
    if def.rework.kind == ReworkKind::Block && !def.rework.per.contains(&Dim::Block) {
        key.push_str(&format!("|b{block}"));
    }
    if def.rework.new_square_resets {
        key.push_str(&format!(
            "|{}|{}",
            geo::square(&q.sent_locator),
            geo::square(&q.rcvd_locator)
        ));
    }
    key
}

/// "007" -> "7", "00" -> "0", so branch numbers compare however they were typed.
fn norm_number(s: &str) -> String {
    let s = s.trim();
    let t = s.trim_start_matches('0');
    if t.is_empty() && !s.is_empty() { "0".into() } else { t.to_string() }
}

/// The multiplier a contact counts towards, before splitting by band/mode/block.
fn mult_value(def: &ContestDef, setup: &LogSetup, q: &Qso, prefix: &str) -> Option<String> {
    let m = def.multiplier.as_ref()?;
    if !callsign::matches_any(&q.call, &m.only) {
        return None;
    }
    let value = match m.kind {
        MultKind::Prefix => prefix.to_string(),
        MultKind::Number => {
            let n = norm_number(&q.rcvd_exch);
            if n.is_empty() || !n.chars().all(|c| c.is_ascii_digit()) {
                return None;
            }
            if m.not_own && n == norm_number(&setup.sent_number) {
                return None;
            }
            n
        }
    };
    let excluded = m.exclude.iter().any(|x| match m.kind {
        MultKind::Prefix => x.eq_ignore_ascii_case(&value),
        MultKind::Number => norm_number(x) == value,
    });
    (!excluded).then_some(value)
}

fn mult_key(def: &ContestDef, q: &Qso, value: &str, block: i64) -> Option<String> {
    let m = def.multiplier.as_ref()?;
    let mut key = value.to_string();
    for dim in &m.per {
        key.push('|');
        match dim {
            Dim::Band => key.push_str(&q.band),
            Dim::Mode => key.push_str(&def.mode_group(&q.mode)),
            Dim::Block => key.push_str(&block.to_string()),
        }
    }
    Some(key)
}

pub fn evaluate(def: &ContestDef, setup: &LogSetup, qsos: &[Qso]) -> Evaluation {
    let mut order: Vec<usize> = (0..qsos.len()).collect();
    order.sort_by_key(|&i| (qsos[i].time, qsos[i].id));

    let groups = def.score_groups.len().max(1);
    let mut tallies: Vec<Tally> = (0..groups).map(|_| Tally::default()).collect();
    let mut last_worked: HashMap<String, DateTime<Utc>> = HashMap::new();
    let mut results = vec![QsoResult::default(); qsos.len()];
    let mut dupes = 0;
    let mut invalid_count = 0;
    let mut by_band: BTreeMap<String, u32> = BTreeMap::new();

    for i in order {
        let q = &qsos[i];
        let block = def.block_of(setup.start, q.time);
        let prefix = callsign::wpx_prefix(&q.call);
        let mut r = QsoResult {
            prefix: prefix.clone(),
            block,
            ..Default::default()
        };

        if let Some(reason) = station_invalid(def, q) {
            r.invalid = Some(reason);
            invalid_count += 1;
            results[i] = r;
            continue;
        }
        r.warning = exchange_warning(def, setup, q);

        let key = dupe_key(def, q, block);
        match def.rework.kind {
            ReworkKind::Never => r.dupe = last_worked.contains_key(&key),
            ReworkKind::Block => {
                r.dupe = last_worked.contains_key(&key);
                if r.dupe {
                    r.next_ok = def.block_start(setup.start, block + 1);
                }
            }
            ReworkKind::Interval => {
                if let Some(t) = last_worked.get(&key) {
                    let next = *t + Duration::minutes(def.rework.minutes);
                    if q.time < next {
                        r.dupe = true;
                        r.next_ok = Some(next);
                    }
                }
            }
        }
        if r.dupe {
            dupes += 1;
            results[i] = r;
            continue;
        }
        last_worked.insert(key, q.time);
        *by_band.entry(q.band.clone()).or_default() += 1;

        let tally = &mut tallies[group_of(def, &q.band)];
        tally.qsos += 1;
        let (p, dist) = points(def, setup, q);
        r.distance_km = dist;
        if callsign::matches_any(&q.call, &def.scoring.count_only) {
            r.points = p;
        }
        tally.points += r.points;
        let block_tally = tally.blocks.entry(block).or_default();
        block_tally.0 += r.points;
        if let Some(value) = mult_value(def, setup, q, &prefix)
            && let Some(mk) = mult_key(def, q, &value, block)
        {
            block_tally.1.insert(mk.clone());
            r.new_mult = tally.mults.insert(mk);
            r.mult = value;
        }
        results[i] = r;
    }

    let mut score = Score {
        dupes,
        invalid: invalid_count,
        by_band: by_band.into_iter().collect(),
        ..Default::default()
    };
    for (gi, t) in tallies.iter().enumerate() {
        let name = def
            .score_groups
            .get(gi)
            .map(|g| g.name.clone())
            .unwrap_or_else(|| "Total".into());
        let value = match def.scoring.formula {
            Formula::SumPoints => t.points,
            Formula::PointsTimesMults => t.points * t.mults.len() as f64,
            Formula::BlockPointsTimesMults => t
                .blocks
                .values()
                .map(|(p, m)| p * m.len() as f64)
                .sum(),
        }
        .ceil();
        score.qsos += t.qsos;
        score.total += value;
        score.groups.push(GroupScore {
            name,
            qsos: t.qsos,
            points: t.points,
            mults: t.mults.len() as u32,
            score: value,
        });
    }
    Evaluation { results, score }
}

/// Evaluate a contact that hasn't been logged yet, as if it were logged now.
pub fn check(def: &ContestDef, setup: &LogSetup, qsos: &[Qso], draft: &Qso) -> QsoResult {
    let mut all: Vec<Qso> = qsos.to_vec();
    all.push(draft.clone());
    evaluate(def, setup, &all).results.pop().unwrap_or_default()
}
