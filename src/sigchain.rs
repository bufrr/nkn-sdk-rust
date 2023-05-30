use crate::pb::sigchain::{SigChain, SigChainElem};
use crate::serialization::{write_bool, write_u32, write_var_bytes};

impl SigChainElem {
    pub(crate) fn serialize_unsigned(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        write_var_bytes(&mut bytes, &self.id);
        write_var_bytes(&mut bytes, &self.next_pubkey);
        write_bool(&mut bytes, self.mining);
        bytes.extend_from_slice(&self.vrf);
        bytes
    }
}

impl SigChain {
    pub fn serialize_metadata(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        write_u32(&mut bytes, self.nonce);
        write_u32(&mut bytes, self.data_size);
        write_var_bytes(&mut bytes, &self.block_hash);
        write_var_bytes(&mut bytes, &self.src_id);
        write_var_bytes(&mut bytes, &self.src_pubkey);
        write_var_bytes(&mut bytes, &self.dest_id);
        write_var_bytes(&mut bytes, &self.dest_pubkey);
        bytes
    }
}
