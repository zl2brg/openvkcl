use chrono::{DateTime, Duration, NaiveTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A contest's rules, loaded from a TOML file so they can be changed without a rebuild.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContestDef {
    pub id: String,
    pub name: String,
    /// A general log for everyday and DX operating: no contest period, scoring or dupes.
    #[serde(default)]
    pub general: bool,
    /// The operator names it and chooses the exchange. No dupes and no score.
    #[serde(default)]
    pub generic: bool,
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
    /// Name of the `number` field, e.g. "Branch".
    #[serde(default = "default_number_label")]
    pub number_label: String,
    /// Prefixes of stations that send the `number` field. Empty means everyone.
    #[serde(default)]
    pub number_from: Vec<String>,
    /// An official list of codes for the `number` field, e.g. VK Shires codes like
    /// "BU4". With a list, the field takes letters as well as digits.
    #[serde(default)]
    pub number_list: Option<NumberList>,
    /// The listed codes and what they stand for, read from `number_list` when the
    /// contests load. Empty when there is no list or it couldn't be read.
    #[serde(default, skip_deserializing)]
    pub number_names: BTreeMap<String, String>,
    /// Why `number_list` couldn't be read.
    #[serde(default, skip_deserializing)]
    pub number_list_error: Option<String>,
    /// New logs default to a separate serial number series on each band.
    /// When true the setup dialog has no checkbox: every log uses one series per band.
    #[serde(default)]
    pub serial_per_band: bool,
    /// When the chosen categories match, the setup dialog ticks separate serials per band.
    /// Off unless a contest's rules restart the serial for some multi-operator entries.
    #[serde(default)]
    pub serial_per_band_when: SerialPerBandWhen,
    /// Prefixes (e.g. "VK", "ZL") of stations that may be worked. Empty means anyone.
    #[serde(default)]
    pub workable: Vec<String>,
    pub rework: Rework,
    #[serde(default)]
    pub blocks: Option<Blocks>,
    /// Operating periods within the contest, for contests with breaks. Empty means
    /// the whole contest is one period.
    #[serde(default)]
    pub sessions: Vec<Session>,
    pub scoring: Scoring,
    #[serde(default)]
    pub multiplier: Option<Multiplier>,
    /// Bands scored as separate entries, e.g. HF and VHF+ in the John Moyle Field Day.
    #[serde(default)]
    pub score_groups: Vec<ScoreGroup>,
    #[serde(default)]
    pub categories: Vec<CategoryField>,
    /// Stations that move between `number` areas during the contest, e.g. VK Shires rovers.
    #[serde(default)]
    pub rover: Option<Rover>,
}

fn default_number_label() -> String {
    "Nr".into()
}

/// A file of valid `number` codes in the user's contests folder, so it can be
/// replaced whenever the organisers publish a new one. Read in the layout of the
/// WIA's `Shire.db3` for VKCL: an SQLite table `shireTbl` with
/// `shire_shireCode`, `shire_shireName` and `shire_stateName`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NumberList {
    /// File name within the contests folder, e.g. "Shire.db3".
    pub file: String,
    /// What one code is called, for messages, e.g. "shire".
    #[serde(default = "default_list_item")]
    pub item: String,
    /// CQ zones 1 to 40 are valid too (sent by stations outside the listed area).
    #[serde(default)]
    pub cq_zones: bool,
    /// Where to download the current list.
    #[serde(default)]
    pub url: String,
}

/// Rovers send the `number` of wherever they are now, so it can change from one
/// contact to the next. A rover in a new area is a new contact for everyone, and
/// counts its multipliers again there.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rover {
    /// The setup category that makes the log a rover's, e.g. CATEGORY-STATION = ROVER.
    pub category: String,
    pub value: String,
    /// Minutes a rover must wait after moving before its next contact.
    #[serde(default)]
    pub move_minutes: i64,
}

fn default_list_item() -> String {
    "code".into()
}

/// A received code as compared and logged: trimmed and in capitals.
pub fn norm_code(s: &str) -> String {
    s.trim().to_ascii_uppercase()
}

/// Which category choices should tick "separate serial numbers on each band".
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SerialPerBandWhen {
    #[default]
    Never,
    /// Any `CATEGORY-OPERATOR` of `MULTI-OP`, including multi-single.
    MultiOp,
    /// `MULTI-OP` with `CATEGORY-TRANSMITTER` of `TWO` or `UNLIMITED`.
    /// Single-op and multi-one keep one series across all bands.
    MultiTx,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExchField {
    Rst,
    Serial,
    /// A fixed number chosen at setup, e.g. years licensed in the Remembrance Day
    /// contest or an NZART branch number. Stored apart from the serial, so a
    /// contest can use both.
    Number,
    Locator,
    /// Free text, such as an air temperature or a shire code. Not checked.
    Text,
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
    /// Every contact counts. Nothing is a dupe.
    Off,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    /// Minutes after the contest start.
    pub start_minute: i64,
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
    /// Rounding of each contact's basic distance score, and of its points after
    /// mode factors and bonuses.
    #[serde(default)]
    pub round: Round,
    /// Factors applied to every contact when a setup category has a given value,
    /// e.g. x1.5 for CATEGORY-POWER = QRP, or to the bands ticked in the setup for
    /// per-band bonuses.
    #[serde(default)]
    pub bonus: Vec<Bonus>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Round {
    #[default]
    None,
    Nearest,
    Up,
}

impl Round {
    pub fn apply(self, x: f64) -> f64 {
        // Products like 37.5 x 1.5 x 1.2 land a hair below .5; snap before rounding.
        let x = if self == Round::None { x } else { (x * 1e9).round() / 1e9 };
        match self {
            Round::None => x,
            Round::Nearest => x.round(),
            Round::Up => x.ceil(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bonus {
    pub category: String,
    pub value: String,
    pub factor: f64,
    /// Claimed band by band (`LogSetup::bonus_bands`) rather than through the category.
    #[serde(default)]
    pub per_band: bool,
    /// Shown next to the band checkboxes for per-band bonuses.
    #[serde(default)]
    pub label: String,
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
        /// Per-mode points, used instead of `base`, e.g. PH 3 and CW 5.
        #[serde(default)]
        mode_points: BTreeMap<String, f64>,
        /// Contacts between a home-country station and an overseas one score differently.
        #[serde(default)]
        foreign: Option<Foreign>,
        /// Per-mode multiplying factor.
        #[serde(default)]
        mode: BTreeMap<String, f64>,
        #[serde(default)]
        local_time: Option<LocalTimeBonus>,
    },
    Distance {
        /// Points per kilometre on each band.
        #[serde(default)]
        band_factor: BTreeMap<String, f64>,
        /// Stepped scores: rows of [up to km, points], checked in order. Takes
        /// precedence over `band_factor`.
        #[serde(default)]
        band_table: BTreeMap<String, Vec<(f64, f64)>>,
        /// Per-mode multiplying factor, e.g. machine modes.
        #[serde(default)]
        mode: BTreeMap<String, f64>,
        /// Beyond this distance, kilometres count at 1/`taper_divisor` on `taper_bands`.
        #[serde(default)]
        taper_km: Option<f64>,
        #[serde(default = "hundred")]
        taper_divisor: f64,
        #[serde(default)]
        taper_bands: Vec<String>,
        /// Measure between the centres of locators truncated to this many characters.
        #[serde(default)]
        max_locator_chars: Option<usize>,
    },
    /// Contacts are counted and not scored.
    None,
}

fn one() -> f64 {
    1.0
}

fn hundred() -> f64 {
    100.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Foreign {
    /// Prefixes of the home country, e.g. ["ZL", "ZM"].
    pub home: Vec<String>,
    pub points: f64,
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
    /// WPX-style callsign prefix.
    Prefix,
    /// The received `number` field, e.g. NZART branch.
    Number,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Multiplier {
    pub kind: MultKind,
    #[serde(default)]
    pub per: Vec<Dim>,
    /// Prefixes of stations that count as multipliers. Empty means all.
    #[serde(default)]
    pub only: Vec<String>,
    /// Values that never count, e.g. branch "00" for home stations.
    #[serde(default)]
    pub exclude: Vec<String>,
    /// Your own `number` (e.g. your own branch) doesn't count.
    #[serde(default)]
    pub not_own: bool,
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
    /// Shown in the setup dialog instead of the Cabrillo key.
    #[serde(default)]
    pub label: String,
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

    /// A generic contest takes its name, length, bands and exchange from the log.
    pub fn prepare(mut self, setup: &crate::model::LogSetup) -> Self {
        if !self.generic {
            return self;
        }
        let chosen = &setup.generic;
        let name = chosen.name.trim();
        if !name.is_empty() {
            self.name = name.to_string();
            self.cabrillo_name = name.to_string();
        }
        if chosen.duration_minutes > 0 {
            self.duration_minutes = chosen.duration_minutes;
        }
        if !chosen.bands.is_empty() {
            self.bands = chosen.bands.clone();
        }
        self.exchange.clear();
        if chosen.include_rst {
            self.exchange.push(ExchField::Rst);
        }
        match chosen.cipher.as_str() {
            "serial" => self.exchange.push(ExchField::Serial),
            "locator" => self.exchange.push(ExchField::Locator),
            "text" => {
                self.exchange.push(ExchField::Text);
                let label = chosen.cipher_label.trim();
                self.number_label = if label.is_empty() { "Cipher".into() } else { label.to_string() };
            }
            _ => {}
        }
        self.rework.kind = ReworkKind::Off;
        self.scoring.points = Points::None;
        self
    }

    /// What a `number` code stands for: a listed code's name, or "CQ zone N".
    pub fn number_meaning(&self, code: &str) -> Option<String> {
        let list = self.number_list.as_ref()?;
        let code = norm_code(code);
        if let Some(name) = self.number_names.get(&code) {
            return Some(name.clone());
        }
        let zone = code.parse::<u32>().ok().filter(|z| (1..=40).contains(z));
        zone.filter(|_| list.cq_zones).map(|z| format!("CQ zone {z}"))
    }

    /// Whether a `number` code is on the list (or a CQ zone, if those count).
    /// `None` when there is no list, or it couldn't be read, so nothing can be said.
    pub fn number_valid(&self, code: &str) -> Option<bool> {
        if self.number_list.is_none() || self.number_names.is_empty() {
            return None;
        }
        Some(self.number_meaning(code).is_some())
    }

    /// Whether this log is a rover's.
    pub fn is_rover(&self, setup: &crate::model::LogSetup) -> bool {
        self.rover
            .as_ref()
            .is_some_and(|r| setup.categories.get(&r.category) == Some(&r.value))
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

    /// Whether `t` falls within the contest's operating periods.
    pub fn in_session(&self, start: DateTime<Utc>, t: DateTime<Utc>) -> bool {
        if self.sessions.is_empty() {
            return t >= start && t < self.end(start);
        }
        self.sessions.iter().any(|s| {
            let from = start + Duration::minutes(s.start_minute);
            t >= from && t < from + Duration::minutes(s.minutes)
        })
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
        if let Some(m) = &self.multiplier
            && m.kind == MultKind::Number
            && !self.has_field(ExchField::Number)
        {
            return Err(format!("{}: number multipliers need a number in the exchange", self.id));
        }
        if self.number_list.is_some() && !self.has_field(ExchField::Number) {
            return Err(format!("{}: number_list needs a number in the exchange", self.id));
        }
        if let Some(r) = &self.rover {
            if !self.has_field(ExchField::Number) {
                return Err(format!("{}: rover needs a number in the exchange", self.id));
            }
            let listed = self.categories.iter().any(|c| c.key == r.category && c.options.contains(&r.value));
            if !listed {
                return Err(format!("{}: rover needs {} = {} among the categories", self.id, r.category, r.value));
            }
        }
        for s in &self.sessions {
            if s.start_minute < 0 || s.minutes <= 0 || s.start_minute + s.minutes > self.duration_minutes {
                return Err(format!("{}: sessions must lie within the contest", self.id));
            }
        }
        if let Points::Distance { band_table, .. } = &self.scoring.points {
            for (band, rows) in band_table {
                if rows.is_empty() || rows.windows(2).any(|w| w[0].0 >= w[1].0) {
                    return Err(format!("{}: band_table for {band} must list increasing distances", self.id));
                }
            }
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
