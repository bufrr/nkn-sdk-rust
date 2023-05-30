use crate::constants::UINT160SIZE;
use crate::pb::transaction::{
    Coinbase, DeleteName, GenerateId, IssueAsset, NanoPay, Payload, PayloadType, RegisterName,
    SigChainTxn, Subscribe, TransferAsset, TransferName, Unsubscribe,
};

pub fn pack_payload<M: prost::Message>(payload_type: &PayloadType, payload_msg: &mut M) -> Payload {
    let r#type = match payload_type {
        PayloadType::CoinbaseType { .. } => PayloadType::CoinbaseType as i32,
        PayloadType::IssueAssetType { .. } => PayloadType::IssueAssetType as i32,
        PayloadType::TransferAssetType { .. } => PayloadType::TransferAssetType as i32,
        PayloadType::SigChainTxnType { .. } => PayloadType::SigChainTxnType as i32,
        PayloadType::GenerateIdType { .. } => PayloadType::GenerateIdType as i32,
        PayloadType::NanoPayType { .. } => PayloadType::NanoPayType as i32,
        PayloadType::SubscribeType { .. } => PayloadType::SubscribeType as i32,
        PayloadType::UnsubscribeType { .. } => PayloadType::UnsubscribeType as i32,
        PayloadType::RegisterNameType { .. } => PayloadType::RegisterNameType as i32,
        PayloadType::DeleteNameType { .. } => PayloadType::DeleteNameType as i32,
        PayloadType::TransferNameType { .. } => PayloadType::TransferNameType as i32,
        PayloadType::GenerateId2Type { .. } => PayloadType::GenerateId2Type as i32,
    };

    let mut buf = Vec::new();
    buf.reserve(payload_msg.encoded_len());
    payload_msg.encode(&mut buf).unwrap();

    Payload { r#type, data: buf }
}

pub fn unpack_payload(payload: &Payload) -> Vec<u8> {
    payload.data.clone()
}

pub fn new_coinbase(
    sender: &[u8; UINT160SIZE],
    recipient: &[u8; UINT160SIZE],
    amount: i64,
) -> Coinbase {
    Coinbase {
        sender: sender.to_vec(),
        recipient: recipient.to_vec(),
        amount,
    }
}

pub fn new_transfer_asset(
    sender: &[u8; UINT160SIZE],
    recipient: &[u8; UINT160SIZE],
    amount: i64,
) -> TransferAsset {
    TransferAsset {
        sender: sender.to_vec(),
        recipient: recipient.to_vec(),
        amount,
    }
}

pub fn new_sigchain(sig_chain: &[u8], submitter: &[u8; UINT160SIZE]) -> SigChainTxn {
    SigChainTxn {
        sig_chain: sig_chain.to_vec(),
        submitter: submitter.to_vec(),
    }
}

pub fn new_generate_id(
    public_key: &[u8],
    sender: &[u8],
    registration_fee: i64,
    version: i32,
) -> GenerateId {
    GenerateId {
        public_key: public_key.to_vec(),
        sender: sender.to_vec(),
        registration_fee,
        version,
    }
}

pub fn new_nanopay(
    sender: &[u8; UINT160SIZE],
    recipient: &[u8; UINT160SIZE],
    id: u64,
    amount: i64,
    txn_expiration: u32,
    nano_pay_expiration: u32,
) -> NanoPay {
    NanoPay {
        sender: sender.to_vec(),
        recipient: recipient.to_vec(),
        id,
        amount,
        txn_expiration,
        nano_pay_expiration,
    }
}

pub fn new_subscribe(
    subscriber: Vec<u8>,
    identifier: String,
    topic: String,
    duration: u32,
    meta: Vec<u8>,
) -> Subscribe {
    Subscribe {
        subscriber,
        identifier,
        topic,
        bucket: 0,
        duration,
        meta,
    }
}

pub fn new_unsubscribe(subscriber: Vec<u8>, identifier: String, topic: String) -> Unsubscribe {
    Unsubscribe {
        subscriber,
        identifier,
        topic,
    }
}

pub fn new_issue_asset(
    sender: &[u8; UINT160SIZE],
    name: String,
    symbol: String,
    precision: u32,
    total_supply: i64,
) -> IssueAsset {
    IssueAsset {
        sender: sender.to_vec(),
        name,
        symbol,
        precision,
        total_supply,
    }
}

pub fn new_register_name(registrant: Vec<u8>, name: String, registration_fee: i64) -> RegisterName {
    RegisterName {
        registrant,
        name,
        registration_fee,
    }
}

pub fn new_transfer_name(registrant: Vec<u8>, recipient: Vec<u8>, name: String) -> TransferName {
    TransferName {
        registrant,
        recipient,
        name,
    }
}

pub fn new_delete_name(registrant: Vec<u8>, name: String) -> DeleteName {
    DeleteName { registrant, name }
}
