use crate::account::Account;
use crate::constants::{
    CLIENT_SALT_LEN, DEFAULT_RPC_CONCURRENCY, DEFAULT_RPC_SERVER, DEFAULT_RPC_TIMEOUT, NONCE_SIZE,
    PUBLIC_KEY_LEN, SHA256_LEN, SHARED_KEY_LEN, SIGNATURE_LEN,
};
use crate::crypto::{
    decrypt, ed25519_private_key_to_curve25519_private_key, ed25519_sign, encrypt, sha256_hash,
};
use crate::message::{Message, MessageConfig, Reply};
use crate::pb::node::SyncState;
use crate::rpc::{get_node_state, get_registrant, get_ws_address, Node, RPCConfig};
use crate::utils::{
    client_config_to_rpc_config, get_or_compute_shared_key, make_address_string,
    parse_client_address, Channel, StSaltAndSignature,
};
use crate::wallet::{Wallet, WalletConfig};
use prost::bytes::Bytes;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::str;
use std::string::String;
use std::sync::{mpsc::channel, Arc, Mutex};

use tokio::task;
use tokio::time::sleep;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message as WsMessage};
use url::Url;

use crate::error::NKNError;
use crate::pb;
use crate::pb::clientmessage::{
    ClientMessage, ClientMessageType, CompressionType, InboundMessage, OutboundMessage, Receipt,
};
use crate::pb::payloads::{Payload, PayloadType, TextData};
use crate::pb::sigchain::{SigAlgo, SigChain, SigChainElem};

use ed25519_dalek::{PublicKey, SecretKey, SECRET_KEY_LENGTH};
use flate2::write::ZlibEncoder;
use flate2::Compression;
use futures_channel::mpsc::{unbounded, UnboundedSender};

use crossbeam_channel::{bounded, Receiver, Sender};

use futures_util::{future, pin_mut, StreamExt};
use hex::ToHex;
use prost::Message as prostMessage;
use rand::{thread_rng, Rng};
use std::io::Write;
use std::time::Duration;

const MAX_CLIENT_MESSAGE_SIZE: usize = 4000000;
const PING_INTERVAL: u64 = 8;
const PONG_TIMEOUT: u64 = 10;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetClientResult {
    node: Node,
    sig_chain_block_hash: String,
}

#[derive(Debug, Clone)]
pub struct ClientConfig {
    pub rpc_server_address: Vec<String>,
    pub rpc_timeout: u64,
    pub rpc_concurrency: u32,
    pub msg_chan_length: u32,
    pub connect_retries: u32,
    pub msg_cache_expiration: u32,
    pub msg_cache_cleanup_interval: u32,
    pub ws_handshake_timeout: u64,
    pub ws_write_timeout: u64,
    pub min_reconnect_interval: u64,
    pub max_reconnect_interval: u64,

    pub message_config: Option<MessageConfig>,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            rpc_server_address: DEFAULT_RPC_SERVER.iter().map(|s| s.to_string()).collect(),
            rpc_timeout: DEFAULT_RPC_TIMEOUT,
            rpc_concurrency: DEFAULT_RPC_CONCURRENCY,
            msg_chan_length: 1024,
            connect_retries: 3,
            msg_cache_expiration: 300000,
            msg_cache_cleanup_interval: 60000,
            ws_handshake_timeout: 5000,
            ws_write_timeout: 10000,
            min_reconnect_interval: 1000,
            max_reconnect_interval: 64000,
            message_config: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Client {
    config: ClientConfig,
    wallet: Arc<Mutex<Wallet>>,
    account: Account,
    address: String,
    address_id: [u8; SHA256_LEN],
    closed: Arc<Mutex<bool>>,
    node: Arc<Mutex<Option<Node>>>,
    shared_keys: Arc<Mutex<HashMap<String, [u8; SHARED_KEY_LEN]>>>,
    curve_secret_key: [u8; SHARED_KEY_LEN],
    sig_chain_block_hash: Arc<Mutex<Option<String>>>,
    connect_channel: Channel<Node>,
    message_channel: Channel<Message>,
    stdin_tx: Arc<Mutex<Option<UnboundedSender<WsMessage>>>>,
    reconnect_tx: Sender<()>,
    reply_tx: Sender<Reply>,
    response_channels: Arc<Mutex<HashMap<String, Channel<Message>>>>,
    challenge_signature_channel: Channel<StSaltAndSignature>,
}

// unsafe impl Sync for Client {}
// unsafe impl Send for Client {}

impl Client {
    pub fn new(
        account: Account,
        identifier: Option<String>,
        config: ClientConfig,
    ) -> Result<Self, String> {
        let w = Wallet::new(
            account.clone(),
            WalletConfig {
                seed_rpc_server: config.rpc_server_address.clone(),
                ..Default::default()
            },
        )?;
        let wallet = Arc::new(Mutex::new(w));
        let curve_secret_key = ed25519_private_key_to_curve25519_private_key(account.private_key());
        let public_key = account.public_key().clone();
        let id = match identifier {
            Some(id) => id,
            None => String::from(""),
        };

        let address = make_address_string(&public_key, &id);
        let closed = Arc::new(Mutex::new(false));
        let node = Arc::new(Mutex::new(None));
        let shared_keys = Arc::new(Mutex::new(HashMap::new()));
        let config = config.clone();
        let response_channels = Arc::new(Mutex::new(HashMap::new()));
        let address_id = sha256_hash(address.as_bytes());
        let sig_chain_block_hash = Arc::new(Mutex::new(None));
        let stdin_tx = Arc::new(Mutex::new(None));
        let (connect_tx, connect_rx) = bounded(1);
        let (message_tx, message_rx) = bounded(1);
        let (reply_tx, reply_rx) = bounded(1024);
        let (reconnect_tx, reconnect_rx) = bounded(1);
        let challenge_signature_channel = bounded(1);

        let address_clone = address.clone();
        let private_key = account.private_key().to_vec();
        let config_clone = config.clone();
        let closed_clone = closed.clone();
        let node_clone = node.clone();
        let sig_chain_block_hash_clone = sig_chain_block_hash.clone();
        let wallet_clone = wallet.clone();
        let stdin_tx_clone = stdin_tx.clone();
        let connect_tx_clone = connect_tx.clone();
        let message_tx_clone = message_tx.clone();
        let reconnect_tx_clone = reconnect_tx.clone();
        let reply_tx_clone = reply_tx.clone();
        let response_channels_clone = response_channels.clone();
        let shared_keys_clone = shared_keys.clone();
        task::spawn(async move {
            handle_reconnect(
                reconnect_rx,
                address_clone,
                private_key,
                config_clone,
                closed_clone,
                node_clone,
                sig_chain_block_hash_clone,
                wallet_clone,
                stdin_tx_clone,
                connect_tx_clone,
                message_tx_clone,
                reconnect_tx_clone,
                reply_tx_clone,
                response_channels_clone,
                curve_secret_key,
                shared_keys_clone,
            )
            .await;
        });

        let address_clone = address.clone();
        let private_key = account.private_key().to_vec();
        let config_clone = config.clone();
        let closed_clone = closed.clone();
        let node_clone = node.clone();
        let sig_chain_block_hash_clone = sig_chain_block_hash.clone();
        let wallet_clone = wallet.clone();
        let stdin_tx_clone = stdin_tx.clone();
        let connect_tx_clone = connect_tx.clone();
        let message_tx_clone = message_tx.clone();
        let reconnect_tx_clone = reconnect_tx.clone();
        let reply_tx_clone = reply_tx.clone();
        let response_channels_clone = response_channels.clone();
        let shared_keys_clone = shared_keys.clone();
        task::spawn(async move {
            connect(
                0,
                address_clone,
                private_key,
                config_clone,
                closed_clone,
                node_clone,
                sig_chain_block_hash_clone,
                wallet_clone,
                stdin_tx_clone,
                connect_tx_clone,
                message_tx_clone,
                reconnect_tx_clone,
                reply_tx_clone,
                response_channels_clone,
                curve_secret_key,
                shared_keys_clone,
            )
            .await
            .unwrap();
        });

        let ws_write_timeout = config.ws_write_timeout;
        let public_key = account.public_key().to_vec();
        let private_key = account.private_key().to_vec();
        let node_clone = node.clone();
        let stdin_tx_clone = stdin_tx.clone();
        let reconnect_tx_clone = reconnect_tx.clone();
        let sig_chain_block_hash_clone = sig_chain_block_hash.clone();
        let shared_keys_clone = shared_keys.clone();
        let rpc_config = client_config_to_rpc_config(&config);
        task::spawn(async move {
            handle_reply(
                reply_rx,
                ws_write_timeout,
                public_key,
                private_key,
                address_id,
                curve_secret_key,
                stdin_tx_clone,
                reconnect_tx_clone,
                node_clone,
                sig_chain_block_hash_clone,
                shared_keys_clone,
                rpc_config,
            )
            .await;
        });

        Ok(Self {
            config,
            wallet,
            account,
            address,
            closed,
            node,
            shared_keys,
            curve_secret_key,
            address_id,
            sig_chain_block_hash,
            connect_channel: (connect_tx, connect_rx),
            message_channel: (message_tx, message_rx),
            stdin_tx,
            reconnect_tx,
            reply_tx,
            response_channels,
            challenge_signature_channel,
        })
    }

    pub fn config(&self) -> ClientConfig {
        self.config.clone()
    }

    pub fn private_key(&self) -> &[u8] {
        self.account.private_key()
    }

    pub fn public_key(&self) -> &[u8] {
        self.account.public_key()
    }

    pub fn is_closed(&self) -> bool {
        *self.closed.lock().unwrap()
    }

    pub fn close(&mut self) -> Result<(), String> {
        *self.closed.lock().unwrap() = true;
        Ok(())
    }

    pub fn wait_for_connect(&self) -> Result<Node, String> {
        let (_, rx) = &self.connect_channel;
        rx.recv().map_err(|_| "Receiver failed".into())
    }

    pub fn wait_for_message(&self) -> Result<Message, String> {
        let (_, rx) = &self.message_channel;
        rx.recv().map_err(|_| "Receiver failed".into())
    }

    async fn connect(&self, max_retries: u32) -> Result<(), String> {
        let (connect_tx, _) = &self.connect_channel;
        let (message_tx, _) = &self.message_channel;

        connect(
            max_retries,
            self.address.clone(),
            self.private_key().to_vec(),
            self.config.clone(),
            self.closed.clone(),
            self.node.clone(),
            self.sig_chain_block_hash.clone(),
            self.wallet.clone(),
            self.stdin_tx.clone(),
            connect_tx.clone(),
            message_tx.clone(),
            self.reconnect_tx.clone(),
            self.reply_tx.clone(),
            self.response_channels.clone(),
            self.curve_secret_key,
            self.shared_keys.clone(),
        )
        .await
    }

    pub async fn reconnect(&mut self) -> Result<(), String> {
        if *self.closed.lock().unwrap() {
            return Ok(());
        }

        self.reconnect_tx.send(()).expect("TODO: panic message");

        Ok(())
    }

    pub fn node(&self) -> Option<Node> {
        self.node.lock().unwrap().clone()
    }

    pub async fn send_messages(
        &self,
        dests: &[&str],
        payload: Payload,
        encrypted: bool,
        max_holding_seconds: u32,
        ws_write_timeout: u64,
    ) -> Result<(), String> {
        send_messages(
            dests,
            payload,
            encrypted,
            max_holding_seconds,
            ws_write_timeout,
            self.public_key(),
            self.private_key(),
            &self.address_id,
            &self.curve_secret_key,
            self.stdin_tx.clone(),
            self.reconnect_tx.clone(),
            &self.node,
            &self.sig_chain_block_hash,
            &self.shared_keys,
            client_config_to_rpc_config(&self.config),
        )
        .await
    }

    async fn send(
        &mut self,
        dests: &[&str],
        payload: Payload,
        config: MessageConfig,
    ) -> Result<(), String> {
        let message_id = payload.message_id.clone();

        let timeout = self.config.ws_write_timeout;
        self.send_messages(
            dests,
            payload,
            !config.unencrypted,
            config.max_holding_secs,
            timeout,
        )
        .await?;

        if !config.no_reply {
            let message_id = str::from_utf8(&message_id).unwrap();
            self.response_channels
                .lock()
                .unwrap()
                .insert(message_id.into(), bounded(1));
        }

        Ok(())
    }

    pub async fn send_data(
        &mut self,
        dests: &[&str],
        text: &[u8],
        config: MessageConfig,
    ) -> Result<(), String> {
        let config_clone = config.clone();
        let payload = Payload {
            r#type: i32::from(PayloadType::Binary),
            message_id: config.message_id,
            data: Vec::from(text),
            reply_to_id: vec![],
            no_reply: config.no_reply,
        };
        self.send(dests, payload, config_clone).await
    }

    pub fn set_write_deadline(&self, deadline: u64) {
        todo!()
    }
}

async fn handle_reconnect(
    reconnect_rx: Receiver<()>,
    address: String,
    private_key: Vec<u8>,
    config: ClientConfig,
    closed: Arc<Mutex<bool>>,
    client_node: Arc<Mutex<Option<Node>>>,
    sig_chain_block_hash: Arc<Mutex<Option<String>>>,
    wallet: Arc<Mutex<Wallet>>,
    stdin_tx: Arc<Mutex<Option<UnboundedSender<WsMessage>>>>,
    connect_tx: Sender<Node>,
    message_tx: Sender<Message>,
    reconnect_tx: Sender<()>,
    reply_tx: Sender<Reply>,
    response_channels: Arc<Mutex<HashMap<String, Channel<Message>>>>,
    curve_secret_key: [u8; SHARED_KEY_LEN],
    shared_keys: Arc<Mutex<HashMap<String, [u8; SHARED_KEY_LEN]>>>,
) {
    loop {
        reconnect_rx.recv().unwrap();

        if *closed.lock().unwrap() {
            return;
        }

        let min_reconnect_interval = config.min_reconnect_interval;
        log::info!("Reconnect in {} ms...", min_reconnect_interval);
        sleep(Duration::from_millis(min_reconnect_interval)).await;

        let connect_tx_clone = connect_tx.clone();
        let message_tx_clone = message_tx.clone();
        let reply_tx_clone = reply_tx.clone();
        let reconnect_tx_clone = reconnect_tx.clone();
        let res = connect(
            0,
            address.clone(),
            private_key.clone(),
            config.clone(),
            closed.clone(),
            client_node.clone(),
            sig_chain_block_hash.clone(),
            wallet.clone(),
            stdin_tx.clone(),
            connect_tx_clone,
            message_tx_clone,
            reconnect_tx_clone,
            reply_tx_clone,
            response_channels.clone(),
            curve_secret_key.clone(),
            shared_keys.clone(),
        )
        .await;

        if let Err(err) = res {
            log::error!("Error: {}", err);
            //close(closed.clone());
        }
    }
}

async fn connect(
    max_retries: u32,
    address: String,
    private_key: Vec<u8>,
    config: ClientConfig,
    closed: Arc<Mutex<bool>>,
    client_node: Arc<Mutex<Option<Node>>>,
    sig_chain_block_hash: Arc<Mutex<Option<String>>>,
    wallet: Arc<Mutex<Wallet>>,
    stdin_tx: Arc<Mutex<Option<UnboundedSender<WsMessage>>>>,
    connect_tx: Sender<Node>,
    message_tx: Sender<Message>,
    reconnect_tx: Sender<()>,
    reply_tx: Sender<Reply>,
    response_channels: Arc<Mutex<HashMap<String, Channel<Message>>>>,
    curve_secret_key: [u8; SHARED_KEY_LEN],
    shared_keys: Arc<Mutex<HashMap<String, [u8; SHARED_KEY_LEN]>>>,
) -> Result<(), String> {
    let max_reconnect_interval = config.max_reconnect_interval;

    let mut retry_interval = config.min_reconnect_interval;
    let mut retry = 0;

    while max_retries == 0 || retry < max_retries {
        if retry > 0 {
            log::info!("Retry in {} ms...", retry_interval);
            sleep(Duration::from_millis(retry_interval)).await;

            retry_interval *= 2;
            if retry_interval > max_reconnect_interval {
                retry_interval = max_reconnect_interval;
            }
        }

        let rpc_config = client_config_to_rpc_config(&config);
        let res = get_ws_address(&address, rpc_config).await;

        match res {
            Ok(node) => {
                let connect_tx_clone = connect_tx.clone();
                let message_tx_clone = message_tx.clone();
                let reply_tx_clone = reply_tx.clone();
                let reconnect_tx_clone = reconnect_tx.clone();
                let res = connect_to_node(
                    node,
                    address.clone(),
                    private_key.clone(),
                    config.clone(),
                    closed.clone(),
                    client_node.clone(),
                    sig_chain_block_hash.clone(),
                    wallet.clone(),
                    stdin_tx.clone(),
                    connect_tx_clone,
                    message_tx_clone,
                    reconnect_tx_clone,
                    reply_tx_clone,
                    response_channels.clone(),
                    curve_secret_key,
                    shared_keys.clone(),
                )
                .await;

                match res {
                    Ok(()) => return Ok(()),
                    Err(err) => log::error!("Error: {err}"),
                }
            }
            Err(err) => log::error!("Error: {err}"),
        }

        retry += 1;
    }

    Err("connect failed".into())
}

async fn connect_to_node(
    node: Node,
    address: String,
    private_key: Vec<u8>,
    config: ClientConfig,
    closed: Arc<Mutex<bool>>,
    client_node: Arc<Mutex<Option<Node>>>,
    sig_chain_block_hash: Arc<Mutex<Option<String>>>,
    wallet: Arc<Mutex<Wallet>>,
    stdin_tx: Arc<Mutex<Option<UnboundedSender<WsMessage>>>>,
    connect_tx: Sender<Node>,
    message_tx: Sender<Message>,
    reconnect_tx: Sender<()>,
    reply_tx: Sender<Reply>,
    response_channels: Arc<Mutex<HashMap<String, Channel<Message>>>>,
    curve_secret_key: [u8; SHARED_KEY_LEN],
    shared_keys: Arc<Mutex<HashMap<String, [u8; SHARED_KEY_LEN]>>>,
) -> Result<(), String> {
    let handle = if !node.rpc_addr.is_empty() {
        let node = node.clone();
        let config = config.clone();

        let handle = task::spawn(async move {
            let addr = format!("http://{}", node.rpc_addr);

            let ws_handshake_timeout = config.ws_handshake_timeout;
            let node_state = match get_node_state(RPCConfig {
                rpc_server_address: vec![addr.clone()],
                rpc_timeout: ws_handshake_timeout,
                ..RPCConfig::default()
            })
            .await
            {
                Ok(node_state) => node_state,
                Err(err) => {
                    log::info!("get node state error: {err}");
                    return None;
                }
            };

            if node_state.sync_state != *SyncState::PersistFinished.as_str_name() {
                return None;
            }

            Some(addr)
        });
        Some(handle)
    } else {
        None
    };

    let url = Url::parse(&format!("ws://{}", node.addr)).unwrap();
    let (ws_stream, _) = connect_async(url).await.expect("Failed to connect");
    let (write, read) = ws_stream.split();

    let (stdin_tx_new, stdin_rx) = unbounded();
    *stdin_tx.lock().unwrap() = Some(stdin_tx_new);

    let (txx, rxx) = bounded(1);
    let stdin_to_ws = stdin_rx.map(Ok).forward(write);

    tokio::spawn(async move {
        let ws_to_stdout = {
            read.for_each(|message| async {
                match message {
                    Ok(message) => {
                        let data = match message {
                            WsMessage::Text(text) => {
                                let mut data = text.as_bytes().to_vec();
                                data.push(0);
                                data
                            }
                            WsMessage::Binary(mut data) => {
                                data.push(1);
                                data
                            }
                            _ => Vec::new(),
                        };
                        if !data.is_empty() {
                            txx.send(data).unwrap();
                        }
                    }
                    Err(err) => log::error!("Error: {err}"),
                }
            })
        };
        pin_mut!(stdin_to_ws, ws_to_stdout);
        future::select(stdin_to_ws, ws_to_stdout).await;
    });

    *client_node.lock().unwrap() = Some(node);

    if let Some(handle) = handle {
        if let Some(rpc_addr) = handle.await.unwrap() {
            let seed_rpc_server = if rpc_addr.is_empty() {
                Vec::new()
            } else {
                vec![rpc_addr]
            };

            let mut wallet = wallet.lock().unwrap();
            let config = wallet.config().clone();
            wallet.set_config(WalletConfig {
                seed_rpc_server,
                password: String::new(),
                master_key: Vec::new(),
                scrypt: config.scrypt.clone(),
                ..config
            });
        }
    }

    let done = Arc::new(Mutex::new(false));

    let done_clone = done.clone();
    let stdin_tx_clone = stdin_tx.clone();
    let reconnect_tx_clone = reconnect_tx.clone();
    tokio::spawn(async move {
        loop {
            if *done_clone.lock().unwrap() {
                return;
            }

            sleep(Duration::from_secs(PING_INTERVAL)).await;

            let res = stdin_tx_clone
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .unbounded_send(WsMessage::Ping(Vec::new()));

            if let Err(err) = res {
                log::error!("Error: {}", err);
                reconnect_tx_clone.send(()).expect("TODO: panic message");
                return;
            }
        }
    });

    let address_clone = address.clone();
    let stdin_tx_clone = stdin_tx.clone();
    let reconnect_tx_clone = reconnect_tx.clone();
    let (challenge_tx, mut challenge_rx) = bounded::<StSaltAndSignature>(100);

    tokio::spawn(async move {
        let v = match challenge_rx.recv() {
            Ok(v) => v,
            Err(err) => {
                log::error!("Error: {err}");
                return;
            }
        };
        let salt = v.client_salt.encode_hex::<String>();
        let signature = v.signature.encode_hex::<String>();
        let req = json!({
            "Action": "setClient",
            "Addr": address_clone,
            "ClientSalt": salt,
            "Signature": signature,
        })
        .to_string();

        let res = stdin_tx_clone
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .unbounded_send(WsMessage::Text(req));

        if let Err(err) = res {
            log::error!("Error: {}", err);
            reconnect_tx_clone.send(()).expect("TODO: panic message");
        }
    });

    let closed_clone = closed.clone();
    let stdin_tx_clone = stdin_tx.clone();
    tokio::spawn(async move {
        loop {
            if *closed_clone.lock().unwrap() {
                return;
            }

            let mut data = match rxx.recv() {
                Ok(n) => n,
                Err(err) => {
                    log::error!("Error: {err}");
                    break;
                }
            };

            let is_text = data.pop().unwrap() == 0;
            let connect_tx_clone = connect_tx.clone();
            let message_tx_clone = message_tx.clone();
            let reply_tx_clone = reply_tx.clone();
            let reconnect_tx_clone = reconnect_tx.clone();
            let res = handle_message(
                is_text,
                data,
                address.clone(),
                private_key.clone(),
                config.clone(),
                closed.clone(),
                client_node.clone(),
                sig_chain_block_hash.clone(),
                wallet.clone(),
                stdin_tx_clone.clone(),
                connect_tx_clone,
                message_tx_clone,
                reconnect_tx_clone,
                reply_tx_clone,
                response_channels.clone(),
                curve_secret_key,
                shared_keys.clone(),
                challenge_tx.clone(),
            )
            .await;

            if let Err(err) = res {
                log::error!("Error: {err}");
            }
        }

        *done.lock().unwrap() = true;
    });

    Ok(())
}

async fn handle_message(
    is_text: bool,
    data: Vec<u8>,
    address: String,
    private_key: Vec<u8>,
    config: ClientConfig,
    closed: Arc<Mutex<bool>>,
    client_node: Arc<Mutex<Option<Node>>>,
    sig_chain_block_hash: Arc<Mutex<Option<String>>>,
    wallet: Arc<Mutex<Wallet>>,
    stdin_tx: Arc<Mutex<Option<UnboundedSender<WsMessage>>>>,
    connect_tx: Sender<Node>,
    message_tx: Sender<Message>,
    reconnect_tx: Sender<()>,
    reply_tx: Sender<Reply>,
    response_channels: Arc<Mutex<HashMap<String, Channel<Message>>>>,
    curve_secret_key: [u8; SHARED_KEY_LEN],
    shared_keys: Arc<Mutex<HashMap<String, [u8; SHARED_KEY_LEN]>>>,
    challenge_tx: Sender<StSaltAndSignature>,
) -> Result<(), String> {
    if *closed.lock().unwrap() {
        return Ok(());
    }

    if is_text {
        let msg: Value = serde_json::from_slice(&data).unwrap();
        let action = msg["Action"].as_str().unwrap();
        let error: NKNError = NKNError::from(msg["Error"].as_i64().unwrap());

        log::info!("msg: {msg}");
        if error != NKNError::Success {
            if error == NKNError::WrongNode {
                todo!()
            } else if action == "setClient" {
                //close(closed);
            }

            return Err("Error".into());
        }

        match action {
            "setClient" => {
                let result: SetClientResult =
                    serde_json::from_value(msg["Result"].clone()).unwrap();
                *sig_chain_block_hash.lock().unwrap() = Some(result.sig_chain_block_hash);

                if *closed.lock().unwrap() {
                    return Ok(());
                }

                let node = client_node.lock().unwrap().clone().unwrap();
                connect_tx.send(node).unwrap();
            }
            "updateSigChainBlockHash" => {
                *sig_chain_block_hash.lock().unwrap() =
                    Some(msg["Result"].as_str().unwrap().to_string());
            }
            "authChallenge" => {
                let challenge: Value = serde_json::from_value(msg["Challenge"].clone()).unwrap();
                let mut challenge_bytes = hex::decode(challenge.as_str().unwrap()).unwrap();
                let client_salt = thread_rng().gen::<[u8; CLIENT_SALT_LEN]>();
                challenge_bytes.append(&mut client_salt.to_vec());
                let hash = sha256_hash(&challenge_bytes);
                let signature = wallet.lock().unwrap().sign(&hash).to_vec();
                let salt_signature = StSaltAndSignature {
                    client_salt,
                    signature,
                };
                if let Err(err) = challenge_tx.send(salt_signature) {
                    log::error!("send salt and signature error: {err}");
                }
            }
            _ => (),
        }
        Ok(())
    } else {
        let client_msg: ClientMessage = ClientMessage::decode(Bytes::from(data)).unwrap();
        match ClientMessageType::from_i32(client_msg.message_type) {
            Some(ClientMessageType::InboundMessage) => {
                let inbound_msg: InboundMessage =
                    InboundMessage::decode(Bytes::from(client_msg.message)).unwrap();
                if !inbound_msg.prev_hash.is_empty() {
                    let prev_hash = inbound_msg.prev_hash;
                    let stdin_tx = stdin_tx.clone();
                    let reconnect_tx = reconnect_tx.clone();
                    task::spawn(async move {
                        send_receipt(&prev_hash, &private_key, stdin_tx, reconnect_tx).unwrap();
                    });
                }

                let payload_msg =
                    pb::payloads::Message::decode(Bytes::from(inbound_msg.payload)).unwrap();
                let encrypted = payload_msg.encrypted;

                let payload_bytes = if encrypted {
                    decrypt_payload(
                        payload_msg,
                        &inbound_msg.src,
                        &curve_secret_key,
                        &shared_keys,
                    )?
                } else {
                    payload_msg.payload
                };

                let payload: Payload = Payload::decode(Bytes::from(payload_bytes)).unwrap();

                let data = match PayloadType::from_i32(payload.r#type) {
                    Some(PayloadType::Text) => {
                        let text_data: TextData =
                            TextData::decode(Bytes::from(payload.data)).unwrap();
                        text_data.text.as_bytes().to_vec()
                    }
                    Some(PayloadType::Ack) => Vec::new(),
                    _ => payload.data,
                };

                let msg = Message {
                    src: inbound_msg.src,
                    data,
                    r#type: payload.r#type as u32,
                    encrypted,
                    message_id: payload.message_id,
                    no_reply: payload.no_reply,
                    reply_tx,
                };

                if !payload.reply_to_id.is_empty() {
                    let mut response_channels = response_channels.lock().unwrap();
                    let msg_id_str = hex::encode(&payload.reply_to_id).to_string();
                    let channel = response_channels.get(&msg_id_str);

                    if let Some((response_tx, _)) = channel {
                        response_tx.send(msg).unwrap();
                        response_channels.remove(&msg_id_str);
                    }

                    return Ok(());
                }

                if *closed.lock().unwrap() {
                    return Ok(());
                }

                message_tx.send(msg).unwrap();

                Ok(())
            }
            _ => Ok(()),
        }
    }
}

async fn handle_reply(
    reply_rx: Receiver<Reply>,
    ws_write_timeout: u64,
    public_key: Vec<u8>,
    private_key: Vec<u8>,
    address_id: [u8; SHA256_LEN],
    curve_secret_key: [u8; SHARED_KEY_LEN],
    stdin_tx: Arc<Mutex<Option<UnboundedSender<WsMessage>>>>,
    reconnect_tx: Sender<()>,
    node: Arc<Mutex<Option<Node>>>,
    sig_chain_block_hash: Arc<Mutex<Option<String>>>,
    shared_keys: Arc<Mutex<HashMap<String, [u8; SHARED_KEY_LEN]>>>,
    rpc_config: RPCConfig,
) {
    loop {
        let (src, payload, encrypted) = reply_rx.recv().unwrap();
        let reconnect_tx_clone = reconnect_tx.clone();

        let res = send_messages(
            &[src.as_str()],
            payload,
            encrypted,
            0,
            ws_write_timeout,
            &public_key,
            &private_key,
            &address_id,
            &curve_secret_key,
            stdin_tx.clone(),
            reconnect_tx_clone,
            &node,
            &sig_chain_block_hash,
            &shared_keys,
            rpc_config.clone(),
        )
        .await;

        if let Err(err) = res {
            log::error!("Error: {}", err);
        }
    }
}

pub async fn send_messages(
    dests: &[&str],
    payload: Payload,
    encrypted: bool,
    max_holding_seconds: u32,
    ws_write_timeout: u64,
    public_key: &[u8],
    private_key: &[u8],
    address_id: &[u8],
    curve_secret_key: &[u8; SECRET_KEY_LENGTH],
    stdin_tx: Arc<Mutex<Option<UnboundedSender<WsMessage>>>>,
    reconnect_tx: Sender<()>,
    node: &Arc<Mutex<Option<Node>>>,
    sig_chain_block_hash: &Arc<Mutex<Option<String>>>,
    shared_keys: &Arc<Mutex<HashMap<String, [u8; SHARED_KEY_LEN]>>>,
    rpc_config: RPCConfig,
) -> Result<(), String> {
    let dests = process_dests(dests, rpc_config).await?;

    if dests.is_empty() {
        return Ok(());
    }

    let payloads = create_payloads(&dests, payload, encrypted, curve_secret_key, shared_keys)?;

    let mut outbound_msgs = Vec::new();
    let mut dest_list = Vec::new();
    let mut payload_list = Vec::new();

    if payloads.len() > 1 {
        let mut total_size = 0;

        for i in 0..payloads.len() {
            let size = payloads[i].len() + dests[i].len() + SIGNATURE_LEN;

            if size > MAX_CLIENT_MESSAGE_SIZE {
                return Err("message oversize".into());
            }

            if total_size + size > MAX_CLIENT_MESSAGE_SIZE {
                outbound_msgs.push(create_outbound_message(
                    &dest_list,
                    &payload_list,
                    encrypted,
                    max_holding_seconds,
                    public_key,
                    private_key,
                    address_id,
                    node,
                    sig_chain_block_hash,
                )?);
                dest_list.clear();
                payload_list.clear();
                total_size = 0;
            }

            dest_list.push(&dests[i]);
            payload_list.push(&payloads[i]);
            total_size += size;
        }
    } else {
        let mut size = payloads[0].len();

        for dest in &dests {
            size += dest.len() + SIGNATURE_LEN;
        }

        if size > MAX_CLIENT_MESSAGE_SIZE {
            return Err("message oversize".into());
        }

        dest_list = dests.iter().map(|s| s.as_str()).collect();
        payload_list = payloads.iter().map(|p| p.as_slice()).collect();
    }

    outbound_msgs.push(create_outbound_message(
        &dest_list,
        &payload_list,
        encrypted,
        max_holding_seconds,
        public_key,
        private_key,
        address_id,
        node,
        sig_chain_block_hash,
    )?);

    if outbound_msgs.len() > 1 {
        log::info!(
            "Client message size is greater than {} bytes, split into {} batches.",
            MAX_CLIENT_MESSAGE_SIZE,
            outbound_msgs.len()
        );
    }

    for outbound_msg in outbound_msgs {
        let client_msg = create_client_message(&outbound_msg)?;
        let mut msg = Vec::new();
        client_msg
            .encode(&mut msg)
            .expect("encode client message failed");
        match write_message(msg.as_slice(), stdin_tx.clone(), reconnect_tx.clone()) {
            Ok(res) => res,
            Err(err) => {
                log::error!("Error: {}", err);
            }
        };
    }

    Ok(())
}

fn encrypt_payload(
    payload: Payload,
    dests: &[String],
    curve_secret_key: &[u8; SECRET_KEY_LENGTH],
    shared_keys: &Arc<Mutex<HashMap<String, [u8; SHARED_KEY_LEN]>>>,
) -> Result<Vec<Vec<u8>>, String> {
    let mut raw_payload = Vec::new();
    payload
        .encode(&mut raw_payload)
        .expect("encode payload failed");
    let mut rng = thread_rng();

    if dests.len() > 1 {
        let mut key = [0u8; SHARED_KEY_LEN];
        rng.fill(&mut key);

        let mut msg_nonce = [0u8; NONCE_SIZE];
        rng.fill(&mut msg_nonce);

        let encrypted_payload = encrypt(&raw_payload, &msg_nonce, &key);

        let mut msgs = Vec::new();

        for dest in dests {
            let (_, dest_pubkey, _) = parse_client_address(dest)?;
            let shared_key =
                get_or_compute_shared_key(&dest_pubkey, curve_secret_key, shared_keys)?;

            let mut key_nonce = [0u8; NONCE_SIZE];
            rng.fill(&mut key_nonce);

            let encrypted_key = encrypt(&key, &key_nonce, &shared_key);
            let nonce = [key_nonce, msg_nonce].concat();

            let pb_message = pb::payloads::Message {
                payload: encrypted_payload.clone(),
                encrypted: true,
                nonce,
                encrypted_key,
            };
            let mut msg = Vec::new();
            pb_message
                .encode(&mut msg)
                .expect("encode pb message failed");
            msgs.push(msg);
        }

        Ok(msgs)
    } else {
        let (_, dest_pubkey, _) = parse_client_address(&dests[0])?;
        let shared_key = get_or_compute_shared_key(&dest_pubkey, curve_secret_key, shared_keys)?;

        let mut nonce = [0u8; NONCE_SIZE];
        rng.fill(&mut nonce);

        let encrypted_payload = encrypt(&raw_payload, &nonce, &shared_key);

        let pb_message = pb::payloads::Message {
            payload: encrypted_payload,
            encrypted: true,
            nonce: nonce.to_vec(),
            encrypted_key: vec![],
        };
        let mut msg = Vec::new();
        pb_message
            .encode(&mut msg)
            .expect("encode pb message failed");

        Ok(vec![msg])
    }
}

fn decrypt_payload(
    msg: pb::payloads::Message,
    src_address: &str,
    curve_secret_key: &[u8; SECRET_KEY_LENGTH],
    shared_keys: &Arc<Mutex<HashMap<String, [u8; SHARED_KEY_LEN]>>>,
) -> Result<Vec<u8>, String> {
    let encrypted_payload = msg.payload;
    let (_, src_pubkey, _) = parse_client_address(src_address)?;

    if !msg.encrypted_key.is_empty() {
        let shared_key = get_or_compute_shared_key(&src_pubkey, curve_secret_key, shared_keys)?;
        let key = decrypt(&msg.encrypted_key, &msg.nonce[..NONCE_SIZE], &shared_key).unwrap();
        let payload =
            decrypt(&encrypted_payload, &msg.nonce[NONCE_SIZE..], key.as_slice()).unwrap();
        Ok(payload)
    } else {
        let shared_key = get_or_compute_shared_key(&src_pubkey, curve_secret_key, shared_keys)?;
        let payload = decrypt(&encrypted_payload, &msg.nonce, &shared_key).unwrap();
        Ok(payload)
    }
}

pub async fn process_dests(dests: &[&str], rpc_config: RPCConfig) -> Result<Vec<String>, String> {
    if dests.is_empty() {
        return Ok(Vec::new());
    }

    let mut processed_dests = Vec::new();

    for dest in dests {
        let mut address: Vec<String> = dest.split('.').map(|s| s.to_string()).collect();

        if address.last().unwrap().len() < 2 * PUBLIC_KEY_LEN {
            let res = get_registrant(address.last().unwrap(), rpc_config.clone()).await;

            let reg = match res {
                Ok(reg) => reg,
                Err(_) => continue,
            };

            if reg.registrant.is_empty() {
                continue;
            }

            *address.last_mut().unwrap() = reg.registrant;
        }

        let processed_dest = address.join(".");

        processed_dests.push(processed_dest);
    }

    if processed_dests.is_empty() {
        Err("invalid destination".into())
    } else {
        Ok(processed_dests)
    }
}

fn create_payloads(
    dests: &[String],
    payload: Payload,
    encrypted: bool,
    curve_secret_key: &[u8; SECRET_KEY_LENGTH],
    shared_keys: &Arc<Mutex<HashMap<String, [u8; SHARED_KEY_LEN]>>>,
) -> Result<Vec<Vec<u8>>, String> {
    if encrypted {
        Ok(encrypt_payload(
            payload,
            dests,
            curve_secret_key,
            shared_keys,
        )?)
    } else {
        let mut payload_data = Vec::new();
        payload
            .encode(&mut payload_data)
            .expect("encode payload failed");
        let pb_message = pb::payloads::Message {
            payload: payload_data,
            encrypted: false,
            nonce: Vec::new(),
            encrypted_key: Vec::new(),
        };
        let mut pb_message_data = Vec::new();
        pb_message
            .encode(&mut pb_message_data)
            .expect("encode pb message failed");

        Ok(vec![pb_message_data])
    }
}

fn create_outbound_message(
    dests: &[&str],
    payloads: &[&[u8]],
    encrypted: bool,
    max_holding_seconds: u32,
    public_key: &[u8],
    private_key: &[u8],
    address_id: &[u8],
    node: &Arc<Mutex<Option<Node>>>,
    sig_chain_block_hash: &Arc<Mutex<Option<String>>>,
) -> Result<OutboundMessage, String> {
    let mut rng = thread_rng();
    let nonce = rng.gen::<u32>();
    let mut outbound_msg = OutboundMessage {
        dest: String::new(),
        dests: dests.iter().map(|s| s.to_string()).collect(),
        payload: Vec::new(),
        max_holding_seconds,
        nonce,
        block_hash: Vec::new(),
        signatures: Vec::new(),
        payloads: payloads.iter().map(|v| v.to_vec()).collect(),
    };

    let node_public_key = hex::decode(&node.lock().unwrap().as_ref().unwrap().pubkey).unwrap();
    let sig_chain_element = SigChainElem {
        id: Vec::new(),
        next_pubkey: node_public_key,
        mining: false,
        signature: Vec::new(),
        sig_algo: i32::from(SigAlgo::Signature),
        vrf: Vec::new(),
        proof: Vec::new(),
    };
    let sig_chain_element_ser = sig_chain_element.serialize_unsigned();

    let mut sig_chain = SigChain {
        nonce,
        data_size: 0,
        block_hash: Vec::new(),
        src_id: address_id.to_vec(),
        src_pubkey: public_key.to_vec(),
        dest_id: Vec::new(),
        dest_pubkey: Vec::new(),
        elems: vec![sig_chain_element],
    };

    if let Some(sig_chain_block_hash) = &*sig_chain_block_hash.lock().unwrap() {
        let sig_chain_block_hash = hex::decode(sig_chain_block_hash).unwrap();
        sig_chain.block_hash = sig_chain_block_hash.clone();
        outbound_msg.block_hash = sig_chain_block_hash;
    }

    let mut signatures = Vec::new();

    for (i, dest) in dests.iter().enumerate() {
        let (dest_id, dest_public_key, _) = parse_client_address(dest)?;
        sig_chain.dest_id = dest_id.to_vec();
        sig_chain.dest_pubkey = Vec::from(dest_public_key);

        if payloads.len() > 1 {
            sig_chain.data_size = payloads[i].len() as u32;
        } else {
            sig_chain.data_size = payloads[0].len() as u32;
        }

        let metadata = sig_chain.serialize_metadata();
        let mut digest = sha256_hash(&metadata).to_vec();
        digest.extend_from_slice(&sig_chain_element_ser);
        let digest = sha256_hash(&digest);

        let signature = ed25519_sign(&digest, [private_key, public_key].concat().as_slice());
        signatures.push(signature.to_vec());
    }

    outbound_msg.signatures = signatures;
    outbound_msg.nonce = nonce;
    Ok(outbound_msg)
}

fn create_client_message(outbound_msg: &OutboundMessage) -> Result<ClientMessage, String> {
    let mut outbound_msg_data = Vec::new();
    outbound_msg
        .encode(&mut outbound_msg_data)
        .expect("TODO: panic message");

    if outbound_msg.payloads.len() > 1 {
        let mut e = ZlibEncoder::new(Vec::new(), Compression::default());
        e.write_all(&outbound_msg_data).expect("write_all failed");
        let message = e.finish().unwrap();

        Ok(ClientMessage {
            message_type: i32::from(ClientMessageType::OutboundMessage),
            compression_type: i32::from(CompressionType::CompressionZlib),
            message,
        })
    } else {
        Ok(ClientMessage {
            message_type: i32::from(ClientMessageType::OutboundMessage),
            compression_type: i32::from(CompressionType::CompressionNone),
            message: outbound_msg_data,
        })
    }
}

fn write_message(
    data: &[u8],
    stdin_tx: Arc<Mutex<Option<UnboundedSender<WsMessage>>>>,
    reconnect_tx: Sender<()>,
) -> Result<(), String> {
    let res = stdin_tx
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .unbounded_send(WsMessage::Binary(data.to_vec()));

    match res {
        Ok(_) => Ok(()),
        Err(err) => {
            //reconnect_tx.send(()).unwrap();
            Err(err.to_string())
        }
    }
}

fn send_receipt(
    prev_signature: &[u8],
    private_key: &[u8],
    stdin_tx: Arc<Mutex<Option<UnboundedSender<WsMessage>>>>,
    reconnect_tx: Sender<()>,
) -> Result<(), String> {
    let sig_chain_element = SigChainElem::default();
    let sig_chain_element_ser = sig_chain_element.serialize_unsigned();

    let mut digest = sha256_hash(prev_signature).to_vec();
    digest.extend_from_slice(&sig_chain_element_ser);
    let digest = sha256_hash(&digest);
    let public_key: PublicKey = (&SecretKey::from_bytes(&private_key).unwrap()).into();
    let signature = ed25519_sign(
        &digest,
        [private_key, public_key.to_bytes().as_slice()]
            .concat()
            .as_slice(),
    );

    let receipt = Receipt {
        prev_hash: prev_signature.to_vec(),
        signature: signature.to_vec(),
    };
    let mut receipt_data = Vec::new();
    receipt
        .encode(&mut receipt_data)
        .expect("encode receipt failed");

    let client_msg = ClientMessage {
        message_type: i32::from(ClientMessageType::Receipt),
        message: receipt_data,
        compression_type: i32::from(CompressionType::CompressionNone),
    };
    let mut client_msg_data = Vec::new();
    client_msg
        .encode(&mut client_msg_data)
        .expect("encode client message failed");

    write_message(&client_msg_data, stdin_tx, reconnect_tx)
}
