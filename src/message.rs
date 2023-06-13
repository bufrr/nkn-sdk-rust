use crate::pb::payloads::Payload;
use crossbeam_channel::Sender;

pub type Reply = (String, Payload, bool);

#[derive(Debug, Clone)]
pub struct MessageConfig {
    pub unencrypted: bool,
    pub no_reply: bool,
    pub max_holding_secs: u32,
    pub message_id: Vec<u8>,
    pub tx_pool: bool,
    pub offset: u32,
    pub limit: u32,
}

impl Default for MessageConfig {
    fn default() -> Self {
        Self {
            unencrypted: false,
            no_reply: false,
            max_holding_secs: 10000,
            message_id: Vec::new(),
            tx_pool: false,
            offset: 0,
            limit: 1000,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Message {
    pub src: String,
    pub data: Vec<u8>,
    pub r#type: u32,
    pub encrypted: bool,
    pub message_id: Vec<u8>,
    pub no_reply: bool,
    pub reply_tx: Sender<(String, Payload, bool)>,
}

impl Message {
    pub fn source(&self) -> &str {
        &self.src
    }

    pub fn data(&self) -> &[u8] {
        &self.data
    }

    fn reply(&self, payload: Payload) -> Result<(), String> {
        if !self.no_reply {
            self.reply_tx
                .send((self.src.clone(), payload, self.encrypted))
                .unwrap();
        }
        Ok(())
    }
}

// fn new_text_payload(text: &str, message_id: &[u8], no_reply: bool) -> Result<Payload, String> {
//     let mut payload = Payload {
//         r#type: PayloadType::Text as i32,
//         message_id: message_id.to_vec(),
//         no_reply,
//         ..Default::default()
//     };
//
//     Ok(payload)
// }
