use crate::constants::UINT160SIZE;
use crate::crypto::{code_hash_to_address, create_program_hash};
use crate::payload::unpack_payload;
use crate::pb;
use crate::pb::payloads::Payload;
use crate::pb::transaction::{PayloadType, UnsignedTx};
use crate::rpc::{get_balance, get_height, send_raw_transaction, RPCConfig};
use crate::tx::Transaction;
use crate::utils::uint160_from_bytes;
use crate::wallet::Wallet;
use prost::Message;
use rand::{thread_rng, Rng};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::time::sleep;

const SENDER_EXPIRATION_DELTA: u32 = 5;
const FORCE_FLUSH_DELTA: u32 = 2;
const RECEIVER_EXPIRATION_DELTA: u32 = 3;
const CONSENSUS_DURATION: u32 = 20;

pub struct NanoPay {
    rpc_config: RPCConfig,
    wallet: Wallet,
    recipient_address: String,
    recipient_program_hash: [u8; UINT160SIZE],
    fee: i64,
    duration: u32,
    id: u64,
    amount: u64,
    expiration: u32,
}

fn clone_rpc_config(rpc_config: &RPCConfig) -> RPCConfig {
    RPCConfig {
        rpc_server_address: rpc_config.rpc_server_address.clone(),
        ..*rpc_config
    }
}

impl NanoPay {
    pub fn new(
        rpc_config: RPCConfig,
        wallet: Wallet,
        recipient_address: String,
        fee: i64,
        duration: u32,
    ) -> Self {
        let program_hash = create_program_hash(recipient_address.as_bytes().try_into().unwrap());
        let mut rng = thread_rng();
        Self {
            rpc_config,
            wallet,
            recipient_address,
            recipient_program_hash: program_hash,
            fee,
            duration,
            id: rng.gen::<u64>(),
            amount: 0,
            expiration: 0,
        }
    }

    pub fn recipient(&self) -> String {
        self.recipient_address.clone()
    }

    pub async fn increase_amount(&mut self, delta: u64, fee: u64) -> Result<Transaction, String> {
        let height = get_height(clone_rpc_config(&self.rpc_config)).await?;
        if self.expiration == 0 || self.expiration < height + SENDER_EXPIRATION_DELTA {
            self.expiration = height + SENDER_EXPIRATION_DELTA;
            self.id = thread_rng().gen::<u64>();
            self.amount = 0;
        }

        self.amount += delta;
        let id = self.id;
        let amount = self.amount;

        let mut tx = Transaction::new_nanopay(
            &self.wallet.account().program_hash,
            &self.recipient_program_hash,
            id,
            amount as i64,
            self.expiration,
            (self.expiration + self.duration),
        );

        let mut new_fee = fee;
        if fee == 0 {
            new_fee = self.fee as u64;
        }
        tx.pb_tx.unsigned_tx = Some(UnsignedTx {
            fee: new_fee as i64,
            ..tx.pb_tx.unsigned_tx.take().unwrap()
        });
        Ok(tx)
    }
}

pub struct NanoPayClaimer {
    rpc_config: Arc<RPCConfig>,
    recipient_address: String,
    recipient_program_hash: [u8; UINT160SIZE],
    id: Option<u64>,
    min_flush_amount: Arc<i64>,
    amount: Arc<Mutex<i64>>,
    closed: Arc<Mutex<bool>>,
    expiration: Arc<Mutex<u32>>,
    last_claim_time: Arc<Mutex<SystemTime>>,
    prev_claimed_amount: Arc<Mutex<i64>>,
    prev_flush_amount: Arc<Mutex<i64>>,
    tx: Arc<Mutex<Option<Transaction>>>,
}

impl NanoPayClaimer {
    pub fn new(
        rpc_config: RPCConfig,
        recipient_address: String,
        claim_interval_ms: u64,
        min_flush_amount: i64,
    ) -> Result<Self, String> {
        let recipient_program_hash =
            create_program_hash(recipient_address.as_bytes().try_into().unwrap());
        let rpc_config = Arc::new(rpc_config);
        let min_flush_amount = Arc::new(min_flush_amount);
        let amount = Arc::new(Mutex::new(0));
        let closed = Arc::new(Mutex::new(false));
        let expiration = Arc::new(Mutex::new(0));
        let last_claim_time = Arc::new(Mutex::new(SystemTime::now()));
        let prev_claimed_amount = Arc::new(Mutex::new(0));
        let prev_flush_amount = Arc::new(Mutex::new(0));
        let tx = Arc::new(Mutex::new(None));

        let rpc_config_clone = rpc_config.clone();
        let min_flush_amount_clone = min_flush_amount.clone();
        let amount_clone = amount.clone();
        let closed_clone = closed.clone();
        let expiration_clone = expiration.clone();
        let last_claim_time_clone = last_claim_time.clone();
        let prev_claimed_amount_clone = prev_claimed_amount.clone();
        let prev_flush_amount_clone = prev_flush_amount.clone();
        let tx_clone = tx.clone();

        tokio::spawn(async move {
            loop {
                sleep(Duration::from_secs(60)).await;

                if *closed_clone.lock().unwrap() {
                    return;
                }

                if tx_clone.lock().unwrap().is_none() {
                    continue;
                }

                if *amount_clone.lock().unwrap() - *prev_flush_amount_clone.lock().unwrap()
                    < *min_flush_amount_clone
                {
                    continue;
                }

                let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
                let last_claim_duration = last_claim_time_clone
                    .lock()
                    .unwrap()
                    .duration_since(UNIX_EPOCH)
                    .unwrap();
                let claim_interval_duration = Duration::from_millis(claim_interval_ms);

                if now < last_claim_duration + claim_interval_duration {
                    let height = match get_height(clone_rpc_config(&*rpc_config_clone)).await {
                        Ok(height) => height,
                        Err(_) => continue,
                    };
                    let expiration = *expiration_clone.lock().unwrap();

                    if expiration > height + FORCE_FLUSH_DELTA {
                        let duration1 = last_claim_duration + claim_interval_duration - now;
                        let duration2 = Duration::from_secs(
                            ((expiration - height + FORCE_FLUSH_DELTA) * CONSENSUS_DURATION) as u64,
                        );

                        sleep(if duration1 > duration2 {
                            duration2
                        } else {
                            duration1
                        })
                        .await;

                        if *closed_clone.lock().unwrap() {
                            break;
                        }
                    }
                }

                let res = Self::flush_fn(
                    false,
                    rpc_config_clone.clone(),
                    amount_clone.clone(),
                    prev_flush_amount_clone.clone(),
                    min_flush_amount_clone.clone(),
                    expiration_clone.clone(),
                    last_claim_time_clone.clone(),
                    tx_clone.clone(),
                )
                .await;

                if res.is_err() {
                    break;
                }
            }
        });

        Ok(Self {
            recipient_address,
            recipient_program_hash,
            id: None,
            rpc_config,
            min_flush_amount,
            amount,
            closed,
            expiration,
            last_claim_time,
            prev_claimed_amount,
            prev_flush_amount,
            tx,
        })
    }

    pub fn recipient(&self) -> &str {
        &self.recipient_address
    }

    pub fn amount(&self) -> i64 {
        *self.prev_claimed_amount.lock().unwrap() + *self.amount.lock().unwrap()
    }

    pub fn close(&mut self) {
        *self.closed.lock().unwrap() = true;
    }

    pub fn is_closed(&self) -> bool {
        *self.closed.lock().unwrap()
    }

    pub async fn flush(&mut self, force: bool) -> Result<(), String> {
        Self::flush_fn(
            force,
            self.rpc_config.clone(),
            self.amount.clone(),
            self.prev_flush_amount.clone(),
            self.min_flush_amount.clone(),
            self.expiration.clone(),
            self.last_claim_time.clone(),
            self.tx.clone(),
        )
        .await
    }

    async fn flush_fn(
        force: bool,
        rpc_config: Arc<RPCConfig>,
        amount: Arc<Mutex<i64>>,
        prev_flush_amount: Arc<Mutex<i64>>,
        min_flush_amount: Arc<i64>,
        expiration: Arc<Mutex<u32>>,
        last_claim_time: Arc<Mutex<SystemTime>>,
        tx: Arc<Mutex<Option<Transaction>>>,
    ) -> Result<(), String> {
        if !force
            && *amount.lock().unwrap() - *prev_flush_amount.lock().unwrap() < *min_flush_amount
        {
            return Ok(());
        }

        if tx.lock().unwrap().is_none() {
            return Ok(());
        }

        let tx_clone = tx.clone();

        let pb_payload = tx_clone
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .pb_tx
            .unsigned_tx
            .as_ref()
            .unwrap()
            .payload
            .as_ref()
            .unwrap()
            .clone();

        let payload = unpack_payload(&pb_payload);
        let np = pb::transaction::NanoPay::decode(payload.as_slice()).unwrap();

        let _ = match PayloadType::from_i32(pb_payload.r#type) {
            Some(PayloadType::NanoPayType) => {}
            _ => return Err("not a NanoPay payload".into()),
        };

        {
            let tx = tx.lock().unwrap().as_ref().unwrap().clone();
            send_raw_transaction(&tx, clone_rpc_config(&*rpc_config)).await?;
        }

        *tx.lock().unwrap() = None;
        *expiration.lock().unwrap() = 0;
        *last_claim_time.lock().unwrap() = SystemTime::now();
        *prev_flush_amount.lock().unwrap() = np.amount;

        Ok(())
    }

    pub async fn claim(&mut self, tx: &Transaction) -> Result<i64, String> {
        let height = get_height(clone_rpc_config(&*self.rpc_config)).await?;

        let payload = tx
            .pb_tx
            .unsigned_tx
            .as_ref()
            .unwrap()
            .payload
            .as_ref()
            .unwrap();
        let mut np: pb::transaction::NanoPay;
        let _ = match PayloadType::from_i32(payload.r#type) {
            Some(PayloadType::NanoPayType) => {
                np = pb::transaction::NanoPay::decode(payload.data.as_slice()).unwrap();
            }
            _ => return Err("not a NanoPay payload".into()),
        };

        let recipient = uint160_from_bytes(np.recipient);

        if recipient != self.recipient_program_hash {
            return Err("wrong recipient".into());
        }

        if !tx.verify(height)? {
            return Err("incorrect transaction".into());
        }

        let sender = uint160_from_bytes(np.sender);
        let sender_address = code_hash_to_address(&sender);

        if *self.closed.lock().unwrap() {
            return Err("nanopay closed".into());
        }

        if let Some(id) = self.id {
            if id == np.id {
                if *self.amount.lock().unwrap() >= np.amount {
                    return Err("invalid amount".into());
                }
            } else {
                self.flush(false).await?;

                self.id = None;
                *self.prev_claimed_amount.lock().unwrap() += *self.amount.lock().unwrap();
                *self.prev_flush_amount.lock().unwrap() = 0;
                *self.amount.lock().unwrap() = 0;
            }
        }

        let sender_balance =
            get_balance(&sender_address, clone_rpc_config(&self.rpc_config)).await?;

        if sender_balance + *self.prev_flush_amount.lock().unwrap() < np.amount {
            return Err("insufficient balance".into());
        }

        if np.txn_expiration <= height + RECEIVER_EXPIRATION_DELTA {
            return Err("expired nanopay transaction".into());
        }

        if np.nano_pay_expiration <= height + RECEIVER_EXPIRATION_DELTA {
            return Err("expired nanopay".into());
        }

        self.id = Some(np.id);
        *self.tx.lock().unwrap() = Some(tx.clone());
        *self.expiration.lock().unwrap() = np.txn_expiration;
        *self.amount.lock().unwrap() = np.amount;

        Ok(*self.prev_claimed_amount.lock().unwrap() + *self.amount.lock().unwrap())
    }
}
