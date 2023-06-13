use crate::constants::{DEFAULT_RPC_CONCURRENCY, DEFAULT_RPC_TIMEOUT};
use crate::tx::Transaction;
use hyper::{body, client::HttpConnector, Body, Client, Method, Request};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::{Arc, Mutex};
use std::{str, time::Duration};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Node {
    pub addr: String,
    #[serde(rename = "rpcAddr")]
    pub rpc_addr: String,
    pub pubkey: String,
    pub id: String,
}

#[derive(Debug, Clone)]
pub struct RPCConfig {
    pub rpc_server_address: Vec<String>,
    pub rpc_timeout: u64,
    pub rpc_concurrency: u32,
}

impl Default for RPCConfig {
    fn default() -> Self {
        Self {
            rpc_server_address: Vec::new(),
            rpc_timeout: DEFAULT_RPC_TIMEOUT,
            rpc_concurrency: DEFAULT_RPC_CONCURRENCY,
        }
    }
}

#[derive(Deserialize)]
struct RPCError {
    code: i32,
    message: String,
    data: String,
}

#[derive(Deserialize)]
struct RPCResponse<D> {
    result: Option<D>,
    error: Option<RPCError>,
}

async fn http_post<D: DeserializeOwned>(
    client: &Client<HttpConnector>,
    uri: &str,
    body: String,
) -> Result<D, String> {
    let req = Request::builder()
        .method(Method::POST)
        .uri(uri)
        .body(Body::from(body))
        .expect("request builder");

    let res = client
        .request(req)
        .await
        .map_err(|err| format!("Client: {err}"))?;
    let body = body::to_bytes(res.into_body())
        .await
        .map_err(|err| format!("Body: {err}"))?;
    let body_str = str::from_utf8(&body).map_err(|err| format!("Body: {err}"))?;
    //println!("body: {:?}", body_str);
    let res: RPCResponse<D> =
        serde_json::from_str(body_str).map_err(|err| format!("Json: {err}"))?;
    if let Some(err) = res.error {
        return Err(format!("{}: {}", err.message, err.data));
    }
    Ok(res.result.unwrap())
}

pub async fn rpc_call<S: Serialize, D: DeserializeOwned + Send + 'static>(
    method: &str,
    params: S,
    config: RPCConfig,
) -> Result<D, String> {
    if config.rpc_server_address.is_empty() {
        return Err("No server addresses in config".into());
    }

    let https = HttpConnector::new();
    let client = Client::builder()
        .pool_idle_timeout(Duration::from_secs(config.rpc_timeout))
        .build::<_, Body>(https);

    let body = json!({
        "id": "nkn-sdk-go",
        "method": method,
        "params": params,
    })
    .to_string();

    let result: Arc<Mutex<Result<D, String>>> = Arc::new(Mutex::new(Err("couldn't find".into())));

    let n = if config.rpc_concurrency > 0 {
        config.rpc_concurrency as usize
    } else {
        config.rpc_server_address.len()
    };

    let mut join_handles = Vec::new();

    for i in 0..n {
        let (left, right) = config
            .rpc_server_address
            .split_at(i % config.rpc_server_address.len());
        let addresses = [right, left].concat();
        let result = result.clone();
        let client = client.clone();
        let body = body.clone();

        join_handles.push(tokio::spawn(async move {
            for address in &addresses {
                if result.lock().unwrap().is_ok() {
                    return;
                }

                let res = http_post(&client, address, body.clone()).await;

                let mut lock = result.lock().unwrap();
                if lock.is_err() {
                    *lock = res;
                }
            }
        }));
    }

    for join_handle in join_handles.drain(..) {
        join_handle.await.unwrap();
    }

    Arc::try_unwrap(result)
        .map_err(|_| "couldn't unwrap")?
        .into_inner()
        .unwrap()
}

#[derive(Debug, Deserialize, Serialize)]
struct Balance {
    pub amount: i64,
}

pub async fn get_balance(address: &str, config: RPCConfig) -> Result<i64, String> {
    let balance: Balance =
        rpc_call("getbalancebyaddr", json!({ "address": address }), config).await?;
    Ok(balance.amount)
}

pub async fn get_height(config: RPCConfig) -> Result<u32, String> {
    rpc_call("getlatestblockheight", json!({}), config).await
}

pub async fn get_ws_address(client_address: &str, config: RPCConfig) -> Result<Node, String> {
    rpc_call("getwsaddr", json!({ "address": client_address }), config).await
}

pub async fn get_wss_address(client_address: &str, config: RPCConfig) -> Result<Node, String> {
    rpc_call("getwssaddr", json!({ "address": client_address }), config).await
}

pub async fn get_registrant(name: &str, config: RPCConfig) -> Result<Registrant, String> {
    rpc_call("getregistrant", json!({ "name": name }), config).await
}

pub async fn send_raw_transaction(tx: &Transaction, config: RPCConfig) -> Result<String, String> {
    let buf = tx.encode();
    let encoded = String::from_utf8(buf).map_err(|err| format!("Utf8: {err}"))?;
    rpc_call("sendrawtransaction", json!({ "tx": encoded }), config).await
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeState {
    pub addr: String,
    pub curr_time_stamp: u64,
    pub height: u32,
    pub id: String,
    pub json_rpc_port: u32,
    pub proposal_submitted: u32,
    pub protocol_version: u32,
    pub public_key: String,
    pub relay_message_count: u64,
    pub sync_state: String,
    pub tls_json_rpc_domain: String,
    pub tls_json_rpc_port: u32,
    pub tls_websocket_domain: String,
    pub tls_websocket_port: u32,
    pub uptime: u64,
    pub version: String,
    pub websocket_port: u32,
}

pub async fn get_node_state(config: RPCConfig) -> Result<NodeState, String> {
    rpc_call("getnodestate", json!({}), config).await
}

#[derive(Debug, Deserialize)]
pub struct Subscription {
    pub meta: String,
    pub expires_at: u64,
}

#[derive(Debug, Deserialize)]
pub struct Registrant {
    pub registrant: String,
    pub expires_at: u64,
}
