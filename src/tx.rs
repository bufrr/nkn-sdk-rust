use crate::constants::*;
use crate::payload::{
    new_generate_id, new_nanopay, new_sigchain, new_subscribe, new_transfer_asset, new_unsubscribe,
    pack_payload,
};
use crate::pb::transaction::Transaction as pb_transcation;
use crate::pb::transaction::*;
use crate::signature::SignableData;
use prost::Message;
use rand::{thread_rng, Rng};

const TRANSACTION_NONCE_LENGTH: usize = 32;

#[derive(Debug, Clone, Default)]
pub struct Transaction {
    pub pb_tx: pb_transcation,
    hash: [u8; UINT256SIZE],
    size: u32,
    is_signature_verified: bool,
}

impl Transaction {
    pub fn new(
        pb: pb_transcation,
        hash: [u8; UINT256SIZE],
        size: u32,
        is_signature_verified: bool,
    ) -> Self {
        Self {
            pb_tx: pb,
            hash,
            size,
            is_signature_verified,
        }
    }

    // Same as `Marshal()` in Go SDK
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.reserve(self.pb_tx.encoded_len());
        self.pb_tx.encode(&mut buf).unwrap();
        buf
    }

    // Same as `Unmarshal()` in Go SDK
    pub fn decode(buf: &[u8]) -> Self {
        let pb_tx = pb_transcation::decode(buf).unwrap();

        Self {
            pb_tx,
            ..Default::default()
        }
    }

    pub fn new_nanopay(
        sender: &[u8; UINT160SIZE],
        recipient: &[u8; UINT160SIZE],
        id: u64,
        amount: i64,
        txn_expiration: u32,
        nano_pay_expiration: u32,
    ) -> Self {
        let mut payload = new_nanopay(
            sender,
            recipient,
            id,
            amount,
            txn_expiration,
            nano_pay_expiration,
        );
        let pl = pack_payload(&PayloadType::NanoPayType, &mut payload);
        let tx = new_msg_tx(
            pl,
            0,
            0,
            &thread_rng().gen::<[u8; TRANSACTION_NONCE_LENGTH]>(),
        );

        Transaction {
            pb_tx: tx,
            ..Default::default()
        }
    }

    pub fn new_transfer_asset(
        sender: &[u8; UINT160SIZE],
        recipient: &[u8; UINT160SIZE],
        amount: i64,
        nonce: u64,
        fee: i64,
        attrs: &[u8],
    ) -> Self {
        let mut payload = new_transfer_asset(sender, recipient, amount);
        let pl = pack_payload(&PayloadType::TransferAssetType, &mut payload);
        let tx = new_msg_tx(pl, nonce, fee, attrs);

        Transaction {
            pb_tx: tx,
            ..Default::default()
        }
    }

    pub fn new_sigchain(sig_chain: &[u8], submitter: &[u8; UINT160SIZE], nonce: u64) -> Self {
        let mut payload = new_sigchain(sig_chain, submitter);
        let pl = pack_payload(&PayloadType::SigChainTxnType, &mut payload);
        let tx = new_msg_tx(
            pl,
            nonce,
            0,
            &thread_rng().gen::<[u8; TRANSACTION_NONCE_LENGTH]>(),
        );

        Transaction {
            pb_tx: tx,
            ..Default::default()
        }
    }

    pub fn new_generate_id(
        public_key: &[u8],
        sender: &[u8],
        registration_fee: i64,
        version: i32,
        nonce: u64,
        fee: i64,
        attrs: &[u8],
    ) -> Self {
        let mut payload = new_generate_id(public_key, sender, registration_fee, version);
        let pl = pack_payload(&PayloadType::GenerateIdType, &mut payload);
        let tx = new_msg_tx(pl, nonce, fee, attrs);
        Transaction {
            pb_tx: tx,
            ..Default::default()
        }
    }

    pub fn new_subscribe(
        subscriber: Vec<u8>,
        identifier: String,
        topic: String,
        duration: u32,
        meta: Vec<u8>,
        nonce: u64,
        fee: i64,
    ) -> Self {
        let mut payload = new_subscribe(subscriber, identifier, topic, duration, meta);
        let pl = pack_payload(&PayloadType::SubscribeType, &mut payload);
        let tx = new_msg_tx(
            pl,
            nonce,
            fee,
            &thread_rng().gen::<[u8; TRANSACTION_NONCE_LENGTH]>(),
        );

        Transaction {
            pb_tx: tx,
            ..Default::default()
        }
    }

    pub fn new_unsubscribe(
        subscriber: Vec<u8>,
        identifier: String,
        topic: String,
        nonce: u64,
        fee: i64,
    ) -> Self {
        let mut payload = new_unsubscribe(subscriber, identifier, topic);
        let pl = pack_payload(&PayloadType::UnsubscribeType, &mut payload);
        let tx = new_msg_tx(
            pl,
            nonce,
            fee,
            &thread_rng().gen::<[u8; TRANSACTION_NONCE_LENGTH]>(),
        );
        Transaction {
            pb_tx: tx,
            ..Default::default()
        }
    }

    pub fn verify(&self, height: u32) -> Result<bool, String> {
        todo!()
    }
}

impl SignableData for Transaction {
    fn program_hashes(&self) -> Vec<[u8; UINT160SIZE]> {
        todo!()
    }
    fn programs(&self) -> &[Program] {
        todo!()
    }
    fn set_programs(&mut self, programs: Vec<Program>) {
        self.pb_tx.programs = programs;
    }
    fn serialize_unsigned(&self) -> Vec<u8> {
        let mut buf = self.encode();
        let unsigned_tx = self.pb_tx.unsigned_tx.as_ref().unwrap().clone();
        let nonce = unsigned_tx.nonce;
        let nonce_bytes = nonce.to_le_bytes();
        let nonce_slice = nonce_bytes.as_slice();
        let fee = unsigned_tx.fee;
        let fee_bytes = fee.to_le_bytes();
        let fee_slice = fee_bytes.as_slice();
        let attrs_len = unsigned_tx.attributes.len() as u8;
        buf.extend_from_slice(nonce_slice);
        buf.extend_from_slice(fee_slice);
        buf.extend_from_slice(attrs_len.to_le_bytes().as_slice());
        buf.extend_from_slice(unsigned_tx.attributes.as_slice());
        buf
    }
}

pub fn new_msg_tx(payload: Payload, nonce: u64, fee: i64, attrs: &[u8]) -> pb_transcation {
    let unsigned_tx = UnsignedTx {
        payload: Some(payload),
        nonce,
        fee,
        attributes: Vec::from(attrs),
    };
    pb_transcation {
        unsigned_tx: Some(unsigned_tx),
        ..Default::default()
    }
}
