// This file is part of Gear.

// Copyright (C) 2026 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Shared crypto helpers used by the `gr_secp256k1_{verify,recover}`
//! syscalls on both Vara (`core/processor/src/ext.rs`) and ethexe
//! (`ethexe/processor/src/host/api/crypto.rs`).
//!
//! Kept in `gear-core` rather than duplicated so both networks use
//! bitwise-identical policy — if this constant ever drifts between
//! networks a high-s signature could be accepted on one and rejected
//! on the other, which is exactly the protocol-level inconsistency
//! the `malleability_flag` ABI was introduced to close.

/// secp256k1 group order half — `floor(n/2)`, where
/// `n = 0xFFFF..._4141` is the secp256k1 curve order.
///
/// Any signature with `s > SECP256K1_N_HALF` is "high-s" (non-canonical);
/// the canonical low-s range is `1 <= s <= floor(n/2)`.
///
/// Derivation: `n = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141`,
/// `floor(n/2) = (n - 1) / 2 = 0x7FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF5D576E7357A4501DDFE92F46681B20A0`.
///
/// Regression-tested in `core/src/crypto/tests.rs` against the
/// hardcoded curve order so any typo fails loudly.
pub const SECP256K1_N_HALF: [u8; 32] = [
    0x7F, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
    0x5D, 0x57, 0x6E, 0x73, 0x57, 0xA4, 0x50, 0x1D, 0xDF, 0xE9, 0x2F, 0x46, 0x68, 0x1B, 0x20, 0xA0,
];

/// Returns `true` if the signature's `s` component is canonical (low-s).
///
/// `sig` is laid out as `r || s || v` where `r` = bytes 0..32,
/// `s` = 32..64, `v` = byte 64. The comparison treats `s` as a
/// big-endian 256-bit integer. `s == SECP256K1_N_HALF` is considered
/// low-s (the canonical form is `s <= n/2`).
///
/// Shared by `gr_secp256k1_verify` and `gr_secp256k1_recover` so both
/// syscalls give identical answers for the same `(sig, flag)` pair.
pub fn is_low_s(sig: &[u8; 65]) -> bool {
    // Big-endian byte-by-byte compare of sig[32..64] against SECP256K1_N_HALF.
    for i in 0..32 {
        match sig[32 + i].cmp(&SECP256K1_N_HALF[i]) {
            core::cmp::Ordering::Less => return true,
            core::cmp::Ordering::Greater => return false,
            core::cmp::Ordering::Equal => continue,
        }
    }
    // All 32 bytes equal → s == n/2 → canonical.
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Guards against any typo in `SECP256K1_N_HALF` by recomputing it
    /// from the hardcoded curve order and asserting equality.
    #[test]
    fn n_half_constant_matches_curve_order_derivation() {
        // secp256k1 group order n (from SEC 2 §2.4.1).
        let n: [u8; 32] = [
            0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
            0xFF, 0xFE, 0xBA, 0xAE, 0xDC, 0xE6, 0xAF, 0x48, 0xA0, 0x3B, 0xBF, 0xD2, 0x5E, 0x8C,
            0xD0, 0x36, 0x41, 0x41,
        ];
        // Compute (n - 1) / 2 as bytes. `n - 1` = `...4140`, then shift right by 1.
        let mut minus_one = n;
        minus_one[31] -= 1;
        let mut half = [0u8; 32];
        let mut carry = 0u8;
        for i in 0..32 {
            let b = minus_one[i];
            half[i] = (b >> 1) | (carry << 7);
            carry = b & 1;
        }
        assert_eq!(
            half, SECP256K1_N_HALF,
            "SECP256K1_N_HALF does not equal (n-1)/2"
        );
    }

    #[test]
    fn is_low_s_boundary_behavior() {
        let mut sig = [0u8; 65];

        // s == n/2 (canonical).
        sig[32..64].copy_from_slice(&SECP256K1_N_HALF);
        assert!(is_low_s(&sig), "s == n/2 must be low-s");

        // s == n/2 + 1 (non-canonical, just above).
        let mut plus_one = SECP256K1_N_HALF;
        // Add 1 big-endian.
        for i in (0..32).rev() {
            let (v, carry) = plus_one[i].overflowing_add(1);
            plus_one[i] = v;
            if !carry {
                break;
            }
        }
        sig[32..64].copy_from_slice(&plus_one);
        assert!(!is_low_s(&sig), "s == n/2 + 1 must be high-s");

        // s == 0 (degenerate but low-s in bare comparison sense;
        // the parse layer rejects it separately).
        sig[32..64].fill(0);
        assert!(is_low_s(&sig), "s == 0 byte-compares as low-s");

        // s == 1 (smallest non-zero low-s).
        sig[63] = 1;
        assert!(is_low_s(&sig), "s == 1 must be low-s");

        // s == 0xFF..FF (way above n/2).
        sig[32..64].fill(0xFF);
        assert!(!is_low_s(&sig), "s == 0xFF..FF must be high-s");
    }
}
