use crate::constants::*;
use crate::pb::transaction::Program;

#[derive(Debug)]
pub struct ProgramContext {
    pub code: Vec<u8>,
    pub parameters: Vec<u8>,
    pub program_hash: [u8; RIPEMD160_LEN],
    pub owner_public_key_hash: [u8; RIPEMD160_LEN],
}

impl ProgramContext {
    pub fn new_program(&self, signature: [u8; 64]) -> Program {
        let size = signature.len();
        let mut parameter: Vec<u8> = vec![size as u8];
        parameter.extend_from_slice(signature.as_slice());
        Program {
            code: self.code.clone(),
            parameter,
        }
    }
}
