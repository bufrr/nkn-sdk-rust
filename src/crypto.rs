use crate::constants::*;
use crate::program::ProgramContext;
use crate::wallet::ScryptConfig;
use crypto::aes::{cbc_decryptor, cbc_encryptor, KeySize};
use crypto::blockmodes::NoPadding;
use crypto::buffer::{RefReadBuffer, RefWriteBuffer};
use crypto::digest::Digest;
use crypto::ed25519;
use crypto::ripemd160::Ripemd160;
use crypto::scrypt;
use crypto::scrypt::ScryptParams;
use crypto::sha2::{Sha256, Sha512};
use hex::decode;

use ed25519_dalek::*;
use sodiumoxide::crypto::box_::{open_precomputed, seal_precomputed, Nonce, PrecomputedKey};
use sodiumoxide::crypto::kdf::Key;
use sodiumoxide::crypto::sign::{to_curve25519_pk, to_curve25519_sk, SECRETKEYBYTES};

const CHECK_SIG: u8 = 0xAC;
const SIGNATURE: u8 = 0;

pub fn create_program_hash(public_key: &[u8; PUBLIC_KEY_LENGTH]) -> [u8; UINT160SIZE] {
    to_code_hash(&create_signature_program_code(public_key))
}

//CODE: len(public_key) + public_key + CHECKSIG
pub fn create_signature_program_code(public_key: &[u8; PUBLIC_KEY_LENGTH]) -> Vec<u8> {
    let mut code = Vec::new();
    code.push(PUBLIC_KEY_LENGTH as u8);
    code.extend_from_slice(public_key);
    code.push(CHECK_SIG);
    code
}

pub fn to_code_hash(code: &[u8]) -> [u8; UINT160SIZE] {
    ripemd160_hash(&sha256_hash(code))
}

pub fn sha256_hash(input: &[u8]) -> [u8; SHA256_LEN] {
    let mut hasher = Sha256::new();
    hasher.input(input);
    let mut hash = [0u8; SHA256_LEN];
    hasher.result(&mut hash);
    hash
}

pub fn ripemd160_hash(input: &[u8]) -> [u8; RIPEMD160_LEN] {
    let mut md = Ripemd160::new();
    md.input(&input);
    let mut hash = [0u8; RIPEMD160_LEN];
    md.result(&mut hash);
    hash
}

pub fn code_hash_to_address(hash: &[u8; UINT160SIZE]) -> String {
    let mut data = Vec::new();
    data.extend_from_slice(ADDRESS_GEN_PREFIX);
    data.extend_from_slice(hash);

    let temp = sha256_hash(&data);
    let temp2 = sha256_hash(&temp);
    data.extend_from_slice(&temp2[0..CHECKSUM_LEN]);

    bs58::encode(data).into_string()
}

pub fn scrypt_kdf(password: &str, config: &ScryptConfig) -> [u8; AES_HASH_LEN] {
    let mut hash = [0u8; AES_HASH_LEN];
    let params = ScryptParams::new(config.log_n, config.r, config.p);
    let salt = decode(&config.salt).unwrap();
    scrypt::scrypt(password.as_bytes(), &salt, &params, &mut hash);
    hash
}

pub fn password_to_aes_key_scrypt(password: &str, config: &ScryptConfig) -> [u8; AES_HASH_LEN] {
    scrypt_kdf(password, config)
}

pub fn aes_encrypt(input: &[u8], key: &[u8], iv: &[u8]) -> Vec<u8> {
    let mut encryptor = cbc_encryptor(KeySize::KeySize256, key, iv, NoPadding);
    let mut input_buf = RefReadBuffer::new(input);
    let mut output = vec![0u8; input.len()];
    let mut output_buf = RefWriteBuffer::new(&mut output);
    encryptor
        .encrypt(&mut input_buf, &mut output_buf, true)
        .unwrap();
    output
}

pub fn aes_decrypt(input: &[u8], key: &[u8], iv: &[u8]) -> Vec<u8> {
    let mut decryptor = cbc_decryptor(KeySize::KeySize256, key, iv, NoPadding);
    let mut input_buf = RefReadBuffer::new(input);
    let mut output = vec![0u8; input.len()];
    let mut output_buf = RefWriteBuffer::new(&mut output);
    decryptor
        .decrypt(&mut input_buf, &mut output_buf, true)
        .unwrap();
    output
}

pub fn encrypt(message: &[u8], nonce: &[u8], shared_key: &[u8]) -> Vec<u8> {
    let n = Nonce::from_slice(nonce).unwrap();
    let key = PrecomputedKey::from_slice(shared_key).unwrap();
    seal_precomputed(message, &n, &key)
}

pub fn decrypt(encrypted: &[u8], nonce: &[u8], shared_key: &[u8]) -> Result<Vec<u8>, ()> {
    let n = Nonce::from_slice(nonce).unwrap();
    let key = PrecomputedKey::from_slice(shared_key).unwrap();
    open_precomputed(encrypted, &n, &key)
}

pub fn create_signature_program_context(owner_pubkey: &[u8; PUBLIC_KEY_LENGTH]) -> ProgramContext {
    let code = create_signature_program_code(owner_pubkey);
    let owner_public_key_hash = to_code_hash(owner_pubkey);
    let program_hash = to_code_hash(&code);

    ProgramContext {
        code,
        parameters: vec![SIGNATURE],
        program_hash,
        owner_public_key_hash,
    }
}

pub fn ed25519_private_key_to_curve25519_private_key(
    private_key: &[u8; PRIVATE_KEY_LEN],
) -> [u8; SECRET_KEY_LENGTH] {
    let secret_key = SecretKey::from_bytes(private_key).unwrap();
    let public_key: PublicKey = (&secret_key).into();
    let keypair = Keypair {
        secret: secret_key,
        public: public_key,
    };
    let keypair = keypair.to_bytes();
    to_curve25519_sk(&sodiumoxide::crypto::sign::ed25519::SecretKey(keypair))
        .unwrap()
        .0
}

pub fn ed25519_public_key_to_curve25519_public_key(
    public_key: &[u8; PUBLIC_KEY_LEN],
) -> [u8; PUBLIC_KEY_LEN] {
    to_curve25519_pk(&sodiumoxide::crypto::sign::ed25519::PublicKey(*public_key))
        .unwrap()
        .0
}

pub fn ed25519_seed_from_private_key(private_key: &[u8]) -> [u8; SEED_LEN] {
    private_key[..32].try_into().unwrap()
}

pub fn sha512_hash(input: &[u8]) -> [u8; SHA256_LEN] {
    let mut hasher = Sha512::new();
    hasher.input(input);
    let mut hash = [0u8; 32];
    hasher.result(&mut hash);
    hash
}

pub fn ed25519_sign(message: &[u8], private_key: &[u8]) -> [u8; SIGNATURE_LEN] {
    Keypair::from_bytes(private_key)
        .unwrap()
        .sign(message)
        .to_bytes()
}

pub fn ed25519_verify(
    data: &[u8],
    public_key: &[u8; PUBLIC_KEY_LEN],
    signature: &[u8; SIGNATURE_LEN],
) -> bool {
    let verified = PublicKey::from_bytes(public_key)
        .unwrap()
        .verify(data, &Signature::from_bytes(signature).unwrap());
    verified.is_ok()
}
