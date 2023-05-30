// use crate::constants::{
//     DEFAULT_RPC_CONCURRENCY, DEFAULT_RPC_SERVER, DEFAULT_RPC_TIMEOUT, PUBLIC_KEY_LEN, SHA256_LEN,
//     SHARED_KEY_LEN, SIGNATURE_LEN,
// };
// use crate::message::{Message, MessageConfig, Reply};
// use crate::rpc::{get_node_state, get_ws_address, Node, NodeState, RPCConfig};
// use crate::utils::client_config_to_rpc_config;
// use crate::wallet::Wallet;
// use futures_channel::mpsc::UnboundedSender;
// use std::collections::HashMap;
// use std::sync::mpsc::{Receiver, Sender};
// use std::sync::{Arc, Mutex};
// use std::time::Duration;
// use tokio::time::sleep;
// use tokio_tungstenite::{
//     connect_async, tungstenite::protocol::Message as WsMessage, MaybeTlsStream, WebSocketStream,
// };
//
// type Channel<T> = (Sender<T>, Receiver<T>);
//
// #[derive(Debug, Clone)]
// pub struct ClientConfig {
//     pub rpc_server_address: Vec<String>,
//     pub rpc_timeout: u64,
//     pub rpc_concurrency: u32,
//     pub msg_chan_length: u32,
//     pub connect_retries: u32,
//     pub msg_cache_expiration: u32,
//     pub msg_cache_cleanup_interval: u32,
//     pub ws_handshake_timeout: u64,
//     pub ws_write_timeout: u64,
//     pub min_reconnect_interval: u64,
//     pub max_reconnect_interval: u64,
//
//     pub message_config: Option<MessageConfig>,
// }
//
// impl Default for ClientConfig {
//     fn default() -> Self {
//         Self {
//             rpc_server_address: DEFAULT_RPC_SERVER.iter().map(|s| s.to_string()).collect(),
//             rpc_timeout: DEFAULT_RPC_TIMEOUT,
//             rpc_concurrency: DEFAULT_RPC_CONCURRENCY,
//             msg_chan_length: 1024,
//             connect_retries: 3,
//             msg_cache_expiration: 300000,
//             msg_cache_cleanup_interval: 60000,
//             ws_handshake_timeout: 5000,
//             ws_write_timeout: 10000,
//             min_reconnect_interval: 1000,
//             max_reconnect_interval: 64000,
//             message_config: None,
//         }
//     }
// }
//
// async fn handle_reconnect(
//     reconnect_rx: Receiver<()>,
//     address: String,
//     private_key: Vec<u8>,
//     config: Arc<Mutex<ClientConfig>>,
//     closed: Arc<Mutex<bool>>,
//     client_node: Arc<Mutex<Option<Node>>>,
//     sig_chain_block_hash: Arc<Mutex<Option<String>>>,
//     wallet: Arc<Mutex<Wallet>>,
//     stdin_tx: Arc<Mutex<Option<UnboundedSender<WsMessage>>>>,
//     connect_tx: Sender<Node>,
//     message_tx: Sender<Message>,
//     reconnect_tx: Sender<()>,
//     reply_tx: Sender<Reply>,
//     response_channels: Arc<Mutex<HashMap<String, Channel<Message>>>>,
//     curve_secret_key: [u8; SHARED_KEY_LEN],
//     shared_keys: Arc<Mutex<HashMap<String, [u8; SHARED_KEY_LEN]>>>,
// ) {
//     loop {
//         reconnect_rx.recv().unwrap();
//
//         if *closed.lock().unwrap() {
//             return;
//         }
//
//         let min_reconnect_interval = config.lock().unwrap().min_reconnect_interval;
//         log::info!("Reconnect in {} ms...", min_reconnect_interval);
//         sleep(Duration::from_millis(min_reconnect_interval)).await;
//
//         let connect_tx_clone = connect_tx.clone();
//         let message_tx_clone = message_tx.clone();
//         let reply_tx_clone = reply_tx.clone();
//         let reconnect_tx_clone = reconnect_tx.clone();
//         let res = connect(
//             0,
//             address.clone(),
//             private_key.clone(),
//             config.clone(),
//             closed.clone(),
//             client_node.clone(),
//             sig_chain_block_hash.clone(),
//             wallet.clone(),
//             stdin_tx.clone(),
//             connect_tx_clone,
//             message_tx_clone,
//             reconnect_tx_clone,
//             reply_tx_clone,
//             response_channels.clone(),
//             curve_secret_key.clone(),
//             shared_keys.clone(),
//         )
//         .await;
//
//         if let Err(err) = res {
//             log::error!("Error: {}", err);
//             close(closed.clone());
//         }
//     }
// }
//
// fn close(closed: Arc<Mutex<bool>>) {
//     *closed.lock().unwrap() = true;
//     todo!(); // close connection
// }
//
// async fn connect(
//     max_retries: u32,
//     address: String,
//     private_key: Vec<u8>,
//     config: Arc<Mutex<ClientConfig>>,
//     closed: Arc<Mutex<bool>>,
//     client_node: Arc<Mutex<Option<Node>>>,
//     sig_chain_block_hash: Arc<Mutex<Option<String>>>,
//     wallet: Arc<Mutex<Wallet>>,
//     stdin_tx: Arc<Mutex<Option<UnboundedSender<WsMessage>>>>,
//     connect_tx: Sender<Node>,
//     message_tx: Sender<Message>,
//     reconnect_tx: Sender<()>,
//     reply_tx: Sender<Reply>,
//     response_channels: Arc<Mutex<HashMap<String, Channel<Message>>>>,
//     curve_secret_key: [u8; SHARED_KEY_LEN],
//     shared_keys: Arc<Mutex<HashMap<String, [u8; SHARED_KEY_LEN]>>>,
// ) -> Result<(), String> {
//     let max_reconnect_interval = config.lock().unwrap().max_reconnect_interval;
//
//     let mut retry_interval = config.lock().unwrap().min_reconnect_interval;
//     let mut retry = 0;
//
//     while max_retries == 0 || retry < max_retries {
//         if retry > 0 {
//             log::info!("Retry in {} ms...", retry_interval);
//             sleep(Duration::from_millis(retry_interval)).await;
//
//             retry_interval *= 2;
//             if retry_interval > max_reconnect_interval {
//                 retry_interval = max_reconnect_interval;
//             }
//         }
//
//         let rpc_config = client_config_to_rpc_config(&config.lock().unwrap());
//         let res = get_ws_address(&address, rpc_config).await;
//
//         match res {
//             Ok(node) => {
//                 let connect_tx_clone = connect_tx.clone();
//                 let message_tx_clone = message_tx.clone();
//                 let reply_tx_clone = reply_tx.clone();
//                 let reconnect_tx_clone = reconnect_tx.clone();
//                 let res = connect_to_node(
//                     node,
//                     address.clone(),
//                     private_key.clone(),
//                     config.clone(),
//                     closed.clone(),
//                     client_node.clone(),
//                     sig_chain_block_hash.clone(),
//                     wallet.clone(),
//                     stdin_tx.clone(),
//                     connect_tx_clone,
//                     message_tx_clone,
//                     reconnect_tx_clone,
//                     reply_tx_clone,
//                     response_channels.clone(),
//                     curve_secret_key.clone(),
//                     shared_keys.clone(),
//                 )
//                 .await;
//
//                 match res {
//                     Ok(()) => return Ok(()),
//                     Err(err) => log::error!("Error: {}", err),
//                 }
//             }
//             Err(err) => log::error!("Error: {}", err),
//         }
//
//         retry += 1;
//     }
//
//     Err("connect failed".into())
// }
//
// async fn handle_reply(
//     reply_rx: Receiver<Reply>,
//     ws_write_timeout: u64,
//     public_key: Vec<u8>,
//     private_key: Vec<u8>,
//     address_id: [u8; SHA256_LEN],
//     curve_secret_key: [u8; SHARED_KEY_LEN],
//     stdin_tx: Arc<Mutex<Option<UnboundedSender<WsMessage>>>>,
//     reconnect_tx: Sender<()>,
//     node: Arc<Mutex<Option<Node>>>,
//     sig_chain_block_hash: Arc<Mutex<Option<String>>>,
//     shared_keys: Arc<Mutex<HashMap<String, [u8; SHARED_KEY_LEN]>>>,
//     rpc_config: RPCConfig,
// ) {
//     loop {
//         let (src, payload, encrypted) = reply_rx.recv().unwrap();
//
//         let reconnect_tx_clone = reconnect_tx.clone();
//         let res = send_messages(
//             &[src.as_str()],
//             payload,
//             encrypted,
//             0,
//             ws_write_timeout,
//             &public_key,
//             &private_key,
//             &address_id,
//             &curve_secret_key,
//             stdin_tx.clone(),
//             reconnect_tx_clone,
//             &node,
//             &sig_chain_block_hash,
//             &shared_keys,
//             rpc_config.clone(),
//         )
//         .await;
//
//         if let Err(err) = res {
//             log::error!("Error: {}", err);
//         }
//     }
// }
//
// async fn send_messages(
//     dests: &[&str],
//     payload: MessagePayload,
//     encrypted: bool,
//     max_holding_seconds: u32,
//     ws_write_timeout: u64,
//     public_key: &[u8],
//     private_key: &[u8],
//     address_id: &[u8],
//     curve_secret_key: &[u8],
//     stdin_tx: Arc<Mutex<Option<UnboundedSender<WsMessage>>>>,
//     reconnect_tx: Sender<()>,
//     node: &Arc<Mutex<Option<Node>>>,
//     sig_chain_block_hash: &Arc<Mutex<Option<String>>>,
//     shared_keys: &Arc<Mutex<HashMap<String, [u8; SHARED_KEY_LEN]>>>,
//     rpc_config: RPCConfig,
// ) -> Result<(), String> {
//     let dests = process_dests(dests, rpc_config).await?;
//
//     if dests.is_empty() {
//         return Ok(());
//     }
//
//     let payloads = create_payloads(&dests, payload, encrypted, curve_secret_key, shared_keys)?;
//
//     let mut outbound_msgs = Vec::new();
//     let mut dest_list = Vec::new();
//     let mut payload_list = Vec::new();
//
//     if payloads.len() > 1 {
//         let mut total_size = 0;
//
//         for i in 0..payloads.len() {
//             let size = payloads[i].len() + dests[i].len() + SIGNATURE_LEN;
//
//             if size > MAX_CLIENT_MESSAGE_SIZE {
//                 return Err("message oversize".into());
//             }
//
//             if total_size + size > MAX_CLIENT_MESSAGE_SIZE {
//                 outbound_msgs.push(create_outbound_message(
//                     &dest_list,
//                     &payload_list,
//                     encrypted,
//                     max_holding_seconds,
//                     public_key,
//                     private_key,
//                     address_id,
//                     node,
//                     sig_chain_block_hash,
//                 )?);
//                 dest_list.clear();
//                 payload_list.clear();
//                 total_size = 0;
//             }
//
//             dest_list.push(&dests[i]);
//             payload_list.push(&payloads[i]);
//             total_size += size;
//         }
//     } else {
//         let mut size = payloads[0].len();
//
//         for dest in &dests {
//             size += dest.len() + SIGNATURE_LEN;
//         }
//
//         if size > MAX_CLIENT_MESSAGE_SIZE {
//             return Err("message oversize".into());
//         }
//
//         dest_list = dests.iter().map(|s| s.as_str()).collect();
//         payload_list = payloads.iter().map(|p| p.as_slice()).collect();
//     }
//
//     outbound_msgs.push(create_outbound_message(
//         &dest_list,
//         &payload_list,
//         encrypted,
//         max_holding_seconds,
//         public_key,
//         private_key,
//         address_id,
//         node,
//         sig_chain_block_hash,
//     )?);
//
//     if outbound_msgs.len() > 1 {
//         log::info!(
//             "Client message size is greater than {} bytes, split into {} batches.",
//             MAX_CLIENT_MESSAGE_SIZE,
//             outbound_msgs.len()
//         );
//     }
//
//     for outbound_msg in outbound_msgs {
//         let client_msg = create_client_message(&outbound_msg)?;
//         write_message(
//             &serde_json::to_vec(&client_msg).unwrap(),
//             stdin_tx.clone(),
//             reconnect_tx.clone(),
//         );
//     }
//
//     Ok(())
// }
