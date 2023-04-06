pub const IV_LEN: usize = 16;
pub const MASTER_KEY_LEN: usize = 32;
pub const SCRYPT_KEY_LEN: usize = 32;
pub const SCRYPT_SALT_LEN: usize = 8;
pub const SCRYPT_LOG_N: u8 = 15;
pub const SCRYPT_R: u32 = 8;
pub const SCRYPT_P: u32 = 1;
pub const WALLET_VERSION: u32 = 2;

pub const DEFAULT_RPC_SERVER: &[&str] = &["http://seed.nkn.org:30003"];
pub const DEFAULT_RPC_TIMEOUT: u32 = 10;
pub const DEFAULT_RPC_CONCURRENCY: u32 = 10;

pub const PUBLIC_KEY_LEN: usize = 32;
pub const PRIVATE_KEY_LEN: usize = 32;
pub const UINT160SIZE: usize = 20;
pub const SIGNATURE_LEN: usize = 64;
pub const SHA256_LEN: usize = 32;
pub const RIPEMD160_LEN: usize = 20;
pub const SEED_LEN: usize = 32;

pub const ADDRESS_GEN_PREFIX: &[u8] = &[0x02, 0xb8, 0x25];
pub const CHECKSUM_LEN: usize = 4;
pub const AES_HASH_LEN: usize = 32;
