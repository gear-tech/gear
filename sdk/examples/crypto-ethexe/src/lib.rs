// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Demo exercising the ethexe `gr_crypto` syscall.
//!
//! `handle` payload layout: `op (1 byte) ++ data`, where `op` is a raw
//! [`gstd::crypto::CryptoOp`] discriminant. The program replies with the
//! operation result bytes, or `b"err"` when the host rejects the input.

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "std")]
mod code {
    include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}

#[cfg(feature = "std")]
pub use code::WASM_BINARY_OPT as WASM_BINARY;

#[cfg(not(feature = "std"))]
mod wasm {
    extern crate alloc;

    use alloc::vec::Vec;
    use gstd::msg;

    #[unsafe(no_mangle)]
    extern "C" fn handle() {
        let payload = msg::load_bytes().expect("failed to load payload");
        let (op, data) = payload.split_first().expect("empty payload");

        msg::reply_bytes(run(*op, data), 0).expect("failed to reply");
    }

    #[cfg(feature = "ethexe")]
    fn run(op: u8, data: &[u8]) -> Vec<u8> {
        use gstd::crypto;

        fn digest_reply<const N: usize, E>(result: Result<[u8; N], E>) -> Vec<u8> {
            match result {
                Ok(digest) => digest.to_vec(),
                Err(_) => b"err".to_vec(),
            }
        }

        match op {
            0 => digest_reply(crypto::keccak256(data)),
            1 => digest_reply(crypto::sha256(data)),
            2 => digest_reply(crypto::blake2b256(data)),
            3 => {
                let (public_key, rest) = data.split_at(crypto::BLS12_381_G1_LEN);
                let (signature, message) = rest.split_at(crypto::BLS12_381_G2_LEN);
                match crypto::bls12_381_verify(
                    public_key.try_into().expect("bad pk len"),
                    signature.try_into().expect("bad sig len"),
                    message,
                ) {
                    Ok(valid) => [valid as u8].to_vec(),
                    Err(_) => b"err".to_vec(),
                }
            }
            4 => match crypto::bls12_381_aggregate_g1(data) {
                Ok(point) => point.to_vec(),
                Err(_) => b"err".to_vec(),
            },
            _ => b"err".to_vec(),
        }
    }

    /// The crypto syscall is unavailable outside the ethexe runtime.
    #[cfg(not(feature = "ethexe"))]
    fn run(_op: u8, _data: &[u8]) -> Vec<u8> {
        b"err".to_vec()
    }
}
