use crate::session::{NcpConfig, Session};
use crossbeam_channel::{after, select, Receiver, Sender};
use std::collections::{BinaryHeap, HashMap};

use crate::pb::packet::Packet;
use crate::utils::{next_seq, SeqElem, SeqHeap};
use futures_util::join;
use prost::Message;
use std::sync::Arc;
use std::thread::sleep;
use std::time::{Duration, SystemTime};
use tokio::sync::Mutex;
use tokio::sync::RwLock;

#[derive(Debug, Clone)]
pub struct Connection {
    pub local_client_id: String,
    pub remote_client_id: String,
    config: NcpConfig,
    pub window_size: f64,
    send_window_update_tx: Sender<()>,
    send_window_update_rx: Receiver<()>,
    time_sent_seq: Arc<RwLock<HashMap<u32, SystemTime>>>,
    resent_seq: Arc<RwLock<HashMap<u32, ()>>>,
    send_ack_queue: Arc<RwLock<SeqHeap>>,
    retransmission_timeout: Arc<RwLock<Duration>>,
}

impl Connection {
    pub fn new(local_client_id: String, remote_client_id: String, window_size: f64) -> Self {
        let (send_window_update_tx, send_window_update_rx) = crossbeam_channel::bounded(1);
        Self {
            config: Default::default(),
            local_client_id,
            remote_client_id,
            window_size,
            send_window_update_tx,
            send_window_update_rx,
            time_sent_seq: Arc::new(RwLock::new(HashMap::new())),
            resent_seq: Arc::new(RwLock::new(HashMap::new())),
            send_ack_queue: Arc::new(RwLock::new(BinaryHeap::new())),
            retransmission_timeout: Arc::new(RwLock::new(Duration::from_millis(5000))),
        }
    }

    pub async fn send_window_used(&self) -> u32 {
        self.time_sent_seq.read().await.len() as u32
    }

    pub async fn retransmission_timeout(&self) -> Duration {
        *self.retransmission_timeout.read().await
    }

    pub async fn send_ack(&self, seq: u32) {
        let seq = SeqElem::new(seq);
        self.send_ack_queue.write().await.push(seq);
    }

    pub async fn send_ack_queue_len(&self) -> usize {
        self.send_ack_queue.read().await.len()
    }

    pub async fn wait_for_send_window(&self) -> Result<(), String> {
        let send_window_update_rx_clone = self.send_window_update_rx.clone();
        let timeout = after(Duration::from_secs(1));
        while self.send_window_used().await >= self.window_size as u32 {
            select! {
                recv(send_window_update_rx_clone) -> _ => {}
                recv(timeout) -> _ => {
                    return Err("send window timeout".to_string());
                }
            }
        }

        Ok(())
    }

    pub async fn receive_ack(&mut self, sequence_id: u32, is_sent_by_me: bool) {
        let time_sent_seq = self.time_sent_seq.read().await;
        let t = time_sent_seq.get(&sequence_id).unwrap();

        let resent_seq = self.resent_seq.read().await;
        match resent_seq.get(&sequence_id) {
            Some(_) => {}
            None => {
                let size = self.window_size + 1.0;
                //set_window_size(&mut conn.window_size, size);
            }
        };

        if is_sent_by_me {
            let rtt = SystemTime::now().duration_since(*t).unwrap();
            let timeout = self.retransmission_timeout().await;
            let d = (3 * rtt - timeout).as_nanos() as f64
                / (Duration::from_millis(1000)).as_nanos() as f64
                * Duration::from_millis(100).as_nanos() as f64;
            let ms = Duration::from_nanos(d.tanh() as u64);
            *self.retransmission_timeout.write().await = ms;
            if ms.as_millis() > self.config.max_retransmission_timeout as u128 {
                *self.retransmission_timeout.write().await =
                    Duration::from_millis(self.config.max_retransmission_timeout);
            }
        }

        self.time_sent_seq.write().await.remove(&sequence_id);
        self.resent_seq.write().await.remove(&sequence_id);

        self.send_window_update_tx
            .send_timeout((), Duration::from_secs(0))
            .expect("send error");
    }

    pub fn set_window_size(&mut self, mut n: f64, min_connection_window_size: f64) {
        if n < min_connection_window_size {
            n = min_connection_window_size;
        }
        self.window_size = n
    }
}

pub async fn start_conn(
    conn: Arc<Mutex<Connection>>,
    session: Arc<Mutex<Session>>,
    config: NcpConfig,
    send_window_data: Arc<RwLock<HashMap<u32, Vec<u8>>>>,
    send_rx: Receiver<u32>,
    resend_rx: Receiver<u32>,
    resend_tx: Sender<u32>,
) {
    join!(
        tx(
            conn.clone(),
            session.clone(),
            send_window_data.clone(),
            send_rx.clone(),
            resend_rx.clone(),
        ),
        send_ack(conn.clone(), session.clone(), config.clone()),
        check_timeout(conn.clone(), session.clone(), resend_tx.clone(),)
    );
}

async fn tx(
    conn: Arc<Mutex<Connection>>,
    session: Arc<Mutex<Session>>,
    send_window_data: Arc<RwLock<HashMap<u32, Vec<u8>>>>,
    send_rx: Receiver<u32>,
    resend_rx: Receiver<u32>,
) {
    let mut conn = conn.lock().await;
    let mut seq = 0;
    loop {
        if seq == 0 {
            select! {
            recv(resend_rx) -> s => seq = s.unwrap(),
            default => {},
            }
        }
        if seq == 0 {
            let res = conn.wait_for_send_window().await;
            if res.is_err() {
                return;
            }
            select! {
                recv(send_rx) -> s => seq = s.unwrap(),
                recv(resend_rx) -> s => seq = s.unwrap(),
            }
        }

        let send_window_data = send_window_data.read().await;
        let buf = send_window_data.get(&seq).unwrap();
        if buf.is_empty() {
            let mut time_sent_seq = conn.time_sent_seq.write().await;
            let mut resent_seq = conn.resent_seq.write().await;
            time_sent_seq.remove(&seq);
            resent_seq.remove(&seq);
            seq = 0;
            continue;
        }
        let mut buf_clone = buf.clone();
        {
            let mut session = session.lock().await;
            let send_with = session.send_with.as_mut();
            let res = send_with(
                conn.local_client_id.clone(),
                conn.local_client_id.clone(),
                &mut buf_clone,
                Duration::from_secs(0),
            );
            match res {
                Ok(_) => {}
                Err(err) => {
                    println!("send_with error: {:?}", err);
                    let window_size = conn.window_size;
                    conn.set_window_size(window_size / 2.0, 1.0);
                    //session.update_conn_window_size TODO
                    seq = 0;
                }
            }
        }

        {
            let mut tss = conn.time_sent_seq.write().await;
            let mut resent_seq = conn.resent_seq.write().await;

            match tss.get(&seq) {
                None => {
                    tss.insert(seq, SystemTime::now());
                }
                Some(_) => {}
            };
            resent_seq.remove(&seq);

            seq = 0;
        }
    }
}

async fn send_ack(conn: Arc<Mutex<Connection>>, session: Arc<Mutex<Session>>, config: NcpConfig) {
    loop {
        sleep(Duration::from_secs(config.send_ack_interval));
        {
            let c = conn.lock().await;
            if c.send_ack_queue_len().await == 0 {
                continue;
            }
        }

        let mut ack_start_seq_list: Vec<u32> = Vec::new();
        let mut ack_seq_count_list: Vec<u32> = Vec::new();

        {
            let c = conn.lock().await;
            loop {
                if c.send_ack_queue_len().await == 0
                    && ack_start_seq_list.len() >= config.max_ask_seq_list_size
                {
                    break;
                }

                let mut send_ack_queue = c.send_ack_queue.write().await;
                let ack_start_seq = send_ack_queue.pop().unwrap();
                let mut ack_seq_count: u32 = 1;
                while send_ack_queue.len() > 0
                    && send_ack_queue.peek().unwrap().v
                        == next_seq(ack_start_seq.v, ack_seq_count as i64)
                {
                    send_ack_queue.pop();
                    ack_seq_count += 1;
                }
                ack_start_seq_list.push(ack_start_seq.v);
                ack_seq_count_list.push(ack_seq_count);
            }
        }

        let mut omit_count = true;
        for ack_seq_count in ack_seq_count_list.iter() {
            if *ack_seq_count != 1 {
                omit_count = false;
                break;
            }
        }
        if omit_count {
            ack_seq_count_list.clear();
        }

        let p = Packet {
            ack_start_seq: ack_start_seq_list,
            ack_seq_count: ack_seq_count_list,
            bytes_read: 0, // todo
            ..Default::default()
        };
        let mut buf = Vec::new();
        p.encode(&mut buf).unwrap();
        let mut buf_clone = buf.clone();
        let mut sess = session.lock().await;
        let send_with = sess.send_with.as_mut();
        let res = send_with(
            conn.lock().await.local_client_id.clone(),
            conn.lock().await.remote_client_id.clone(),
            &mut buf_clone,
            Duration::from_secs(0), //todo
        );
        if res.is_ok() {}

        // sess.update_bytes_read_sent_time
    }
}

async fn check_timeout(
    conn: Arc<Mutex<Connection>>,
    session: Arc<Mutex<Session>>,
    resend_tx: Sender<u32>,
) {
    let sess = session.lock().await;
    let config = sess.config.clone();
    drop(sess);
    let mut new_resend = false;
    loop {
        sleep(Duration::from_secs(config.check_timeout_interval));

        let conn = conn.lock().await;
        let threshold = SystemTime::now() - conn.retransmission_timeout().await;

        let tss = conn.time_sent_seq.read().await;
        let mut rss = conn.resent_seq.write().await;
        for (seq, t) in tss.iter() {
            if rss.contains_key(seq) {
                continue;
            }
            if *t < threshold {
                resend_tx.send(*seq).unwrap();
                rss.insert(*seq, ());
                new_resend = true;
            }
        }
        if new_resend {
            // todo update window size
        }
    }
}
