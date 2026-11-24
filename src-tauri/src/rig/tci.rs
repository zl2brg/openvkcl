//! TCI (Transceiver Control Interface) over WebSocket, as served by ExpertSDR and
//! cudaSDR. The server pushes `vfo:` and `modulation:` messages whenever the radio
//! changes, so nothing is polled.

use futures_util::{SinkExt, StreamExt};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::time::{Instant, timeout, timeout_at};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

const TIMEOUT: Duration = Duration::from_secs(3);
/// Transmit audio is sent at this rate, float32, with left and right the same.
pub const TX_RATE: u32 = 48_000;
/// How long to wait after keying for the server to ask for audio.
const FIRST_CHRONO: Duration = Duration::from_secs(3);

/// Binary stream types in the 64-byte TCI frame header.
const TX_AUDIO: u32 = 2;
const TX_CHRONO: u32 = 3;
/// The float32 sample format.
const FLOAT32: u32 = 3;

/// A TX_AUDIO frame: the TCI stream header (16 little-endian u32: receiver,
/// sample rate, format, codec, crc, length in samples over all channels, type,
/// channels, then reserved) and float32 stereo samples.
fn tx_audio_frame(mono: &[f32]) -> Vec<u8> {
    let header = [0, TX_RATE, FLOAT32, 0, 0, (mono.len() * 2) as u32, TX_AUDIO, 2, 0, 0, 0, 0, 0, 0, 0, 0];
    let mut frame = Vec::with_capacity(64 + mono.len() * 8);
    for v in header {
        frame.extend_from_slice(&v.to_le_bytes());
    }
    for s in mono {
        // The same in both channels, which servers recognise as mono audio.
        frame.extend_from_slice(&s.to_le_bytes());
        frame.extend_from_slice(&s.to_le_bytes());
    }
    frame
}

/// A TX_CHRONO frame's request: how many stereo frames of audio to send next.
fn chrono_frames(frame: &[u8]) -> Option<usize> {
    let word = |i: usize| -> Option<u32> { Some(u32::from_le_bytes(frame.get(i * 4..i * 4 + 4)?.try_into().ok()?)) };
    if word(6)? != TX_CHRONO {
        return None;
    }
    let channels = word(7)?.max(1);
    Some((word(5)? / channels) as usize)
}
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

    /// Key the transmitter and send `audio` (mono, at TX_RATE) as its transmit
    /// audio, as the server asks for it, then unkey; or stop early when `stop` is
    /// set. `trx:0,true,tci;` tells the server to transmit our audio rather than
    /// the microphone, in voice modes too.
    pub async fn transmit(&mut self, audio: &[f32], stop: &AtomicBool) -> Result<(), String> {
        self.send("trx:0,true,tci;").await?;
        let result = self.stream_audio(audio, stop).await;
        // Always unkey, whatever happened.
        let unkeyed = self.send("trx:0,false;").await;
        result.and(unkeyed)
    }

    async fn stream_audio(&mut self, audio: &[f32], stop: &AtomicBool) -> Result<(), String> {
        let started = Instant::now();
        let mut sent = 0;
        let mut asked = false;
        while sent < audio.len() && !stop.load(Ordering::Relaxed) {
            // Wake now and then to notice Esc.
            match timeout(Duration::from_millis(100), self.ws.next()).await {
                Err(_) => {
                    if !asked && started.elapsed() >= FIRST_CHRONO {
                        return Err("The TCI server keyed up but didn't ask for audio".into());
                    }
                }
                Ok(None) | Ok(Some(Ok(Message::Close(_)))) => return Err("TCI server closed the connection".into()),
                Ok(Some(Err(e))) => return Err(e.to_string()),
                Ok(Some(Ok(Message::Binary(frame)))) => {
                    if let Some(n) = chrono_frames(&frame) {
                        asked = true;
                        let end = (sent + n).min(audio.len());
                        let mut chunk = audio[sent..end].to_vec();
                        // The last request gets silence after the message.
                        chunk.resize(n, 0.0);
                        timeout(TIMEOUT, self.ws.send(Message::Binary(tx_audio_frame(&chunk).into())))
                            .await
                            .map_err(|_| "TCI server timed out".to_string())?
                            .map_err(|e| e.to_string())?;
                        sent = end;
                    }
                }
                Ok(Some(Ok(Message::Text(text)))) => {
                    self.radio.apply(text.as_str());
                    // The server answers a refused key-up, or an early unkey, with trx false.
                    if commands(text.as_str()).any(|(name, args)| name == "trx" && args.get(1) == Some(&"false")) {
                        return Err(if asked {
                            "The TCI server stopped transmitting".into()
                        } else {
                            "The TCI server refused to transmit: in AetherSDR, check the DAX TX stream is Selected and no other program is transmitting".into()
                        });
                    }
                }
                Ok(Some(Ok(_))) => {}
            }
        }
        Ok(())
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

    /// A fake TCI server: answers `trx:0,true,tci;` with `answer`, then sends
    /// TX_CHRONO frames asking for 1024 stereo frames each and collects the audio.
    async fn fake_server(answer: &'static str, chronos: usize) -> (u16, tokio::task::JoinHandle<(Vec<String>, Vec<f32>)>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move {
            let (tcp, _) = listener.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(tcp).await.unwrap();
            let (mut texts, mut audio) = (Vec::new(), Vec::new());
            let mut chrono = vec![0u8; 64];
            for (i, v) in [0, 48_000, 3, 0, 0, 2048, TX_CHRONO, 2].iter().enumerate() {
                chrono[i * 4..i * 4 + 4].copy_from_slice(&u32::to_le_bytes(*v));
            }
            while let Ok(Some(Ok(msg))) = timeout(Duration::from_secs(2), ws.next()).await {
                match msg {
                    Message::Text(t) => {
                        texts.push(t.to_string());
                        if t.as_str() == "trx:0,true,tci;" {
                            if !answer.is_empty() {
                                ws.send(Message::Text(answer.into())).await.unwrap();
                            }
                            for _ in 0..chronos {
                                ws.send(Message::Binary(chrono.clone().into())).await.unwrap();
                            }
                        }
                        if t.as_str() == "trx:0,false;" {
                            break;
                        }
                    }
                    Message::Binary(b) => {
                        let words: Vec<u32> = b[..64].chunks(4).map(|c| u32::from_le_bytes(c.try_into().unwrap())).collect();
                        assert_eq!(&words[..8], &[0, 48_000, 3, 0, 0, 2048, TX_AUDIO, 2]);
                        // Keep the left channel.
                        audio.extend(b[64..].chunks(8).map(|c| f32::from_le_bytes(c[..4].try_into().unwrap())));
                    }
                    _ => {}
                }
            }
            (texts, audio)
        });
        (port, server)
    }

    #[tokio::test]
    async fn transmits_audio_as_asked() {
        let (port, server) = fake_server("trx:0,true;", 3).await;
        let mut c = Client::connect("127.0.0.1", port).await.unwrap();
        // 1.5 requests' worth of audio: two frames, the second padded with silence.
        let audio: Vec<f32> = (0..1536).map(|i| i as f32 / 1536.0).collect();
        c.transmit(&audio, &AtomicBool::new(false)).await.unwrap();
        drop(c);
        let (texts, got) = server.await.unwrap();
        assert_eq!(texts, ["app:OpenVKCL;", "trx:0,true,tci;", "trx:0,false;"]);
        assert_eq!(got.len(), 2048);
        assert_eq!(&got[..1536], &audio[..]);
        assert!(got[1536..].iter().all(|&s| s == 0.0));
    }

    #[tokio::test]
    async fn reports_a_refused_key_up() {
        let (port, server) = fake_server("trx:0,false;", 0).await;
        let mut c = Client::connect("127.0.0.1", port).await.unwrap();
        let e = c.transmit(&[0.1; 4800], &AtomicBool::new(false)).await.unwrap_err();
        assert!(e.contains("refused to transmit"), "{e}");
        drop(c);
        // It still unkeys.
        assert_eq!(server.await.unwrap().0.last().unwrap(), "trx:0,false;");
    }

    #[tokio::test]
    async fn esc_stops_and_unkeys() {
        let (port, server) = fake_server("", 1).await;
        let mut c = Client::connect("127.0.0.1", port).await.unwrap();
        let stop = AtomicBool::new(true);
        c.transmit(&[0.1; 48_000], &stop).await.unwrap();
        drop(c);
        let (texts, got) = server.await.unwrap();
        assert_eq!(texts.last().unwrap(), "trx:0,false;");
        assert!(got.is_empty());
    }
}
