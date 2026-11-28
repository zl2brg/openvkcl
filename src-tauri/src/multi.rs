//! One open log, shared on the LAN. The app that opens the `.ovkcl` is the host
//! and the only writer. Other PCs join, log on their own band, and keep contacts
//! locally while the host cannot be reached.

use crate::model::{Qso, QsoInput};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

pub const DEFAULT_PORT: u16 = 7374;
pub const BEACON_PORT: u16 = 7375;
const MAX_LINE: usize = 32 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Station {
    pub operator: String,
    pub band: String,
    pub connected: bool,
    pub host: bool,
    /// Host-assigned number of this copy. Zero for the PC that has the log open.
    /// Two copies may share a callsign; the number is what keeps them apart.
    #[serde(default)]
    pub client: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Offer {
    pub contest: String,
    pub call: String,
    pub bands: Vec<String>,
    pub stations: Vec<Station>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Found {
    pub addr: String,
    pub contest: String,
    pub call: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiStatus {
    pub role: String,
    pub connected: bool,
    pub address: String,
    pub operator: String,
    pub band: String,
    pub queued: usize,
    pub queue_host: String,
    pub stations: Vec<Station>,
    pub taken: Vec<String>,
    pub error: String,
    pub contest: String,
    /// Host-assigned number of this copy. The same callsign can be on the air twice.
    pub client: u32,
}

impl Default for MultiStatus {
    fn default() -> Self {
        Self {
            role: "off".into(),
            connected: false,
            address: String::new(),
            operator: String::new(),
            band: String::new(),
            queued: 0,
            queue_host: String::new(),
            stations: Vec::new(),
            taken: Vec::new(),
            error: String::new(),
            contest: String::new(),
            client: 0,
        }
    }
}

pub enum ShareEvent {
    View { view: Value, stations: Vec<Station> },
    Status(MultiStatus),
    /// The band table changed. Clients grey out bands another station holds.
    Stations(Vec<Station>),
    Notice { text: String, error: bool },
}

pub type Listener = Arc<dyn Fn(ShareEvent) + Send + Sync>;

pub trait HostOps: Send + Sync {
    fn offer(&self) -> Offer;
    fn view(&self) -> Result<Value, String>;
    fn log(&self, operator: &str, input: QsoInput) -> Result<Value, String>;
    fn edit(&self, operator: &str, client: u32, qso: Qso) -> Result<Value, String>;
    fn delete(&self, operator: &str, client: u32, id: i64) -> Result<Value, String>;
    fn check(&self, input: QsoInput) -> Result<Value, String>;
    fn export(&self, kind: &str, from: Option<String>, to: Option<String>) -> Result<Value, String>;
    /// Contest name and the host station's callsign.
    fn label(&self) -> (String, String);
}

pub enum Submit {
    Log(QsoInput),
    Edit(Qso),
    Delete(i64),
    Check(QsoInput),
    Band(String),
    Export { kind: String, from: Option<String>, to: Option<String> },
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Msg {
    Offer {
        #[serde(flatten)]
        offer: Offer,
    },
    Join {
        operator: String,
        band: String,
        /// Number this copy was given before. Zero asks the host for a new one.
        #[serde(default)]
        client: u32,
    },
    Joined {
        stations: Vec<Station>,
        view: Value,
        /// Number of this copy. A later join with the same number takes the same band back.
        #[serde(default)]
        client: u32,
    },
    Leave,
    Bye,
    Band { tag: String, band: String },
    Log { tag: String, input: QsoInput },
    Edit { tag: String, qso: Qso },
    Delete { tag: String, id: i64 },
    Check { tag: String, input: QsoInput },
    Export { tag: String, kind: String, from: Option<String>, to: Option<String> },
    Ok { tag: String, body: Value },
    View { stations: Vec<Station>, view: Value },
    /// Who holds each band. Sent when a station changes band, without the whole log.
    Stations { stations: Vec<Station> },
    Rejected { tag: String, error: String },
    Error { error: String },
}

struct Seat {
    operator: String,
    band: String,
    connected: bool,
    scoreboard: bool,
    generation: u64,
}

/// Who currently holds each band. A seat is one running copy, not a callsign,
/// so two radios can use the same call. A dropped link keeps the band until
/// that copy leaves the shared log.
#[derive(Default)]
pub struct Bands {
    host: String,
    seats: BTreeMap<u32, Seat>,
    next_gen: u64,
}

impl Bands {
    pub fn claim_host(&mut self, band: &str) -> Result<(), String> {
        let band = crate::bands::canonical(band);
        if band.is_empty() {
            self.host.clear();
            return Ok(());
        }
        if self.host == band {
            return Ok(());
        }
        if let Some(op) = self.holder(&band, None) {
            return Err(format!("{band} is in use by {op}"));
        }
        self.host = band;
        Ok(())
    }

    /// Returns the generation of this join. A later drop from an older
    /// connection does not release the band. `client` is this copy's number.
    pub fn claim_client(&mut self, client: u32, operator: &str, band: &str) -> Result<u64, String> {
        if client == 0 {
            return Err("Join the log first".into());
        }
        let operator = norm_op(operator);
        if operator.is_empty() {
            return Err("Enter an operator call".into());
        }
        if !band.is_empty() {
            let band = crate::bands::canonical(band);
            if self.host == band {
                return Err(format!("{band} is in use at the host"));
            }
            if let Some(op) = self.holder(&band, Some(client)) {
                return Err(format!("{band} is in use by {op}"));
            }
            self.next_gen += 1;
            let generation = self.next_gen;
            self.seats.insert(client, Seat { operator, band, connected: true, scoreboard: false, generation });
            return Ok(generation);
        }
        self.next_gen += 1;
        let generation = self.next_gen;
        self.seats.insert(
            client,
            Seat { operator, band: String::new(), connected: true, scoreboard: true, generation },
        );
        Ok(generation)
    }

    pub fn allow_client(&self, client: u32, band: &str) -> Result<(), String> {
        let seat = self.seats.get(&client).ok_or("Join the log first")?;
        if seat.scoreboard {
            return Err("A scoreboard only watches the log".into());
        }
        let band = crate::bands::canonical(band);
        if seat.band != band {
            return Err(format!("This station is on {}", seat.band));
        }
        Ok(())
    }

    pub fn client_band(&self, client: u32) -> Result<String, String> {
        self.seats
            .get(&client)
            .filter(|s| !s.scoreboard)
            .map(|s| s.band.clone())
            .ok_or_else(|| "A scoreboard only watches the log".into())
    }

    pub fn is_connected(&self, client: u32) -> bool {
        self.seats.get(&client).is_some_and(|s| s.connected)
    }

    pub fn disconnect_if(&mut self, client: u32, generation: u64) {
        if let Some(seat) = self.seats.get_mut(&client)
            && seat.generation == generation
        {
            seat.connected = false;
        }
    }

    /// Drop the band only when this is still the connection that held it.
    pub fn release_if(&mut self, client: u32, generation: u64) {
        if self.seats.get(&client).is_some_and(|s| s.generation == generation) {
            self.seats.remove(&client);
        }
    }

    fn holder(&self, band: &str, except: Option<u32>) -> Option<String> {
        self.seats.iter().find(|(id, s)| s.band == band && except != Some(**id)).map(|(_, s)| s.operator.clone())
    }

    pub fn stations(&self, call: &str) -> Vec<Station> {
        let mut out = vec![Station {
            operator: call.to_string(),
            band: self.host.clone(),
            connected: true,
            host: true,
            client: 0,
        }];
        for (client, seat) in &self.seats {
            out.push(Station {
                operator: seat.operator.clone(),
                band: seat.band.clone(),
                connected: seat.connected,
                host: false,
                client: *client,
            });
        }
        out
    }

    /// Bands this copy must not move onto. `None` is the host.
    pub fn taken_for(&self, client: Option<u32>) -> Vec<String> {
        let mine = client.and_then(|id| self.seats.get(&id)).map(|s| s.band.clone());
        let mut taken: Vec<String> = self.seats.values().filter(|s| !s.band.is_empty()).map(|s| s.band.clone()).collect();
        if client.is_some() && !self.host.is_empty() {
            taken.push(self.host.clone());
        }
        if let Some(mine) = mine.filter(|b| !b.is_empty()) {
            taken.retain(|b| b != &mine);
        }
        taken.sort();
        taken.dedup();
        taken
    }

    pub fn host_band(&self) -> String {
        self.host.clone()
    }
}

fn norm_op(operator: &str) -> String {
    operator.trim().to_ascii_uppercase()
}

#[derive(Clone, Serialize, Deserialize)]
struct Queued {
    host: String,
    tag: String,
    line: String,
}

struct Server {
    port: u16,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    beacon: Option<JoinHandle<()>>,
    bands: Arc<Mutex<Bands>>,
    slots: Arc<Mutex<Vec<ConnSlot>>>,
    ops: Arc<dyn HostOps>,
    listen: Arc<Mutex<Option<Listener>>>,
}

struct ConnSlot {
    client: u32,
    generation: u64,
    tx: Sender<String>,
}

/// The next client number the host will hand out. A copy keeps its own number
/// and sends it back on the next join, including when the callsign is the same.
#[derive(Default, Serialize, Deserialize)]
struct ClientNumbers {
    next: u32,
}

struct Roster {
    path: PathBuf,
    lock: Mutex<()>,
}

impl Roster {
    /// `requested` is kept when that copy is not already connected. Otherwise
    /// the next number is handed out, so a second radio with the same callsign
    /// becomes its own station.
    fn resolve(&self, requested: u32, already_connected: bool) -> u32 {
        let _guard = self.lock.lock().unwrap();
        resolve_client_number(&self.path, requested, already_connected)
    }
}

fn resolve_client_number(path: &std::path::Path, requested: u32, already_connected: bool) -> u32 {
    let mut book: ClientNumbers = std::fs::read_to_string(path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();
    let n = if requested > 0 && !already_connected { requested } else { book.next.max(1) };
    if book.next <= n {
        book.next = n.saturating_add(1);
    }
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(text) = serde_json::to_string(&book) {
        let _ = std::fs::write(path, text);
    }
    n
}

impl Server {
    fn bind(
        addr: SocketAddr,
        ops: Arc<dyn HostOps>,
        bands: Arc<Mutex<Bands>>,
        listen: Arc<Mutex<Option<Listener>>>,
        numbers_path: PathBuf,
    ) -> Result<Self, String> {
        let listener = listen_on(addr)?;
        let port = listener.local_addr().map_err(|e| e.to_string())?.port();
        let stop = Arc::new(AtomicBool::new(false));
        let slots = Arc::new(Mutex::new(Vec::new()));
        let thread = thread::Builder::new().name("openvkcl-share".into()).spawn({
            let stop = Arc::clone(&stop);
            let ops = Arc::clone(&ops);
            let bands = Arc::clone(&bands);
            let slots = Arc::clone(&slots);
            let listen = Arc::clone(&listen);
            let roster = Arc::new(Roster { path: numbers_path, lock: Mutex::new(()) });
            move || accept_loop(listener, stop, ops, bands, slots, listen, roster)
        }).map_err(|e| e.to_string())?;
        let beacon = thread::Builder::new().name("openvkcl-beacon".into()).spawn({
            let stop = Arc::clone(&stop);
            let ops = Arc::clone(&ops);
            move || beacon_loop(port, stop, ops)
        }).ok();
        Ok(Self { port, stop, thread: Some(thread), beacon, bands, slots, ops, listen })
    }

    fn shutdown(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Ok(line) = encode(&Msg::Bye) {
            for slot in self.slots.lock().unwrap().iter() {
                let _ = slot.tx.send(line.clone());
            }
        }
        let _ = TcpStream::connect_timeout(&SocketAddr::from(([127, 0, 0, 1], self.port)), Duration::from_millis(200));
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
        if let Some(t) = self.beacon.take() {
            let _ = t.join();
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn listen_on(addr: SocketAddr) -> Result<TcpListener, String> {
    use socket2::{Domain, Socket, Type};
    let socket = Socket::new(Domain::IPV4, Type::STREAM, None).map_err(|e| e.to_string())?;
    socket.set_reuse_address(true).map_err(|e| e.to_string())?;
    socket.bind(&addr.into()).map_err(|e| format!("Can't share the log on {addr}: {e}"))?;
    socket.listen(128).map_err(|e| e.to_string())?;
    Ok(socket.into())
}

fn accept_loop(
    listener: TcpListener,
    stop: Arc<AtomicBool>,
    ops: Arc<dyn HostOps>,
    bands: Arc<Mutex<Bands>>,
    slots: Arc<Mutex<Vec<ConnSlot>>>,
    listen: Arc<Mutex<Option<Listener>>>,
    roster: Arc<Roster>,
) {
    let port = listener.local_addr().map(|a| a.port()).unwrap_or(0);
    while !stop.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((stream, _)) => {
                if stop.load(Ordering::Relaxed) {
                    break;
                }
                let _ = stream.set_nodelay(true);
                let ops = Arc::clone(&ops);
                let bands = Arc::clone(&bands);
                let slots = Arc::clone(&slots);
                let listen = Arc::clone(&listen);
                let roster = Arc::clone(&roster);
                thread::spawn(move || client_conn(stream, ops, bands, slots, listen, roster, port));
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => thread::sleep(Duration::from_millis(40)),
            Err(_) => thread::sleep(Duration::from_millis(40)),
        }
    }
}

fn client_conn(
    stream: TcpStream,
    ops: Arc<dyn HostOps>,
    bands: Arc<Mutex<Bands>>,
    slots: Arc<Mutex<Vec<ConnSlot>>>,
    listen: Arc<Mutex<Option<Listener>>>,
    roster: Arc<Roster>,
    port: u16,
) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(3600)));
    let _ = stream.set_nodelay(true);
    let mut reader = match stream.try_clone() {
        Ok(s) => BufReader::new(s),
        Err(_) => return,
    };
    let (tx, rx) = mpsc::channel::<String>();
    let mut writer = stream;
    thread::spawn(move || {
        while let Ok(line) = rx.recv() {
            if writer.write_all(line.as_bytes()).and_then(|_| writer.flush()).is_err() {
                break;
            }
        }
    });
    if tx.send(encode(&Msg::Offer { offer: ops.offer() }).unwrap_or_default()).is_err() {
        return;
    }
    let mut joined: Option<(u32, String, u64)> = None;
    loop {
        let msg = match read_msg(&mut reader) {
            Ok(m) => m,
            Err(_) => break,
        };
        match msg {
            Msg::Join { operator, band, client } => {
                let claimed = {
                    let mut guard = bands.lock().unwrap();
                    let already = guard.is_connected(client);
                    let client = roster.resolve(client, already);
                    match guard.claim_client(client, &operator, &band) {
                        Ok(generation) => Ok((client, generation)),
                        Err(e) => Err(e),
                    }
                };
                match claimed {
                Ok((client, generation)) => {
                    let operator = norm_op(&operator);
                    joined = Some((client, operator, generation));
                    let stations = stations_of(&ops, &bands);
                    let view = match ops.view() {
                        Ok(v) => v,
                        Err(e) => {
                            let _ = tx.send(encode(&Msg::Error { error: e }).unwrap_or_default());
                            break;
                        }
                    };
                    slots.lock().unwrap().push(ConnSlot { client, generation, tx: tx.clone() });
                    if tx.send(encode(&Msg::Joined { stations: stations.clone(), view: view.clone(), client }).unwrap_or_default()).is_err() {
                        break;
                    }
                    emit(&listen, ShareEvent::View { view, stations });
                    publish_bands(&listen, &ops, &bands, &slots, port);
                }
                Err(e) => {
                    let _ = tx.send(encode(&Msg::Error { error: e }).unwrap_or_default());
                }
                }
            }
            Msg::Leave => {
                if let Some((client, _, generation)) = joined.take() {
                    bands.lock().unwrap().release_if(client, generation);
                    slots.lock().unwrap().retain(|s| s.client != client || s.generation != generation);
                    publish_bands(&listen, &ops, &bands, &slots, port);
                }
                let _ = tx.send(encode(&Msg::Bye).unwrap_or_default());
                break;
            }
            Msg::Band { tag, band } => {
                let Some((client, op, old_gen)) = joined.clone() else {
                    let _ = tx.send(line_err(&tag, "Join the log first"));
                    continue;
                };
                let claimed = bands.lock().unwrap().claim_client(client, &op, &band);
                match claimed {
                    Ok(generation) => {
                        joined = Some((client, op, generation));
                        for slot in slots.lock().unwrap().iter_mut() {
                            if slot.client == client && slot.generation == old_gen {
                                slot.generation = generation;
                            }
                        }
                        reply_ok(&tx, &tag, json!({"kind": "band"}));
                        publish_bands(&listen, &ops, &bands, &slots, port);
                    }
                    Err(e) => {
                        let _ = tx.send(line_err(&tag, &e));
                    }
                }
            }
            Msg::Log { tag, input } => {
                let Some((client, op, _)) = joined.clone() else {
                    let _ = tx.send(line_err(&tag, "Join the log first"));
                    continue;
                };
                if let Err(e) = bands.lock().unwrap().allow_client(client, &input.band) {
                    let _ = tx.send(line_err(&tag, &e));
                    continue;
                }
                finish(&tx, &tag, ops.log(&op, input), &ops, &bands, &slots, &listen);
            }
            Msg::Edit { tag, qso } => {
                let Some((client, op, _)) = joined.clone() else {
                    let _ = tx.send(line_err(&tag, "Join the log first"));
                    continue;
                };
                finish(&tx, &tag, ops.edit(&op, client, qso), &ops, &bands, &slots, &listen);
            }
            Msg::Delete { tag, id } => {
                let Some((client, op, _)) = joined.clone() else {
                    let _ = tx.send(line_err(&tag, "Join the log first"));
                    continue;
                };
                finish(&tx, &tag, ops.delete(&op, client, id), &ops, &bands, &slots, &listen);
            }
            Msg::Check { tag, input } => match ops.check(input) {
                Ok(body) => reply_ok(&tx, &tag, body),
                Err(e) => {
                    let _ = tx.send(line_err(&tag, &e));
                }
            },
            Msg::Export { tag, kind, from, to } => match ops.export(&kind, from, to) {
                Ok(body) => reply_ok(&tx, &tag, body),
                Err(e) => {
                    let _ = tx.send(line_err(&tag, &e));
                }
            },
            _ => {}
        }
    }
    if let Some((client, _, generation)) = joined {
        bands.lock().unwrap().disconnect_if(client, generation);
        slots.lock().unwrap().retain(|s| s.client != client || s.generation != generation);
        publish_bands(&listen, &ops, &bands, &slots, port);
    }
}

fn finish(
    tx: &Sender<String>,
    tag: &str,
    result: Result<Value, String>,
    ops: &Arc<dyn HostOps>,
    bands: &Arc<Mutex<Bands>>,
    slots: &Arc<Mutex<Vec<ConnSlot>>>,
    listen: &Arc<Mutex<Option<Listener>>>,
) {
    match result {
        Ok(body) => {
            reply_ok(tx, tag, body);
            broadcast(ops, bands, slots, listen);
        }
        Err(e) => {
            let _ = tx.send(line_err(tag, &e));
        }
    }
}

fn stations_of(ops: &Arc<dyn HostOps>, bands: &Arc<Mutex<Bands>>) -> Vec<Station> {
    let (_, call) = ops.label();
    bands.lock().unwrap().stations(&call)
}

fn broadcast(
    ops: &Arc<dyn HostOps>,
    bands: &Arc<Mutex<Bands>>,
    slots: &Arc<Mutex<Vec<ConnSlot>>>,
    listen: &Arc<Mutex<Option<Listener>>>,
) {
    let stations = stations_of(ops, bands);
    let view = match ops.view() {
        Ok(v) => v,
        Err(e) => {
            emit(listen, ShareEvent::Notice { text: e, error: true });
            return;
        }
    };
    if let Ok(line) = encode(&Msg::View { stations: stations.clone(), view: view.clone() }) {
        slots.lock().unwrap().retain(|s| s.tx.send(line.clone()).is_ok());
    }
    emit(listen, ShareEvent::View { view, stations });
}

fn emit(listen: &Mutex<Option<Listener>>, event: ShareEvent) {
    // Drop the lock before the callback. The callback can reach back into this
    // table, and a std mutex is not reentrant.
    let listener = listen.lock().unwrap().clone();
    if let Some(l) = listener {
        l(event);
    }
}

fn host_address(port: u16) -> String {
    match lan_ip() {
        Some(ip) => format!("{ip}:{port}"),
        None => format!("port {port}"),
    }
}

fn publish_bands(
    listen: &Mutex<Option<Listener>>,
    ops: &Arc<dyn HostOps>,
    bands: &Arc<Mutex<Bands>>,
    slots: &Arc<Mutex<Vec<ConnSlot>>>,
    port: u16,
) {
    emit_host_status(listen, ops, bands, port);
    let stations = stations_of(ops, bands);
    if let Ok(line) = encode(&Msg::Stations { stations }) {
        slots.lock().unwrap().retain(|s| s.tx.send(line.clone()).is_ok());
    }
}

fn emit_host_status(listen: &Mutex<Option<Listener>>, ops: &Arc<dyn HostOps>, bands: &Arc<Mutex<Bands>>, port: u16) {
    let (contest, call) = ops.label();
    let (band, stations, taken) = {
        let bands = bands.lock().unwrap();
        (bands.host_band(), bands.stations(&call), bands.taken_for(None))
    };
    emit(
        listen,
        ShareEvent::Status(MultiStatus {
            role: "host".into(),
            connected: true,
            address: host_address(port),
            contest,
            operator: call.clone(),
            band,
            stations,
            taken,
            ..MultiStatus::default()
        }),
    );
}

fn reply_ok(tx: &Sender<String>, tag: &str, body: Value) {
    let _ = tx.send(encode(&Msg::Ok { tag: tag.to_string(), body }).unwrap_or_default());
}

fn line_err(tag: &str, error: &str) -> String {
    encode(&Msg::Rejected { tag: tag.to_string(), error: error.to_string() }).unwrap_or_default()
}

fn encode(msg: &Msg) -> Result<String, String> {
    let mut line = serde_json::to_string(msg).map_err(|e| e.to_string())?;
    line.push('\n');
    Ok(line)
}

fn read_msg(reader: &mut impl BufRead) -> std::io::Result<Msg> {
    let mut buf = Vec::new();
    let n = reader.read_until(b'\n', &mut buf)?;
    if n == 0 {
        return Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "closed"));
    }
    if buf.len() > MAX_LINE {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "message too long"));
    }
    let end = buf.iter().rposition(|b| !b.is_ascii_whitespace()).map(|i| i + 1).unwrap_or(0);
    serde_json::from_slice(&buf[..end]).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

fn beacon_loop(tcp_port: u16, stop: Arc<AtomicBool>, ops: Arc<dyn HostOps>) {
    let Ok(socket) = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)) else { return };
    let _ = socket.set_broadcast(true);
    let dest = SocketAddr::from((Ipv4Addr::BROADCAST, BEACON_PORT));
    while !stop.load(Ordering::Relaxed) {
        let (contest, call) = ops.label();
        let packet = format!("openvkcl\t{tcp_port}\t{contest}\t{call}");
        let _ = socket.send_to(packet.as_bytes(), dest);
        for _ in 0..10 {
            if stop.load(Ordering::Relaxed) {
                return;
            }
            thread::sleep(Duration::from_millis(100));
        }
    }
}

pub fn parse_beacon(text: &str, from: IpAddr) -> Option<Found> {
    let mut parts = text.trim_end_matches('\0').trim().split('\t');
    if parts.next() != Some("openvkcl") {
        return None;
    }
    let port: u16 = parts.next()?.parse().ok()?;
    let contest = parts.next().unwrap_or("").to_string();
    let call = parts.next().unwrap_or("").to_string();
    Some(Found { addr: format!("{from}:{port}"), contest, call })
}

pub fn discover(listen_for: Duration) -> Result<Vec<Found>, String> {
    use socket2::{Domain, Socket, Type};
    let socket = Socket::new(Domain::IPV4, Type::DGRAM, None).map_err(|e| e.to_string())?;
    socket.set_reuse_address(true).map_err(|e| e.to_string())?;
    socket.set_reuse_port(true).map_err(|e| e.to_string())?;
    socket
        .bind(&SocketAddr::from((Ipv4Addr::UNSPECIFIED, BEACON_PORT)).into())
        .map_err(|e| format!("Can't listen for logs: {e}"))?;
    socket.set_nonblocking(true).map_err(|e| e.to_string())?;
    let socket: UdpSocket = socket.into();
    let mut found = Vec::new();
    let mut buf = [0u8; 2048];
    let deadline = Instant::now() + listen_for;
    while Instant::now() < deadline {
        match socket.recv_from(&mut buf) {
            Ok((n, from)) => {
                let text = String::from_utf8_lossy(&buf[..n]);
                if let Some(host) = parse_beacon(&text, from.ip())
                    && !found.iter().any(|f: &Found| f.addr == host.addr)
                {
                    found.push(host);
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => thread::sleep(Duration::from_millis(40)),
            Err(e) => return Err(format!("Can't listen for logs: {e}")),
        }
    }
    Ok(found)
}

pub fn peek(addr: &str) -> Result<Offer, String> {
    let addr = parse_addr(addr)?;
    let stream = TcpStream::connect_timeout(&addr, Duration::from_secs(3)).map_err(|e| format!("Can't reach {addr}: {e}"))?;
    stream.set_read_timeout(Some(Duration::from_secs(3))).map_err(|e| e.to_string())?;
    match read_msg(&mut BufReader::new(stream)).map_err(|e| format!("No answer from the host: {e}"))? {
        Msg::Offer { offer } => Ok(offer),
        Msg::Error { error } => Err(error),
        _ => Err("The host sent an unexpected greeting".into()),
    }
}

pub fn parse_addr(text: &str) -> Result<SocketAddr, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("Enter the host address".into());
    }
    let with_port = if text.contains(':') { text.to_string() } else { format!("{text}:{DEFAULT_PORT}") };
    with_port.parse().map_err(|_| format!("'{text}' is not a host address"))
}

pub fn lan_ip() -> Option<String> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).ok()?;
    socket.connect((Ipv4Addr::new(192, 0, 2, 1), 9)).ok()?;
    match socket.local_addr().ok()?.ip() {
        IpAddr::V4(ip) if !ip.is_unspecified() && !ip.is_loopback() => Some(ip.to_string()),
        _ => None,
    }
}

struct Client {
    stop: Arc<AtomicBool>,
    connected: Arc<AtomicBool>,
    addr: String,
    operator: String,
    band: String,
    writer: Arc<Mutex<Option<Sender<String>>>>,
    kick: Arc<Mutex<Option<TcpStream>>>,
    pending: Arc<Mutex<BTreeMap<String, Sender<Result<Value, String>>>>>,
    order: Arc<Mutex<()>>,
    queue: Arc<Mutex<Vec<Queued>>>,
    number: Arc<AtomicU32>,
    thread: Option<JoinHandle<()>>,
}

impl Client {
    fn start(
        addr: String,
        operator: String,
        band: String,
        queue: Arc<Mutex<Vec<Queued>>>,
        queue_path: Arc<Mutex<PathBuf>>,
        files_dir: PathBuf,
        listen: Arc<Mutex<Option<Listener>>>,
        ready: Sender<Result<(), String>>,
    ) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let connected = Arc::new(AtomicBool::new(false));
        let writer = Arc::new(Mutex::new(None));
        let kick = Arc::new(Mutex::new(None));
        let pending = Arc::new(Mutex::new(BTreeMap::new()));
        let order = Arc::new(Mutex::new(()));
        let number = Arc::new(AtomicU32::new(load_client_number(&files_dir)));
        let thread = thread::spawn({
            let stop = Arc::clone(&stop);
            let connected = Arc::clone(&connected);
            let writer = Arc::clone(&writer);
            let kick = Arc::clone(&kick);
            let pending = Arc::clone(&pending);
            let order = Arc::clone(&order);
            let queue = Arc::clone(&queue);
            let number = Arc::clone(&number);
            let addr = addr.clone();
            let operator = operator.clone();
            let band = band.clone();
            move || client_loop(addr, operator, band, stop, connected, writer, kick, pending, order, queue, queue_path, files_dir, number, listen, ready)
        });
        Self { stop, connected, addr, operator, band, writer, kick, pending, order, queue, number, thread: Some(thread) }
    }

    fn submit(&self, msg: Msg, queueable: bool) -> Result<Value, String> {
        let _order = self.order.lock().unwrap();
        let tag = msg_tag(&msg);
        let line = encode(&msg)?;
        if !self.connected.load(Ordering::Relaxed) {
            return self.hold(queueable, tag, line);
        }
        match roundtrip(&self.writer, &self.pending, &self.connected, tag.clone(), line.clone()) {
            Err(e) if queueable && e == "The host is offline" => self.hold(true, tag, line),
            other => other,
        }
    }

    fn hold(&self, queueable: bool, tag: String, line: String) -> Result<Value, String> {
        if !queueable {
            return Err("The host is offline".into());
        }
        self.queue.lock().unwrap().push(Queued { host: self.addr.clone(), tag, line });
        Ok(json!({"kind": "queued"}))
    }

    fn shutdown(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(w) = self.writer.lock().unwrap().as_ref() {
            let _ = w.send(encode(&Msg::Leave).unwrap_or_default());
        }
        if let Some(socket) = self.kick.lock().unwrap().take() {
            let _ = socket.shutdown(std::net::Shutdown::Both);
        }
        self.connected.store(false, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn msg_tag(msg: &Msg) -> String {
    match msg {
        Msg::Band { tag, .. } | Msg::Log { tag, .. } | Msg::Edit { tag, .. } | Msg::Delete { tag, .. } | Msg::Check { tag, .. } | Msg::Export { tag, .. } => {
            tag.clone()
        }
        _ => String::new(),
    }
}

fn client_loop(
    addr: String,
    operator: String,
    band: String,
    stop: Arc<AtomicBool>,
    connected: Arc<AtomicBool>,
    writer: Arc<Mutex<Option<Sender<String>>>>,
    kick: Arc<Mutex<Option<TcpStream>>>,
    pending: Arc<Mutex<BTreeMap<String, Sender<Result<Value, String>>>>>,
    order: Arc<Mutex<()>>,
    queue: Arc<Mutex<Vec<Queued>>>,
    queue_path: Arc<Mutex<PathBuf>>,
    files_dir: PathBuf,
    number: Arc<AtomicU32>,
    listen: Arc<Mutex<Option<Listener>>>,
    ready: Sender<Result<(), String>>,
) {
    let mut ready = Some(ready);
    while !stop.load(Ordering::Relaxed) {
        let Ok(socket_addr) = parse_addr(&addr) else {
            fail_ready(&mut ready, "Enter the host address".into());
            return;
        };
        let stream = match TcpStream::connect_timeout(&socket_addr, Duration::from_secs(3)) {
            Ok(s) => s,
            Err(e) => {
                if ready.is_some() {
                    fail_ready(&mut ready, format!("Can't reach the host: {e}"));
                    return;
                }
                thread::sleep(Duration::from_secs(1));
                continue;
            }
        };
        let _ = stream.set_nodelay(true);
        let _ = stream.set_read_timeout(Some(Duration::from_secs(3600)));
        if let Ok(socket) = stream.try_clone() {
            *kick.lock().unwrap() = Some(socket);
        }
        let Ok(reader_stream) = stream.try_clone() else {
            thread::sleep(Duration::from_secs(1));
            continue;
        };
        let (tx, rx) = mpsc::channel::<String>();
        *writer.lock().unwrap() = Some(tx.clone());
        thread::spawn(move || {
            let mut write_stream = stream;
            while let Ok(line) = rx.recv() {
                if write_stream.write_all(line.as_bytes()).and_then(|_| write_stream.flush()).is_err() {
                    break;
                }
            }
        });
        let mut greeting = BufReader::new(reader_stream);
        // The offer is one line. The rest of the stream stays with the reader,
        // so the greeting reader must not buffer past it. Read a single line
        // with a fresh BufReader, then hand the underlying stream on... BufReader
        // may have buffered more. Read the offer from the same BufReader the
        // reader loop will use.
        let pending_r = Arc::clone(&pending);
        let listen_r = Arc::clone(&listen);
        let (joined_tx, joined_rx) = mpsc::channel();
        // Read the offer here, then spawn the reader on the same BufReader via
        // into_inner. If the BufReader buffered the next line we would lose it;
        // the host sends the offer and then waits, so one line is all that is
        // sitting there.
        if let Err(e) = read_msg(&mut greeting) {
            *writer.lock().unwrap() = None;
            if ready.is_some() {
                fail_ready(&mut ready, format!("No answer from the host: {e}"));
                return;
            }
            thread::sleep(Duration::from_secs(1));
            continue;
        }
        let reader_stream = greeting.into_inner();
        let reader_stop = Arc::clone(&stop);
        let reader = thread::spawn(move || reader_loop(reader_stream, pending_r, listen_r, joined_tx, reader_stop));
        if tx.send(encode(&Msg::Join { operator: operator.clone(), band: band.clone(), client: number.load(Ordering::Relaxed) }).unwrap_or_default()).is_err() {
            *writer.lock().unwrap() = None;
            fail_ready(&mut ready, "The host is offline".into());
            let _ = reader.join();
            continue;
        }
        match joined_rx.recv_timeout(Duration::from_secs(5)) {
            Ok(Ok(client)) => {
                number.store(client, Ordering::Relaxed);
                save_client_number(&files_dir, client);
                adopt_client_dir(&files_dir, &queue_path, &queue, client);
                connected.store(true, Ordering::Relaxed);
                if let Some(tx) = ready.take() {
                    let _ = tx.send(Ok(()));
                }
                flush_queue(&order, &queue, &writer, &pending, &connected, &listen, &addr);
                emit_client_status(&listen, &addr, &operator, &band, true, &queue, number.load(Ordering::Relaxed));
                while !stop.load(Ordering::Relaxed) && !reader.is_finished() {
                    thread::sleep(Duration::from_millis(80));
                }
                connected.store(false, Ordering::Relaxed);
                *writer.lock().unwrap() = None;
                let _ = reader.join();
                emit_client_status(&listen, &addr, &operator, &band, false, &queue, number.load(Ordering::Relaxed));
            }
            Ok(Err(e)) => {
                *writer.lock().unwrap() = None;
                let _ = reader.join();
                if ready.is_some() {
                    fail_ready(&mut ready, e);
                    return;
                }
                emit(&listen, ShareEvent::Notice { text: e, error: true });
                thread::sleep(Duration::from_secs(1));
            }
            Err(_) => {
                *writer.lock().unwrap() = None;
                let _ = reader.join();
                if ready.is_some() {
                    fail_ready(&mut ready, "The host didn't answer".into());
                    return;
                }
                thread::sleep(Duration::from_secs(1));
            }
        }
    }
}

fn fail_ready(ready: &mut Option<Sender<Result<(), String>>>, error: String) {
    if let Some(tx) = ready.take() {
        let _ = tx.send(Err(error));
    }
}

fn reader_loop(
    stream: TcpStream,
    pending: Arc<Mutex<BTreeMap<String, Sender<Result<Value, String>>>>>,
    listen: Arc<Mutex<Option<Listener>>>,
    joined: Sender<Result<u32, String>>,
    stop: Arc<AtomicBool>,
) {
    let mut reader = BufReader::new(stream);
    let mut told = false;
    loop {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        match read_msg(&mut reader) {
            Ok(Msg::Joined { stations, view, client }) => {
                emit(&listen, ShareEvent::View { view, stations });
                if !told {
                    told = true;
                    let _ = joined.send(Ok(client));
                }
            }
            Ok(Msg::View { stations, view }) => emit(&listen, ShareEvent::View { view, stations }),
            Ok(Msg::Stations { stations }) => emit(&listen, ShareEvent::Stations(stations)),
            Ok(Msg::Ok { tag, body }) => wake(&pending, &tag, Ok(body)),
            Ok(Msg::Rejected { tag, error }) => wake(&pending, &tag, Err(error)),
            Ok(Msg::Error { error }) => {
                if !told {
                    told = true;
                    let _ = joined.send(Err(error.clone()));
                }
                emit(&listen, ShareEvent::Notice { text: error, error: true });
            }
            Ok(Msg::Bye) => break,
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock || e.kind() == std::io::ErrorKind::TimedOut => continue,
            Err(_) => break,
        }
    }
    if !told {
        let _ = joined.send(Err("The host closed the connection".into()));
    }
    let leftover = std::mem::take(&mut *pending.lock().unwrap());
    for (_, tx) in leftover {
        let _ = tx.send(Err("The host is offline".into()));
    }
}

fn wake(pending: &Mutex<BTreeMap<String, Sender<Result<Value, String>>>>, tag: &str, result: Result<Value, String>) {
    if let Some(tx) = pending.lock().unwrap().remove(tag) {
        let _ = tx.send(result);
    }
}

fn roundtrip(
    writer: &Mutex<Option<Sender<String>>>,
    pending: &Mutex<BTreeMap<String, Sender<Result<Value, String>>>>,
    connected: &AtomicBool,
    tag: String,
    line: String,
) -> Result<Value, String> {
    let (tx, rx) = mpsc::channel();
    pending.lock().unwrap().insert(tag.clone(), tx);
    let sent = writer.lock().unwrap().as_ref().is_some_and(|w| w.send(line).is_ok());
    if !sent {
        pending.lock().unwrap().remove(&tag);
        connected.store(false, Ordering::Relaxed);
        return Err("The host is offline".into());
    }
    match rx.recv_timeout(Duration::from_secs(8)) {
        Ok(result) => result,
        Err(_) => {
            pending.lock().unwrap().remove(&tag);
            Err("The host didn't answer".into())
        }
    }
}

fn flush_queue(
    order: &Mutex<()>,
    queue: &Mutex<Vec<Queued>>,
    writer: &Mutex<Option<Sender<String>>>,
    pending: &Mutex<BTreeMap<String, Sender<Result<Value, String>>>>,
    connected: &AtomicBool,
    listen: &Mutex<Option<Listener>>,
    addr: &str,
) {
    let _order = order.lock().unwrap();
    loop {
        let next = queue.lock().unwrap().first().cloned();
        let Some(item) = next else { break };
        if item.host != addr {
            break;
        }
        match roundtrip(writer, pending, connected, item.tag.clone(), item.line.clone()) {
            Ok(body) => {
                queue.lock().unwrap().remove(0);
                if let Some(call) = dupe_call(&body) {
                    emit(listen, ShareEvent::Notice { text: format!("{call} was a duplicate and was logged"), error: false });
                }
            }
            Err(e) if e == "The host is offline" || e == "The host didn't answer" => break,
            Err(e) => {
                queue.lock().unwrap().remove(0);
                emit(listen, ShareEvent::Notice { text: e, error: true });
            }
        }
    }
}

/// A logged contact the host marked as a dupe, so the operator hears why.
fn dupe_call(body: &Value) -> Option<String> {
    let row = if body.get("kind").and_then(|k| k.as_str()) == Some("row") {
        body.get("row")?
    } else if body.get("kind").and_then(|k| k.as_str()) == Some("view") {
        body.get("view")?.get("qsos")?.as_array()?.last()?
    } else {
        return None;
    };
    if row.get("dupe") == Some(&Value::Bool(true)) {
        row.get("call").and_then(|c| c.as_str()).map(|s| s.to_string())
    } else {
        None
    }
}

/// The number this copy was given last time, or zero if it has never joined.
fn load_client_number(dir: &std::path::Path) -> u32 {
    std::fs::read_to_string(dir.join("client-number")).ok().and_then(|t| t.trim().parse().ok()).unwrap_or(0)
}

fn save_client_number(dir: &std::path::Path, n: u32) {
    if n == 0 {
        return;
    }
    let _ = std::fs::write(dir.join("client-number"), n.to_string());
}

/// Move this client's queue into `files_dir/clients/{n}/` once the host has numbered it.
fn adopt_client_dir(files_dir: &std::path::Path, path: &Mutex<PathBuf>, queue: &Mutex<Vec<Queued>>, n: u32) {
    if n == 0 {
        return;
    }
    let file = files_dir.join("clients").join(n.to_string()).join("multi-queue.json");
    if let Some(dir) = file.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let saved = load_queue(&file);
    let mut current = queue.lock().unwrap();
    let mut merged = saved;
    for item in current.iter().cloned() {
        if !merged.iter().any(|e| e.host == item.host && e.line == item.line) {
            merged.push(item);
        }
    }
    *current = merged;
    *path.lock().unwrap() = file.clone();
    if current.is_empty() {
        let _ = std::fs::remove_file(&file);
    } else if let Ok(text) = serde_json::to_string(current.as_slice()) {
        let _ = std::fs::write(&file, text);
    }
}

fn emit_client_status(listen: &Mutex<Option<Listener>>, addr: &str, operator: &str, band: &str, up: bool, queue: &Mutex<Vec<Queued>>, client: u32) {
    let queued = queue.lock().unwrap().iter().filter(|q| q.host == addr).count();
    emit(
        listen,
        ShareEvent::Status(MultiStatus {
            role: if band.is_empty() { "scoreboard" } else { "client" }.into(),
            connected: up,
            address: addr.to_string(),
            operator: operator.to_string(),
            band: band.to_string(),
            queued,
            queue_host: if queued > 0 { addr.to_string() } else { String::new() },
            client,
            ..MultiStatus::default()
        }),
    );
}

static TAGS: AtomicU64 = AtomicU64::new(1);

fn next_tag() -> String {
    format!("{:x}", TAGS.fetch_add(1, Ordering::Relaxed))
}

/// The host listener and, on a joining PC, the client connection.
pub struct Share {
    /// Config directory. Client queues live in `clients/{number}/` under here.
    files_dir: PathBuf,
    queue_path: Arc<Mutex<PathBuf>>,
    queue: Arc<Mutex<Vec<Queued>>>,
    /// Number saved from the last join, sent again so a dropped link takes the same band back.
    remembered: AtomicU32,
    server: Mutex<Option<Server>>,
    client: Mutex<Option<Client>>,
    listen: Arc<Mutex<Option<Listener>>>,
    error: Mutex<String>,
}

impl Share {
    pub fn new(queue_path: PathBuf) -> Self {
        let files_dir = queue_path.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| PathBuf::from("."));
        Self {
            queue: Arc::new(Mutex::new(load_queue(&queue_path))),
            remembered: AtomicU32::new(load_client_number(&files_dir)),
            files_dir,
            queue_path: Arc::new(Mutex::new(queue_path)),
            server: Mutex::new(None),
            client: Mutex::new(None),
            listen: Arc::new(Mutex::new(None)),
            error: Mutex::new(String::new()),
        }
    }

    pub fn set_listener(&self, listener: Listener) {
        *self.listen.lock().unwrap() = Some(listener);
    }

    pub fn is_client(&self) -> bool {
        self.client.lock().unwrap().is_some()
    }

    pub fn is_host(&self) -> bool {
        self.server.lock().unwrap().is_some()
    }

    /// `make` is called with the band table the listener will use, and may be
    /// called twice when the usual port is already taken.
    pub fn open_sharing(&self, make: &dyn Fn(Arc<Mutex<Bands>>) -> Arc<dyn HostOps>) -> Result<u16, String> {
        if self.is_client() {
            return Err("Leave the shared log before opening one on this PC".into());
        }
        let start = |port: u16| {
            let bands = Arc::new(Mutex::new(Bands::default()));
            self.start_host_on(port, make(Arc::clone(&bands)), bands)
        };
        match start(DEFAULT_PORT) {
            Ok(port) => Ok(port),
            Err(_) => start(0),
        }
    }

    pub fn stop_host(&self) {
        if let Some(mut server) = self.server.lock().unwrap().take() {
            server.shutdown();
        }
    }

    pub fn claim_host_band(&self, band: &str) -> Result<(), String> {
        let guard = self.server.lock().unwrap();
        let Some(server) = guard.as_ref() else { return Ok(()) };
        server.bands.lock().unwrap().claim_host(band)?;
        Ok(())
    }

    pub fn start_host_on(&self, port: u16, ops: Arc<dyn HostOps>, bands: Arc<Mutex<Bands>>) -> Result<u16, String> {
        if self.is_client() {
            return Err("Leave the shared log before opening one on this PC".into());
        }
        self.stop_host();
        let numbers = self.files_dir.join("client-numbers.json");
        let server = Server::bind(SocketAddr::from((Ipv4Addr::UNSPECIFIED, port)), ops, bands, Arc::clone(&self.listen), numbers)?;
        let bound = server.port;
        *self.error.lock().unwrap() = String::new();
        *self.server.lock().unwrap() = Some(server);
        Ok(bound)
    }

    pub fn publish(&self) {
        let (ops, bands, slots, listen) = {
            let guard = self.server.lock().unwrap();
            let Some(server) = guard.as_ref() else { return };
            (Arc::clone(&server.ops), Arc::clone(&server.bands), Arc::clone(&server.slots), Arc::clone(&server.listen))
        };
        broadcast(&ops, &bands, &slots, &listen);
    }

    /// The port the host is listening on. Tests use it to connect; the app uses the address from `status`.
    #[cfg(test)]
    pub fn port(&self) -> Option<u16> {
        self.server.lock().unwrap().as_ref().map(|s| s.port)
    }

    pub fn join(&self, addr: &str, operator: &str, band: &str) -> Result<(), String> {
        if self.is_host() {
            return Err("This PC has a log open, so it is the host. Close it before joining.".into());
        }
        if self.is_client() {
            return Err("Already joined to a log".into());
        }
        let addr = parse_addr(addr)?.to_string();
        let waiting = self.queue.lock().unwrap().iter().find(|q| q.host != addr).map(|q| q.host.clone());
        if let Some(other) = waiting {
            return Err(format!("Contacts are waiting for {other}. Join that host, or discard the held contacts."));
        }
        let (tx, rx) = mpsc::channel();
        let client = Client::start(
            addr,
            norm_op(operator),
            crate::bands::canonical(band),
            Arc::clone(&self.queue),
            Arc::clone(&self.queue_path),
            self.files_dir.clone(),
            Arc::clone(&self.listen),
            tx,
        );
        *self.client.lock().unwrap() = Some(client);
        match rx.recv_timeout(Duration::from_secs(8)) {
            Ok(Ok(())) => {
                self.remembered.store(load_client_number(&self.files_dir), Ordering::Relaxed);
                self.save_queue();
                Ok(())
            }
            Ok(Err(e)) => {
                self.leave();
                Err(e)
            }
            Err(_) => {
                self.leave();
                Err("The host didn't answer".into())
            }
        }
    }

    pub fn leave(&self) {
        if let Some(mut client) = self.client.lock().unwrap().take() {
            client.shutdown();
        }
    }

    pub fn submit(&self, submit: Submit) -> Result<Value, String> {
        let guard = self.client.lock().unwrap();
        let Some(client) = guard.as_ref() else { return Err("Not joined to a log".into()) };
        let tag = next_tag();
        let (msg, queueable) = match submit {
            Submit::Log(input) => (Msg::Log { tag, input }, true),
            Submit::Edit(qso) => (Msg::Edit { tag, qso }, true),
            Submit::Delete(id) => (Msg::Delete { tag, id }, true),
            Submit::Check(input) => (Msg::Check { tag, input }, false),
            Submit::Band(band) => (Msg::Band { tag, band }, true),
            Submit::Export { kind, from, to } => (Msg::Export { tag, kind, from, to }, false),
        };
        let result = client.submit(msg, queueable);
        drop(guard);
        self.save_queue();
        result
    }

    pub fn discard_queue(&self) {
        self.queue.lock().unwrap().clear();
        self.save_queue();
    }

    pub fn status(&self) -> MultiStatus {
        if let Some(client) = self.client.lock().unwrap().as_ref() {
            let queued = self.queue.lock().unwrap().iter().filter(|q| q.host == client.addr).count();
            return MultiStatus {
                role: if client.band.is_empty() { "scoreboard" } else { "client" }.into(),
                connected: client.connected.load(Ordering::Relaxed),
                address: client.addr.clone(),
                operator: client.operator.clone(),
                band: client.band.clone(),
                queued,
                queue_host: self.queue_host(),
                error: self.error.lock().unwrap().clone(),
                client: client.number.load(Ordering::Relaxed),
                ..MultiStatus::default()
            };
        }
        if let Some(server) = self.server.lock().unwrap().as_ref() {
            let (contest, call) = server.ops.label();
            let bands = server.bands.lock().unwrap();
            let address = host_address(server.port);
            return MultiStatus {
                role: "host".into(),
                connected: true,
                address,
                operator: call.clone(),
                band: bands.host_band(),
                stations: bands.stations(&call),
                taken: bands.taken_for(None),
                error: self.error.lock().unwrap().clone(),
                contest,
                queued: 0,
                queue_host: String::new(),
                client: 0,
            };
        }
        // Lock each mutex in its own statement. A guard created inside the
        // struct literal stays alive for the whole expression, so taking the
        // queue lock there and again in queue_host deadlocks the UI thread.
        let queued = self.queue.lock().unwrap().len();
        let queue_host = self.queue_host();
        let error = self.error.lock().unwrap().clone();
        let client = self.remembered.load(Ordering::Relaxed);
        MultiStatus {
            queued,
            queue_host,
            error,
            client,
            ..MultiStatus::default()
        }
    }

    pub fn set_error(&self, error: String) {
        *self.error.lock().unwrap() = error;
    }

    pub fn announce(&self) {
        let (listen, ops, bands, slots, port) = {
            let guard = self.server.lock().unwrap();
            let Some(server) = guard.as_ref() else { return };
            (
                Arc::clone(&server.listen),
                Arc::clone(&server.ops),
                Arc::clone(&server.bands),
                Arc::clone(&server.slots),
                server.port,
            )
        };
        publish_bands(&listen, &ops, &bands, &slots, port);
    }

    pub fn shutdown(&self) {
        self.leave();
        self.stop_host();
    }

    fn queue_host(&self) -> String {
        self.queue.lock().unwrap().first().map(|q| q.host.clone()).unwrap_or_default()
    }

    fn save_queue(&self) {
        let queue = self.queue.lock().unwrap();
        let path = self.queue_path.lock().unwrap().clone();
        if queue.is_empty() {
            let _ = std::fs::remove_file(&path);
            return;
        }
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(text) = serde_json::to_string(queue.as_slice()) {
            let _ = std::fs::write(&path, text);
        }
    }
}

fn load_queue(path: &std::path::Path) -> Vec<Queued> {
    std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex as StdMutex;

    struct Mem {
        log: StdMutex<Vec<QsoInput>>,
        fail: StdMutex<bool>,
    }

    impl HostOps for Mem {
        fn offer(&self) -> Offer {
            Offer { contest: "Test".into(), call: "VK3AAA".into(), bands: vec!["40m".into(), "20m".into()], stations: Vec::new() }
        }
        fn view(&self) -> Result<Value, String> {
            let qsos: Vec<Value> = self
                .log
                .lock()
                .unwrap()
                .iter()
                .map(|q| json!({"call": q.call, "band": q.band, "dupe": q.call == "DUPE"}))
                .collect();
            Ok(json!({"kind": "view", "view": {"qsos": qsos}}))
        }
        fn log(&self, _operator: &str, input: QsoInput) -> Result<Value, String> {
            if *self.fail.lock().unwrap() {
                return Err(format!("{} was refused", input.call));
            }
            self.log.lock().unwrap().push(input);
            self.view()
        }
        fn edit(&self, _: &str, _: u32, _: Qso) -> Result<Value, String> {
            Ok(json!({"kind": "view"}))
        }
        fn delete(&self, _: &str, _: u32, _: i64) -> Result<Value, String> {
            Ok(json!({"kind": "view"}))
        }
        fn check(&self, _: QsoInput) -> Result<Value, String> {
            Ok(json!({}))
        }
        fn export(&self, kind: &str, _: Option<String>, _: Option<String>) -> Result<Value, String> {
            Ok(json!({"text": kind, "count": 1}))
        }
        fn label(&self) -> (String, String) {
            ("Test".into(), "VK3AAA".into())
        }
    }

    #[test]
    fn bands_stay_taken_until_the_station_leaves() {
        let mut bands = Bands::default();
        bands.claim_host("40m").unwrap();
        assert!(bands.claim_client(1, "ZL2BRG", "40m").unwrap_err().contains("host"));
        let generation = bands.claim_client(1, "ZL2BRG", "20m").unwrap();
        assert!(bands.claim_client(2, "ZL1AA", "20m").unwrap_err().contains("ZL2BRG"));
        assert!(bands.claim_host("20m").unwrap_err().contains("ZL2BRG"));
        bands.disconnect_if(1, generation);
        assert!(bands.claim_client(2, "ZL1AA", "20m").unwrap_err().contains("ZL2BRG"));
        let again = bands.claim_client(1, "ZL2BRG", "20m").unwrap();
        bands.release_if(1, again);
        bands.claim_client(2, "ZL1AA", "20m").unwrap();
        assert!(bands.allow_client(9, "80m").is_err());
    }

    #[test]
    fn the_same_callsign_can_hold_two_bands() {
        let mut bands = Bands::default();
        bands.claim_client(1, "ZL2BRG", "2m").unwrap();
        bands.claim_client(2, "ZL2BRG", "70cm").unwrap();
        assert!(bands.claim_client(2, "ZL2BRG", "2m").unwrap_err().contains("ZL2BRG"));
        assert_eq!(bands.client_band(1).unwrap(), "2m");
        assert_eq!(bands.client_band(2).unwrap(), "70cm");
        let names: Vec<_> = bands.stations("ZL2BRG").into_iter().map(|s| (s.operator, s.band, s.client)).collect();
        assert!(names.contains(&("ZL2BRG".into(), "2m".into(), 1)));
        assert!(names.contains(&("ZL2BRG".into(), "70cm".into(), 2)));
    }

    #[test]
    fn a_joined_client_writes_and_a_dropped_link_keeps_the_band() {
        let mem = Arc::new(Mem { log: StdMutex::new(Vec::new()), fail: StdMutex::new(false) });
        let bands = Arc::new(Mutex::new(Bands::default()));
        let numbers = std::env::temp_dir().join(format!("openvkcl-numbers-{}.json", std::process::id()));
        let server = Server::bind(
            SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
            mem.clone() as Arc<dyn HostOps>,
            Arc::clone(&bands),
            Arc::new(Mutex::new(None)),
            numbers,
        )
        .unwrap();
        let port = server.port;
        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        stream.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
        stream.set_nodelay(true).unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let offer = read_msg(&mut reader).unwrap();
        assert!(matches!(offer, Msg::Offer { .. }));
        stream.write_all(b"{\"op\":\"join\",\"operator\":\"ZL2BRG\",\"band\":\"20m\"}\n").unwrap();
        assert!(matches!(skip_stations(&mut reader), Msg::Joined { .. }));
        stream.write_all(b"{\"op\":\"log\",\"tag\":\"1\",\"input\":{\"call\":\"VK2ABC\",\"band\":\"20m\",\"mode\":\"CW\"}}\n").unwrap();
        assert!(matches!(skip_stations(&mut reader), Msg::Ok { .. }));
        // The view push follows the reply.
        let _ = skip_stations(&mut reader);
        assert_eq!(mem.log.lock().unwrap().len(), 1);
        drop(stream);
        drop(reader);
        thread::sleep(Duration::from_millis(150));
        assert!(bands.lock().unwrap().claim_client(2, "ZL1AA", "20m").is_err());
    }

    #[test]
    fn a_host_band_change_reaches_the_client() {
        let mem = Arc::new(Mem { log: StdMutex::new(Vec::new()), fail: StdMutex::new(false) });
        let dir = std::env::temp_dir().join(format!("openvkcl-hostband-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let host = Share::new(dir.join("q.json"));
        let bands = Arc::new(Mutex::new(Bands::default()));
        let port = host.start_host_on(0, mem.clone() as Arc<dyn HostOps>, Arc::clone(&bands)).unwrap();
        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        stream.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
        stream.set_nodelay(true).unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let _ = read_msg(&mut reader).unwrap();
        stream.write_all(b"{\"op\":\"join\",\"operator\":\"ZL2BRG\",\"band\":\"20m\"}\n").unwrap();
        assert!(matches!(skip_stations(&mut reader), Msg::Joined { .. }));
        host.claim_host_band("40m").unwrap();
        host.announce();
        let mut saw = false;
        for _ in 0..6 {
            match read_msg(&mut reader) {
                Ok(Msg::Stations { stations }) if stations.iter().any(|s| s.host && s.band == "40m") => {
                    saw = true;
                    break;
                }
                Ok(_) => {}
                Err(_) => break,
            }
        }
        assert!(saw, "client was not told the host moved to 40m");
        drop(stream);
        host.shutdown();
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The next message that is not a band-table update.
    fn skip_stations(reader: &mut impl std::io::BufRead) -> Msg {
        loop {
            match read_msg(reader).unwrap() {
                Msg::Stations { .. } => {}
                other => return other,
            }
        }
    }

    #[test]
    fn a_queued_contact_is_sent_when_the_host_returns() {
        let mem = Arc::new(Mem { log: StdMutex::new(Vec::new()), fail: StdMutex::new(false) });
        let dir = std::env::temp_dir().join(format!("openvkcl-queue-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let host = Share::new(dir.join("q.json"));
        let bands = Arc::new(Mutex::new(Bands::default()));
        let port = host.start_host_on(0, mem.clone() as Arc<dyn HostOps>, Arc::clone(&bands)).unwrap();
        let client = Share::new(dir.join("c.json"));
        client.join(&format!("127.0.0.1:{port}"), "ZL2BRG", "20m").unwrap();
        host.stop_host();
        thread::sleep(Duration::from_millis(300));
        let queued = client.submit(Submit::Log(QsoInput { call: "VK2ABC".into(), band: "20m".into(), mode: "CW".into(), ..Default::default() })).unwrap();
        assert_eq!(queued["kind"], "queued");
        let again = host.start_host_on(port, mem.clone() as Arc<dyn HostOps>, bands).unwrap();
        assert_eq!(again, port);
        let deadline = Instant::now() + Duration::from_secs(6);
        while mem.log.lock().unwrap().is_empty() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(50));
        }
        assert_eq!(mem.log.lock().unwrap().first().map(|q| q.call.as_str()), Some("VK2ABC"), "queued contact was not written");
        client.shutdown();
        host.shutdown();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn beacon_text_carries_the_port_and_contest() {
        let found = parse_beacon("openvkcl\t7374\tJohn Moyle\tVK3AAA", IpAddr::V4(Ipv4Addr::new(192, 168, 1, 9))).unwrap();
        assert_eq!(found.addr, "192.168.1.9:7374");
        assert_eq!(found.contest, "John Moyle");
        assert_eq!(found.call, "VK3AAA");
        assert!(parse_beacon("not a beacon", IpAddr::V4(Ipv4Addr::LOCALHOST)).is_none());
    }

    #[test]
    fn a_second_radio_with_the_same_callsign_gets_its_own_number() {
        let path = std::env::temp_dir().join(format!("openvkcl-numbers-assign-{}.json", std::process::id()));
        let _ = std::fs::remove_file(&path);
        assert_eq!(resolve_client_number(&path, 0, false), 1);
        assert_eq!(resolve_client_number(&path, 0, false), 2);
        assert_eq!(resolve_client_number(&path, 1, false), 1);
        assert_eq!(resolve_client_number(&path, 1, true), 3);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_client_number_selects_its_queue_directory() {
        let dir = std::env::temp_dir().join(format!("openvkcl-client-dir-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = Arc::new(Mutex::new(dir.join("multi-queue.json")));
        let queue = Mutex::new(vec![Queued { host: "h".into(), tag: "1".into(), line: "held".into() }]);
        adopt_client_dir(&dir, &path, &queue, 2);
        assert_eq!(*path.lock().unwrap(), dir.join("clients/2/multi-queue.json"));
        assert_eq!(load_queue(&dir.join("clients/2/multi-queue.json")).len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_refused_queued_contact_is_reported() {
        let mem = Arc::new(Mem { log: StdMutex::new(Vec::new()), fail: StdMutex::new(true) });
        let body = mem.log("ZL2BRG", QsoInput { call: "VK2BAD".into(), ..Default::default() });
        assert!(body.unwrap_err().contains("refused"));
        let logged = json!({"kind": "view", "view": {"qsos": [{"call": "DUPE", "dupe": true}]}});
        assert_eq!(dupe_call(&logged).as_deref(), Some("DUPE"));
    }
}
