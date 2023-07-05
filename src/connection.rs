use crate::session::{NcpConfig, Session};
use crossbeam_channel::{after, select, Receiver, Sender};
use std::collections::{BinaryHeap, HashMap};
use std::mem::replace;

use crate::pb::packet::Packet;
use crate::utils::{next_seq, SeqHeap};
use futures_util::{join, try_join};
use prost::Message;
use std::sync::{Arc, Mutex};
use std::thread::sleep;
use std::time::{Duration, SystemTime};

#[derive(Debug, Clone)]
pub struct Connection {
    config: NcpConfig,
    pub local_client_id: String,
    pub remote_client_id: String,
    window_size: f64,
    send_window_update_tx: Sender<()>,
    send_window_update_rx: Receiver<()>,
    //time_sent_seq: Arc<Mutex<HashMap<u32, SystemTime>>>,
    //resent_seq: Arc<Mutex<HashMap<u32, ()>>>,
    send_ack_queue: SeqHeap,
    retransmission_timeout: u64,
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
            //time_sent_seq: Arc::new(Mutex::new(HashMap::new())),
            //resent_seq: Arc::new(Mutex::new(HashMap::new())),
            send_ack_queue: BinaryHeap::new(),
            retransmission_timeout: 5000,
        }
    }

    // pub fn send_window_used(&self) -> u32 {
    //     self.time_sent_seq.lock().unwrap().len() as u32
    // }

    pub fn send_ack_queue_len(&self) -> usize {
        self.send_ack_queue.len()
    }

    pub fn wait_for_send_window(
        &self,
        time_sent_seq: Arc<Mutex<HashMap<u32, SystemTime>>>,
    ) -> Result<(), String> {
        let send_window_update_rx_clone = self.send_window_update_rx.clone();
        let timeout = after(Duration::from_secs(1));
        let tss = time_sent_seq.lock().unwrap();
        while tss.len() >= self.window_size as usize {
            select! {
                recv(send_window_update_rx_clone) -> _ => {}
                recv(timeout) -> _ => {
                    return Err("send window timeout".to_string());
                }
            }
        }

        Ok(())
    }
}

pub async fn start(
    conn: Arc<Mutex<Connection>>,
    session: Arc<Mutex<Session>>,
    config: NcpConfig,
    send_window_data: Arc<Mutex<HashMap<u32, Vec<u8>>>>,
    time_sent_seq: Arc<Mutex<HashMap<u32, SystemTime>>>,
    resent_seq: Arc<Mutex<HashMap<u32, ()>>>,
    send_rx: Receiver<u32>,
    resend_rx: Receiver<u32>,
    resend_tx: Sender<u32>,
) {
    join!(
        tx(
            conn.clone(),
            session.clone(),
            send_window_data.clone(),
            time_sent_seq.clone(),
            resent_seq.clone(),
            send_rx.clone(),
            resend_rx.clone(),
        ),
        send_ack(conn.clone(), session.clone(), config.clone()),
        check_timeout(
            conn.clone(),
            session.clone(),
            time_sent_seq.clone(),
            resent_seq.clone(),
            resend_tx.clone(),
        )
    );
}

pub fn set_window_size(dest: &mut f64, src: f64) {
    let _ = replace(dest, src);
}

async fn tx(
    conn: Arc<Mutex<Connection>>,
    session: Arc<Mutex<Session>>,
    send_window_data: Arc<Mutex<HashMap<u32, Vec<u8>>>>,
    time_sent_seq: Arc<Mutex<HashMap<u32, SystemTime>>>,
    resent_seq: Arc<Mutex<HashMap<u32, ()>>>,
    send_rx: Receiver<u32>,
    resend_rx: Receiver<u32>,
) {
    let conn = conn.lock().unwrap();
    let mut seq = 0;
    loop {
        if seq == 0 {
            select! {
            recv(resend_rx) -> s => seq = s.unwrap(),
            default => {},
            }
        }
        if seq == 0 {
            let res = conn.wait_for_send_window(time_sent_seq.clone());
            if res.is_err() {
                return;
            }
            select! {
                recv(send_rx) -> s => seq = s.unwrap(),
                recv(resend_rx) -> s => seq = s.unwrap(),
            }
        }
        let send_window_data = send_window_data.lock().unwrap();
        let buf = send_window_data.get(&seq).unwrap();
        if buf.len() == 0 {
            time_sent_seq.lock().unwrap().remove(&seq);
            resent_seq.lock().unwrap().remove(&seq);
            seq = 0;
            continue;
        }
        let send_with = session.lock().unwrap().send_with;
        let res = send_with(
            conn.local_client_id.clone(),
            conn.local_client_id.clone(),
            buf,
            Duration::from_secs(0),
        );
        match res {
            Ok(_) => {}
            Err(_) => {
                seq = 0;
            }
        }
        {
            let mut tss = time_sent_seq.lock().unwrap();
            let _ = match tss.get(&seq) {
                None => {
                    tss.insert(seq, SystemTime::now());
                }
                Some(_) => {}
            };
            tss.remove(&seq);

            seq = 0;
        }
    }
}

async fn send_ack(conn: Arc<Mutex<Connection>>, session: Arc<Mutex<Session>>, config: NcpConfig) {
    loop {
        sleep(Duration::from_secs(config.send_ack_interval));
        {
            let c = conn.lock().unwrap();
            if c.send_ack_queue_len() == 0 {
                continue;
            }
        }

        let mut ack_start_seq_list: Vec<u32> = Vec::new();
        let mut ack_seq_count_list: Vec<u32> = Vec::new();

        {
            let mut c = conn.lock().unwrap();
            loop {
                if c.send_ack_queue_len() == 0
                    && ack_start_seq_list.len() >= config.max_ask_seq_list_size
                {
                    break;
                }
                let ack_start_seq = c.send_ack_queue.pop().unwrap();
                let mut ack_seq_count: u32 = 1;
                while c.send_ack_queue.len() > 0
                    && c.send_ack_queue.peek().unwrap().v
                        == next_seq(ack_start_seq.v, ack_seq_count as i64)
                {
                    c.send_ack_queue.pop();
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

        let sess = session.lock().unwrap();
        let send_with = sess.send_with;
        let res = send_with(
            conn.lock().unwrap().local_client_id.clone(),
            conn.lock().unwrap().remote_client_id.clone(),
            &buf,
            Duration::from_secs(0), //todo
        );
        match res {
            Ok(_) => {}
            Err(_) => {}
        }

        // sess.update_bytes_read_sent_time
    }
}

async fn check_timeout(
    conn: Arc<Mutex<Connection>>,
    session: Arc<Mutex<Session>>,
    time_sent_seq: Arc<Mutex<HashMap<u32, SystemTime>>>,
    resent_seq: Arc<Mutex<HashMap<u32, ()>>>,
    resend_tx: Sender<u32>,
) {
    let sess = session.lock().unwrap();
    let config = sess.config.clone();
    drop(sess);
    loop {
        let c = conn.lock().unwrap();
        sleep(Duration::from_secs(config.check_timeout_interval));
        let threshold = SystemTime::now() - Duration::from_millis(c.retransmission_timeout);
        let mut new_resend = false;
        let tss = time_sent_seq.lock().unwrap();
        let mut rss = resent_seq.lock().unwrap();
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
    }
    // todo update window size
}

pub async fn receive_ack(
    conn: Arc<Mutex<Connection>>,
    time_sent_seq: Arc<Mutex<HashMap<u32, SystemTime>>>,
    resent_seq: Arc<Mutex<HashMap<u32, ()>>>,
    sequence_id: u32,
    is_sent_by_me: bool,
) -> Result<(), String> {
    let mut conn = conn.lock().unwrap();
    let seq = time_sent_seq.lock().unwrap();
    let t = match seq.get(&sequence_id) {
        Some(t) => t,
        None => return Ok(()),
    };

    match resent_seq.lock().unwrap().get(&sequence_id) {
        Some(_) => {}
        None => {
            let size = conn.window_size + 1.0;
            //set_window_size(&mut conn.window_size, size);
        }
    };

    if is_sent_by_me {
        let rtt = SystemTime::now().duration_since(*t).unwrap();
        let timeout = Duration::from_millis(conn.retransmission_timeout);
        let d = (3 * rtt - timeout).as_nanos() as f64
            / (Duration::from_millis(1000)).as_nanos() as f64
            * Duration::from_millis(100).as_nanos() as f64;
        let ms = Duration::from_nanos(d.tanh() as u64);
        conn.retransmission_timeout = ms.as_millis() as u64;
        // *self.retransmission_timeout.lock().unwrap() +=
        // Duration::from_millis(100);
    }

    time_sent_seq.lock().unwrap().remove(&sequence_id);
    resent_seq.lock().unwrap().remove(&sequence_id);

    conn.send_window_update_tx
        .send_timeout((), Duration::from_secs(0))
        .expect("send error");

    Ok(())
}
