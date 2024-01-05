//! SS58 encoding implementation
//!
//! This library is extracted from [ss58 codec][ss58-codec] in `sp-core`, not
//! importing `sp-core` because it is super big (~300 dependencies).
//!
//! [ss58-codec]: https://paritytech.github.io/polkadot-sdk/master/sp_core/crypto/trait.Ss58Codec.html

use blake2::{Blake2b512, Digest};

/// The SS58 prefix of vara network.
pub const VARA_SS58_PREFIX: u16 = 137;

/// SS58 prefix
const SS58_PREFIX: &[u8] = b"SS58PRE";

/// The checksum length used in ss58 encoding
const CHECKSUM_LENGTH: usize = 2;

/// Encode data to SS58 format.
pub fn encode(data: &[u8], version: u16) -> String {
    let ident: u16 = version & 0b0011_1111_1111_1111;
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

/// blake2b_512 hash
fn blake2b_512(data: &[u8]) -> Vec<u8> {
    let mut ctx = Blake2b512::new();
    ctx.update(SS58_PREFIX);
    ctx.update(data);
    ctx.finalize().to_vec()
}
