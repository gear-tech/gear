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

#![allow(clippy::assertions_on_constants)]
#![allow(
    clippy::empty_line_after_doc_comments,
    clippy::empty_line_after_outer_attr
)]

use core::{
    arch::{aarch64::*, asm},
    mem::transmute,
};

use unroll::unroll_for_loops;

use crate::{
    field::goldilocks_field::GoldilocksField, hash::poseidon::Poseidon, util::branch_hint,
};

// ========================================== CONSTANTS ===========================================

const WIDTH: usize = 12;

const EPSILON: u64 = 0xffffffff;

// ===================================== COMPILE-TIME CHECKS ======================================

/// The MDS matrix multiplication ASM is specific to the MDS matrix below. We want this file to
/// fail to compile if it has been changed.
#[allow(dead_code)]
const fn check_mds_matrix() -> bool {
    // Can't == two arrays in a const_assert! (:
    let mut i = 0;
    let wanted_matrix_circ = [17, 15, 41, 16, 2, 28, 13, 13, 39, 18, 34, 20];
    let wanted_matrix_diag = [8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    while i < WIDTH {
        if <GoldilocksField as Poseidon>::MDS_MATRIX_CIRC[i] != wanted_matrix_circ[i]
            || <GoldilocksField as Poseidon>::MDS_MATRIX_DIAG[i] != wanted_matrix_diag[i]
        {
            return false;
        }
        i += 1;
    }
    true
}

const _: () = assert!(check_mds_matrix());

/// Ensure that the first WIDTH round constants are in canonical* form. This is required because
/// the first constant layer does not handle double overflow.
/// *: round_const == GoldilocksField::ORDER is safe.
/*
#[allow(dead_code)]
const fn check_round_const_bounds_init() -> bool {
    let mut i = 0;
    while i < WIDTH {
        if ALL_ROUND_CONSTANTS[i] > GoldilocksField::ORDER {
            return false;
        }
        i += 1;
    }
    true
}
const_assert!(check_round_const_bounds_init());
*/
// ====================================== SCALAR ARITHMETIC =======================================

/// Addition modulo ORDER accounting for wraparound. Correct only when a + b < 2**64 + ORDER.
#[inline(always)]
unsafe fn add_with_wraparound(a: u64, b: u64) -> u64 {
    let res: u64;
    let adj: u64;
    asm!(
        "adds  {res}, {a}, {b}",
        // Set adj to 0xffffffff if addition overflowed and 0 otherwise.
        // 'cs' for 'carry set'.
        "csetm {adj:w}, cs",
        a = in(reg) a,
        b = in(reg) b,
        res = lateout(reg) res,
        adj = lateout(reg) adj,
        options(pure, nomem, nostack),
    );
    res + adj // adj is EPSILON if wraparound occurred and 0 otherwise
}

/// Subtraction of a and (b >> 32) modulo ORDER accounting for wraparound.
#[inline(always)]
unsafe fn sub_with_wraparound_lsr32(a: u64, b: u64) -> u64 {
    let mut b_hi = b >> 32;
    // Make sure that LLVM emits two separate instructions for the shift and the subtraction. This
    // reduces pressure on the execution units with access to the flags, as they are no longer
    // responsible for the shift. The hack is to insert a fake computation between the two
    // instructions with an `asm` block to make LLVM think that they can't be merged.
    asm!(
        "/* {0} */", // Make Rust think we're using the register.
        inlateout(reg) b_hi,
        options(nomem, nostack, preserves_flags, pure),
    );
    // This could be done with a.overflowing_add(b_hi), but `checked_sub` signals to the compiler
    // that overflow is unlikely (note: this is a standard library implementation detail, not part
    // of the spec).
    match a.checked_sub(b_hi) {
        Some(res) => res,
        None => {
            // Super rare. Better off branching.
            branch_hint();
            let res_wrapped = a.wrapping_sub(b_hi);
            res_wrapped - EPSILON
        }
    }
}

/// Multiplication of the low word (i.e., x as u32) by EPSILON.
#[inline(always)]
unsafe fn mul_epsilon(x: u64) -> u64 {
    let res;
    asm!(
        // Use UMULL to save one instruction. The compiler emits two: extract the low word and then
        // multiply.
        "umull {res}, {x:w}, {epsilon:w}",
        x = in(reg) x,
        epsilon = in(reg) EPSILON,
        res = lateout(reg) res,
        options(pure, nomem, nostack, preserves_flags),
    );
    res
}

#[inline(always)]
unsafe fn multiply(x: u64, y: u64) -> u64 {
    let xy = (x as u128) * (y as u128);
    let xy_lo = xy as u64;
    let xy_hi = (xy >> 64) as u64;

    let res0 = sub_with_wraparound_lsr32(xy_lo, xy_hi);

    let xy_hi_lo_mul_epsilon = mul_epsilon(xy_hi);

    // add_with_wraparound is safe, as xy_hi_lo_mul_epsilon <= 0xfffffffe00000001 <= ORDER.
    add_with_wraparound(res0, xy_hi_lo_mul_epsilon)
}

// ==================================== STANDALONE CONST LAYER =====================================

/// Standalone const layer. Run only once, at the start of round 1. Remaining const layers are fused
/// with the preceding MDS matrix multiplication.
/*
#[inline(always)]
#[unroll_for_loops]
unsafe fn const_layer_full(
    mut state: [u64; WIDTH],
    round_constants: &[u64; WIDTH],
) -> [u64; WIDTH] {
    assert!(WIDTH == 12);
    for i in 0..12 {
        let rc = round_constants[i];
        // add_with_wraparound is safe, because rc is in canonical form.
        state[i] = add_with_wraparound(state[i], rc);
    }
    state
}
*/
// ========================================== FULL ROUNDS ==========================================

/// Full S-box.
#[inline(always)]
#[unroll_for_loops]
unsafe fn sbox_layer_full(state: [u64; WIDTH]) -> [u64; WIDTH] {
    // This is done in scalar. S-boxes in vector are only slightly slower throughput-wise but have
    // an insane latency (~100 cycles) on the M1.

    let mut state2 = [0u64; WIDTH];
    assert!(WIDTH == 12);
    for i in 0..12 {
        state2[i] = multiply(state[i], state[i]);
    }

    let mut state3 = [0u64; WIDTH];
    let mut state4 = [0u64; WIDTH];
    assert!(WIDTH == 12);
    for i in 0..12 {
        state3[i] = multiply(state[i], state2[i]);
        state4[i] = multiply(state2[i], state2[i]);
    }

    let mut state7 = [0u64; WIDTH];
    assert!(WIDTH == 12);
    for i in 0..12 {
        state7[i] = multiply(state3[i], state4[i]);
    }

    state7
}

#[inline(always)]
unsafe fn mds_reduce(
    // `cumul_a` and `cumul_b` represent two separate field elements. We take advantage of
    // vectorization by reducing them simultaneously.
    [cumul_a, cumul_b]: [uint32x4_t; 2],
) -> uint64x2_t {
    // Form:
    // `lo = [cumul_a[0] + cumul_a[2] * 2**32, cumul_b[0] + cumul_b[2] * 2**32]`
    // `hi = [cumul_a[1] + cumul_a[3] * 2**32, cumul_b[1] + cumul_b[3] * 2**32]`
    // Observe that the result `== lo + hi * 2**16 (mod Goldilocks)`.
    let mut lo = vreinterpretq_u64_u32(vuzp1q_u32(cumul_a, cumul_b));
    let mut hi = vreinterpretq_u64_u32(vuzp2q_u32(cumul_a, cumul_b));
    // Add the high 48 bits of `lo` to `hi`. This cannot overflow.
    hi = vsraq_n_u64::<16>(hi, lo);
    // Now, result `== lo.bits[0..16] + hi * 2**16 (mod Goldilocks)`.
    // Set the high 48 bits of `lo` to the low 48 bits of `hi`.
    lo = vsliq_n_u64::<16>(lo, hi);
    // At this point, result `== lo + hi.bits[48..64] * 2**64 (mod Goldilocks)`.
    // It remains to fold `hi.bits[48..64]` into `lo`.
    let top = {
        // Extract the top 16 bits of `hi` as a `u32`.
        // Interpret `hi` as a vector of bytes, so we can use a table lookup instruction.
        let hi_u8 = vreinterpretq_u8_u64(hi);
        // Indices defining the permutation. `0xff` is out of bounds, producing `0`.
        let top_idx =
            transmute::<[u8; 8], uint8x8_t>([0x06, 0x07, 0xff, 0xff, 0x0e, 0x0f, 0xff, 0xff]);
        let top_u8 = vqtbl1_u8(hi_u8, top_idx);
        vreinterpret_u32_u8(top_u8)
    };
    // result `== lo + top * 2**64 (mod Goldilocks)`.
    let adj_lo = vmlal_n_u32(lo, top, EPSILON as u32);
    let wraparound_mask = vcgtq_u64(lo, adj_lo);
    vsraq_n_u64::<32>(adj_lo, wraparound_mask) // Add epsilon on overflow.
}

#[inline(always)]
unsafe fn mds_layer_full(state: [u64; WIDTH]) -> [u64; WIDTH] {
    // This function performs an MDS multiplication in complex FFT space.
    // However, instead of performing a width-12 FFT, we perform three width-4 FFTs, which is
    // cheaper. The 12x12 matrix-vector multiplication (a convolution) becomes two 3x3 real
    // matrix-vector multiplications and one 3x3 complex matrix-vector multiplication.

    // We split each 64-bit into four chunks of 16 bits. To prevent overflow, each chunk is 32 bits
    // long. Each NEON vector below represents one field element and consists of four 32-bit chunks:
    // `elem == vector[0] + vector[1] * 2**16 + vector[2] * 2**32 + vector[3] * 2**48`.

    // Constants that we multiply by.
    let mut consts: uint32x4_t = transmute::<[u32; 4], _>([2, 4, 8, 16]);

    // Prevent LLVM from turning fused multiply (by power of 2)-add (1 instruction) into shift and
    // add (two instructions). This fake `asm` block means that LLVM no longer knows the contents of
    // `consts`.
    asm!("/* {0:v} */", // Make Rust think the register is being used.
         inout(vreg) consts,
         options(pure, nomem, nostack, preserves_flags),
    );

    // Four length-3 complex FFTs.
    let mut state_fft = [vdupq_n_u32(0); 12];
    for i in 0..3 {
        // Interpret each field element as a 4-vector of `u16`s.
        let x0 = vcreate_u16(state[i]);
        let x1 = vcreate_u16(state[i + 3]);
        let x2 = vcreate_u16(state[i + 6]);
        let x3 = vcreate_u16(state[i + 9]);

        // `vaddl_u16` and `vsubl_u16` yield 4-vectors of `u32`s.
        let y0 = vaddl_u16(x0, x2);
        let y1 = vaddl_u16(x1, x3);
        let y2 = vsubl_u16(x0, x2);
        let y3 = vsubl_u16(x1, x3);

        let z0 = vaddq_u32(y0, y1);
        let z1 = vsubq_u32(y0, y1);
        let z2 = y2;
        let z3 = y3;

        // The FFT is `[z0, z2 + z3 i, z1, z2 - z3 i]`.

        state_fft[i] = z0;
        state_fft[i + 3] = z1;
        state_fft[i + 6] = z2;
        state_fft[i + 9] = z3;
    }

    // 3x3 real matrix-vector mul for component 0 of the FFTs.
    // Multiply the vector `[x0, x1, x2]` by the matrix
    // `[[ 64,  64, 128],`
    // ` [128,  64,  64],`
    // ` [ 64, 128,  64]]`
    // The results are divided by 4 (this ends up cancelling out some later computations).
    {
        let x0 = state_fft[0];
        let x1 = state_fft[1];
        let x2 = state_fft[2];

        let t = vshlq_n_u32::<4>(x0);
        let u = vaddq_u32(x1, x2);

        let y0 = vshlq_n_u32::<4>(u);
        let y1 = vmlaq_laneq_u32::<3>(t, x2, consts);
        let y2 = vmlaq_laneq_u32::<3>(t, x1, consts);

        state_fft[0] = vaddq_u32(y0, y1);
        state_fft[1] = vaddq_u32(y1, y2);
        state_fft[2] = vaddq_u32(y0, y2);
    }

    // 3x3 real matrix-vector mul for component 2 of the FFTs.
    // Multiply the vector `[x0, x1, x2]` by the matrix
    // `[[ -4,  -8,  32],`
    // ` [-32,  -4,  -8],`
    // ` [  8, -32,  -4]]`
    // The results are divided by 4 (this ends up cancelling out some later computations).
    {
        let x0 = state_fft[3];
        let x1 = state_fft[4];
        let x2 = state_fft[5];
        state_fft[3] = vmlsq_laneq_u32::<2>(vmlaq_laneq_u32::<0>(x0, x1, consts), x2, consts);
        state_fft[4] = vmlaq_laneq_u32::<0>(vmlaq_laneq_u32::<2>(x1, x0, consts), x2, consts);
        state_fft[5] = vmlsq_laneq_u32::<0>(x2, vmlsq_laneq_u32::<1>(x0, x1, consts), consts);
    }

    // 3x3 complex matrix-vector mul for components 1 and 3 of the FFTs.
    // Multiply the vector `[x0r + x0i i, x1r + x1i i, x2r + x2i i]` by the matrix
    // `[[ 4 +  2i,  2 + 32i,  2 -  8i],`
    // ` [-8 -  2i,  4 +  2i,  2 + 32i],`
    // ` [32 -  2i, -8 -  2i,  4 +  2i]]`
    // The results are divided by 2 (this ends up cancelling out some later computations).
    {
        let x0r = state_fft[6];
        let x1r = state_fft[7];
        let x2r = state_fft[8];

        let x0i = state_fft[9];
        let x1i = state_fft[10];
        let x2i = state_fft[11];

        // real part of result <- real part of input
        let r0rr = vaddq_u32(vmlaq_laneq_u32::<0>(x1r, x0r, consts), x2r);
        let r1rr = vmlaq_laneq_u32::<0>(x2r, vmlsq_laneq_u32::<0>(x1r, x0r, consts), consts);
        let r2rr = vmlsq_laneq_u32::<0>(x2r, vmlsq_laneq_u32::<1>(x1r, x0r, consts), consts);

        // real part of result <- imaginary part of input
        let r0ri = vmlsq_laneq_u32::<1>(vmlaq_laneq_u32::<3>(x0i, x1i, consts), x2i, consts);
        let r1ri = vmlsq_laneq_u32::<3>(vsubq_u32(x0i, x1i), x2i, consts);
        let r2ri = vsubq_u32(vaddq_u32(x0i, x1i), x2i);

        // real part of result (total)
        let r0r = vsubq_u32(r0rr, r0ri);
        let r1r = vaddq_u32(r1rr, r1ri);
        let r2r = vmlaq_laneq_u32::<0>(r2ri, r2rr, consts);

        // imaginary part of result <- real part of input
        let r0ir = vmlsq_laneq_u32::<1>(vmlaq_laneq_u32::<3>(x0r, x1r, consts), x2r, consts);
        let r1ir = vmlaq_laneq_u32::<3>(vsubq_u32(x1r, x0r), x2r, consts);
        let r2ir = vsubq_u32(x2r, vaddq_u32(x0r, x1r));

        // imaginary part of result <- imaginary part of input
        let r0ii = vaddq_u32(vmlaq_laneq_u32::<0>(x1i, x0i, consts), x2i);
        let r1ii = vmlaq_laneq_u32::<0>(x2i, vmlsq_laneq_u32::<0>(x1i, x0i, consts), consts);
        let r2ii = vmlsq_laneq_u32::<0>(x2i, vmlsq_laneq_u32::<1>(x1i, x0i, consts), consts);

        // imaginary part of result (total)
        let r0i = vaddq_u32(r0ir, r0ii);
        let r1i = vaddq_u32(r1ir, r1ii);
        let r2i = vmlaq_laneq_u32::<0>(r2ir, r2ii, consts);

        state_fft[6] = r0r;
        state_fft[7] = r1r;
        state_fft[8] = r2r;

        state_fft[9] = r0i;
        state_fft[10] = r1i;
        state_fft[11] = r2i;
    }

    // Three length-4 inverse FFTs.
    // Normally, such IFFT would divide by 4, but we've already taken care of that.
    for i in 0..3 {
        let z0 = state_fft[i];
        let z1 = state_fft[i + 3];
        let z2 = state_fft[i + 6];
        let z3 = state_fft[i + 9];

        let y0 = vsubq_u32(z0, z1);
        let y1 = vaddq_u32(z0, z1);
        let y2 = z2;
        let y3 = z3;

        let x0 = vaddq_u32(y0, y2);
        let x1 = vaddq_u32(y1, y3);
        let x2 = vsubq_u32(y0, y2);
        let x3 = vsubq_u32(y1, y3);

        state_fft[i] = x0;
        state_fft[i + 3] = x1;
        state_fft[i + 6] = x2;
        state_fft[i + 9] = x3;
    }

    // Perform `res[0] += state[0] * 8` for the diagonal component of the MDS matrix.
    state_fft[0] = vmlal_laneq_u16::<4>(
        state_fft[0],
        vcreate_u16(state[0]),         // Each 16-bit chunk gets zero-extended.
        vreinterpretq_u16_u32(consts), // Hack: these constants fit in `u16s`, so we can bit-cast.
    );

    let mut res_arr = [0; 12];
    for i in 0..6 {
        let res = mds_reduce([state_fft[2 * i], state_fft[2 * i + 1]]);
        res_arr[2 * i] = vgetq_lane_u64::<0>(res);
        res_arr[2 * i + 1] = vgetq_lane_u64::<1>(res);
    }

    res_arr
}

#[inline(always)]
fn unwrap_state(state: [GoldilocksField; 12]) -> [u64; 12] {
    state.map(|s| s.0)
}

#[inline(always)]
fn wrap_state(state: [u64; 12]) -> [GoldilocksField; 12] {
    state.map(GoldilocksField)
}

#[inline(always)]
pub unsafe fn sbox_layer(state: &mut [GoldilocksField; WIDTH]) {
    *state = wrap_state(sbox_layer_full(unwrap_state(*state)));
}

#[inline(always)]
pub unsafe fn mds_layer(state: &[GoldilocksField; WIDTH]) -> [GoldilocksField; WIDTH] {
    let state = unwrap_state(*state);
    let state = mds_layer_full(state);
    wrap_state(state)
}
