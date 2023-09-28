use crate::account::Account;
use crate::client::{process_dests, Client, ClientConfig};
use crate::message::{Message, MessageConfig};
use crate::pb;
use crate::pb::payloads::{Payload, PayloadType};
use crate::rpc::{Node, RPCConfig};
use crate::utils::{
    add_identifier, add_multiclient_prefix, make_address_string, remove_identifier, Channel,
};
use crossbeam_channel::{bounded, Receiver, Sender};
use futures::executor::block_on;
use hyper::body::HttpBody;
use log::error;
use moka::future::Cache;
use rand::{thread_rng, Rng};
use std::collections::HashMap;
use std::future::Future;
use std::ops::Deref;
use std::sync::{Arc, Barrier, Mutex, RwLock};
use std::thread::sleep;
use std::time::Duration;

#[derive(Debug)]
pub struct MultiClient {
    connect_channel: Channel<Node>,
    message_channel: Channel<Message>,

    config: Arc<Mutex<ClientConfig>>,
    offset: i32,
    address: String,
    closed: Arc<Mutex<bool>>,
    create_done: Arc<Mutex<bool>>,
    on_close_tx: Sender<()>,
    on_close_rx: Receiver<()>,
    clients: Arc<Mutex<HashMap<i32, Client>>>,
    default_client: Option<u32>,
    lock: RwLock<()>,
    msg_cache: Cache<String, ()>,
}

impl MultiClient {
    pub fn new(
        account: Account,
        base_identifier: String,
        num_sub_clients: i32,
        original_client: bool,
        config: &ClientConfig,
    ) -> Result<Self, String> {
        let mut num_clients = num_sub_clients;
        let mut offset: i32 = 0;
        if original_client {
            num_clients += 1;
            offset = 1;
        }
        let public_key = *account.public_key();
        let addr = make_address_string(&public_key, &base_identifier);

        let (connect_tx, connect_rx) = bounded(1);
        let (message_tx, message_rx) = bounded(1);
        let (on_close_tx, on_close_rx) = bounded(0);
        let mut m = Self {
            connect_channel: (connect_tx, connect_rx),
            message_channel: (message_tx, message_rx),
            config: Arc::new(Mutex::new(config.clone())),
            offset,
            address: addr,
            closed: Arc::new(Mutex::new(false)),
            create_done: Arc::new(Mutex::new(false)),
            on_close_tx,
            on_close_rx,
            clients: Arc::new(Mutex::new(HashMap::new())),
            default_client: None,
            lock: RwLock::new(()),
            msg_cache: Cache::builder()
                .max_capacity(10_000)
                .time_to_live(Duration::from_secs(300))
                .build(),
        };

        let mut i = -offset;
        let default_client_idx = num_sub_clients;
        //let (success_tx, success_rx) = bounded(1);
        //let (fail_tx, fail_rx) = sync_channel(1);
        while i < num_sub_clients {
            //let c = barrier.clone();
            //let success_tx_clone = success_tx.clone();
            let identifier = Some(add_identifier(base_identifier.clone(), i));
            let account_clone = account.clone();
            let config_clone = config.clone();
            let clients_clone = m.clients.clone();
            let close_clone = m.closed.clone();
            let (connect_tx_clone, _) = m.connect_channel.clone();
            let (message_tx_clone, message_rx_clone) = m.message_channel.clone();
            let cache = m.msg_cache.clone();
            tokio::spawn(async move {
                let res = Client::new(account_clone, identifier, config_clone);
                let mut client = match res {
                    Ok(c) => c,
                    Err(e) => {
                        error!("Error creating client: {e}");
                        return;
                    }
                };
                println!("Created client {i}");

                let closed = *close_clone.lock().unwrap();
                if closed {
                    client.close().expect("client closed:");
                    return;
                }

                //success_tx_clone.send(()).unwrap();

                let node = match client.wait_for_connect() {
                    Ok(n) => n,
                    Err(e) => {
                        error!("Error connecting to node: {e}");
                        //c.wait();
                        return;
                    }
                };
                log::info!("Connected to node {i}");

                let mut cc = clients_clone.lock().unwrap();
                cc.insert(i, client);

                if i < default_client_idx {
                    m.default_client = Some(i as u32);
                }

                match connect_tx_clone.send(node) {
                    Ok(_) => (),
                    Err(e) => {
                        log::error!("Error sending node: {e}");
                    }
                }

                loop {
                    let mut msg = match message_rx_clone.recv() {
                        Ok(m) => m,
                        Err(e) => {
                            println!("Error receiving message: {e}");
                            return;
                        }
                    };
                    if msg.r#type == PayloadType::Session as u32 {
                        todo!()
                    } else {
                        let cache_key = hex::encode(&msg.message_id);
                        if cache.contains_key(&cache_key) {
                            println!("Duplicate message received, ignoring");
                            continue;
                        }
                        cache.blocking().insert(cache_key, ());
                        (msg.src, _) = remove_identifier(msg.src.clone());
                        if msg.no_reply {
                            todo!()
                        } else {
                            todo!()
                        }
                        if *close_clone.lock().unwrap() {
                            return;
                        }
                        message_tx_clone.send(msg.clone()).unwrap();
                    }
                }
            });
            i += 1;
        }

        Ok(m)
    }

    pub fn close(&self) -> Result<(), String> {
        todo!()
    }

    pub async fn send_data(&mut self, dests: &[&str], text: &[u8], config: MessageConfig) {
        let config_clone = config.clone();
        let rpc_config = RPCConfig::default();
        let dests = process_dests(dests, rpc_config).await.unwrap();

        if dests.is_empty() {
            println!("No valid destinations");
            return;
        }
        let msg_id = thread_rng().gen::<[u8; 8]>();

        let payload = Payload {
            r#type: i32::from(PayloadType::Binary),
            message_id: Vec::from(msg_id),
            data: Vec::from(text),
            reply_to_id: vec![],
            no_reply: config.no_reply,
        };

        let clients_clone = self.clients.clone();
        //let cc = clients_clone.lock().unwrap().get(&1).unwrap();

        let mut sent = 0;
        //let c = clients_clone.lock().unwrap();
        let len = clients_clone.lock().unwrap().len();
        println!("111");
        for (i, client) in clients_clone.lock().unwrap().iter() {
            let fu = send_with_client(
                client,
                *i,
                &dests,
                payload.clone(),
                !config_clone.unencrypted,
                config_clone.max_holding_secs,
            );
            block_on(fu).expect("block on error");
            sent += 1;
        }
        if sent == 0 {
            //todo!()
        }
    }

    pub fn get_clients(&self) -> HashMap<i32, Client> {
        todo!()
    }

    pub fn wait_for_connect(&self) -> Result<Node, String> {
        let (_, rx) = self.connect_channel.clone();
        rx.recv().map_err(|_| "Receiver failed".into())
    }
}

pub async fn send_with_client(
    client: &Client,
    client_id: i32,
    dests: &Vec<String>,
    payload: Payload,
    encrypted: bool,
    max_holding_secs: u32,
) -> Result<(), String> {
    let dd = add_multiclient_prefix(dests, client_id as i32);
    let ss = dd.iter().map(|s| s.as_str()).collect::<Vec<&str>>();

    client
        .send_messages(&ss, payload, encrypted, max_holding_secs, 10000)
        .await
}
