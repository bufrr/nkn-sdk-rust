use hex::encode;
use rand::rngs::OsRng;
use rand::Rng;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{from_str, to_string};

use crate::account::Account;
use crate::constants::*;
use crate::utils::{aes_decrypt, aes_encrypt, password_to_aes_key_scrypt, scrypt_kdf};

#[derive(Debug, Clone)]
pub struct WalletConfig {
    pub seed_rcp_server: Vec<String>,
    pub rpc_timeout: u32,
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
            seed_rcp_server: DEFAULT_RPC_SERVER.iter().map(|s| s.to_string()).collect(),
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
            address: account.address().to_string(),
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
    pub extern "C" fn new(account: Account, config: WalletConfig) -> Result<Self, String> {
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
