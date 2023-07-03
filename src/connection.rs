use crate::session::NcpConfig;
use crossbeam_channel::{after, select, Receiver, Sender};
use std::collections::{BinaryHeap, HashMap};
use std::mem::replace;

use futures_util::{join, try_join};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

#[derive(Debug, Clone)]
pub struct Connection {
    config: NcpConfig,
    local_client_id: String,
    remote_client_id: String,
    window_size: f64,
    send_window_update_tx: Sender<()>,
    send_window_update_rx: Receiver<()>,
    time_sent_seq: Arc<Mutex<HashMap<u32, SystemTime>>>,
    resent_seq: Arc<Mutex<HashMap<u32, ()>>>,
    send_ack_queue: BinaryHeap<u32>,
    retransmission_timeout: Arc<Mutex<Duration>>,
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
            time_sent_seq: Arc::new(Mutex::new(HashMap::new())),
            resent_seq: Arc::new(Mutex::new(HashMap::new())),
            send_ack_queue: BinaryHeap::new(),
            retransmission_timeout: Arc::new(Mutex::new(Duration::from_millis(5000))),
        }
    }

    pub fn send_windos_used(&self) -> u32 {
        self.time_sent_seq.lock().unwrap().len() as u32
    }

    pub fn retransmission_timeout(&self) -> Duration {
        let d = *self.retransmission_timeout.lock().unwrap();
        d
    }

    pub fn send_ack(&mut self, sequence_id: u32) {
        self.send_ack_queue.push(sequence_id);
    }

    pub fn send_ack_queue_len(&self) -> usize {
        self.send_ack_queue.len()
    }

    pub fn wait_for_send_window(&self) -> Result<(), String> {
        let send_window_update_rx_clone = self.send_window_update_rx.clone();
        let timeout = after(Duration::from_secs(1));
        //let (mut ctx, _handle) = Context::with_timeout(Duration::from_secs(1));
        while self.send_windos_used() >= self.window_size as u32 {
            select! {
                recv(send_window_update_rx_clone) -> _ => {}
                recv(timeout) -> _ => {
                    return Err("send window timeout".to_string());
                }
            }
        }

        Ok(())
    }

    pub async fn start(&self) {
        try_join!(tx(), send_ack(), check_timeout());
    }
}

pub fn set_window_size(dest: &mut f64, src: f64) {
    let _ = replace(dest, src);
}

async fn tx() -> Result<(), String> {
    todo!()
}

async fn send_ack() -> Result<(), String> {
    todo!()
}

async fn check_timeout() -> Result<(), String> {
    todo!()
}

pub async fn receive_ack(
    conn: Arc<Mutex<Connection>>,
    sequence_id: u32,
    is_sent_by_me: bool,
) -> Result<(), String> {
    let conn = conn.lock().unwrap();
    let seq = conn.time_sent_seq.lock().unwrap();
    let t = match seq.get(&sequence_id) {
        Some(t) => t,
        None => return Ok(()),
    };

    match conn.resent_seq.lock().unwrap().get(&sequence_id) {
        Some(_) => {}
        None => {
            let size = conn.window_size + 1.0;
            //set_window_size(&mut conn.window_size, size);
        }
    };

    if is_sent_by_me {
        let rtt = SystemTime::now().duration_since(*t).unwrap();
        let timeout = *conn.retransmission_timeout.lock().unwrap();
        let d = (3 * rtt - timeout).as_nanos() as f64
            / (Duration::from_millis(1000)).as_nanos() as f64
            * Duration::from_millis(100).as_nanos() as f64;
        let ms = Duration::from_nanos(d.tanh() as u64);
        *conn.retransmission_timeout.lock().unwrap() = ms;
        // *self.retransmission_timeout.lock().unwrap() +=
        // Duration::from_millis(100);
    }

    conn.time_sent_seq.lock().unwrap().remove(&sequence_id);
    conn.resent_seq.lock().unwrap().remove(&sequence_id);

    conn.send_window_update_tx
        .send_timeout((), Duration::from_secs(0))
        .expect("send error");

    Ok(())
}
