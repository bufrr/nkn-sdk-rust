use crate::connection::Connection;
use prost::Message;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};
use tokio::net::unix::SocketAddr;

use crate::pb::packet::Packet;

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
    pub initial_retransmission_timeout: i32,
    pub max_retransmission_timeout: i32,
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
    pub(crate) config: NcpConfig,
    local_addr: SocketAddr,
    remote_addr: SocketAddr,
    local_client_ids: Vec<String>,
    remote_client_ids: Vec<String>,

    pub send_with: SendWith,
    send_window_size: u32,
    recv_window_size: u32,
    send_mtu: u32,
    recv_mtu: u32,
    connections: HashMap<String, Connection>,

    is_accepted: Arc<Mutex<bool>>,
    is_established: Arc<Mutex<bool>>,
    is_closed: Arc<Mutex<bool>>,

    send_buffer: Vec<u8>,
    send_window_start_seq: u32,
    send_window_end_seq: u32,
    send_window_data: HashMap<u32, Vec<u8>>,
    bytes_write: u64,
    bytes_read: u64,
    bytes_read_sent_time: SystemTime,
    bytes_read_update_time: SystemTime,
    remote_bytes_read: u64,
    sent_window_packet_count: f64,
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
        Session {
            config,
            local_addr,
            remote_addr,
            local_client_ids,
            remote_client_ids,
            send_window_size: 0,
            recv_window_size: 0,
            send_mtu: 0,
            recv_mtu: 0,
            is_accepted: Arc::new(Mutex::new(false)),
            is_established: Arc::new(Mutex::new(false)),
            is_closed: Arc::new(Mutex::new(false)),
            send_buffer: Vec::new(),
            send_window_start_seq: 0,
            connections: HashMap::new(),
            send_with,
            send_window_end_seq: 0,
            send_window_data: HashMap::new(),
            bytes_write: 0,
            bytes_read: 0,
            bytes_read_sent_time: SystemTime::now(),
            bytes_read_update_time: SystemTime::now(),
            remote_bytes_read: 0,
            sent_window_packet_count: 0.0,
        }
    }

    pub fn start() {
        // todo!
    }

    pub fn dial(&self) -> Result<(), String> {
        if *self.is_accepted.lock().unwrap() {
            return Err("Session already established".to_string());
        }

        Ok(())
    }

    fn send_handshake_packet(&self) {
        let local_client_ids = self.local_client_ids.clone();
        let p = Packet {
            client_ids: local_client_ids,
            window_size: self.recv_window_size as u32,
            mtu: self.recv_mtu as u32,
            handshake: true,
            ..Default::default()
        };
        let mut buf = Vec::new();
        p.encode(&mut buf).unwrap();

        if !self.connections.is_empty() {
            for (_, conn) in self.connections.iter() {}
        }
    }
}
