//! SS58 encoding implementation
//!
//! This library is extracted from [ss58 codec][ss58-codec] in `sp-core`, not
//! importing `sp-core` because it is super big (~300 dependencies).
//!
//! [ss58-codec]: https://paritytech.github.io/polkadot-sdk/master/sp_core/crypto/trait.Ss58Codec.html

use anyhow::{anyhow, Result};
use blake2::{Blake2b512, Digest};
use core::sync::atomic::{AtomicU16, Ordering};

/// The SS58 prefix of vara network.
pub const VARA_SS58_PREFIX: u16 = 137;

/// The default ss58 version.
pub static DEFAULT_SS58_VERSION: AtomicU16 = AtomicU16::new(VARA_SS58_PREFIX);

/// SS58 prefix
const SS58_PREFIX: &[u8] = b"SS58PRE";

/// The checksum length used in ss58 encoding
const CHECKSUM_LENGTH: usize = 2;

/// Encode data to SS58 format.
pub fn encode(data: &[u8]) -> String {
    let ident: u16 = default_ss58_version() & 0b0011_1111_1111_1111;
    let mut v = match ident {
        0..=63 => vec![ident as u8],
        64..=16_383 => {
            // upper six bits of the lower byte(!)
            let first = ((ident & 0b0000_0000_1111_1100) as u8) >> 2;
            // lower two bits of the lower byte in the high pos,
            // lower bits of the upper byte in the low pos
            let second = ((ident >> 8) as u8) | ((ident & 0b0000_0000_0000_0011) as u8) << 6;
            vec![first | 0b01000000, second]
        }
        _ => unreachable!("masked out the upper two bits; qed"),
    };

    v.extend_from_slice(data);
    let r = blake2b_512(&v);
    v.extend(&r[0..CHECKSUM_LENGTH]);
    bs58::encode(v).into_string()
}

/// Decode data from SS58 format.
pub fn decode(encoded: &[u8], body_len: usize) -> Result<Vec<u8>> {
    let data = bs58::decode(encoded)
        .into_vec()
        .map_err(|e| anyhow!("Invalid ss58 data: {}", e))?;
    if data.len() < CHECKSUM_LENGTH {
        return Err(anyhow!("Invalid length of encoded ss58 data."));
    }

    let (prefix_len, _) = match data[0] {
        0..=63 => (1, data[0] as u16),
        64..=127 => {
            // weird bit manipulation owing to the combination of LE encoding and missing two
            // bits from the left.
            // d[0] d[1] are: 01aaaaaa bbcccccc
            // they make the LE-encoded 16-bit value: aaaaaabb 00cccccc
            // so the lower byte is formed of aaaaaabb and the higher byte is 00cccccc
            let lower = (data[0] << 2) | (data[1] >> 6);
            let upper = data[1] & 0b00111111;
            (2, (lower as u16) | ((upper as u16) << 8))
        }
        _ => return Err(anyhow!("Invalid prefix of encoded ss58 data.")),
    };

    if data.len() != prefix_len + body_len + CHECKSUM_LENGTH {
        return Err(anyhow!("Invalid length of encoded ss58 data."));
    }

    let hash = blake2b_512(&data[..prefix_len + body_len]);
    let checksum = &hash[0..CHECKSUM_LENGTH];
    if data[body_len + prefix_len..body_len + prefix_len + CHECKSUM_LENGTH] != *checksum {
        return Err(anyhow!("Invalid checksum of encoded ss58 data."));
    }

    Ok(data[prefix_len..body_len + prefix_len].to_vec())
}

/// Get the default ss58 version.
pub fn default_ss58_version() -> u16 {
    DEFAULT_SS58_VERSION.load(Ordering::Relaxed)
}

/// Set the default ss58 version.
pub fn set_default_ss58_version(version: u16) {
    DEFAULT_SS58_VERSION.store(version, Ordering::Relaxed);
}

/// blake2b_512 hash
fn blake2b_512(data: &[u8]) -> Vec<u8> {
    let mut ctx = Blake2b512::new();
    ctx.update(SS58_PREFIX);
    ctx.update(data);
    ctx.finalize().to_vec()
}
