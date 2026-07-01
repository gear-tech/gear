// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Cryptographic host calls (ethexe-only).
//!
//! Each function forwards a [`CryptoOp`] to the host via the `gr_crypto`
//! syscall, so programs get audited native implementations without
//! bundling crypto code into their Wasm binary.

use crate::{
    errors::{Result, SyscallError},
    stack_buffer,
};
pub use gsys::CryptoOp;

/// Compressed BLS12-381 G1 point size (public keys, aggregates).
pub const BLS12_381_G1_LEN: usize = 48;
/// Compressed BLS12-381 G2 point size (signatures).
pub const BLS12_381_G2_LEN: usize = 96;

fn crypto<const N: usize>(op: CryptoOp, input: &[u8]) -> Result<[u8; N]> {
    debug_assert_eq!(N as u32, op.output_len());

    let mut output = [0u8; N];
    let mut error_code = 0u32;

    unsafe {
        gsys::gr_crypto(
            op as u32,
            input.as_ptr(),
            input.len() as u32,
            output.as_mut_ptr(),
            N as u32,
            &mut error_code,
        )
    };
    SyscallError(error_code).into_result()?;

    Ok(output)
}

/// Keccak-256 digest of `data`.
pub fn keccak256(data: &[u8]) -> Result<[u8; 32]> {
    crypto(CryptoOp::Keccak256, data)
}

/// SHA-256 digest of `data`.
pub fn sha256(data: &[u8]) -> Result<[u8; 32]> {
    crypto(CryptoOp::Sha256, data)
}

/// BLAKE2b-256 digest of `data`.
pub fn blake2b256(data: &[u8]) -> Result<[u8; 32]> {
    crypto(CryptoOp::Blake2b256, data)
}

/// Verify a BLS12-381 signature (min-pk: G1 public key, G2 signature,
/// `BLS_SIG_BLS12381G2_XMD:SHA-256_SSWU_RO_POP_` ciphersuite).
///
/// Returns `Ok(false)` for a well-formed but invalid signature and an
/// error for malformed points (or the identity public key).
pub fn bls12_381_verify(
    public_key: &[u8; BLS12_381_G1_LEN],
    signature: &[u8; BLS12_381_G2_LEN],
    message: &[u8],
) -> Result<bool> {
    // The syscall takes one contiguous buffer: pk ++ signature ++ message.
    const PREFIX_LEN: usize = BLS12_381_G1_LEN + BLS12_381_G2_LEN;
    let total = PREFIX_LEN + message.len();

    let [valid] = stack_buffer::with_byte_buffer(total, |buffer| {
        let ptr = buffer.as_mut_ptr() as *mut u8;
        // SAFETY: `buffer` is at least `total` bytes; regions don't overlap.
        unsafe {
            ptr.copy_from_nonoverlapping(public_key.as_ptr(), BLS12_381_G1_LEN);
            ptr.add(BLS12_381_G1_LEN)
                .copy_from_nonoverlapping(signature.as_ptr(), BLS12_381_G2_LEN);
            ptr.add(PREFIX_LEN)
                .copy_from_nonoverlapping(message.as_ptr(), message.len());
            crypto::<1>(
                CryptoOp::Bls12381Verify,
                core::slice::from_raw_parts(ptr, total),
            )
        }
    })?;
    Ok(valid == 1)
}

/// Aggregate (sum) compressed BLS12-381 G1 points.
/// `points` is a non-empty concatenation of 48-byte compressed points.
pub fn bls12_381_aggregate_g1(points: &[u8]) -> Result<[u8; BLS12_381_G1_LEN]> {
    crypto(CryptoOp::Bls12381AggregateG1, points)
}
