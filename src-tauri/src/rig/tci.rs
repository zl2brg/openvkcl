//! TCI (Transceiver Control Interface) over WebSocket, as served by ExpertSDR and
//! cudaSDR. The server pushes `vfo:` and `modulation:` messages whenever the radio
//! changes, so nothing is polled.

use futures_util::{SinkExt, StreamExt};
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::time::{Instant, timeout, timeout_at};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

const TIMEOUT: Duration = Duration::from_secs(3);
/// How long to wait after connecting for the server to report the frequency and mode.
const FIRST_STATE: Duration = Duration::from_secs(5);

pub struct Client {
    ws: WebSocketStream<MaybeTlsStream<TcpStream>>,
    radio: Radio,
}

/// What the server has told us about the first transceiver.
#[derive(Default)]
struct Radio {
    /// VFO A and B.
    vfo: [Option<u64>; 2],
    /// cudaSDR's `active_vfo` extension: which VFO the dial is on.
    active: usize,
    mode: Option<String>,
}

impl Radio {
    fn state(&self) -> Option<(u64, String)> {
        Some((self.vfo[self.active]?, self.mode.clone()?))
    }

    /// Apply a text frame; true if the frequency or mode changed.
    fn apply(&mut self, text: &str) -> bool {
        let before = self.state();
        for (name, args) in commands(text) {
            match (name.as_str(), args.as_slice()) {
                ("vfo", [trx, channel, hz]) if *trx == "0" => {
                    if let (Ok(ch), Ok(hz)) = (channel.parse::<usize>(), hz.parse::<f64>())
                        && ch < 2
                    {
                        self.vfo[ch] = Some(hz as u64);
                    }
                }
                ("active_vfo", [trx, channel]) if *trx == "0" => {
                    self.active = usize::from(*channel == "1");
                }
                ("modulation", [trx, mode]) if *trx == "0" => {
                    self.mode = Some(mode.to_ascii_uppercase());
                }
                _ => {}
            }
        }
        self.state() != before
    }
}

/// Split a text frame into `(name, args)` commands, e.g. "vfo:0,0,14074000;".
fn commands(text: &str) -> impl Iterator<Item = (String, Vec<&str>)> {
    text.split(';').filter_map(|cmd| {
        let cmd = cmd.trim();
        if cmd.is_empty() {
            return None;
        }
        let (name, args) = cmd.split_once(':').unwrap_or((cmd, ""));
        let args = if args.is_empty() { Vec::new() } else { args.split(',').map(str::trim).collect() };
        Some((name.trim().to_ascii_lowercase(), args))
    })
}

impl Client {
    pub async fn connect(host: &str, port: u16) -> Result<Self, String> {
        let url = format!("ws://{host}:{port}");
        let (ws, _) = timeout(TIMEOUT, connect_async(url.as_str()))
            .await
            .map_err(|_| format!("TCI server at {host}:{port} did not answer"))?
            .map_err(|e| format!("TCI server at {host}:{port}: {e}"))?;
        let mut client = Client {
            ws,
            radio: Radio::default(),
        };
        client.send("app:OpenVKCL;").await?;
        Ok(client)
    }

    pub async fn send(&mut self, text: &str) -> Result<(), String> {
        timeout(TIMEOUT, self.ws.send(Message::Text(text.into())))
            .await
            .map_err(|_| "TCI server timed out".to_string())?
            .map_err(|e| e.to_string())
    }

    /// Wait up to `wait` for the radio to change, then return its frequency and mode.
    pub async fn poll(&mut self, wait: Duration) -> Result<(u64, String), String> {
        let started = Instant::now();
        let mut deadline = started + wait;
        loop {
            match timeout_at(deadline, self.ws.next()).await {
                Err(_) => {
                    if let Some(state) = self.radio.state() {
                        return Ok(state);
                    }
                    if started.elapsed() >= FIRST_STATE {
                        return Err("TCI server did not report a frequency and mode".into());
                    }
                    deadline = Instant::now() + wait;
                }
                Ok(None) | Ok(Some(Ok(Message::Close(_)))) => {
                    return Err("TCI server closed the connection".into());
                }
                Ok(Some(Err(e))) => return Err(e.to_string()),
                Ok(Some(Ok(Message::Text(text)))) => {
                    if self.radio.apply(text.as_str())
                        && let Some(state) = self.radio.state()
                    {
                        return Ok(state);
                    }
                }
                // IQ and audio streams, pings.
                Ok(Some(Ok(_))) => {}
            }
        }
    }
}

/// TCI modulation name for a contest mode, for spots.
pub fn spot_mode(contest_mode: &str, freq_hz: u64) -> &'static str {
    match contest_mode {
        "CW" => "cw",
        "RY" | "DG" => "digu",
        _ if freq_hz < 10_000_000 => "lsb",
        _ => "usb",
    }
}

/// A `spot:` command marking a station on the SDR's band display.
pub fn spot(call: &str, mode: &str, freq_hz: u64, text: &str) -> String {
    // Opaque light grey; commas and semicolons would break the argument list.
    const ARGB: u32 = 0xFFC0_C0C0;
    let clean = |s: &str| s.replace([',', ';', ':'], " ");
    format!("spot:{},{},{},{},{};", clean(call), mode, freq_hz, ARGB, clean(text))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_commands() {
        let cmds: Vec<_> = commands("vfo:0,0,14074000;MODULATION:0,usb;ready;").collect();
        assert_eq!(cmds[0], ("vfo".to_string(), vec!["0", "0", "14074000"]));
        assert_eq!(cmds[1], ("modulation".to_string(), vec!["0", "usb"]));
        assert_eq!(cmds[2], ("ready".to_string(), vec![]));
    }

    #[test]
    fn follows_active_vfo_and_mode() {
        let mut r = Radio::default();
        assert!(!r.apply("vfo:0,0,3650000;"));
        assert!(r.apply("vfo:0,1,7090000;modulation:0,lsb;"));
        assert_eq!(r.state(), Some((3_650_000, "LSB".into())));
        assert!(!r.apply("vfo:1,0,14200000;"));
        assert!(r.apply("active_vfo:0,1;"));
        assert_eq!(r.state(), Some((7_090_000, "LSB".into())));
        assert!(r.apply("modulation:0,cw;"));
    }

    #[test]
    fn spots() {
        assert_eq!(
            spot("ZL2ABC", spot_mode("PH", 3_650_000), 3_650_000, "worked 0812, 80m"),
            "spot:ZL2ABC,lsb,3650000,4290822336,worked 0812  80m;"
        );
        assert_eq!(spot_mode("CW", 7_020_000), "cw");
        assert_eq!(spot_mode("PH", 144_200_000), "usb");
    }
}
