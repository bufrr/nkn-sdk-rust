use hex::encode;
use rand::rngs::OsRng;
use rand::Rng;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{from_str, to_string};

use crate::account::Account;
use crate::constants::*;
use crate::crypto::{
    aes_decrypt, aes_encrypt, create_signature_program_context, password_to_aes_key_scrypt,
    scrypt_kdf,
};
use crate::crypto::{ed25519_sign, sha256_hash};
use crate::signature::SignableData;
use crate::tx::Transaction;

#[derive(Debug, Clone)]
pub struct WalletConfig {
    pub seed_rpc_server: Vec<String>,
    pub rpc_timeout: u64,
    pub rcp_concurrency: u32,
    pub password: String,
    pub iv: [u8; IV_LEN],
    pub master_key: Vec<u8>,
    pub scrypt: ScryptConfig,
}

impl Default for WalletConfig {
    fn default() -> Self {
        let mut rng = OsRng {};
        let mut iv = [0u8; IV_LEN];
        rng.fill(&mut iv);

        let mut master_key = [0u8; MASTER_KEY_LEN];
        rng.fill(&mut master_key);

        Self {
            seed_rpc_server: DEFAULT_RPC_SERVER.iter().map(|s| s.to_string()).collect(),
            rpc_timeout: DEFAULT_RPC_TIMEOUT,
            rcp_concurrency: DEFAULT_RPC_CONCURRENCY,
            password: String::from("123456"),
            iv,
            master_key: master_key.to_vec(),
            scrypt: ScryptConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct WalletData {
    pub version: u32,
    #[serde(rename = "IV")]
    pub iv: String,
    pub master_key: String,
    pub seed_encrypted: String,
    pub address: String,
    pub scrypt: ScryptConfig,
}

impl WalletData {
    pub fn new(account: &Account, config: &WalletConfig) -> Result<Self, String> {
        let password_key = password_to_aes_key_scrypt(&config.password, &config.scrypt);
        let seed = account.private_key();
        let master_key_cipher = aes_encrypt(&config.master_key, &password_key, &config.iv);
        let seed_cipher = aes_encrypt(seed, &config.master_key, &config.iv);
        Ok(Self {
            version: WALLET_VERSION,
            iv: encode(config.iv),
            master_key: encode(master_key_cipher),
            seed_encrypted: encode(seed_cipher),
            address: account.address(),
            scrypt: config.scrypt.clone(),
        })
    }

    pub fn decrypt_master_key(&self, password: &str) -> Result<Vec<u8>, String> {
        let password_key = scrypt_kdf(password, &self.scrypt);
        let iv = hex::decode(&self.iv).unwrap();
        let master_key_cipher = hex::decode(&self.master_key).unwrap();
        Ok(aes_decrypt(&master_key_cipher, &password_key, &iv))
    }

    pub fn decrypt_account(&self, password: &str) -> Result<Account, String> {
        let master_key = self.decrypt_master_key(password)?;
        let iv = hex::decode(&self.iv).unwrap();
        let seed_cipher = hex::decode(&self.seed_encrypted).unwrap();
        let seed = aes_decrypt(&seed_cipher, &master_key, &iv)
            .try_into()
            .unwrap();
        Account::from_seed(&seed)
    }
}

#[derive(Debug)]
pub struct Wallet {
    config: WalletConfig,
    account: Account,
    wallet_data: WalletData,
}

impl Wallet {
    #[no_mangle]
    pub fn new(account: Account, config: WalletConfig) -> Result<Self, String> {
        let wallet_data = WalletData::new(&account, &config)?;
        Ok(Self {
            config,
            account,
            wallet_data,
        })
    }

    pub fn to_json(&self) -> String {
        to_string(&self.wallet_data).unwrap()
    }

    pub fn from_json(json: &str, config: WalletConfig) -> Result<Self, String> {
        let wallet_data: WalletData = from_str(json).map_err(|_| "Invalid JSON".to_string())?;
        let master_key = wallet_data.decrypt_master_key(&config.password)?;
        let account = wallet_data.decrypt_account(&config.password)?;
        if account.address() != wallet_data.address {
            return Err("Wrong password".into());
        }
        let config = WalletConfig {
            password: config.password,
            master_key: master_key.to_vec(),
            ..config
        };

        Ok(Self {
            config,
            account,
            wallet_data,
        })
    }

    pub fn account(&self) -> &Account {
        &self.account
    }

    pub fn public_key(&self) -> &[u8; PUBLIC_KEY_LEN] {
        self.account.public_key()
    }

    pub fn private_key(&self) -> &[u8; PRIVATE_KEY_LEN] {
        self.account.private_key()
    }

    pub fn sign(&self, message: &[u8]) -> [u8; SIGNATURE_LEN] {
        ed25519_sign(
            message,
            [
                self.account.private_key().as_slice(),
                self.account.public_key().as_slice(),
            ]
            .concat()
            .as_slice(),
        )
    }

    pub fn config(&self) -> &WalletConfig {
        &self.config
    }

    pub fn set_config(&mut self, config: WalletConfig) {
        self.config = config
    }

    pub fn sign_transaction(&self, tx: &mut Transaction) {
        let ct = create_signature_program_context(self.public_key());
        let msg = sha256_hash(tx.serialize_unsigned().as_slice());
        let signature = self.sign(msg.as_slice());
        let programs = ct.new_program(signature);
        tx.set_programs(vec![programs])
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct ScryptConfig {
    #[serde(
        rename = "N",
        serialize_with = "log2_serialize",
        deserialize_with = "log2_deserialize"
    )]
    pub log_n: u8,
    pub r: u32,
    pub p: u32,
    pub salt: String,
}

fn log2_serialize<S>(x: &u8, s: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    (1 << x).serialize(s)
}

fn log2_deserialize<'de, D>(d: D) -> Result<u8, D::Error>
where
    D: Deserializer<'de>,
{
    let x = u32::deserialize(d)?;
    Ok(x.trailing_zeros() as u8)
}

impl Default for ScryptConfig {
    fn default() -> Self {
        let mut rng = rand::thread_rng();
        let mut salt = [0u8; SCRYPT_SALT_LEN];
        rng.fill(&mut salt);

        let salt = encode(salt);
        Self {
            log_n: SCRYPT_LOG_N,
            r: SCRYPT_R,
            p: SCRYPT_P,
            salt,
        }
    }
}
