//! Live scene server: the `utsim-scene/1` stream over plain TCP and WebSocket at once.
//!
//! One listening port serves both transports. A client that starts with an HTTP `GET`
//! (a browser, a WebSocket library) is upgraded to WebSocket and receives one JSON per
//! text message; any other client is plain TCP and receives one JSON per line, the same
//! bytes as `utsim cena` writes to a file. Either way the first message is the
//! [`Header`], then frames, then maybe extension messages ([`ServerMessage`]). Clients
//! may send [`ClientMessage`]s back, one JSON per line (TCP) or per text message (WS).
//!
//! Two layers:
//! - [`SceneServer`]: the transport. A [`SceneSink`] that fans every message out to the
//!   connected clients and collects their commands.
//! - [`LiveServer`]: a [`LiveScene`] plus a ticker thread that renders a frame of the
//!   running simulation at a fixed wall-clock rate and gives it to a [`SceneServer`].
//!
//! # Threads
//! - the simulation thread only calls [`LiveServer::push`] (a mutex-protected `Vec`
//!   append and an atomic store);
//! - one **ticker** thread rebuilds the scene index when records arrived, renders the
//!   frame and serialises it once;
//! - one **acceptor** thread, and one thread **per client** that writes its messages and
//!   polls its commands.
//!
//! # Back-pressure
//! Every client has a bounded queue ([`QUEUE`] messages). The producer never waits: if a
//! client's queue is full the message is *dropped for that client* (counted in
//! [`SceneServer::dropped`]), so a slow or stuck client sees a lower frame rate and a
//! jumpy view while the simulation and the other clients are unaffected. A client whose
//! socket does not accept data for [`WRITE_TIMEOUT`] is disconnected.
//!
//! # Security
//! There is no authentication and no encryption, and the WebSocket `Origin` is not
//! checked: whoever can reach the port can watch the plant **and stop its blocks**. Bind
//! to the loopback address (the default of the command line) or to a trusted local
//! network only.

use std::io::{self, Read};
use std::net::{SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use tungstenite::protocol::WebSocketConfig;
use tungstenite::{Message, WebSocket};
use utsim_core::TraceRecord;
use utsim_model::Model;

use crate::live::LiveScene;
use crate::{ClientMessage, CommandValue, Frame, Header, SceneSink, ServerMessage};

/// Messages a client may have queued before new ones are dropped for it.
pub const QUEUE: usize = 4;
/// A client whose socket accepts no data for this long is disconnected.
pub const WRITE_TIMEOUT: Duration = Duration::from_secs(1);
/// Most simultaneous clients; further connections are closed at once.
pub const MAX_CLIENTS: usize = 64;
/// Longest message a client may send, in bytes.
const MAX_INCOMING: usize = 64 * 1024;
/// How long a new connection may stay silent before it is taken for plain TCP.
const SNIFF: Duration = Duration::from_millis(300);
/// How long a WebSocket handshake may take.
const HANDSHAKE: Duration = Duration::from_secs(5);

/// A command received from a client.
#[derive(Clone, Debug, PartialEq)]
pub struct ReceivedCommand {
    /// Which client sent it (answer with [`SceneServer::send_to`]).
    pub client: u64,
    pub variable: String,
    pub value: CommandValue,
}

struct ClientHandle {
    id: u64,
    tx: SyncSender<Arc<str>>,
}

struct Shared {
    /// Serialised header, sent first to every new client.
    header: Mutex<Arc<str>>,
    clients: Mutex<Vec<ClientHandle>>,
    commands: Mutex<Vec<ReceivedCommand>>,
    next_id: AtomicU64,
    dropped: AtomicU64,
    stop_accepting: AtomicBool,
}

/// The transport. Dropping it stops accepting, lets every client drain its queue and
/// closes the connections.
pub struct SceneServer {
    shared: Arc<Shared>,
    addr: SocketAddr,
    acceptor: Option<JoinHandle<()>>,
    workers: Arc<Mutex<Vec<JoinHandle<()>>>>,
}

fn to_json<T: serde::Serialize>(value: &T) -> io::Result<Arc<str>> {
    serde_json::to_string(value)
        .map(Arc::from)
        .map_err(io::Error::from)
}

impl SceneServer {
    /// Starts listening on `addr` (port 0 picks a free one, see [`Self::local_addr`]).
    /// `header` is what every client receives first.
    pub fn bind(addr: impl ToSocketAddrs, header: &Header) -> io::Result<SceneServer> {
        let listener = TcpListener::bind(addr)?;
        listener.set_nonblocking(true)?;
        let addr = listener.local_addr()?;
        let shared = Arc::new(Shared {
            header: Mutex::new(to_json(header)?),
            clients: Mutex::new(Vec::new()),
            commands: Mutex::new(Vec::new()),
            next_id: AtomicU64::new(1),
            dropped: AtomicU64::new(0),
            stop_accepting: AtomicBool::new(false),
        });
        let workers = Arc::new(Mutex::new(Vec::new()));
        let acceptor = {
            let (shared, workers) = (shared.clone(), workers.clone());
            thread::Builder::new()
                .name("cena-aceita".into())
                .spawn(move || accept_loop(listener, shared, workers))?
        };
        Ok(SceneServer {
            shared,
            addr,
            acceptor: Some(acceptor),
            workers,
        })
    }

    /// The address the server listens on.
    pub fn local_addr(&self) -> SocketAddr {
        self.addr
    }

    /// Clients connected right now.
    pub fn clients(&self) -> usize {
        self.shared.clients.lock().expect("clients").len()
    }

    /// Messages dropped so far because a client could not keep up.
    pub fn dropped(&self) -> u64 {
        self.shared.dropped.load(Ordering::Relaxed)
    }

    /// Queues `line` for every client; a full queue drops it for that client only, a
    /// closed one removes the client.
    pub fn broadcast(&self, line: Arc<str>) {
        let mut clients = self.shared.clients.lock().expect("clients");
        clients.retain(|c| match c.tx.try_send(line.clone()) {
            Ok(()) => true,
            Err(TrySendError::Full(_)) => {
                self.shared.dropped.fetch_add(1, Ordering::Relaxed);
                true
            }
            Err(TrySendError::Disconnected(_)) => false,
        });
    }

    /// Sends an extension message to one client (dropped if its queue is full).
    pub fn send_to(&self, client: u64, message: &ServerMessage) {
        let Ok(line) = to_json(message) else { return };
        let clients = self.shared.clients.lock().expect("clients");
        if let Some(c) = clients.iter().find(|c| c.id == client) {
            let _ = c.tx.try_send(line);
        }
    }

    /// Sends an extension message to every client.
    pub fn send_all(&self, message: &ServerMessage) {
        if let Ok(line) = to_json(message) {
            self.broadcast(line);
        }
    }

    /// Commands received from clients since the last call, in arrival order.
    pub fn take_commands(&self) -> Vec<ReceivedCommand> {
        std::mem::take(&mut *self.shared.commands.lock().expect("commands"))
    }
}

impl SceneSink for SceneServer {
    /// Sets the header of the clients that connect from now on and sends it again to
    /// those already connected (a new run starts).
    fn header(&mut self, header: &Header) -> io::Result<()> {
        let line = to_json(header)?;
        *self.shared.header.lock().expect("header") = line.clone();
        self.broadcast(line);
        Ok(())
    }

    fn frame(&mut self, frame: &Frame) -> io::Result<()> {
        self.broadcast(to_json(frame)?);
        Ok(())
    }
}

impl Drop for SceneServer {
    fn drop(&mut self) {
        self.shared.stop_accepting.store(true, Ordering::Relaxed);
        if let Some(h) = self.acceptor.take() {
            let _ = h.join();
        }
        // Dropping the senders lets each client thread send what is queued and leave.
        self.shared.clients.lock().expect("clients").clear();
        let workers = std::mem::take(&mut *self.workers.lock().expect("workers"));
        for w in workers {
            let _ = w.join();
        }
    }
}

fn accept_loop(
    listener: TcpListener,
    shared: Arc<Shared>,
    workers: Arc<Mutex<Vec<JoinHandle<()>>>>,
) {
    while !shared.stop_accepting.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((stream, _)) => {
                let mut list = workers.lock().expect("workers");
                list.retain(|w| !w.is_finished());
                if list.len() >= MAX_CLIENTS {
                    continue; // dropping the stream closes it
                }
                let shared = shared.clone();
                if let Ok(h) = thread::Builder::new()
                    .name("cena-cliente".into())
                    .spawn(move || serve_client(stream, shared))
                {
                    list.push(h);
                }
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(15));
            }
            Err(_) => thread::sleep(Duration::from_millis(100)),
        }
    }
}

/// One connection, either transport.
enum Conn {
    Tcp { stream: TcpStream, inbuf: Vec<u8> },
    Ws(Box<WebSocket<TcpStream>>),
}

fn is_timeout(e: &io::Error) -> bool {
    matches!(
        e.kind(),
        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut | io::ErrorKind::Interrupted
    )
}

impl Conn {
    /// Sends one message. Any error (including a write timeout) ends the client.
    fn send(&mut self, line: &str) -> Result<(), ()> {
        use io::Write;
        match self {
            Conn::Tcp { stream, .. } => {
                let mut buf = Vec::with_capacity(line.len() + 1);
                buf.extend_from_slice(line.as_bytes());
                buf.push(b'\n');
                stream.write_all(&buf).map_err(|_| ())
            }
            Conn::Ws(ws) => ws.send(Message::text(line)).map_err(|_| ()),
        }
    }

    /// Reads what the client sent, appending complete messages to `out`. `Err` = closed.
    fn receive(&mut self, out: &mut Vec<String>) -> Result<(), ()> {
        match self {
            Conn::Tcp { stream, inbuf } => {
                let mut chunk = [0u8; 2048];
                match stream.read(&mut chunk) {
                    Ok(0) => return Err(()),
                    Ok(n) => inbuf.extend_from_slice(&chunk[..n]),
                    Err(e) if is_timeout(&e) => {}
                    Err(_) => return Err(()),
                }
                while let Some(pos) = inbuf.iter().position(|&b| b == b'\n') {
                    let rest = inbuf.split_off(pos + 1);
                    let line = String::from_utf8_lossy(&inbuf[..pos]).trim().to_string();
                    *inbuf = rest;
                    if !line.is_empty() {
                        out.push(line);
                    }
                }
                if inbuf.len() > MAX_INCOMING {
                    return Err(());
                }
                Ok(())
            }
            Conn::Ws(ws) => match ws.read() {
                Ok(Message::Text(t)) => {
                    out.push(t.as_str().to_string());
                    Ok(())
                }
                Ok(Message::Close(_)) => Err(()),
                // Pings are answered by the library; binary and pongs are ignored.
                Ok(_) => {
                    let _ = ws.flush();
                    Ok(())
                }
                Err(tungstenite::Error::Io(e)) if is_timeout(&e) => Ok(()),
                Err(_) => Err(()),
            },
        }
    }
}

/// Decides the transport of a new connection and, for WebSocket, shakes hands.
fn open(stream: TcpStream) -> Option<Conn> {
    // Some platforms hand out accepted sockets in the listener's non-blocking mode.
    stream.set_nonblocking(false).ok()?;
    stream.set_nodelay(true).ok()?;
    stream.set_write_timeout(Some(WRITE_TIMEOUT)).ok()?;
    // A WebSocket client speaks first ("GET "); a plain TCP client may stay silent.
    stream.set_read_timeout(Some(SNIFF)).ok()?;
    let mut head = [0u8; 4];
    let deadline = Instant::now() + SNIFF;
    let is_ws = loop {
        match stream.peek(&mut head) {
            Ok(0) => return None,
            Ok(n) if n >= 3 && head[..n].starts_with(b"GET") => break true,
            Ok(n) if n >= 1 && head[0] != b'G' => break false,
            Ok(_) if Instant::now() < deadline => thread::sleep(Duration::from_millis(5)),
            Ok(_) => break false,
            Err(e) if is_timeout(&e) => break false,
            Err(_) => return None,
        }
    };
    if is_ws {
        stream.set_read_timeout(Some(HANDSHAKE)).ok()?;
        let config = WebSocketConfig::default()
            .max_message_size(Some(MAX_INCOMING))
            .max_frame_size(Some(MAX_INCOMING));
        let ws = tungstenite::accept_with_config(stream, Some(config)).ok()?;
        ws.get_ref()
            .set_read_timeout(Some(Duration::from_millis(2)))
            .ok()?;
        Some(Conn::Ws(Box::new(ws)))
    } else {
        stream
            .set_read_timeout(Some(Duration::from_millis(2)))
            .ok()?;
        Some(Conn::Tcp {
            stream,
            inbuf: Vec::new(),
        })
    }
}

fn serve_client(stream: TcpStream, shared: Arc<Shared>) {
    let Some(mut conn) = open(stream) else { return };
    let id = shared.next_id.fetch_add(1, Ordering::Relaxed);
    let (tx, rx) = mpsc::sync_channel::<Arc<str>>(QUEUE);
    // Registered before the header is read: a header change in between is broadcast to
    // this queue and arrives right after (a repeated header is harmless).
    shared
        .clients
        .lock()
        .expect("clients")
        .push(ClientHandle { id, tx });
    let header = shared.header.lock().expect("header").clone();
    if conn.send(&header).is_err() {
        remove(&shared, id);
        return;
    }
    let mut incoming = Vec::new();
    loop {
        match rx.recv_timeout(Duration::from_millis(10)) {
            Ok(line) => {
                if conn.send(&line).is_err() {
                    break;
                }
                // Send what else is waiting before looking at the input.
                while let Ok(line) = rx.try_recv() {
                    if conn.send(&line).is_err() {
                        remove(&shared, id);
                        return;
                    }
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
        incoming.clear();
        if conn.receive(&mut incoming).is_err() {
            break;
        }
        for text in incoming.drain(..) {
            handle_incoming(&shared, id, &text);
        }
    }
    remove(&shared, id);
}

fn remove(shared: &Shared, id: u64) {
    shared
        .clients
        .lock()
        .expect("clients")
        .retain(|c| c.id != id);
}

/// Parses one message from a client. Unknown `"tipo"` values are ignored by design
/// (forward compatibility); broken JSON gets a warning back.
fn handle_incoming(shared: &Shared, id: u64, text: &str) {
    let warn = |msg: String| {
        if let Ok(line) = to_json(&ServerMessage::Aviso { mensagem: msg }) {
            let clients = shared.clients.lock().expect("clients");
            if let Some(c) = clients.iter().find(|c| c.id == id) {
                let _ = c.tx.try_send(line);
            }
        }
    };
    let value: serde_json::Value = match serde_json::from_str(text) {
        Ok(v) => v,
        Err(e) => return warn(format!("mensagem não é JSON: {e}")),
    };
    if value.get("tipo").and_then(|t| t.as_str()) != Some("comando") {
        return;
    }
    match serde_json::from_value::<ClientMessage>(value) {
        Ok(ClientMessage::Comando { variavel, valor }) => {
            shared
                .commands
                .lock()
                .expect("commands")
                .push(ReceivedCommand {
                    client: id,
                    variable: variavel,
                    value: valor,
                });
        }
        Err(e) => warn(format!("comando inválido: {e}")),
    }
}

// ---------------------------------------------------------------------------------
// Live server
// ---------------------------------------------------------------------------------

struct LiveShared {
    /// Records not yet handed to the scene.
    pending: Mutex<Vec<TraceRecord>>,
    /// Simulated time, as `f64` bits.
    time: AtomicU64,
    stop: AtomicBool,
}

/// A [`SceneServer`] fed by a running simulation. See the module docs.
pub struct LiveServer {
    server: Arc<SceneServer>,
    shared: Arc<LiveShared>,
    ticker: Mutex<Option<JoinHandle<()>>>,
}

impl LiveServer {
    /// Starts serving the scene of `model` (validated) on `addr`, rendering `fps` frames
    /// per wall-clock second while at least one client is connected.
    pub fn start(
        addr: impl ToSocketAddrs,
        model: &Model,
        replication: u32,
        fps: f64,
    ) -> io::Result<LiveServer> {
        if !(fps.is_finite() && fps > 0.0) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "fps deve ser positivo",
            ));
        }
        let mut scene = LiveScene::new(model, replication);
        let server = Arc::new(SceneServer::bind(addr, &scene.header())?);
        let shared = Arc::new(LiveShared {
            pending: Mutex::new(Vec::new()),
            time: AtomicU64::new(0f64.to_bits()),
            stop: AtomicBool::new(false),
        });
        let period = Duration::from_secs_f64(1.0 / fps);
        let ticker = {
            let (server, shared) = (server.clone(), shared.clone());
            thread::Builder::new()
                .name("cena-quadros".into())
                .spawn(move || tick_loop(scene, &server, &shared, period))?
        };
        Ok(LiveServer {
            server,
            shared,
            ticker: Mutex::new(Some(ticker)),
        })
    }

    pub fn local_addr(&self) -> SocketAddr {
        self.server.local_addr()
    }

    /// The transport (client count, drop counter, commands).
    pub fn server(&self) -> &SceneServer {
        &self.server
    }

    /// Called by the simulation thread after advancing to `time`: the records produced
    /// since the last call. Never waits for a client.
    pub fn push(&self, time: f64, records: Vec<TraceRecord>) {
        if !records.is_empty() {
            self.shared.pending.lock().expect("pending").extend(records);
        }
        // After the records: a frame rendered for `time` already knows them.
        self.shared.time.store(time.to_bits(), Ordering::Release);
    }

    /// Stops rendering after one last frame, then tells the clients the run ended at
    /// `time`. Queued messages are still delivered before the connections close.
    pub fn finish(&self, time: f64) {
        self.stop_ticker();
        self.server.send_all(&ServerMessage::Fim { tempo: time });
    }

    fn stop_ticker(&self) {
        self.shared.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.ticker.lock().expect("ticker").take() {
            let _ = h.join();
        }
    }
}

impl Drop for LiveServer {
    fn drop(&mut self) {
        self.stop_ticker();
    }
}

fn tick_loop(mut scene: LiveScene, server: &SceneServer, shared: &LiveShared, period: Duration) {
    let mut next = Instant::now();
    loop {
        // One more pass after the stop request, so the last frame shows the end state.
        let last = shared.stop.load(Ordering::Relaxed);
        next += period;
        let now = Instant::now();
        if next > now {
            if !last {
                thread::sleep(next - now);
            }
        } else {
            // Rendering is slower than the period: do not try to catch up.
            next = now;
        }
        let time = f64::from_bits(shared.time.load(Ordering::Acquire));
        let records = std::mem::take(&mut *shared.pending.lock().expect("pending"));
        scene.push(records);
        // Without viewers the index is not rebuilt (the records are kept for whoever
        // connects later).
        if server.clients() > 0 {
            if let Ok(line) = to_json(&scene.frame(time)) {
                server.broadcast(line);
            }
        }
        if last {
            break;
        }
    }
}
