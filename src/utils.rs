use crate::client::ClientConfig;
use crate::constants::{
    CHECKSUM_LEN, CLIENT_SALT_LEN, PRIVATE_KEY_LEN, PUBLIC_KEY_LEN, SHA256_LEN, SHARED_KEY_LEN,
    SIGNATURE_LEN, UINT160SIZE,
};
use crate::crypto::{ed25519_public_key_to_curve25519_public_key, sha256_hash};
use crate::rpc::RPCConfig;
use crate::session::MIN_SEQUENCE_ID;
use crossbeam_channel::bounded;
use crossbeam_channel::{Receiver, Sender};
use regex::Regex;
use serde::{Deserialize, Serialize};
use sodiumoxide::crypto::box_::{precompute, PublicKey, SecretKey};
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};
use std::str;
use std::string;
use std::sync::{Arc, Mutex};

//static RE: Regex = Regex::new(r"^__\\d+__$").unwrap();

pub(crate) type Channel<T> = (Sender<T>, Receiver<T>);

// +1 for avoid affected by lower 192bits shift-add
const FOOLPROOFPREFIX: &[u8] = &[0x02, 0xb8, 0x25];

pub fn client_config_to_rpc_config(config: &ClientConfig) -> RPCConfig {
    RPCConfig {
        rpc_server_address: config.rpc_server_address.clone(),
        rpc_timeout: config.rpc_timeout,
        rpc_concurrency: config.rpc_concurrency,
    }
}

pub fn make_address_string(public_key: &[u8], identifier: &String) -> String {
    let pubkey_str = hex::encode(public_key);
    if identifier.is_empty() {
        pubkey_str
    } else {
        format!("{identifier}.{pubkey_str}")
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct StSaltAndSignature {
    pub client_salt: [u8; CLIENT_SALT_LEN],
    pub signature: Vec<u8>,
}

pub fn parse_client_address(
    address_str: &str,
) -> Result<([u8; SHA256_LEN], [u8; PUBLIC_KEY_LEN], String), String> {
    let client_id = sha256_hash(address_str.as_bytes());
    let substrings: Vec<&str> = address_str.split('.').collect();
    let public_key_str = substrings.last().unwrap();
    let public_key = hex::decode(public_key_str)
        .map_err(|_| "Invalid public key string converting to hex".to_string())?;
    let identifier = substrings[..substrings.len() - 1].join(".");
    let key = <&[u8; PUBLIC_KEY_LEN]>::try_from(public_key.as_slice());
    let k = key.map_err(|_| "Invalid public key string".to_string())?;
    Ok((client_id, *k, identifier))
}

pub fn get_or_compute_shared_key(
    remote_public_key: &[u8; PUBLIC_KEY_LEN],
    curve_secret_key: &[u8; PRIVATE_KEY_LEN],
    shared_keys: &Arc<Mutex<HashMap<String, [u8; SHARED_KEY_LEN]>>>,
) -> Result<[u8; SHARED_KEY_LEN], String> {
    let remote_public_key_str = String::from_utf8_lossy(remote_public_key);
    let shared_keys_lock = shared_keys.lock().unwrap();

    if let Some(shared_key) = shared_keys_lock.get(remote_public_key_str.as_ref()) {
        Ok(*shared_key)
    } else {
        drop(shared_keys_lock);

        if remote_public_key.len() != PUBLIC_KEY_LEN {
            return Err("invalid public key size".into());
        }

        let curve_public_key = ed25519_public_key_to_curve25519_public_key(remote_public_key);
        let shared_key = precompute(&PublicKey(curve_public_key), &SecretKey(*curve_secret_key));

        shared_keys
            .lock()
            .unwrap()
            .insert(remote_public_key_str.into(), shared_key.0);

        Ok(shared_key.0)
    }
}

pub fn uint160_from_bytes(bytes: Vec<u8>) -> [u8; UINT160SIZE] {
    let mut result = [0u8; UINT160SIZE];
    result.copy_from_slice(&bytes[..UINT160SIZE]);
    result
}

pub fn add_identifier(addr: String, id: i32) -> String {
    if id < 0 {
        return addr;
    }
    let underscore = "__";
    add_identifier_prefix(addr, format!("{underscore}{id}{underscore}"))
}

pub fn add_identifier_prefix(base: String, prefix: String) -> String {
    if prefix.is_empty() {
        return base;
    }
    if base.is_empty() {
        return prefix;
    }

    format!("{prefix}.{base}")
}

pub fn add_multiclient_prefix(dest: &Vec<String>, client_id: i32) -> Vec<String> {
    let mut result = Vec::new();
    for addr in dest {
        result.push(add_identifier(addr.clone(), client_id));
    }
    result
}

pub fn remote_identifier(src: String) -> (String, String) {
    let s = src.split('.').collect::<Vec<&str>>();
    let RE: Regex = Regex::new(r"^__\\d+__$").unwrap();

    if s.len() > 1 && RE.is_match(s[0]) {
        return (s[0].to_string(), s[1].to_string());
    }
    (src, "".to_string())
}

pub fn next_seq(seq: u32, step: i64) -> u32 {
    let max: i64 = (u32::MAX - MIN_SEQUENCE_ID + 1) as i64;
    let mut res = (seq as i64 - MIN_SEQUENCE_ID as i64 + step) % max;
    if res < 0 {
        res += max;
    }
    (res + MIN_SEQUENCE_ID as i64) as u32
}

pub fn seq_in_between(seq: u32, start: u32, end: u32) -> bool {
    if start <= end {
        return seq >= start && seq < end;
    }
    seq >= start || seq < end
}

pub fn compare_seq(a: u32, b: u32) -> i32 {
    if a == b {
        return 0;
    }
    if a < b {
        if b - a < u32::MAX / 2 {
            return -1;
        }
        return 1;
    }
    if a - b < u32::MAX / 2 {
        return 1;
    }
    -1
}

pub fn conn_key(local_client_id: String, remote_client_id: String) -> String {
    format!("{local_client_id} - {remote_client_id}")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeqElem {
    pub v: u32,
}
pub type SeqHeap = BinaryHeap<SeqElem>;

impl PartialOrd<Self> for SeqElem {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        let res = compare_seq(self.v, other.v);
        if res == 0 {
            Some(Ordering::Equal)
        } else if res == -1 {
            Some(Ordering::Less)
        } else {
            Some(Ordering::Greater)
        }
    }
}

impl Ord for SeqElem {
    fn cmp(&self, other: &Self) -> Ordering {
        let res = compare_seq(self.v, other.v);
        if res == 0 {
            Ordering::Equal
        } else if res == -1 {
            Ordering::Less
        } else {
            Ordering::Greater
        }
    }
}
