use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rustls::{
    pki_types::{CertificateDer, PrivatePkcs8KeyDer, ServerName},
    ClientConfig, ClientConnection, RootCertStore, ServerConfig, ServerConnection, StreamOwned,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, VecDeque},
    io::{Read, Write},
    net::{Ipv4Addr, Shutdown, SocketAddr, TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
const MAX_PACKET: usize = 512 * 1024;
pub type Changed = Arc<dyn Fn() + Send + Sync>;
pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
pub fn random() -> Result<String, String> {
    let mut b = [0; 24];
    getrandom::fill(&mut b).map_err(|e| e.to_string())?;
    Ok(URL_SAFE_NO_PAD.encode(b))
}
fn error(e: impl std::fmt::Display) -> String {
    e.to_string()
}
fn clean(s: &str, max: usize) -> String {
    s.chars()
        .filter(|c| !c.is_control() || *c == '\n')
        .take(max)
        .collect()
}
pub fn host_ip() -> String {
    std::net::UdpSocket::bind("0.0.0.0:0")
        .and_then(|s| {
            s.connect("192.0.0.1:9")?;
            s.local_addr()
        })
        .map(|a| a.ip().to_string())
        .unwrap_or_else(|_| "127.0.0.1".into())
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Invite {
    pub v: u8,
    pub host: String,
    pub port: u16,
    pub cert: String,
    pub token: String,
}
impl Invite {
    pub fn encrypt(&self, password: &str) -> Result<String, String> {
        crate::invitation::encrypt(&serde_json::to_vec(self).map_err(error)?, password)
    }
    pub fn parse_protected(code: &str, password: &str) -> Result<Self, String> {
        let plain = crate::invitation::decrypt(code, password)?;
        let i: Self = serde_json::from_slice(&plain).map_err(|_| "Повреждено приглашение")?;
        if i.v != 2 || i.token.len() != 32 || i.port == 0 || i.host.parse::<Ipv4Addr>().is_err() {
            return Err("Неверный код подключения".into());
        }
        Ok(i)
    }
    pub fn encode(&self) -> String {
        format!(
            "OV2-{}",
            URL_SAFE_NO_PAD.encode(serde_json::to_vec(self).unwrap())
        )
    }
    pub fn parse(code: &str) -> Result<Self, String> {
        if code.len() > 12000 {
            return Err("Код слишком длинный".into());
        }
        let bytes = URL_SAFE_NO_PAD
            .decode(
                code.trim()
                    .strip_prefix("OV2-")
                    .ok_or("Нужен код OV2 из окна учителя")?,
            )
            .map_err(|_| "Повреждён код подключения")?;
        let i: Self = serde_json::from_slice(&bytes).map_err(|_| "Повреждён код подключения")?;
        if i.v != 2 || i.token.len() != 32 || i.port == 0 || i.host.parse::<Ipv4Addr>().is_err() {
            return Err("Неверный код подключения".into());
        }
        Ok(i)
    }
    fn config(&self) -> Result<Arc<ClientConfig>, String> {
        let mut roots = RootCertStore::empty();
        roots
            .add(CertificateDer::from(
                URL_SAFE_NO_PAD.decode(&self.cert).map_err(error)?,
            ))
            .map_err(error)?;
        Ok(Arc::new(
            ClientConfig::builder()
                .with_root_certificates(roots)
                .with_no_client_auth(),
        ))
    }
}
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Frame {
    pub seq: u64,
    pub data: Arc<str>,
}
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Message {
    pub at: u64,
    pub from: String,
    pub text: String,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Poll {
    pub token: String,
    pub id: String,
    pub name: String,
    pub machine: String,
    pub help: bool,
    pub sharing: bool,
    pub frame: Option<Frame>,
    pub note: String,
    #[serde(default)]
    pub answer: Option<crate::lesson::Submission>,
    #[serde(default)]
    pub monitoring: bool,
    #[serde(default)]
    pub monitor_generation: u64,
    #[serde(default)]
    pub detections: Vec<crate::assessment::Detection>,
    #[serde(default)]
    pub monitor_error: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Update {
    pub ok: bool,
    pub error: String,
    pub messages: Vec<Message>,
    pub help_ack: bool,
    pub broadcasting: bool,
    pub broadcast: Option<Frame>,
    #[serde(default)]
    pub question: Option<crate::lesson::StudentQuestion>,
    #[serde(default)]
    pub policy: crate::assessment::Policy,
}
impl Update {
    fn rejected() -> Self {
        Self {
            ok: false,
            error: "Сеанс завершён или код неверен".into(),
            messages: vec![],
            help_ack: false,
            broadcasting: false,
            broadcast: None,
            question: None,
            policy: crate::assessment::Policy::default(),
        }
    }
}
pub fn write_packet(s: &mut impl Write, v: &impl Serialize) -> Result<(), String> {
    let b = serde_json::to_vec(v).map_err(error)?;
    if b.len() > MAX_PACKET {
        return Err("Пакет слишком большой".into());
    }
    s.write_all(&(b.len() as u32).to_be_bytes())
        .map_err(error)?;
    s.write_all(&b).map_err(error)?;
    s.flush().map_err(error)
}
pub fn read_packet<T: serde::de::DeserializeOwned>(s: &mut impl Read) -> Result<T, String> {
    let mut n = [0; 4];
    s.read_exact(&mut n).map_err(error)?;
    let n = u32::from_be_bytes(n) as usize;
    if n == 0 || n > MAX_PACKET {
        return Err("Недопустимый размер пакета".into());
    }
    let mut b = vec![0; n];
    s.read_exact(&mut b).map_err(error)?;
    serde_json::from_slice(&b).map_err(error)
}
fn tls_client(
    i: &Invite,
    config: Arc<ClientConfig>,
) -> Result<StreamOwned<ClientConnection, TcpStream>, String> {
    let addr = SocketAddr::new(i.host.parse::<Ipv4Addr>().map_err(error)?.into(), i.port);
    let sock = TcpStream::connect_timeout(&addr, Duration::from_secs(4)).map_err(error)?;
    sock.set_read_timeout(Some(Duration::from_secs(6)))
        .map_err(error)?;
    sock.set_write_timeout(Some(Duration::from_secs(6)))
        .map_err(error)?;
    sock.set_nodelay(true).map_err(error)?;
    let conn = ClientConnection::new(
        config,
        ServerName::try_from("omniview.local").unwrap().to_owned(),
    )
    .map_err(error)?;
    Ok(StreamOwned::new(conn, sock))
}
#[derive(Clone, Serialize)]
pub struct Peer {
    pub monitoring: bool,
    pub monitor_error: Option<String>,
    pub id: String,
    pub name: String,
    pub machine: String,
    pub online: bool,
    pub help: bool,
    pub sharing: bool,
    pub frame: Option<Frame>,
    pub last_seen: u64,
    #[serde(skip)]
    last: Option<Instant>,
    #[serde(skip)]
    owner: String,
    #[serde(skip)]
    queue: VecDeque<Message>,
    #[serde(skip)]
    ack: bool,
}
pub struct Teacher {
    pub assessment: Mutex<crate::assessment::Assessment>,
    pub lesson: Mutex<crate::lesson::Lesson>,
    pub invite: Invite,
    pub stop: Arc<AtomicBool>,
    pub peers: Mutex<HashMap<String, Peer>>,
    pub log: Mutex<VecDeque<Message>>,
    pub broadcasting: AtomicBool,
    pub broadcast: Mutex<Option<Frame>>,
    pub low: AtomicBool,
    pub changed: Changed,
    connections: AtomicUsize,
}
impl Teacher {
    pub fn start(host: String, low: bool, changed: Changed) -> Result<Arc<Self>, String> {
        host.parse::<Ipv4Addr>()
            .map_err(|_| "Укажи локальный IPv4-адрес учителя")?;
        let cert =
            rcgen::generate_simple_self_signed(vec!["omniview.local".into()]).map_err(error)?;
        let key = PrivatePkcs8KeyDer::from(cert.signing_key.serialize_der());
        let config = Arc::new(
            ServerConfig::builder()
                .with_no_client_auth()
                .with_single_cert(vec![cert.cert.der().clone()], key.into())
                .map_err(error)?,
        );
        let listener = TcpListener::bind((Ipv4Addr::UNSPECIFIED, 0)).map_err(error)?;
        listener.set_nonblocking(true).map_err(error)?;
        let invite = Invite {
            v: 2,
            host,
            port: listener.local_addr().map_err(error)?.port(),
            cert: URL_SAFE_NO_PAD.encode(cert.cert.der()),
            token: random()?,
        };
        let t = Arc::new(Self {
            assessment: Mutex::new(crate::assessment::Assessment::default()),
            lesson: Mutex::new(crate::lesson::Lesson::default()),
            invite,
            stop: Arc::new(AtomicBool::new(false)),
            peers: Mutex::new(HashMap::new()),
            log: Mutex::new(VecDeque::new()),
            broadcasting: AtomicBool::new(false),
            broadcast: Mutex::new(None),
            low: AtomicBool::new(low),
            changed,
            connections: AtomicUsize::new(0),
        });
        let listener_t = t.clone();
        std::thread::spawn(move || {
            while !listener_t.stop.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((sock, _)) => {
                        if listener_t.connections.load(Ordering::Relaxed) >= 48 {
                            let _ = sock.shutdown(Shutdown::Both);
                            continue;
                        }
                        listener_t.connections.fetch_add(1, Ordering::Relaxed);
                        let t = listener_t.clone();
                        let c = config.clone();
                        std::thread::spawn(move || {
                            t.serve(sock, c);
                            t.connections.fetch_sub(1, Ordering::Relaxed);
                        });
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(100))
                    }
                    Err(_) => break,
                }
            }
        });
        let capture_t = t.clone();
        std::thread::spawn(move || {
            let mut last = Instant::now() - Duration::from_secs(10);
            while !capture_t.stop.load(Ordering::Relaxed) {
                if capture_t.broadcasting.load(Ordering::Relaxed)
                    && !capture_t.peers.lock().unwrap().is_empty()
                {
                    let low = capture_t.low.load(Ordering::Relaxed);
                    if last.elapsed() >= Duration::from_millis(if low { 5000 } else { 250 }) {
                        if let Ok(data) = crate::capture::screen(if low { 640 } else { 1280 }) {
                            let mut frame = capture_t.broadcast.lock().unwrap();
                            if capture_t.broadcasting.load(Ordering::Relaxed) {
                                *frame = Some(Frame {
                                    seq: now(),
                                    data: data.into(),
                                });
                            }
                            (capture_t.changed)();
                        }
                        last = Instant::now();
                    }
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        });
        Ok(t)
    }
    fn serve(&self, sock: TcpStream, config: Arc<ServerConfig>) {
        let _ = sock.set_nonblocking(false);
        let _ = sock.set_read_timeout(Some(Duration::from_secs(7)));
        let _ = sock.set_write_timeout(Some(Duration::from_secs(7)));
        let Ok(conn) = ServerConnection::new(config) else {
            return;
        };
        let mut stream = StreamOwned::new(conn, sock);
        let owner = random().unwrap_or_default();
        let mut bound = String::new();
        let mut last_broadcast = 0;
        while !self.stop.load(Ordering::Relaxed) {
            let poll = match read_packet::<Poll>(&mut stream) {
                Ok(p) => p,
                Err(_e) => {
                    #[cfg(test)]
                    eprintln!("peer read: {_e}");
                    break;
                }
            };
            if self.stop.load(Ordering::Relaxed) {
                break;
            }
            if poll.token != self.invite.token
                || poll.id.len() != 32
                || poll.frame.as_ref().is_some_and(|f| f.data.len() > 300_000)
                || poll.note.chars().count() > 1000
            {
                let _ = write_packet(&mut stream, &Update::rejected());
                break;
            }
            if !bound.is_empty() && bound != poll.id {
                break;
            }
            let mut peers = self.peers.lock().unwrap();
            if !peers.contains_key(&poll.id) && peers.len() >= 40 {
                let _ = write_packet(&mut stream, &Update::rejected());
                break;
            }
            if peers
                .get(&poll.id)
                .is_some_and(|p| p.online && p.owner != owner)
            {
                break;
            }
            bound = poll.id.clone();
            let p = peers.entry(poll.id.clone()).or_insert_with(|| Peer {
                monitoring: false,
                monitor_error: None,
                id: poll.id.clone(),
                name: String::new(),
                machine: String::new(),
                online: true,
                help: false,
                sharing: false,
                frame: None,
                last_seen: 0,
                last: None,
                owner: owner.clone(),
                queue: VecDeque::new(),
                ack: false,
            });
            p.owner = owner.clone();
            p.monitoring = poll.monitoring;
            p.monitor_error = poll.monitor_error.as_deref().map(|s| clean(s, 120));
            p.name = clean(&poll.name, 64);
            p.machine = clean(&poll.machine, 64);
            p.online = true;
            p.help = poll.help;
            p.sharing = poll.sharing;
            p.last = Some(Instant::now());
            p.last_seen = now();
            if poll.sharing {
                if let Some(f) = poll.frame {
                    p.frame = Some(f);
                }
            } else {
                p.frame = None;
            }
            let messages = p.queue.drain(..).collect();
            let ack = std::mem::take(&mut p.ack);
            if !poll.note.trim().is_empty() {
                self.log_message(&p.name, &poll.note);
            }
            drop(peers);
            let policy = {
                let mut assessment = self.assessment.lock().unwrap();
                if poll.monitor_generation == assessment.policy.generation {
                    assessment.record(
                        &poll.id,
                        &poll.name,
                        poll.monitoring,
                        &poll.detections,
                        now(),
                    );
                }
                assessment.policy.clone()
            };
            let question = {
                let mut lesson = self.lesson.lock().unwrap();
                if let Some(answer) = &poll.answer {
                    lesson.submit(&poll.id, answer, now());
                }
                lesson.for_student(&poll.id, now())
            };
            let broadcasting = self.broadcasting.load(Ordering::Relaxed);
            let broadcast = if broadcasting {
                self.broadcast
                    .lock()
                    .unwrap()
                    .as_ref()
                    .filter(|f| f.seq != last_broadcast)
                    .cloned()
            } else {
                None
            };
            if let Some(f) = &broadcast {
                last_broadcast = f.seq;
            }
            if write_packet(
                &mut stream,
                &Update {
                    ok: true,
                    error: String::new(),
                    messages,
                    help_ack: ack,
                    broadcasting,
                    broadcast,
                    question,
                    policy,
                },
            )
            .is_err()
            {
                break;
            }
            (self.changed)();
        }
        stream.conn.send_close_notify();
        let _ = stream.flush();
        let _ = stream.sock.shutdown(Shutdown::Write);
        if let Some(p) = self.peers.lock().unwrap().get_mut(&bound) {
            if p.owner == owner {
                p.online = false;
                p.frame = None;
                p.sharing = false;
            }
        }
        (self.changed)();
    }
    pub fn list(&self) -> Vec<Peer> {
        let mut ps: Vec<_> = self.peers.lock().unwrap().values().cloned().collect();
        for p in &mut ps {
            p.online = p.online && p.last.is_some_and(|l| l.elapsed() < Duration::from_secs(9));
            if !p.online {
                p.frame = None;
                p.sharing = false;
            }
        }
        ps.sort_by(|a, b| a.name.cmp(&b.name));
        ps
    }
    pub fn send(&self, target: &str, text: &str) -> Result<usize, String> {
        let text = clean(text, 1000);
        if text.trim().is_empty() {
            return Err("Напиши сообщение".into());
        }
        let m = Message {
            at: now(),
            from: "Учитель".into(),
            text: text.clone(),
        };
        let mut count = 0;
        for p in self.peers.lock().unwrap().values_mut() {
            if (target == "all" || target == p.id) && p.online {
                if p.queue.len() >= 32 {
                    return Err("Очередь сообщений заполнена".into());
                }
                p.queue.push_back(m.clone());
                count += 1;
            }
        }
        if count == 0 {
            return Err("Нет подключённых учеников".into());
        }
        self.log_message("Учитель", &text);
        (self.changed)();
        Ok(count)
    }
    pub fn acknowledge(&self, id: &str) -> Result<(), String> {
        let mut ps = self.peers.lock().unwrap();
        let p = ps.get_mut(id).ok_or("Ученик не найден")?;
        p.ack = true;
        p.help = false;
        (self.changed)();
        Ok(())
    }
    pub fn log_message(&self, from: &str, text: &str) {
        let mut l = self.log.lock().unwrap();
        if l.len() >= 100 {
            l.pop_front();
        }
        l.push_back(Message {
            at: now(),
            from: clean(from, 64),
            text: clean(text, 1000),
        });
    }
    pub fn set_broadcast(&self, on: bool) {
        self.broadcasting.store(on, Ordering::Relaxed);
        if !on {
            *self.broadcast.lock().unwrap() = None;
        }
        (self.changed)();
    }
    pub fn end(&self) {
        self.stop.store(true, Ordering::Relaxed);
        self.set_broadcast(false);
    }
}
#[derive(Clone, Serialize)]
pub struct StudentState {
    pub policy: crate::assessment::Policy,
    pub monitoring: bool,
    pub detections: Vec<crate::assessment::Detection>,
    pub monitor_error: Option<String>,
    pub question: Option<crate::lesson::StudentQuestion>,
    pub answer: Option<crate::lesson::Submission>,
    pub connected: bool,
    pub connecting: bool,
    pub status: String,
    pub sharing: bool,
    pub help: bool,
    pub messages: VecDeque<Message>,
    pub broadcasting: bool,
    pub broadcast: Option<Frame>,
    pub note: String,
}
pub struct Student {
    pub state: Mutex<StudentState>,
    pub stop: AtomicBool,
    pub low: AtomicBool,
    changed: Changed,
}
impl Student {
    pub fn connect_protected(
        code: &str,
        password: &str,
        name: String,
        low: bool,
        changed: Changed,
    ) -> Result<Arc<Self>, String> {
        let invite = if code.trim().starts_with("OV3-") {
            Invite::parse_protected(code, password)?
        } else {
            Invite::parse(code)?
        };
        let config = invite.config()?;
        let id = random()?;
        let s = Arc::new(Self {
            state: Mutex::new(StudentState {
                policy: crate::assessment::Policy::default(),
                monitoring: false,
                detections: vec![],
                monitor_error: None,
                question: None,
                answer: None,
                connected: false,
                connecting: true,
                status: "Подключаемся к классу…".into(),
                sharing: false,
                help: false,
                messages: VecDeque::new(),
                broadcasting: false,
                broadcast: None,
                note: String::new(),
            }),
            stop: AtomicBool::new(false),
            low: AtomicBool::new(low),
            changed,
        });
        let student = s.clone();
        std::thread::spawn(move || {
            let result = (|| -> Result<(), String> {
                let mut stream = tls_client(&invite, config)?;
                let mut last = Instant::now() - Duration::from_secs(10);
                let mut last_scan = Instant::now() - Duration::from_secs(10);
                while !student.stop.load(Ordering::Relaxed) {
                    let policy = {
                        let st = student.state.lock().unwrap();
                        st.monitoring.then(|| st.policy.clone())
                    };
                    if let Some(policy) = policy.filter(|p| p.active) {
                        if last_scan.elapsed() >= Duration::from_secs(2) {
                            let result = crate::assessment::scan(&policy.blocked);
                            let mut st = student.state.lock().unwrap();
                            if st.monitoring && st.policy == policy {
                                match result {
                                    Ok(d) => {
                                        st.detections = d;
                                        st.monitor_error = None;
                                    }
                                    Err(e) => {
                                        st.detections.clear();
                                        st.monitor_error = Some(e);
                                    }
                                }
                            }
                            last_scan = Instant::now();
                        }
                    }
                    let share = student.state.lock().unwrap().sharing;
                    let low = student.low.load(Ordering::Relaxed);
                    let frame = if share
                        && last.elapsed() >= Duration::from_millis(if low { 5000 } else { 250 })
                    {
                        last = Instant::now();
                        Some(Frame {
                            seq: now(),
                            data: crate::capture::screen(if low { 640 } else { 1280 })?.into(),
                        })
                    } else {
                        None
                    };
                    let (share, help, note) = {
                        let st = student.state.lock().unwrap();
                        (st.sharing, st.help, st.note.clone())
                    };
                    if student.stop.load(Ordering::Relaxed) {
                        break;
                    }
                    let (answer, monitoring, monitor_generation, detections, monitor_error) = {
                        let st = student.state.lock().unwrap();
                        (
                            st.answer.clone(),
                            st.monitoring,
                            st.policy.generation,
                            st.detections.clone(),
                            st.monitor_error.clone(),
                        )
                    };
                    let poll = Poll {
                        token: invite.token.clone(),
                        id: id.clone(),
                        name: name.clone(),
                        machine: std::env::var("COMPUTERNAME").unwrap_or_else(|_| "PC".into()),
                        help,
                        sharing: share,
                        frame: if share { frame } else { None },
                        note: note.clone(),
                        answer,
                        monitoring,
                        monitor_generation,
                        detections,
                        monitor_error,
                    };
                    write_packet(&mut stream, &poll)?;
                    let u: Update = read_packet(&mut stream)?;
                    if !u.ok {
                        return Err(u.error);
                    }
                    if student.stop.load(Ordering::Relaxed) {
                        break;
                    }
                    {
                        let mut st = student.state.lock().unwrap();
                        st.connected = true;
                        if st.policy != u.policy {
                            st.monitoring = false;
                            st.detections.clear();
                            st.monitor_error = None;
                        }
                        st.policy = u.policy;
                        if let Some(q) = &u.question {
                            if st.answer.as_ref().is_some_and(|a| {
                                a.question_id != q.id || q.own_choice.is_some() || q.closed
                            }) {
                                st.answer = None;
                            }
                        } else {
                            st.answer = None;
                        }
                        st.question = u.question;
                        st.connecting = false;
                        st.status = "В классе. Соединение защищено TLS".into();
                        if u.help_ack {
                            st.help = false;
                        }
                        if st.note == note {
                            st.note.clear();
                            if !note.is_empty() {
                                if st.messages.len() >= 100 {
                                    st.messages.pop_front();
                                }
                                st.messages.push_back(Message {
                                    at: now(),
                                    from: "Вы".into(),
                                    text: note,
                                });
                            }
                        }
                        for m in u.messages {
                            if st.messages.len() >= 100 {
                                st.messages.pop_front();
                            }
                            st.messages.push_back(m);
                        }
                        st.broadcasting = u.broadcasting;
                        if !st.broadcasting {
                            st.broadcast = None;
                        } else if u.broadcast.is_some() {
                            st.broadcast = u.broadcast;
                        }
                    }
                    (student.changed)();
                    for _ in 0..if low { 10 } else { 2 } {
                        if student.stop.load(Ordering::Relaxed) {
                            break;
                        }
                        std::thread::sleep(Duration::from_millis(100));
                    }
                }
                let _ = stream.sock.shutdown(Shutdown::Both);
                Ok(())
            })();
            {
                let mut st = student.state.lock().unwrap();
                st.connected = false;
                st.monitoring = false;
                st.detections.clear();
                st.monitor_error = None;
                st.connecting = false;
                st.sharing = false;
                st.broadcast = None;
                st.broadcasting = false;
                st.status = if result.is_ok() {
                    "Ты вышел из класса".into()
                } else {
                    "Соединение завершено. Проверь код, адрес учителя и доступ по локальной сети."
                        .into()
                };
            }
            (student.changed)();
        });
        Ok(s)
    }
    pub fn end(&self) {
        self.stop.store(true, Ordering::Relaxed);
        let mut s = self.state.lock().unwrap();
        s.sharing = false;
        s.monitoring = false;
        s.detections.clear();
        s.monitor_error = None;
        s.connected = false;
        s.connecting = false;
        s.broadcast = None;
        s.broadcasting = false;
        s.status = "Выход из класса…".into();
        (self.changed)();
    }
    pub fn share(&self, on: bool) {
        self.state.lock().unwrap().sharing = on;
        (self.changed)();
    }
    pub fn help(&self) {
        self.state.lock().unwrap().help = true;
        (self.changed)();
    }
    pub fn note(&self, text: String) -> Result<(), String> {
        let mut s = self.state.lock().unwrap();
        if !s.connected {
            return Err("Сначала подключись к классу".into());
        }
        if !s.note.is_empty() {
            return Err("Сообщение ещё отправляется".into());
        }
        if text.trim().is_empty() || text.chars().count() > 1000 {
            return Err("В сообщении должно быть от 1 до 1000 символов".into());
        }
        s.note = text;
        (self.changed)();
        Ok(())
    }
}
/// Exercises the actual student worker and screen opt-in on this PC only.
pub fn worker_self_test() -> Result<(), String> {
    fn wait(label: &str, check: impl Fn() -> bool) -> Result<(), String> {
        let start = Instant::now();
        while !check() {
            if start.elapsed() > Duration::from_secs(10) {
                return Err(format!("Истекло время проверки: {label}"));
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        Ok(())
    }
    let t = Teacher::start("127.0.0.1".into(), true, Arc::new(|| {}))?;
    let s = match Student::connect_protected(
        &t.invite.encrypt("OmniView self-test")?,
        "OmniView self-test",
        "Loopback test".into(),
        true,
        Arc::new(|| {}),
    ) {
        Ok(s) => s,
        Err(e) => {
            t.end();
            return Err(e);
        }
    };
    let result = (|| {
        wait("подключение", || {
            s.state.lock().unwrap().connected && t.list().len() == 1
        })?;
        let p = t.list().remove(0);
        t.lesson.lock().unwrap().start(
            "Сколько бит в байте?",
            vec!["4".into(), "8".into()],
            1,
            60,
            std::collections::BTreeMap::from([(p.id.clone(), p.name.clone())]),
            now(),
        )?;
        wait("получение вопроса", || {
            s.state.lock().unwrap().question.is_some()
        })?;
        {
            let mut st = s.state.lock().unwrap();
            let q = st.question.as_ref().unwrap();
            if q.correct.is_some() {
                return Err("Ключ ответа передан до завершения".into());
            }
            st.answer = Some(crate::lesson::Submission {
                question_id: q.id.clone(),
                choice: 1,
            });
        }
        wait("подтверждение ответа", || {
            s.state
                .lock()
                .unwrap()
                .question
                .as_ref()
                .is_some_and(|q| q.own_choice == Some(1))
        })?;
        t.lesson.lock().unwrap().close();
        wait("раскрытие ответа", || {
            s.state
                .lock()
                .unwrap()
                .question
                .as_ref()
                .is_some_and(|q| q.closed && q.correct == Some(1))
        })?;
        let exe = std::env::current_exe()
            .map_err(error)?
            .file_name()
            .ok_or("Нет имени EXE")?
            .to_string_lossy()
            .to_ascii_lowercase();
        t.assessment.lock().unwrap().configure(&exe, true)?;
        wait("условия проверки", || {
            s.state.lock().unwrap().policy.active
        })?;
        if s.state.lock().unwrap().monitoring {
            return Err("Мониторинг включён без согласия".into());
        }
        s.state.lock().unwrap().monitoring = true;
        wait(
            "обнаружение собственного тестового EXE",
            || !t.assessment.lock().unwrap().events.is_empty(),
        )?;
        t.assessment
            .lock()
            .unwrap()
            .configure("not-running-omniview-test.exe", true)?;
        wait(
            "отзыв согласия при смене списка",
            || {
                let st = s.state.lock().unwrap();
                st.policy.blocked == vec!["not-running-omniview-test.exe"] && !st.monitoring
            },
        )?;
        t.assessment.lock().unwrap().configure("", false)?;
        if p.sharing || p.frame.is_some() || s.state.lock().unwrap().sharing {
            return Err("Экран включён без согласия".into());
        }
        s.note("Сообщение от ученика 😀".into())?;
        s.help();
        wait("сообщение и запрос помощи", || {
            t.list()[0].help
                && t.log
                    .lock()
                    .unwrap()
                    .iter()
                    .any(|m| m.text == "Сообщение от ученика 😀")
        })?;
        t.send(&p.id, "Ответ преподавателя")?;
        t.acknowledge(&p.id)?;
        wait(
            "ответ и подтверждение помощи",
            || {
                let st = s.state.lock().unwrap();
                !st.help && st.messages.iter().any(|m| m.text == "Ответ преподавателя")
            },
        )?;
        s.share(true);
        wait("показ экрана ученика", || {
            t.list()[0].frame.is_some()
        })?;
        s.share(false);
        wait("отключение показа ученика", || {
            let p = &t.list()[0];
            !p.sharing && p.frame.is_none()
        })?;
        t.set_broadcast(true);
        wait("показ экрана учителя", || {
            s.state.lock().unwrap().broadcast.is_some()
        })?;
        t.set_broadcast(false);
        wait("отключение показа учителя", || {
            let st = s.state.lock().unwrap();
            !st.broadcasting && st.broadcast.is_none()
        })?;
        s.end();
        wait("отключение ученика", || {
            !t.list()[0].online
        })?;
        Ok(())
    })();
    s.end();
    t.end();
    result
}
pub fn network_self_test() -> Result<(), String> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let t = Teacher::start("127.0.0.1".into(), true, Arc::new(|| {}))?;
    let result = (|| {
        let i = Invite::parse(&t.invite.encode())?;
        let mut s = tls_client(&i, i.config()?)?;
        let mut p = Poll {
            token: i.token.clone(),
            id: random()?,
            name: "Проверка Я_Unicode".into(),
            machine: "LOOPBACK".into(),
            help: true,
            sharing: false,
            frame: None,
            note: "Тест | = _ 😀".into(),
            answer: None,
            monitoring: false,
            monitor_generation: 0,
            detections: vec![],
            monitor_error: None,
        };
        write_packet(&mut s, &p).map_err(|e| format!("First write: {e}"))?;
        let u: Update = read_packet(&mut s).map_err(|e| format!("First read: {e}"))?;
        if !u.ok || t.list().len() != 1 || !t.list()[0].help {
            return Err("Не прошла регистрация".into());
        }
        t.send("all", "Привет | = _ 😀")?;
        t.acknowledge(&p.id)?;
        p.note.clear();
        write_packet(&mut s, &p).map_err(|e| format!("Message write: {e}"))?;
        let u: Update = read_packet(&mut s).map_err(|e| format!("Message read: {e}"))?;
        if !u.help_ack || u.messages.first().map(|m| m.text.as_str()) != Some("Привет | = _ 😀")
        {
            return Err("Не прошла передача сообщения или помощь".into());
        }
        p.token = "invalid".into();
        write_packet(&mut s, &p).map_err(|e| format!("Reject write: {e}"))?;
        let u: Update = read_packet(&mut s).map_err(|e| format!("Reject read: {e}"))?;
        if u.ok {
            return Err("Неверный код не отклонён".into());
        }
        Ok(())
    })();
    t.end();
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn encrypted_pairing_messages_and_invalid_secret() {
        network_self_test().unwrap();
    }
    #[test]
    fn rejects_unbounded_packet() {
        let mut b = std::io::Cursor::new((MAX_PACKET as u32 + 1).to_be_bytes());
        assert!(read_packet::<Poll>(&mut b).is_err());
    }
    #[test]
    fn rejects_bad_invite() {
        assert!(Invite::parse("OV2-hello").is_err());
        assert!(Invite::parse("http://example.com").is_err());
    }
    #[test]
    fn rejects_wrong_certificate() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let t = Teacher::start("127.0.0.1".into(), true, Arc::new(|| {})).unwrap();
        let other = rcgen::generate_simple_self_signed(vec!["omniview.local".into()]).unwrap();
        let mut wrong = t.invite.clone();
        wrong.cert = URL_SAFE_NO_PAD.encode(other.cert.der());
        let mut s = tls_client(&wrong, wrong.config().unwrap()).unwrap();
        assert!(write_packet(&mut s, &serde_json::json!({"invalid":true})).is_err());
        t.end();
    }
}
