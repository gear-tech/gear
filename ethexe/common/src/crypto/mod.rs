mod address;
mod digest;
mod keys;
mod signature;

pub use address::*;
pub use digest::*;
pub use keys::*;
pub use signature::*;

use hex::FromHexError;

/// Decodes hexed string to a byte array.
fn decode_to_array<const N: usize>(s: &str) -> Result<[u8; N], FromHexError> {
    // Strip the "0x" prefix if it exists.
    let stripped = s.strip_prefix("0x").unwrap_or(s);

    // Decode
    let mut buf = [0u8; N];
    hex::decode_to_slice(stripped, &mut buf)?;

    Ok(buf)
}
