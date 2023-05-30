use crate::account::Account;
use crate::constants::*;
use crate::crypto::{ed25519_sign, ed25519_verify, sha256_hash};
use crate::pb::transaction::Program;
use crypto::ed25519::keypair;
use ed25519_dalek::{Keypair, SecretKey, Signature, Signer};
use sodiumoxide::crypto::sign::ed25519;

pub trait SignableData {
    fn program_hashes(&self) -> Vec<[u8; UINT160SIZE]>;
    fn programs(&self) -> &[Program];
    fn set_programs(&mut self, programs: Vec<Program>);
    fn serialize_unsigned(&self) -> Vec<u8>;
}

pub fn get_hash_data<D: SignableData>(data: &D) -> Vec<u8> {
    data.serialize_unsigned()
}

pub fn get_hash_for_signing<D: SignableData>(data: &D) -> [u8; SHA256_LEN] {
    sha256_hash(&get_hash_data(data))
}
pub fn sign<D: SignableData>(data: &D, private_key: &[u8]) -> [u8; SIGNATURE_LEN] {
    ed25519_sign(&get_hash_for_signing(data), private_key)
}

pub fn verify_signable_data<D: SignableData>(data: &D) -> bool {
    todo!()
}

pub fn verify_signature<D: SignableData>(
    data: &D,
    public_key: &[u8; PUBLIC_KEY_LEN],
    signature: &[u8; SIGNATURE_LEN],
) -> bool {
    ed25519_verify(&get_hash_for_signing(data), public_key, signature)
}
