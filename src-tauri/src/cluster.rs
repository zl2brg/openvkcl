//! DX cluster: a telnet connection to a DXSpider (or similar) node, logging in
//! with the station's callsign and passing each spot on with its DXCC country.

use crate::commands::AppState;
use crate::dxcc::Country;
use crate::{bands, callsign};
use chrono::{DateTime, Duration as Days, NaiveDate, NaiveTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::async_runtime::JoinHandle;
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;

pub const SPOT_EVENT: &str = "cluster-spot";
pub const STATUS_EVENT: &str = "cluster-status";

/// Spots kept for the UI to catch up with when it starts: those from the last
/// two hours (the longest time frame it offers), up to a limit for busy clusters.
const KEEP_FOR: Days = Days::hours(2);
const KEEP_MAX: usize = 5000;
/// Spots asked for after logging in, so the list isn't empty until the next one.
const BACKLOG: u32 = 200;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// Log in anyway if the node hasn't asked for a callsign by then.
const LOGIN_WAIT: Duration = Duration::from_secs(10);
const RETRY: Duration = Duration::from_secs(20);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ClusterConfig {
    pub enabled: bool,
    pub host: String,
    pub port: u16,
    /// Login callsign; empty uses the station callsign.
    pub callsign: String,
}

impl Default for ClusterConfig {
    fn default() -> Self {
        ClusterConfig {
            enabled: true,
            host: "hrd.wa9pie.net".into(),
            port: 8000,
            callsign: String::new(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct ClusterStatus {
    pub connected: bool,
    /// "host:port" of the node, while enabled.
    pub node: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Spot {
    pub time: DateTime<Utc>,
    pub freq_khz: f64,
    pub call: String,
    pub spotter: String,
    pub comment: String,
    pub band: Option<String>,
    #[serde(flatten)]
    pub country: Option<Country>,
}

fn is_time(t: &str) -> bool {
    t.len() == 5 && t.ends_with('Z') && t[..4].chars().all(|c| c.is_ascii_digit())
}

/// Today's `HHMMZ`, or yesterday's if that would be in the future.
fn time_today(hhmm: &str, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
    let t = NaiveTime::parse_from_str(&hhmm[..4], "%H%M").ok()?;
    let at = now.date_naive().and_time(t).and_utc();
    Some(if at > now + Days::minutes(5) { at - Days::days(1) } else { at })
}

fn spot(time: DateTime<Utc>, freq: &str, call: &str, spotter: &str, comment: String) -> Option<Spot> {
    let freq_khz: f64 = freq.parse().ok()?;
    let call = callsign::normalize(call);
    if !callsign::is_plausible(&call) {
        return None;
    }
    Some(Spot {
        time,
        freq_khz,
        band: bands::from_khz(freq_khz).map(|b| b.name.to_string()),
        call,
        spotter: spotter.trim().to_ascii_uppercase(),
        comment,
        country: None,
    })
}

/// The line without control characters: DXSpider ends live spots with a bell (BEL).
fn printable(line: &str) -> String {
    line.chars().map(|c| if c.is_control() { ' ' } else { c }).collect()
}

/// A live spot: `DX de W3LPL:     14025.0  JA1ABC       CW 599 TU      1234Z`,
/// sometimes with the spotter's locator after the time.
pub fn parse_live(line: &str, now: DateTime<Utc>) -> Option<Spot> {
    let line = printable(line);
    let rest = line.trim().strip_prefix("DX de ")?;
    let (spotter, rest) = rest.split_once(':')?;
    let mut words = rest.split_whitespace();
    let freq = words.next()?;
    let call = words.next()?;
    let tail: Vec<&str> = words.collect();
    let at = tail.iter().rposition(|t| is_time(t));
    let time = at.and_then(|i| time_today(tail[i], now)).unwrap_or(now);
    let comment = tail[..at.unwrap_or(tail.len())].join(" ");
    spot(time, freq, call, spotter, comment)
}

/// A spot from `sh/dx`: `14074.0  JA1ABC  23-Nov-2026 0123Z  FT8 -10dB   <W3LPL>`.
pub fn parse_show_dx(line: &str) -> Option<Spot> {
    let line = printable(line);
    let words: Vec<&str> = line.split_whitespace().collect();
    if words.len() < 5 {
        return None;
    }
    let date = NaiveDate::parse_from_str(words[2], "%d-%b-%Y").ok()?;
    if !is_time(words[3]) {
        return None;
    }
    let time = date.and_time(NaiveTime::parse_from_str(&words[3][..4], "%H%M").ok()?).and_utc();
    let last = words[words.len() - 1];
    let spotter = last.strip_prefix('<')?.strip_suffix('>')?;
    spot(time, words[0], words[1], spotter, words[4..words.len() - 1].join(" "))
}

/// Remove telnet commands (IAC sequences) from received bytes.
fn strip_telnet(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == 0xFF {
            // IAC IAC is a literal 0xFF; IAC SB ... IAC SE is a subnegotiation;
            // anything else is IAC plus a command and an option.
            match bytes.get(i + 1) {
                Some(0xFF) => {
                    out.push(0xFF);
                    i += 2;
                }
                Some(0xFA) => {
                    i += 2;
                    while i + 1 < bytes.len() && !(bytes[i] == 0xFF && bytes[i + 1] == 0xF0) {
                        i += 1;
                    }
                    i += 2;
                }
                _ => i += 3,
            }
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    out
}

/// Why the node turned the login away, if this line says it did.
fn login_refused(line: &str, login: &str) -> Option<String> {
    let l = line.to_ascii_lowercase();
    if l.contains("already connected") {
        // DXSpider allows one session per callsign; other programs use an SSID.
        let base = login.split('-').next().unwrap_or(login);
        return Some(format!(
            "{login} is already logged in to this cluster from another program. \
             Log in as {base}-1 (or another number) under Lookups…"
        ));
    }
    l.contains("not a valid callsign").then(|| format!("The cluster doesn't accept {login} as a callsign"))
}

fn asks_for_login(text: &str) -> bool {
    let t = text.to_ascii_lowercase();
    t.contains("login:") || t.contains("your call") || t.contains("callsign:")
}

#[derive(Default)]
pub struct Cluster {
    task: Mutex<Option<JoinHandle<()>>>,
    status: Arc<Mutex<ClusterStatus>>,
    spots: Arc<Mutex<VecDeque<Spot>>>,
}

struct Shared {
    app: AppHandle,
    status: Arc<Mutex<ClusterStatus>>,
    spots: Arc<Mutex<VecDeque<Spot>>>,
}

impl Shared {
    fn publish(&self, status: ClusterStatus) {
        {
            let mut cur = self.status.lock().unwrap();
            if *cur == status {
                return;
            }
            *cur = status.clone();
        }
        // Same as the radio: never emit while this lock is held. The UI thread
        // reads the status, and the emit can need that thread before it returns.
        let _ = self.app.emit(STATUS_EVENT, status);
    }

    fn add(&self, mut spot: Spot) {
        spot.country = self.app.state::<AppState>().cty.lock().unwrap().lookup(&spot.call);
        {
            let mut spots = self.spots.lock().unwrap();
            if spots.iter().any(|s| s.call == spot.call && s.freq_khz == spot.freq_khz && s.time == spot.time) {
                return;
            }
            spots.push_back(spot.clone());
            let cutoff = Utc::now() - KEEP_FOR;
            while spots.len() > KEEP_MAX || spots.front().is_some_and(|s| s.time < cutoff) {
                spots.pop_front();
            }
        }
        let _ = self.app.emit(SPOT_EVENT, spot);
    }
}

async fn session(shared: &Shared, cfg: &ClusterConfig, login: &str, node: &str) -> Result<(), String> {
    let mut stream = timeout(CONNECT_TIMEOUT, TcpStream::connect((cfg.host.as_str(), cfg.port)))
        .await
        .map_err(|_| format!("{node} did not answer"))?
        .map_err(|e| format!("{node}: {e}"))?;
    let mut pending = String::new();
    let mut logged_in = false;
    let started = tokio::time::Instant::now();
    let mut buf = [0u8; 4096];
    loop {
        let wait = if logged_in { Duration::from_secs(3600) } else { LOGIN_WAIT };
        let n = match timeout(wait, stream.read(&mut buf)).await {
            Ok(Ok(0)) => return Err(format!("{node} closed the connection")),
            Ok(Ok(n)) => n,
            Ok(Err(e)) => return Err(format!("{node}: {e}")),
            // Quiet for an hour: make sure the connection is still there.
            Err(_) if logged_in => {
                stream.write_all(b"\r\n").await.map_err(|e| format!("{node}: {e}"))?;
                continue;
            }
            Err(_) => 0,
        };
        let bytes = strip_telnet(&buf[..n]);
        pending.push_str(&String::from_utf8_lossy(&bytes));
        // The login prompt has no line ending, so look at what's waiting too.
        if !logged_in && (asks_for_login(&pending) || started.elapsed() >= LOGIN_WAIT) {
            let hello = format!("{login}\r\nsh/dx {BACKLOG}\r\n");
            stream.write_all(hello.as_bytes()).await.map_err(|e| format!("{node}: {e}"))?;
            logged_in = true;
            pending.clear();
            shared.publish(ClusterStatus {
                connected: true,
                node: node.to_string(),
                error: None,
            });
            continue;
        }
        while let Some(end) = pending.find('\n') {
            let line: String = pending.drain(..=end).collect();
            let line = line.trim_end();
            if let Some(s) = parse_live(line, Utc::now()).or_else(|| parse_show_dx(line)) {
                shared.add(s);
            } else if let Some(e) = login_refused(line, login) {
                return Err(e);
            }
        }
    }
}

impl Cluster {
    /// (Re)connect with `cfg`, logging in as `login`; disconnect if disabled.
    pub fn start(&self, app: AppHandle, cfg: ClusterConfig, login: String) {
        if let Some(t) = self.task.lock().unwrap().take() {
            t.abort();
        }
        let shared = Shared {
            app,
            status: self.status.clone(),
            spots: self.spots.clone(),
        };
        if !cfg.enabled {
            shared.publish(ClusterStatus::default());
            return;
        }
        let node = format!("{}:{}", cfg.host, cfg.port);
        if login.trim().is_empty() {
            shared.publish(ClusterStatus {
                node,
                error: Some("Enter your callsign to log in to the DX cluster".into()),
                ..Default::default()
            });
            return;
        }
        let task = tauri::async_runtime::spawn(async move {
            let login = callsign::normalize(&login);
            loop {
                let error = match session(&shared, &cfg, &login, &node).await {
                    Ok(()) => continue,
                    Err(e) => e,
                };
                shared.publish(ClusterStatus {
                    connected: false,
                    node: node.clone(),
                    error: Some(error),
                });
                tokio::time::sleep(RETRY).await;
            }
        });
        *self.task.lock().unwrap() = Some(task);
    }

    pub fn status(&self) -> ClusterStatus {
        self.status.lock().unwrap().clone()
    }

    pub fn spots(&self) -> Vec<Spot> {
        self.spots.lock().unwrap().iter().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn live_spots() {
        let now = Utc.with_ymd_and_hms(2026, 11, 23, 12, 40, 0).unwrap();
        let s = parse_live("DX de W3LPL:     14025.0  JA1ABC       CW 599 TU                      1234Z", now).unwrap();
        assert_eq!((s.call.as_str(), s.spotter.as_str(), s.freq_khz), ("JA1ABC", "W3LPL", 14025.0));
        assert_eq!(s.comment, "CW 599 TU");
        assert_eq!(s.band.as_deref(), Some("20m"));
        assert_eq!(s.time, Utc.with_ymd_and_hms(2026, 11, 23, 12, 34, 0).unwrap());
        // DXSpider rings a bell after live spots.
        let s = parse_live("DX de W5WX:      7207.5  K1KI         CT                             0140Z\x07\x07", now).unwrap();
        assert_eq!((s.comment.as_str(), s.time), ("CT", Utc.with_ymd_and_hms(2026, 11, 23, 1, 40, 0).unwrap()));
        // With the spotter's locator after the time, and a time just before midnight.
        let s = parse_live("DX de VK2YI-#:   7074.0  ZL1ABC       FT8 -10dB              2358Z RF73", now).unwrap();
        assert_eq!((s.spotter.as_str(), s.comment.as_str()), ("VK2YI-#", "FT8 -10dB"));
        assert_eq!(s.time, Utc.with_ymd_and_hms(2026, 11, 22, 23, 58, 0).unwrap());
        assert_eq!(parse_live("DX de W3LPL:  14025.0  NOTACALL  hi 1234Z", now), None);
        assert_eq!(parse_live("Hello VK2YI, this is WA9PIE-2", now), None);
    }

    #[test]
    fn show_dx() {
        let s = parse_show_dx("  14074.0  JA1ABC      23-Nov-2026 0123Z  FT8 -10dB                     <W3LPL>").unwrap();
        assert_eq!((s.call.as_str(), s.spotter.as_str(), s.comment.as_str()), ("JA1ABC", "W3LPL", "FT8 -10dB"));
        assert_eq!(s.time, Utc.with_ymd_and_hms(2026, 11, 23, 1, 23, 0).unwrap());
        assert_eq!(parse_show_dx(" 28074.0  VK9XX  23-Nov-2026 0200Z <K1ABC>").unwrap().comment, "");
        assert_eq!(parse_show_dx("WA9PIE-2 de VK2YI 23-Nov-2026 0123Z dxspider >"), None);
    }

    #[test]
    fn telnet_and_login() {
        assert_eq!(strip_telnet(b"\xFF\xFB\x01login: \xFF\xFF"), b"login: \xFF");
        assert_eq!(strip_telnet(b"a\xFF\xFA\x18\x01\xFF\xF0b"), b"ab");
        assert!(asks_for_login("=== \r\nlogin: "));
        assert!(asks_for_login("Please enter your call: "));
        assert!(!asks_for_login("Welcome to the cluster"));
        let busy = "login: Another session on 1.2.3.4 for VK2YI from 5.6.7.8 is already connected. Existing session preserved.";
        assert!(login_refused(busy, "VK2YI").unwrap().contains("VK2YI-1"));
        assert!(login_refused(busy, "VK2YI-1").unwrap().contains("VK2YI-1 is already"));
        assert_eq!(login_refused("Hello VK2YI-9, this is WA9PIE-2", "VK2YI-9"), None);
    }
}
