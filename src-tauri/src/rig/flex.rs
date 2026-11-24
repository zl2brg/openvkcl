//! FlexRadio 6000, 8000 and Aurora series over the SmartSDR API: a line-based
//! text protocol on TCP port 4992. After `sub slice all` the radio pushes slice
//! status lines whenever a slice is tuned or changes mode, so nothing is polled.

use std::collections::BTreeMap;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::net::TcpStream;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::time::{Instant, timeout, timeout_at};

pub const DEFAULT_PORT: u16 = 4992;
const TIMEOUT: Duration = Duration::from_secs(3);
/// How long to wait after connecting for the radio to report its slices before
/// saying it has none open.
const FIRST_STATE: Duration = Duration::from_secs(5);
/// With keepalive enabled the radio drops clients that stop pinging, and a failed
/// ping tells us the radio has gone.
const PING_EVERY: Duration = Duration::from_secs(5);
/// How long to wait after `xmit 1` for the interlock to say it's transmitting.
const KEY_UP: Duration = Duration::from_millis(1500);

pub struct Client {
    lines: Lines<BufReader<OwnedReadHalf>>,
    writer: OwnedWriteHalf,
    seq: u32,
    /// Sequence number of `sub slice all`, to report it failing.
    sub_seq: u32,
    last_ping: Instant,
    /// When the connection opened; the radio reports its slices soon after.
    connected_at: Instant,
    radio: Radio,
    /// The GUI client (SmartSDR, AetherSDR, Maestro) we're bound to, if any.
    bound: Option<String>,
    /// The transmit audio setting to put back on unkeying, when keying up
    /// switched it to DAX.
    dax_restore: Option<bool>,
}

#[derive(Debug, Default, Clone)]
struct Slice {
    hz: Option<u64>,
    mode: Option<String>,
    tx: bool,
    active: bool,
    /// Handle of the client the slice belongs to, e.g. "0x1A2B3C4D".
    owner: String,
}

/// A GUI client connected to the radio (SmartSDR, AetherSDR, Maestro).
#[derive(Debug, Clone, PartialEq)]
struct GuiClient {
    client_id: String,
    program: String,
}

/// What the radio has told us: its slices by index, the GUI clients by handle,
/// and the transmit interlock.
#[derive(Default)]
struct Radio {
    slices: BTreeMap<u32, Slice>,
    clients: BTreeMap<String, GuiClient>,
    /// Interlock state, e.g. RECEIVE, TRANSMITTING, NOT_READY.
    interlock: String,
    interlock_reason: String,
    tx_allowed: Option<bool>,
    /// Whether transmit audio comes from DAX (the computer) rather than the mic.
    tx_dax: Option<bool>,
}

impl Radio {
    /// The transmit slice, else the active one, else the first.
    fn state(&self) -> Option<(u64, String)> {
        let slices = || self.slices.values();
        let s = slices()
            .find(|s| s.tx)
            .or_else(|| slices().find(|s| s.active))
            .or_else(|| slices().next())?;
        Some((s.hz?, s.mode.clone()?))
    }

    /// The GUI client to bind to so we may transmit: the one whose slice
    /// transmits, else the first.
    fn bind_target(&self) -> Option<&GuiClient> {
        let tx_owner = self.slices.values().find(|s| s.tx).map(|s| s.owner.as_str());
        tx_owner
            .and_then(|h| self.clients.get(h))
            .or_else(|| self.clients.values().next())
    }

    /// Why the radio isn't transmitting, from the interlock, or None if it is
    /// (or hasn't said).
    fn not_transmitting(&self) -> Option<String> {
        match self.interlock.as_str() {
            "TRANSMITTING" | "PTT_REQUESTED" | "" => None,
            "RECEIVE" | "READY" if self.tx_allowed != Some(false) => None,
            state => {
                let why = if !self.interlock_reason.is_empty() {
                    self.interlock_reason.replace('_', " ").to_lowercase()
                } else if self.tx_allowed == Some(false) {
                    "transmit isn't allowed".to_string()
                } else {
                    state.replace('_', " ").to_lowercase()
                };
                Some(why)
            }
        }
    }

    fn apply_client(&mut self, words: &mut std::str::SplitAsciiWhitespace) {
        let Some(handle) = words.next() else { return };
        match words.next() {
            Some("disconnected") => {
                self.clients.remove(handle);
            }
            Some("connected") => {
                let fields: BTreeMap<&str, &str> = words.filter_map(|w| w.split_once('=')).collect();
                // Only GUI clients have a client_id; programs like this one don't.
                if let Some(id) = fields.get("client_id").filter(|id| !id.is_empty()) {
                    self.clients.insert(
                        handle.to_string(),
                        GuiClient {
                            client_id: id.to_string(),
                            program: fields.get("program").unwrap_or(&"").to_string(),
                        },
                    );
                }
            }
            _ => {}
        }
    }

    fn apply_interlock(&mut self, words: &mut std::str::SplitAsciiWhitespace) {
        for (key, value) in words.filter_map(|w| w.split_once('=')) {
            match key {
                "state" => self.interlock = value.to_string(),
                "reason" => self.interlock_reason = value.to_string(),
                "tx_allowed" => self.tx_allowed = Some(value == "1"),
                _ => {}
            }
        }
    }

    /// Apply one line from the radio; true if the frequency or mode changed.
    fn apply(&mut self, line: &str) -> bool {
        let Some(body) = line.strip_prefix('S').and_then(|s| s.split_once('|')).map(|(_, b)| b) else {
            return false;
        };
        let mut words = body.split_ascii_whitespace();
        match words.next() {
            Some("slice") => {}
            Some("client") => {
                self.apply_client(&mut words);
                return false;
            }
            Some("interlock") => {
                self.apply_interlock(&mut words);
                return false;
            }
            Some("transmit") => {
                for (key, value) in words.filter_map(|w| w.split_once('=')) {
                    if key == "dax" {
                        self.tx_dax = Some(value == "1");
                    }
                }
                return false;
            }
            _ => return false,
        }
        let Some(index) = words.next().and_then(|i| i.parse::<u32>().ok()) else {
            return false;
        };
        let before = self.state();
        let mut slice = self.slices.get(&index).cloned().unwrap_or_default();
        let mut removed = false;
        for word in words {
            let (key, value) = word.split_once('=').unwrap_or((word, ""));
            match key {
                "RF_frequency" => {
                    if let Ok(mhz) = value.parse::<f64>() {
                        slice.hz = Some((mhz * 1e6).round() as u64);
                    }
                }
                "mode" => slice.mode = Some(value.to_ascii_uppercase()),
                "tx" => slice.tx = value == "1",
                "active" => slice.active = value == "1",
                "client_handle" => slice.owner = value.to_string(),
                "in_use" => removed = value == "0",
                "removed" => removed = true,
                _ => {}
            }
        }
        if removed {
            self.slices.remove(&index);
        } else {
            self.slices.insert(index, slice);
        }
        self.state() != before
    }
}

/// A command response, `R<seq>|<hex code>|<message>`: the sequence number and,
/// when the radio refused the command, its message.
fn response(line: &str) -> Option<(u32, Option<String>)> {
    let mut parts = line.strip_prefix('R')?.splitn(3, '|');
    let seq = parts.next()?.parse().ok()?;
    let code = u32::from_str_radix(parts.next()?.trim(), 16).ok()?;
    let message = parts.next().unwrap_or("").trim();
    let error = (code != 0).then(|| {
        if message.is_empty() { format!("error {code:08X}") } else { message.to_string() }
    });
    Some((seq, error))
}

impl Client {
    pub async fn connect(host: &str, port: u16) -> Result<Self, String> {
        let stream = timeout(TIMEOUT, TcpStream::connect((host, port)))
            .await
            .map_err(|_| format!("FlexRadio at {host}:{port} did not answer"))?
            .map_err(|e| format!("FlexRadio at {host}:{port}: {e}"))?;
        let _ = stream.set_nodelay(true);
        let (read, writer) = stream.into_split();
        let mut client = Client {
            lines: BufReader::new(read).lines(),
            writer,
            seq: 0,
            sub_seq: 0,
            last_ping: Instant::now(),
            connected_at: Instant::now(),
            radio: Radio::default(),
            bound: None,
            dax_restore: None,
        };
        client.send("client program OpenVKCL").await?;
        client.send("keepalive enable").await?;
        client.sub_seq = client.seq + 1;
        client.send("sub slice all").await?;
        // GUI clients, to bind to one, and the interlock, to see whether we transmit.
        client.send("sub client all").await?;
        client.send("sub tx all").await?;
        Ok(client)
    }

    /// Bind to the GUI client whose slice transmits: the radio only lets a
    /// program like this one key up on a GUI client's behalf. Done when keying
    /// up, by when the radio has said whose slice transmits.
    async fn bind(&mut self) -> Result<(), String> {
        let Some(target) = self.radio.bind_target().map(|c| c.client_id.clone()) else {
            return Ok(());
        };
        if self.bound.as_deref() != Some(target.as_str()) {
            self.send(&format!("client bind client_id={target}")).await?;
            self.bound = Some(target);
        }
        Ok(())
    }

    /// Key or unkey the transmitter, and check the radio really did key up.
    pub async fn set_ptt(&mut self, on: bool) -> Result<(), String> {
        if !on {
            let unkeyed = self.xmit(false).await;
            // Put the transmit audio back as it was: the operator's mic.
            if let Some(dax) = self.dax_restore.take() {
                self.request(&format!("transmit set dax={}", u8::from(dax))).await?;
            }
            return unkeyed;
        }
        if self.radio.slices.is_empty() {
            return Err("FlexRadio has no slice open: start AetherSDR or SmartSDR with a slice to transmit on".into());
        }
        self.bind().await?;
        // The voice keyer's audio comes from the computer, through DAX, not the mic.
        if self.radio.tx_dax != Some(true) {
            self.request("transmit set dax=1")
                .await
                .map_err(|e| format!("FlexRadio wouldn't take transmit audio from DAX: {e}"))?;
            self.dax_restore = Some(self.radio.tx_dax.unwrap_or(false));
        }
        let keyed = self.xmit(true).await;
        if keyed.is_err()
            && let Some(dax) = self.dax_restore.take()
        {
            let _ = self.request(&format!("transmit set dax={}", u8::from(dax))).await;
        }
        keyed
    }

    /// Send a command and wait for the radio's answer, taking in whatever else
    /// it says meanwhile.
    async fn request(&mut self, command: &str) -> Result<(), String> {
        self.send(command).await?;
        let seq = self.seq;
        let deadline = Instant::now() + TIMEOUT;
        loop {
            match timeout_at(deadline, self.lines.next_line()).await {
                Err(_) => return Err("no answer".into()),
                Ok(Ok(None)) => return Err("FlexRadio closed the connection".into()),
                Ok(Err(e)) => return Err(format!("FlexRadio: {e}")),
                Ok(Ok(Some(line))) => {
                    if let Some((s, error)) = response(&line)
                        && s == seq
                    {
                        return error.map_or(Ok(()), Err);
                    }
                    self.radio.apply(&line);
                }
            }
        }
    }

    /// Key or unkey, and when keying, check the interlock says it's transmitting.
    async fn xmit(&mut self, on: bool) -> Result<(), String> {
        self.send(if on { "xmit 1" } else { "xmit 0" }).await?;
        let seq = self.seq;
        let deadline = Instant::now() + TIMEOUT;
        let mut answered = false;
        let mut key_up = deadline;
        loop {
            match timeout_at(if answered { key_up } else { deadline }, self.lines.next_line()).await {
                Err(_) if answered => {
                    // No interlock news: assume it keyed rather than leave it unkeyed.
                    return Ok(());
                }
                Err(_) => return Err("FlexRadio didn't answer the transmit command".into()),
                Ok(Ok(None)) => return Err("FlexRadio closed the connection".into()),
                Ok(Err(e)) => return Err(format!("FlexRadio: {e}")),
                Ok(Ok(Some(line))) => {
                    if let Some((s, error)) = response(&line)
                        && s == seq
                    {
                        if let Some(e) = error {
                            return Err(format!("FlexRadio refused to transmit: {e}"));
                        }
                        if !on {
                            return Ok(());
                        }
                        answered = true;
                        key_up = Instant::now() + KEY_UP;
                        continue;
                    }
                    self.radio.apply(&line);
                    if answered && line.contains("interlock") {
                        if matches!(self.radio.interlock.as_str(), "TRANSMITTING" | "PTT_REQUESTED") {
                            return Ok(());
                        }
                        if let Some(why) = self.radio.not_transmitting() {
                            return Err(format!("FlexRadio won't transmit: {why}"));
                        }
                    }
                }
            }
        }
    }

    /// Send one SmartSDR API command, e.g. `spot add ...`.
    pub async fn send(&mut self, command: &str) -> Result<(), String> {
        self.seq += 1;
        let line = format!("C{}|{}\n", self.seq, command);
        timeout(TIMEOUT, self.writer.write_all(line.as_bytes()))
            .await
            .map_err(|_| "FlexRadio timed out".to_string())?
            .map_err(|e| format!("FlexRadio: {e}"))
    }

    /// Wait up to `wait` for the radio to change, then return its frequency and
    /// mode, or None while it has no slice open.
    pub async fn poll(&mut self, wait: Duration) -> Result<Option<(u64, String)>, String> {
        let mut deadline = Instant::now() + wait;
        loop {
            if self.last_ping.elapsed() >= PING_EVERY {
                self.last_ping = Instant::now();
                self.send("ping").await?;
            }
            match timeout_at(deadline, self.lines.next_line()).await {
                Err(_) => {
                    let state = self.radio.state();
                    if state.is_some() || self.connected_at.elapsed() >= FIRST_STATE {
                        return Ok(state);
                    }
                    // Still waiting for the first slices: don't spin when `wait` is zero.
                    deadline = Instant::now() + wait.max(Duration::from_millis(100));
                }
                Ok(Ok(None)) => return Err("FlexRadio closed the connection".into()),
                Ok(Err(e)) => return Err(format!("FlexRadio: {e}")),
                Ok(Ok(Some(line))) => {
                    if let Some((seq, Some(error))) = response(&line)
                        && seq == self.sub_seq
                    {
                        return Err(format!("FlexRadio refused slice updates: {error}"));
                    }
                    if self.radio.apply(&line) {
                        let state = self.radio.state();
                        if state.is_some() || self.connected_at.elapsed() >= FIRST_STATE {
                            return Ok(state);
                        }
                    }
                }
            }
        }
    }
}

/// A `spot add` command marking a station on the SmartSDR panadapter.
pub fn spot(call: &str, mode: &str, freq_hz: u64, text: &str) -> String {
    // The API separates fields with spaces; FlexLib sends spaces inside a value as 0x7F.
    let clean = |s: &str| s.replace('=', " ").trim().replace(' ', "\u{7f}");
    format!(
        "spot add rx_freq={:.6} callsign={} mode={} color=#FFC0C0C0 source=OpenVKCL comment={} lifetime_seconds=3600",
        freq_hz as f64 / 1e6,
        clean(call),
        clean(mode),
        clean(text),
    )
}

/// A radio found on the local network.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Found {
    pub ip: String,
    pub port: u16,
    pub model: String,
    pub nickname: String,
    pub callsign: String,
    pub serial: String,
}

/// Parse a discovery broadcast: a VITA-49 packet whose payload is
/// space-separated `key=value` text.
fn parse_discovery(packet: &[u8]) -> Option<Found> {
    let text = String::from_utf8_lossy(packet);
    let start = text.find("discovery_protocol_version=").or_else(|| text.find("model="))?;
    let fields: BTreeMap<&str, &str> = text[start..]
        .trim_end_matches('\0')
        .split_ascii_whitespace()
        .filter_map(|w| w.split_once('='))
        .collect();
    let get = |k: &str| fields.get(k).map(|v| v.trim_end_matches('\0').to_string()).unwrap_or_default();
    let ip = get("ip");
    if ip.is_empty() {
        return None;
    }
    Some(Found {
        port: fields.get("port").and_then(|p| p.parse().ok()).unwrap_or(DEFAULT_PORT),
        ip,
        model: get("model"),
        nickname: get("nickname"),
        callsign: get("callsign"),
        serial: get("serial"),
    })
}

/// Listen for the radios' discovery broadcasts (sent about once a second).
pub async fn discover(listen: Duration) -> Result<Vec<Found>, String> {
    use socket2::{Domain, Protocol, Socket, Type};
    let fail = |e: std::io::Error| format!("Can't listen for FlexRadios: {e}");
    // SmartSDR on the same computer listens on the same port, so share it.
    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP)).map_err(fail)?;
    socket.set_reuse_address(true).map_err(fail)?;
    #[cfg(unix)]
    socket.set_reuse_port(true).map_err(fail)?;
    socket.set_broadcast(true).map_err(fail)?;
    socket.set_nonblocking(true).map_err(fail)?;
    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], DEFAULT_PORT));
    socket.bind(&addr.into()).map_err(fail)?;
    let socket = tokio::net::UdpSocket::from_std(socket.into()).map_err(fail)?;

    let mut found: Vec<Found> = Vec::new();
    let mut buf = [0u8; 2048];
    let deadline = Instant::now() + listen;
    while let Ok(received) = timeout_at(deadline, socket.recv_from(&mut buf)).await {
        let (n, _) = received.map_err(fail)?;
        if let Some(radio) = parse_discovery(&buf[..n])
            && !found.iter().any(|r| r.ip == radio.ip && r.port == radio.port)
        {
            found.push(radio);
        }
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn follows_tx_slice() {
        let mut r = Radio::default();
        assert!(!r.apply("V1.4.0.0"));
        assert!(!r.apply("H2D3A1B2C"));
        assert!(r.apply(
            "S2D3A1B2C|slice 0 in_use=1 sample_rate=24000 RF_frequency=14.074000 client_handle=0x2D3A1B2C \
             index_letter=A rit_on=0 mode=DIGU wide=0 tx=1 active=1"
        ));
        assert_eq!(r.state(), Some((14_074_000, "DIGU".into())));
        // A second slice that isn't transmitting doesn't take over.
        assert!(!r.apply("S2D3A1B2C|slice 1 in_use=1 RF_frequency=7.025000 mode=CW tx=0 active=1"));
        assert_eq!(r.state(), Some((14_074_000, "DIGU".into())));
        // Moving TX to slice B follows it.
        r.apply("S2D3A1B2C|slice 0 tx=0");
        assert!(r.apply("S2D3A1B2C|slice 1 tx=1"));
        assert_eq!(r.state(), Some((7_025_000, "CW".into())));
        // Partial updates keep what we already know.
        assert!(r.apply("S2D3A1B2C|slice 1 RF_frequency=7.031500"));
        assert_eq!(r.state(), Some((7_031_500, "CW".into())));
        // Closing slice B falls back to slice A.
        assert!(r.apply("S2D3A1B2C|slice 1 in_use=0"));
        assert_eq!(r.state(), Some((14_074_000, "DIGU".into())));
    }

    #[test]
    fn ignores_other_status() {
        let mut r = Radio::default();
        assert!(!r.apply("S0|radio slices=4 panadapters=4 lineout_gain=60"));
        assert!(!r.apply("M10000001|Client connected"));
        assert!(!r.apply("R3|0|"));
        assert_eq!(r.state(), None);
    }

    #[test]
    fn responses() {
        assert_eq!(response("R3|0|"), Some((3, None)));
        assert_eq!(response("R7|50000016|Unknown command"), Some((7, Some("Unknown command".into()))));
        assert_eq!(response("S0|radio slices=4"), None);
    }

    #[test]
    fn spots() {
        assert_eq!(
            spot("ZL2ABC", "CW", 7_025_000, "worked 40m 0812Z"),
            "spot add rx_freq=7.025000 callsign=ZL2ABC mode=CW color=#FFC0C0C0 source=OpenVKCL \
             comment=worked\u{7f}40m\u{7f}0812Z lifetime_seconds=3600"
        );
    }

    #[test]
    fn discovery() {
        let mut packet = vec![0x38, 0x52, 0x00, 0x7A, 0, 0, 0x08, 0];
        packet.resize(28, 0);
        packet.extend_from_slice(
            b"discovery_protocol_version=3.0.0.2 model=FLEX-6600 serial=1234-5678-6600-0001 version=3.8.19.36 \
              nickname=Shack callsign=VK2YI ip=192.168.1.50 port=4992 status=Available\0\0\0",
        );
        assert_eq!(
            parse_discovery(&packet),
            Some(Found {
                ip: "192.168.1.50".into(),
                port: 4992,
                model: "FLEX-6600".into(),
                nickname: "Shack".into(),
                callsign: "VK2YI".into(),
                serial: "1234-5678-6600-0001".into(),
            })
        );
        assert_eq!(parse_discovery(b"\0\0\0garbage"), None);
    }

    /// A fake radio: check what the client sends, push slice status, then hang up.
    #[tokio::test]
    async fn talks_to_a_radio() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let radio = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let (read, mut write) = stream.into_split();
            write.write_all(b"V1.4.0.0\nH2D3A1B2C\n").await.unwrap();
            let mut lines = BufReader::new(read).lines();
            let mut got = Vec::new();
            while got.len() < 5 {
                got.push(lines.next_line().await.unwrap().unwrap());
            }
            write
                .write_all(b"R1|0|\nR2|0|\nR3|0|\nS2D3A1B2C|slice 0 in_use=1 RF_frequency=3.650000 mode=LSB tx=1\n")
                .await
                .unwrap();
            got.push(lines.next_line().await.unwrap().unwrap());
            write.write_all(b"S2D3A1B2C|slice 0 mode=CW\n").await.unwrap();
            got
        });

        let mut c = Client::connect("127.0.0.1", port).await.unwrap();
        assert_eq!(c.poll(Duration::ZERO).await.unwrap(), Some((3_650_000, "LSB".into())));
        c.send(&spot("VK2ABC", "LSB", 3_650_000, "worked")).await.unwrap();
        assert_eq!(c.poll(Duration::from_secs(1)).await.unwrap(), Some((3_650_000, "CW".into())));
        let got = radio.await.unwrap();
        assert_eq!(
            got[..5],
            ["C1|client program OpenVKCL", "C2|keepalive enable", "C3|sub slice all", "C4|sub client all", "C5|sub tx all"]
        );
        assert!(got[5].starts_with("C6|spot add rx_freq=3.650000 callsign=VK2ABC"));
        assert_eq!(c.poll(Duration::from_secs(1)).await, Err("FlexRadio closed the connection".into()));
    }

    /// A fake radio for PTT: AetherSDR's slice transmits and transmit audio
    /// starts on the mic. It answers every command; `answer` is its answer to
    /// `xmit 1`, followed by the `interlock` line. Returns the commands it got.
    async fn ptt_radio(answer: &'static str, interlock: &'static str) -> (Client, tokio::task::JoinHandle<Vec<String>>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let radio = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let (read, mut write) = stream.into_split();
            let mut lines = BufReader::new(read).lines();
            let mut got = Vec::new();
            while got.len() < 5 {
                got.push(lines.next_line().await.unwrap().unwrap());
            }
            write
                .write_all(
                    b"S0|client 0x11111111 connected local_ptt=0 client_id=AAAA-BBBB program=SmartSDR-Win station=Shack\n\
                      S0|client 0x22222222 connected local_ptt=1 client_id=CCCC-DDDD program=AetherSDR station=Mac\n\
                      S1|slice 0 in_use=1 RF_frequency=14.200000 mode=USB tx=1 client_handle=0x22222222\n\
                      S0|transmit freq=14.200000 dax=0 mic_level=50\n\
                      S0|interlock state=RECEIVE reason= tx_allowed=1\n",
                )
                .await
                .unwrap();
            while let Ok(Ok(Some(line))) = timeout(Duration::from_millis(500), lines.next_line()).await {
                let seq = line[1..].split('|').next().unwrap().to_string();
                let reply = if line.ends_with("xmit 1") {
                    format!("R{seq}|{answer}\n{interlock}\n")
                } else if let Some(dax) = line.strip_suffix("transmit set dax=1").map(|_| 1).or(line.strip_suffix("transmit set dax=0").map(|_| 0)) {
                    format!("R{seq}|0|\nS0|transmit dax={dax}\n")
                } else {
                    format!("R{seq}|0|\n")
                };
                write.write_all(reply.as_bytes()).await.unwrap();
                got.push(line);
            }
            got
        });
        let mut c = Client::connect("127.0.0.1", port).await.unwrap();
        assert_eq!(c.poll(Duration::from_millis(200)).await.unwrap(), Some((14_200_000, "USB".into())));
        (c, radio)
    }

    #[tokio::test]
    async fn binds_and_keys_up_on_dax() {
        let (mut c, radio) = ptt_radio("0|", "S0|interlock state=TRANSMITTING reason= tx_allowed=1").await;
        assert_eq!(c.set_ptt(true).await, Ok(()));
        assert_eq!(c.set_ptt(false).await, Ok(()));
        drop(c);
        let got = radio.await.unwrap();
        // Bound to AetherSDR, whose slice transmits; transmit audio switched to
        // DAX for the message, then back to the mic.
        assert_eq!(
            got[5..],
            [
                "C6|client bind client_id=CCCC-DDDD",
                "C7|transmit set dax=1",
                "C8|xmit 1",
                "C9|xmit 0",
                "C10|transmit set dax=0"
            ]
        );
    }

    #[tokio::test]
    async fn says_why_it_wont_transmit() {
        let (mut c, radio) = ptt_radio("50000016|Transmit not allowed", "").await;
        assert_eq!(c.set_ptt(true).await, Err("FlexRadio refused to transmit: Transmit not allowed".into()));
        drop(c);
        // DAX is put back when keying up fails.
        assert_eq!(radio.await.unwrap().last().unwrap(), "C9|transmit set dax=0");
        let (mut c, _) = ptt_radio("0|", "S0|interlock state=NOT_READY reason=AMP_FAULT tx_allowed=1").await;
        assert_eq!(c.set_ptt(true).await, Err("FlexRadio won't transmit: amp fault".into()));
    }

    #[tokio::test]
    async fn no_slice_no_transmit() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            let _conn = listener.accept().await.unwrap();
            tokio::time::sleep(Duration::from_secs(1)).await;
        });
        let mut c = Client::connect("127.0.0.1", port).await.unwrap();
        let e = c.set_ptt(true).await.unwrap_err();
        assert!(e.contains("no slice open"), "{e}");
    }

    /// Connected with no slice open, until one opens and after it closes.
    #[tokio::test]
    async fn no_slice_open() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let (open, opened) = tokio::sync::oneshot::channel::<()>();
        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            stream.write_all(b"R1|0|\nR2|0|\nR3|0|\n").await.unwrap();
            opened.await.unwrap();
            stream
                .write_all(b"S1|slice 0 in_use=1 RF_frequency=14.200000 mode=USB tx=1\nS1|slice 0 in_use=0\n")
                .await
                .unwrap();
            tokio::time::sleep(Duration::from_secs(2)).await;
        });
        let mut c = Client::connect("127.0.0.1", port).await.unwrap();
        // As if the radio had been given its few seconds to report slices.
        c.connected_at -= FIRST_STATE;
        assert_eq!(c.poll(Duration::from_millis(100)).await, Ok(None));
        open.send(()).unwrap();
        assert_eq!(c.poll(Duration::from_secs(1)).await, Ok(Some((14_200_000, "USB".into()))));
        assert_eq!(c.poll(Duration::from_secs(1)).await, Ok(None));
    }

    #[tokio::test]
    async fn reports_refused_subscription() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            stream.write_all(b"R3|50000016|Unknown command\n").await.unwrap();
            tokio::time::sleep(Duration::from_secs(2)).await;
        });
        let mut c = Client::connect("127.0.0.1", port).await.unwrap();
        assert_eq!(
            c.poll(Duration::from_millis(100)).await,
            Err("FlexRadio refused slice updates: Unknown command".into())
        );
    }
}
