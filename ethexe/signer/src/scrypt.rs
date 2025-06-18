use anyhow::{anyhow, Result};
use rand::RngCore;

#[derive(Clone, Debug)]
pub struct ScryptParams {
    pub salt: [u8; Self::SALT_LENGTH],
    pub n: u32,
    pub r: u32,
    pub p: u32,
}

impl ScryptParams {
    pub const ENCODED_LENGTH: usize = 44;

    const SALT_LENGTH: usize = 32;

    pub fn new() -> Self {
        let mut salt = [0; Self::SALT_LENGTH];
        rand::thread_rng().fill_bytes(&mut salt);

        Self {
            salt,
            n: 15, // 2^15 = 32768
            r: 8,
            p: 1,
        }
    }

    pub fn decode(encoded: [u8; Self::ENCODED_LENGTH]) -> Self {
        let mut salt = [0; Self::SALT_LENGTH];
        salt.copy_from_slice(&encoded[..Self::SALT_LENGTH]);

        let params = encoded[Self::SALT_LENGTH..]
            .chunks(4)
            .map(|bytes| {
                let mut buf = [0; 4];
                buf.copy_from_slice(bytes);
                u32::from_le_bytes(buf)
            })
            .collect::<Vec<_>>();

        Self {
            salt,
            n: params[0].ilog2(),
            r: params[2],
            p: params[1],
        }
    }

    pub fn encode(&self) -> [u8; Self::ENCODED_LENGTH] {
        let mut buf = [0; Self::ENCODED_LENGTH];
        let n = 1 << self.n;
        buf[..Self::SALT_LENGTH].copy_from_slice(&self.salt);
        buf[Self::SALT_LENGTH..].copy_from_slice(
            [n, self.p, self.r]
                .iter()
                .flat_map(|n| n.to_le_bytes())
                .collect::<Vec<_>>()
                .as_slice(),
        );

        buf
    }

    pub fn derive_key(&self, passphrase: &[u8]) -> Result<[u8; 32]> {
        let mut key = [0; 32];
        let output = nacl::scrypt(
            passphrase,
            &self.salt,
            self.n as u8,
            self.r as usize,
            self.p as usize,
            32, // Output length
            &|_: u32| {},
        )
        .map_err(|e| anyhow!("Scrypt key derivation failed: {e:?}"))?;

        key.copy_from_slice(&output[..32]);
        Ok(key)
    }
}

impl Default for ScryptParams {
    fn default() -> Self {
        Self::new()
    }
}
