use chrono::{DateTime, Duration, NaiveTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A contest's rules, loaded from a TOML file so they can be changed without a rebuild.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContestDef {
    pub id: String,
    pub name: String,
    pub cabrillo_name: String,
    #[serde(default)]
    pub rules_url: String,
    pub duration_minutes: i64,
    /// UTC time of day the contest normally starts, "HH:MM".
    pub default_start_utc: String,
    pub bands: Vec<String>,
    /// Cabrillo modes accepted in this contest: PH, CW, RY, DG.
    pub modes: Vec<String>,
    /// Modes that are treated as the same mode for dupes and multipliers, e.g. RY = "CW".
    #[serde(default)]
    pub mode_groups: BTreeMap<String, String>,
    pub exchange: Vec<ExchField>,
    #[serde(default = "default_number_label")]
    pub number_label: String,
    /// Prefixes (e.g. "VK", "ZL") of stations that may be worked. Empty means anyone.
    #[serde(default)]
    pub workable: Vec<String>,
    pub rework: Rework,
    #[serde(default)]
    pub blocks: Option<Blocks>,
    pub scoring: Scoring,
    #[serde(default)]
    pub multiplier: Option<Multiplier>,
    /// Bands scored as separate entries, e.g. HF and VHF+ in the John Moyle Field Day.
    #[serde(default)]
    pub score_groups: Vec<ScoreGroup>,
    #[serde(default)]
    pub categories: Vec<CategoryField>,
}

fn default_number_label() -> String {
    "Nr".into()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExchField {
    Rst,
    Serial,
    /// A fixed number chosen at setup, e.g. years licensed in the Remembrance Day contest.
    Number,
    Locator,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Dim {
    Band,
    Mode,
    Block,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReworkKind {
    /// Each station once per `per` for the whole contest.
    Never,
    /// Once per `per` in each time block.
    Block,
    /// Again after `minutes` have passed.
    Interval,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rework {
    pub kind: ReworkKind,
    #[serde(default)]
    pub minutes: i64,
    #[serde(default)]
    pub per: Vec<Dim>,
    /// Either station moving to a new 4-character grid square allows an immediate rework.
    #[serde(default)]
    pub new_square_resets: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Blocks {
    pub minutes: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Formula {
    SumPoints,
    PointsTimesMults,
    /// Sum over blocks of (points in block x multipliers in block).
    BlockPointsTimesMults,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scoring {
    pub formula: Formula,
    pub points: Points,
    /// Prefixes whose contacts score points. Empty means all workable stations.
    #[serde(default)]
    pub count_only: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Points {
    Fixed {
        #[serde(default = "one")]
        base: f64,
        /// Per-band points; bands not listed score `base`.
        #[serde(default)]
        band: BTreeMap<String, f64>,
        /// Per-mode multiplying factor.
        #[serde(default)]
        mode: BTreeMap<String, f64>,
        #[serde(default)]
        local_time: Option<LocalTimeBonus>,
    },
    Distance {
        band_factor: BTreeMap<String, f64>,
        /// Beyond this distance, kilometres count at 1/`taper_divisor` on `taper_bands`.
        #[serde(default)]
        taper_km: Option<f64>,
        #[serde(default = "hundred")]
        taper_divisor: f64,
        #[serde(default)]
        taper_bands: Vec<String>,
    },
}

fn one() -> f64 {
    1.0
}

fn hundred() -> f64 {
    100.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalTimeBonus {
    pub from: String,
    pub to: String,
    pub factor: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MultKind {
    Prefix,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Multiplier {
    pub kind: MultKind,
    #[serde(default)]
    pub per: Vec<Dim>,
    /// Prefixes that count as multipliers. Empty means all.
    #[serde(default)]
    pub only: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoreGroup {
    pub name: String,
    pub bands: Vec<String>,
}

/// A Cabrillo CATEGORY-* header choice. An empty option means the header is omitted.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryField {
    pub key: String,
    pub options: Vec<String>,
    #[serde(default)]
    pub default: String,
}

impl ContestDef {
    pub fn mode_group(&self, mode: &str) -> String {
        self.mode_groups
            .get(mode)
            .cloned()
            .unwrap_or_else(|| mode.to_string())
    }

    pub fn has_field(&self, f: ExchField) -> bool {
        self.exchange.contains(&f)
    }

    pub fn block_of(&self, start: DateTime<Utc>, t: DateTime<Utc>) -> i64 {
        match &self.blocks {
            Some(b) if b.minutes > 0 => (t - start).num_minutes().div_euclid(b.minutes).max(0),
            _ => 0,
        }
    }

    pub fn block_start(&self, start: DateTime<Utc>, block: i64) -> Option<DateTime<Utc>> {
        self.blocks
            .as_ref()
            .map(|b| start + Duration::minutes(block * b.minutes))
    }

    pub fn end(&self, start: DateTime<Utc>) -> DateTime<Utc> {
        start + Duration::minutes(self.duration_minutes)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.bands.is_empty() || self.modes.is_empty() {
            return Err(format!("{}: bands and modes must not be empty", self.id));
        }
        NaiveTime::parse_from_str(&self.default_start_utc, "%H:%M")
            .map_err(|e| format!("{}: default_start_utc: {e}", self.id))?;
        if self.rework.kind == ReworkKind::Interval && self.rework.minutes <= 0 {
            return Err(format!("{}: interval rework needs minutes", self.id));
        }
        if self.rework.kind == ReworkKind::Block && self.blocks.is_none() {
            return Err(format!("{}: block rework needs [blocks]", self.id));
        }
        if let Points::Fixed {
            local_time: Some(lt),
            ..
        } = &self.scoring.points
        {
            for t in [&lt.from, &lt.to] {
                NaiveTime::parse_from_str(t, "%H:%M")
                    .map_err(|e| format!("{}: local_time: {e}", self.id))?;
            }
        }
        Ok(())
    }
}
