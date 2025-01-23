// This file is part of Gear.

// Copyright (C) 2021-2024 Gear Technologies Inc.
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

//! Utility functions for field arithmetic and hash computation.

#![allow(clippy::needless_range_loop)]

use alloc::vec::Vec;
use core::hint::unreachable_unchecked;

pub const fn bits_u64(n: u64) -> usize {
    (64 - n.leading_zeros()) as usize
}

/// Computes `ceil(log_2(n))`.
#[must_use]
pub const fn log2_ceil(n: usize) -> usize {
    (usize::BITS - n.saturating_sub(1).leading_zeros()) as usize
}

/// Computes `log_2(n)`, panicking if `n` is not a power of two.
pub fn log2_strict(n: usize) -> usize {
    let res = n.trailing_zeros();
    assert!(n.wrapping_shr(res) == 1, "Not a power of two: {n}");
    // Tell the optimizer about the semantics of `log2_strict`. i.e. it can replace `n` with
    // `1 << res` and vice versa.
    assume(n == 1 << res);
    res as usize
}

/// Returns the largest integer `i` such that `base**i <= n`.
pub const fn log_floor(n: u64, base: u64) -> usize {
    assert!(n > 0);
    assert!(base > 1);
    let mut i = 0;
    let mut cur: u64 = 1;
    loop {
        let (mul, overflow) = cur.overflowing_mul(base);
        if overflow || mul > n {
            return i;
        } else {
            i += 1;
            cur = mul;
        }
    }
}

/// Permutes `arr` such that each index is mapped to its reverse in binary.
pub fn reverse_index_bits<T: Copy>(arr: &[T]) -> Vec<T> {
    let n = arr.len();
    let n_power = log2_strict(n);

    if n_power <= 6 {
        reverse_index_bits_small(arr, n_power)
    } else {
        reverse_index_bits_large(arr, n_power)
    }
}

/* Both functions below are semantically equivalent to:
        for i in 0..n {
            result.push(arr[reverse_bits(i, n_power)]);
        }
   where reverse_bits(i, n_power) computes the n_power-bit reverse. The complications are there
   to guide the compiler to generate optimal assembly.
*/

fn reverse_index_bits_small<T: Copy>(arr: &[T], n_power: usize) -> Vec<T> {
    let n = arr.len();
    let mut result = Vec::with_capacity(n);
    // BIT_REVERSE_6BIT holds 6-bit reverses. This shift makes them n_power-bit reverses.
    let dst_shr_amt = 6 - n_power;
    for i in 0..n {
        let src = (BIT_REVERSE_6BIT[i] as usize) >> dst_shr_amt;
        result.push(arr[src]);
    }
    result
}

fn reverse_index_bits_large<T: Copy>(arr: &[T], n_power: usize) -> Vec<T> {
    let n = arr.len();
    // LLVM does not know that it does not need to reverse src at each iteration (which is expensive
    // on x86). We take advantage of the fact that the low bits of dst change rarely and the high
    // bits of dst are dependent only on the low bits of src.
    let src_lo_shr_amt = 64 - (n_power - 6);
    let src_hi_shl_amt = n_power - 6;
    let mut result = Vec::with_capacity(n);
    for i_chunk in 0..(n >> 6) {
        let src_lo = i_chunk.reverse_bits() >> src_lo_shr_amt;
        for i_lo in 0..(1 << 6) {
            let src_hi = (BIT_REVERSE_6BIT[i_lo] as usize) << src_hi_shl_amt;
            let src = src_hi + src_lo;
            result.push(arr[src]);
        }
    }
    result
}

// Lookup table of 6-bit reverses.
// NB: 2^6=64 bytes is a cacheline. A smaller table wastes cache space.
#[rustfmt::skip]
const BIT_REVERSE_6BIT: &[u8] = &[
    0o00, 0o40, 0o20, 0o60, 0o10, 0o50, 0o30, 0o70,
    0o04, 0o44, 0o24, 0o64, 0o14, 0o54, 0o34, 0o74,
    0o02, 0o42, 0o22, 0o62, 0o12, 0o52, 0o32, 0o72,
    0o06, 0o46, 0o26, 0o66, 0o16, 0o56, 0o36, 0o76,
    0o01, 0o41, 0o21, 0o61, 0o11, 0o51, 0o31, 0o71,
    0o05, 0o45, 0o25, 0o65, 0o15, 0o55, 0o35, 0o75,
    0o03, 0o43, 0o23, 0o63, 0o13, 0o53, 0o33, 0o73,
    0o07, 0o47, 0o27, 0o67, 0o17, 0o57, 0o37, 0o77,
];

#[inline(always)]
pub fn assume(p: bool) {
    debug_assert!(p);
    if !p {
        unsafe {
            unreachable_unchecked();
        }
    }
}

/// Try to force Rust to emit a branch. Example:
///     if x > 2 {
///         y = foo();
///         branch_hint();
///     } else {
///         y = bar();
///     }
/// This function has no semantics. It is a hint only.
#[inline(always)]
pub fn branch_hint() {
    // NOTE: These are the currently supported assembly architectures. See the
    // [nightly reference](https://doc.rust-lang.org/nightly/reference/inline-assembly.html) for
    // the most up-to-date list.
    #[cfg(any(
        target_arch = "aarch64",
        target_arch = "arm",
        target_arch = "riscv32",
        target_arch = "riscv64",
        target_arch = "x86",
        target_arch = "x86_64",
    ))]
    unsafe {
        core::arch::asm!("", options(nomem, nostack, preserves_flags));
    }
}

#[cfg(test)]
mod tests {
    use alloc::{vec, vec::Vec};

    use rand::{rngs::OsRng, Rng};

    use super::{log2_ceil, log2_strict};

    #[test]
    fn test_reverse_index_bits() {
        let lengths = [32, 128, 1 << 16];
        let mut rng = OsRng;
        for _ in 0..32 {
            for length in lengths {
                let mut rand_list: Vec<u32> = Vec::with_capacity(length);
                rand_list.resize_with(length, || rng.gen());

                let out = super::reverse_index_bits(&rand_list);
                let expect = reverse_index_bits_naive(&rand_list);

                for (out, expect) in out.iter().zip(&expect) {
                    assert_eq!(out, expect);
                }
            }
        }
    }

    // #[test]
    // fn test_reverse_index_bits_in_place() {
    //     let lengths = [32, 128, 1 << 16];
    //     let mut rng = OsRng;
    //     for _ in 0..32 {
    //         for length in lengths {
    //             let mut rand_list: Vec<u32> = Vec::with_capacity(length);
    //             rand_list.resize_with(length, || rng.gen());

    //             let expect = reverse_index_bits_naive(&rand_list);

    //             super::reverse_index_bits_in_place(&mut rand_list);

    //             for (got, expect) in rand_list.iter().zip(&expect) {
    //                 assert_eq!(got, expect);
    //             }
    //         }
    //     }
    // }

    #[test]
    fn test_log2_strict() {
        assert_eq!(log2_strict(1), 0);
        assert_eq!(log2_strict(2), 1);
        assert_eq!(log2_strict(1 << 18), 18);
        assert_eq!(log2_strict(1 << 31), 31);
        assert_eq!(
            log2_strict(1 << (usize::BITS - 1)),
            usize::BITS as usize - 1
        );
    }

    #[test]
    #[should_panic]
    fn test_log2_strict_zero() {
        log2_strict(0);
    }

    #[test]
    #[should_panic]
    fn test_log2_strict_nonpower_2() {
        log2_strict(0x78c341c65ae6d262);
    }

    #[test]
    #[should_panic]
    fn test_log2_strict_usize_max() {
        log2_strict(usize::MAX);
    }

    #[test]
    fn test_log2_ceil() {
        // Powers of 2
        assert_eq!(log2_ceil(0), 0);
        assert_eq!(log2_ceil(1), 0);
        assert_eq!(log2_ceil(2), 1);
        assert_eq!(log2_ceil(1 << 18), 18);
        assert_eq!(log2_ceil(1 << 31), 31);
        assert_eq!(log2_ceil(1 << (usize::BITS - 1)), usize::BITS as usize - 1);

        // Nonpowers; want to round up
        assert_eq!(log2_ceil(3), 2);
        assert_eq!(log2_ceil(0x14fe901b), 29);
        assert_eq!(
            log2_ceil((1 << (usize::BITS - 1)) + 1),
            usize::BITS as usize
        );
        assert_eq!(log2_ceil(usize::MAX - 1), usize::BITS as usize);
        assert_eq!(log2_ceil(usize::MAX), usize::BITS as usize);
    }

    fn reverse_index_bits_naive<T: Copy>(arr: &[T]) -> Vec<T> {
        let n = arr.len();
        let n_power = log2_strict(n);

        let mut out = vec![None; n];
        for (i, v) in arr.iter().enumerate() {
            let dst = i.reverse_bits() >> (64 - n_power);
            out[dst] = Some(*v);
        }

        out.into_iter().map(|x| x.unwrap()).collect()
    }
}
