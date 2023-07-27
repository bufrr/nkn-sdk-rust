use crate::connection::{start_conn, Connection};
use crate::pb::packet::Packet;
use crate::utils::{conn_key, next_seq, Channel};
use crossbeam_channel::{bounded, select, tick};
use futures_util::future::{join_all, select_ok};
use prost::Message;
use std::collections::HashMap;
use std::ops::Sub;
use std::sync::Arc;
use std::thread::sleep;
use std::time::{Duration, SystemTime};
use tokio::net::unix::SocketAddr;
use tokio::spawn;
use tokio::sync::Mutex;
use tokio::sync::RwLock;

type SendWith = fn(String, String, &Vec<u8>, Duration) -> Result<(), String>;

pub const MIN_SEQUENCE_ID: u32 = 1;

#[derive(Debug, Clone)]
pub struct NcpConfig {
    pub non_stream: bool,
    pub session_window_size: i32,
    pub mtu: i32,
    pub min_connection_window_size: i32,
    pub max_ask_seq_list_size: usize,
    pub flush_interval: i32,
    pub linger: i32,
    pub initial_retransmission_timeout: u64,
    pub max_retransmission_timeout: u64,
    pub send_ack_interval: u64,
    pub check_timeout_interval: u64,
    pub send_bytes_read_threshold: i32,
}

impl Default for NcpConfig {
    fn default() -> Self {
        Self {
            non_stream: false,
            session_window_size: 4 << 20,
            mtu: 1024,
            min_connection_window_size: 1,
            max_ask_seq_list_size: 32,
            flush_interval: 10,
            linger: 1000,
            initial_retransmission_timeout: 5000,
            max_retransmission_timeout: 10000,
            send_ack_interval: 50,
            check_timeout_interval: 100,
            send_bytes_read_threshold: 200,
        }
    }
}

#[derive(Debug)]
pub struct Session {
    pub config: NcpConfig,
    local_addr: SocketAddr,
    remote_addr: SocketAddr,
    local_client_ids: Vec<String>,
    remote_client_ids: Vec<String>,

    pub send_with: SendWith,
    send_window_size: u32,
    recv_window_size: u32,
    send_mtu: u32,
    recv_mtu: u32,
    connections: Arc<RwLock<HashMap<String, Connection>>>,

    is_accepted: Arc<RwLock<bool>>,
    is_established: Arc<RwLock<bool>>,
    is_closed: Arc<RwLock<bool>>,

    send_buffer: Arc<RwLock<Vec<u8>>>,
    send_window_start_seq: Arc<RwLock<u32>>,
    send_window_end_seq: Arc<RwLock<u32>>,
    send_window_data: Arc<RwLock<HashMap<u32, Vec<u8>>>>,
    recv_window_start_seq: Arc<RwLock<u32>>,
    recv_window_used: Arc<RwLock<u32>>,
    recv_window_data: Arc<RwLock<HashMap<u32, Vec<u8>>>>,
    bytes_write: Arc<RwLock<u64>>,
    bytes_read: Arc<RwLock<u64>>,
    bytes_read_sent_time: Arc<RwLock<SystemTime>>,
    bytes_read_update_time: Arc<RwLock<SystemTime>>,
    remote_bytes_read: Arc<RwLock<u64>>,
    send_window_packet_count: f64,

    send_chan: Channel<u32>,
    resend_chan: Channel<u32>,
    send_window_update: Channel<()>,
    recv_data_update: Channel<()>,
    on_accept: Channel<()>,
}

impl Session {
    pub fn new(
        config: NcpConfig,
        local_addr: SocketAddr,
        remote_addr: SocketAddr,
        local_client_ids: Vec<String>,
        remote_client_ids: Vec<String>,
        send_with: SendWith,
    ) -> Session {
        let (send_tx, send_rx) = bounded(1);
        let (resend_tx, resend_rx) = bounded(1);
        let (send_window_update_tx, send_window_update_rx) = bounded(1);
        let (recv_data_update_tx, recv_data_update_rx) = bounded(1);
        let (on_accept_tx, on_accept_rx) = bounded(1);
        let buffer_size = config.mtu as u32;

        Session {
            config,
            local_addr,
            remote_addr,
            local_client_ids,
            remote_client_ids,
            send_window_size: 0,
            recv_window_size: 0,
            send_mtu: buffer_size,
            recv_mtu: buffer_size,
            is_accepted: Arc::new(RwLock::new(false)),
            is_established: Arc::new(RwLock::new(false)),
            is_closed: Arc::new(RwLock::new(false)),
            send_buffer: Arc::new(RwLock::new(Vec::new())),
            send_window_start_seq: Arc::new(RwLock::new(MIN_SEQUENCE_ID)),
            send_window_end_seq: Arc::new(RwLock::new(MIN_SEQUENCE_ID)),
            connections: Arc::new(RwLock::new(HashMap::new())),
            send_with,
            send_window_data: Arc::new(RwLock::new(HashMap::new())),
            recv_window_start_seq: Arc::new(RwLock::new(MIN_SEQUENCE_ID)),
            recv_window_used: Arc::new(RwLock::new(0)),
            recv_window_data: Arc::new(RwLock::new(HashMap::new())),
            bytes_write: Arc::new(RwLock::new(0)),
            bytes_read: Arc::new(RwLock::new(0)),
            bytes_read_sent_time: Arc::new(RwLock::new(SystemTime::now())),
            bytes_read_update_time: Arc::new(RwLock::new(SystemTime::now())),
            remote_bytes_read: Arc::new(RwLock::new(0)),
            send_window_packet_count: 0.0,
            send_chan: (send_tx, send_rx),
            resend_chan: (resend_tx, resend_rx),
            send_window_update: (send_window_update_tx, send_window_update_rx),
            recv_data_update: (recv_data_update_tx, recv_data_update_rx),
            on_accept: (on_accept_tx, on_accept_rx),
        }
    }

    pub fn is_stream(&self) -> bool {
        !self.config.non_stream
    }

    pub async fn is_established(&self) -> bool {
        *self.is_established.read().await
    }

    pub async fn is_closed(&self) -> bool {
        *self.is_closed.read().await
    }

    pub async fn get_bytes_read(&self) -> u64 {
        *self.bytes_read.read().await
    }

    pub async fn update_bytes_read_sent_time(&self) {
        *self.bytes_read_sent_time.write().await = SystemTime::now();
    }

    pub async fn send_window_used(&self) -> u32 {
        let bytes_write = self.bytes_write.read().await;
        let remote_bytes_read = self.remote_bytes_read.read().await;
        let res = bytes_write.checked_sub(*remote_bytes_read).unwrap_or(0);
        res as u32
    }

    pub async fn recv_window_used(&self) -> u32 {
        *self.recv_window_used.read().await
    }

    pub async fn get_data_to_send(&self, sequence_id: u32) -> Vec<u8> {
        let send_window_data = self.send_window_data.read().await;
        send_window_data.get(&sequence_id).unwrap().clone()
    }

    pub async fn get_conn_window_size(&self) -> u32 {
        let mut window_size = 0;
        let connections = self.connections.read().await;
        for (_, conn) in connections.iter() {
            window_size += conn.window_size as u32;
        }
        window_size
    }

    pub fn get_resend_seq(&self) -> Result<u32, String> {
        let mut seq: u32 = 0;
        let (_, rx) = self.resend_chan.clone();
        select! {
            recv(rx) -> v => {
                match v {
                    Ok(v) => {
                        seq = v;
                        Ok(seq)
                    },
                    Err(e) => {
                        Err(format!("get_resend_seq error: {}", e))
                    }
                }
            },
            default(Duration::from_millis(0)) => {
                Ok(0)
            }
        }
    }

    pub fn get_send_seq(&self) -> Result<u32, String> {
        let mut seq: u32 = 0;
        let (_, rx) = self.send_chan.clone();
        let (_, rx_resend) = self.resend_chan.clone();
        select! {
            recv(rx) -> v => {
                match v {
                    Ok(v) => {
                        seq = v;
                        Ok(seq)
                    },
                    Err(e) => {
                        Err(format!("get_send_seq error: {}", e))
                    }
                }
            },
            recv(rx_resend) -> v => {
                match v {
                    Ok(v) => {
                        seq = v;
                        Ok(seq)
                    },
                    Err(e) => {
                        Err(format!("get_resend_seq error: {}", e))
                    }
                }
            },
            default(Duration::from_millis(0)) => {
                Ok(0)
            }
        }
    }

    pub async fn flush_send_buffer(&self) {
        let mut send_buffer = self.send_buffer.write().await;
        if send_buffer.len() == 0 {
            return;
        }

        let seq = self.send_window_end_seq.read().await;
        let p = Packet {
            sequence_id: *seq,
            data: send_buffer.clone(),
            ..Default::default()
        };
        let mut buf = Vec::new();
        p.encode(&mut buf).unwrap();

        let mut send_window_data = self.send_window_data.write().await;
        let mut send_window_end_seq = self.send_window_end_seq.write().await;
        send_window_data.insert(*seq, buf);
        *send_window_end_seq = next_seq(*seq, 1);
        send_buffer.clear();

        let (tx, _) = self.send_chan.clone();
        tx.send(*seq).unwrap();
    }

    pub async fn start_flush(&self) {
        loop {
            sleep(Duration::from_millis(self.config.flush_interval as u64));
            {
                let buffer = self.send_buffer.read().await;
                let should_flush = buffer.len() > 0;
                if !should_flush {
                    continue;
                }
            }
            self.flush_send_buffer().await;
        }
    }

    pub async fn start_check_bytes_read(&self) {
        loop {
            sleep(Duration::from_millis(self.config.check_timeout_interval));
            let sent_time = self.bytes_read_sent_time.read().await;
            let update_time = self.bytes_read_update_time.read().await;
            let bytes_read = self.bytes_read.read().await;

            let wait = SystemTime::now()
                .sub(Duration::from_millis(
                    self.config.send_bytes_read_threshold as u64,
                ))
                .duration_since(*update_time)
                .is_err();
            if *bytes_read == 0 || sent_time.duration_since(*update_time).is_ok() || wait {
                continue;
            }

            let p = Packet {
                bytes_read: *bytes_read,
                ..Default::default()
            };
            let mut buf = Vec::new();
            p.encode(&mut buf).unwrap();

            let mut success = false;
            let connections = self.connections.read().await;
            for (_, conn) in connections.iter() {
                let res = (self.send_with)(
                    conn.local_client_id.clone(),
                    conn.remote_client_id.clone(),
                    &buf,
                    Duration::from_millis(conn.retransmission_timeout().await.as_millis() as u64),
                );
                success = true;
            }
            if success {
                self.update_bytes_read_sent_time().await;
            }
        }
    }

    pub async fn wait_for_send_window(&self, n: u32) -> u32 {
        let ticker = tick(Duration::from_millis(100));
        let (_, rx) = self.send_window_update.clone();
        while self.send_window_used().await + n > self.send_window_size {
            select! {
                recv(rx) -> _ => {},
                recv(ticker) -> _ => {},
            }
        }
        self.send_window_size - self.send_window_used().await
    }

    pub async fn send_handshake_packet(&self, write_timeout: Duration) {
        let p = Packet {
            client_ids: self.local_client_ids.clone(),
            window_size: self.recv_window_size,
            mtu: self.recv_mtu,
            handshake: true,
            ..Default::default()
        };
        let mut buf = Vec::new();
        p.encode(&mut buf).unwrap();

        let mut tasks = vec![];

        let connections = self.connections.read().await;
        if !connections.is_empty() {
            for (_, conn) in connections.iter() {
                let send_with = self.send_with;
                let local_client_id = conn.local_client_id.clone();
                let remote_client_id = conn.remote_client_id.clone();
                let buf = buf.clone();
                let h = spawn(async move {
                    (send_with)(local_client_id, remote_client_id, &buf, write_timeout)
                });
                tasks.push(h);
            }
        } else {
            let local_client_ids = self.local_client_ids.clone();
            let remote_client_ids = self.remote_client_ids.clone();
            for (i, id) in local_client_ids.iter().enumerate() {
                let local_client_id = id.clone();
                let mut remote_client_id = id.clone();
                if remote_client_ids.len() > 0 {
                    remote_client_id = remote_client_ids[i].clone();
                }
                let send_with = self.send_with;
                let buf = buf.clone();
                let h = spawn(async move {
                    (send_with)(
                        local_client_id,
                        remote_client_id,
                        &buf.clone(),
                        write_timeout,
                    )
                });
                tasks.push(h);
            }
        }
        let res = select_ok(tasks).await;
        match res {
            Ok(_) => {}
            Err(e) => {
                println!("send_handshake_packet error: {}", e);
            }
        }
    }

    pub async fn handle_handshake_packet(&mut self, packet: Packet) {
        if self.is_established().await {
            return;
        }

        if packet.window_size == 0 {
            println!("handle_handshake_packet: window_size is 0");
            return;
        }

        if packet.window_size < self.send_window_size {
            self.send_window_size = packet.window_size;
        }

        if packet.mtu == 0 {
            println!("handle_handshake_packet: mtu is 0");
            return;
        }

        if packet.mtu < self.send_mtu {
            self.send_mtu = packet.mtu;
        }
        self.send_window_packet_count = self.send_window_size as f64 / self.send_mtu as f64;

        if packet.client_ids.is_empty() {
            println!("handle_handshake_packet: client_ids is empty");
            return;
        }

        let mut n = self.local_client_ids.len();
        if packet.client_ids.len() < n {
            n = packet.client_ids.len();
        }

        let initial_window_size = self.send_window_packet_count / n as f64;
        let mut connections = HashMap::new();
        for i in 0..n {
            let local_client_id = self.local_client_ids[i].clone();
            let remote_client_id = packet.client_ids[i].clone();
            let key = conn_key(local_client_id.clone(), remote_client_id.clone());
            let conn = Connection::new(local_client_id, remote_client_id, initial_window_size);
            connections.insert(key, conn);
        }
        self.connections = Arc::new(RwLock::new(connections));

        self.remote_client_ids = packet.client_ids.clone();
        *self.is_established.write().await = true;

        let (tx, _) = self.on_accept.clone();
        tx.send(()).expect("send on_accept error");
    }

    pub async fn send_close_packet(&self) {
        if !self.is_established().await {
            println!("send_close_packet: session is not established");
            return;
        }

        let p = Packet {
            close: true,
            ..Default::default()
        };
        let mut buf = Vec::new();
        p.encode(&mut buf).unwrap();

        let connections = self.connections.clone();
        let mut tasks = vec![];
        for (_, conn) in connections.read().await.iter() {
            let send_with = self.send_with;
            let local_client_id = conn.local_client_id.clone();
            let remote_client_id = conn.remote_client_id.clone();
            let buf = buf.clone();
            let timeout = conn.retransmission_timeout().await;
            let h =
                spawn(async move { (send_with)(local_client_id, remote_client_id, &buf, timeout) });
            tasks.push(h);
        }
        let res = select_ok(tasks).await;
        match res {
            Ok(_) => {}
            Err(e) => {
                println!("send_close_packet error: {}", e);
            }
        }
    }

    pub async fn handle_close_packet(&self) {
        if !self.is_established().await {
            println!("handle_close_packet: session is not established");
            return;
        }

        *self.is_closed.write().await = true;
    }

    pub async fn dial(&self) {
        let mut accept = self.is_accepted.write().await;
        if *accept {
            println!("dial: session is already accepted");
            return;
        }

        let timeout = Duration::from_millis(self.config.initial_retransmission_timeout);
        self.send_handshake_packet(timeout).await;

        //let mut sess = self.clone();

        //start(Arc::new(AsyncMutex::new(*sess)));

        *accept = true;
    }

    pub async fn accept(&self) {
        let mut accept = self.is_accepted.write().await;
        if *accept {
            println!("accept: session is already accepted");
            return;
        }

        let timeout = Duration::from_millis(self.config.max_retransmission_timeout);
        self.send_handshake_packet(timeout).await;

        //let mut sess = self.clone();

        //start(Arc::new(AsyncMutex::new(*sess)));

        *accept = true;
    }

    pub async fn read(&self, buf: &mut [u8]) -> usize {
        if self.is_closed().await {
            println!("read: session is closed");
            return 0;
        }

        if !self.is_established().await {
            println!("read: session is not established");
            return 0;
        }

        if buf.is_empty() {
            println!("read: buf is empty");
            return 0;
        }

        let mut recv_window_data = self.recv_window_data.write().await;
        let mut bytes_received = 0;
        loop {
            let mut recv_window_start_seq = self.recv_window_start_seq.write().await;
            if recv_window_data.contains_key(&recv_window_start_seq) {
                break;
            }

            let wait = tick(Duration::from_secs(1));
            let (_, rx) = self.recv_data_update.clone();
            select! {
                recv(wait) -> _ => {},
                recv(rx) -> _ => {},
            }

            let data = recv_window_data.get(&recv_window_start_seq).unwrap();
            if !self.is_stream() && data.len() > buf.len() {
                println!("read: buf is too small");
                return 0;
            }

            if buf.len() <= data.len() {
                buf.copy_from_slice(&data[0..buf.len()]);
                bytes_received = buf.len();
                let seq = *recv_window_start_seq;
                let x = recv_window_data.get_mut(&seq).unwrap();
                *x = x[buf.len()..].to_vec();
            } else {
                buf[0..data.len()].copy_from_slice(data);
                bytes_received = data.len();
                recv_window_data.remove(&recv_window_start_seq);
                *recv_window_start_seq = next_seq(*recv_window_start_seq, 1);
            }
            *self.recv_window_used.write().await -= bytes_received as u32;
            *self.bytes_read.write().await += bytes_received as u64;
            *self.bytes_read_update_time.write().await = SystemTime::now();

            if self.is_stream() {
                while bytes_received < buf.len() {
                    //todo
                }
            }
        }
        bytes_received
    }

    pub async fn write(&self, buf: &mut [u8]) -> usize {
        if self.is_closed().await {
            println!("write: session is closed");
            return 0;
        }
        if self.is_established().await {
            println!("write: session is not established");
            return 0;
        }

        if !self.is_stream() && buf.len() > self.send_mtu as usize
            || buf.len() > self.send_window_size as usize
        {
            println!("write: buf is too large");
            return 0;
        }

        if buf.is_empty() {
            return 0;
        }

        let mut bytes_send: usize = 0;

        if self.is_stream() {
            //todo
        } else {
            self.wait_for_send_window(buf.len() as u32).await;
            let mut send_buffer = self.send_buffer.write().await;
            *send_buffer = send_buffer[..buf.len()].to_owned();
            *self.bytes_write.write().await += buf.len() as u64;
            bytes_send += buf.len();

            self.flush_send_buffer().await;
        }
        bytes_send
    }
}

async fn start(session: Arc<Mutex<Session>>) {
    let session_clone = session.clone();
    let session_clone2 = session.clone();
    let mut tasks = vec![];

    let flush = async move {
        let sess = session_clone.lock().await;
        sess.start_flush().await;
    };

    let check = async move {
        let sess = session_clone2.lock().await;
        sess.start_check_bytes_read().await;
    };
    tasks.push(spawn(flush));
    tasks.push(spawn(check));

    let sess = session.lock().await;
    for conn in sess.connections.read().await.values() {
        let conn = Arc::new(Mutex::new(conn.clone()));
        let sess_clone = session.clone();
        let config_clone = sess.config.clone();
        tasks.push(spawn(start_conn(
            conn.clone(),
            sess_clone,
            config_clone,
            sess.send_window_data.clone(),
            sess.send_chan.1.clone(),
            sess.resend_chan.1.clone(),
            sess.resend_chan.0.clone(),
        )));
    }
    join_all(tasks).await;
}
