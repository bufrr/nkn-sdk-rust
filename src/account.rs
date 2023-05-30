use crate::constants::*;
use crate::crypto::*;
use ed25519_dalek::*;
use rand::rngs::OsRng;

#[derive(Debug, Clone)]
#[repr(C)]
pub struct Account {
    pub private_key: [u8; PRIVATE_KEY_LEN],
    pub public_key: [u8; PUBLIC_KEY_LEN],
    pub program_hash: [u8; UINT160SIZE],
    //keypair: Keypair,
}

impl Account {
    pub fn new() -> Result<Self, String> {
        let mut csprng = OsRng {};
        let keypair: Keypair = Keypair::generate(&mut csprng);
        Self::from_seed(&keypair.secret.to_bytes())
    }

    pub fn from_seed(seed: &[u8; SEED_LEN]) -> Result<Self, String> {
        if seed.len() != 32 {
            return Err("Invalid seed length".into());
        }
        let sk: SecretKey =
            SecretKey::from_bytes(seed).expect("Error creating secret key from seed");
        let pk: PublicKey = (&sk).into();

        let keypair: Keypair = Keypair {
            secret: sk,
            public: pk,
        };
        let (private_key, public_key) = (keypair.secret.to_bytes(), keypair.public.to_bytes());

        let program_hash = create_program_hash(&public_key);

        Ok(Self {
            private_key,
            public_key,
            program_hash,
        })
    }

    // pub fn keypair(&self) -> &Keypair {
    //     &self.keypair
    // }

    pub fn private_key(&self) -> &[u8; PRIVATE_KEY_LEN] {
        &self.private_key
    }

    pub fn public_key(&self) -> &[u8; PUBLIC_KEY_LEN] {
        &self.public_key
    }

    pub fn address(&self) -> String {
        code_hash_to_address(&self.program_hash)
    }
}
