// This file is part of Gear.
//
// Copyright (C) 2024 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Goldilocks field wrapper gas counting.

use core::{
    fmt::{self, Debug, Display, Formatter},
    hash::{Hash, Hasher},
    iter::{Product, Sum},
    ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Neg, Sub, SubAssign},
};
use gcore::exec;
use num::BigUint;
use serde::{Deserialize, Serialize};

use plonky2::hash::{
    hash_types::RichField,
    poseidon::{Poseidon, N_PARTIAL_ROUNDS},
};
use plonky2_field::{
    extension::{quadratic::QuadraticExtension, Extendable, Frobenius},
    goldilocks_field::GoldilocksField,
    ops::Square,
    types::{Field, Field64, PrimeField, PrimeField64, Sample},
};

/// Goldilocks field extension degree.
pub const D: usize = 2;

/// Goldilocks field wrapper with custom Poseidon permutation implementation.
#[derive(Copy, Clone, Serialize, Deserialize)]
pub struct GoldilocksFieldWrapper(pub GoldilocksField);

impl Default for GoldilocksFieldWrapper {
    fn default() -> Self {
        Self(GoldilocksField::ZERO)
    }
}

impl PartialEq for GoldilocksFieldWrapper {
    fn eq(&self, other: &Self) -> bool {
        self.0.eq(&other.0)
    }
}

impl Eq for GoldilocksFieldWrapper {}

impl Hash for GoldilocksFieldWrapper {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

impl Display for GoldilocksFieldWrapper {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        Display::fmt(&self.0, f)
    }
}

impl Debug for GoldilocksFieldWrapper {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        Debug::fmt(&self.0, f)
    }
}

impl Sample for GoldilocksFieldWrapper {
    fn sample<R>(_rng: &mut R) -> Self
    where
        R: ?Sized,
    {
        unimplemented!("Not used in proofs verification")
    }
}

impl Field for GoldilocksFieldWrapper {
    const ZERO: Self = Self(GoldilocksField::ZERO);
    const ONE: Self = Self(GoldilocksField::ONE);
    const TWO: Self = Self(GoldilocksField::TWO);
    const NEG_ONE: Self = Self(GoldilocksField::NEG_ONE);

    const TWO_ADICITY: usize = GoldilocksField::TWO_ADICITY;
    const CHARACTERISTIC_TWO_ADICITY: usize = GoldilocksField::TWO_ADICITY;

    // Sage: `g = GF(p).multiplicative_generator()`
    const MULTIPLICATIVE_GROUP_GENERATOR: Self =
        Self(GoldilocksField::MULTIPLICATIVE_GROUP_GENERATOR);

    // Sage:
    // ```
    // g_2 = g^((p - 1) / 2^32)
    // g_2.multiplicative_order().factor()
    // ```
    const POWER_OF_TWO_GENERATOR: Self = Self(GoldilocksField::POWER_OF_TWO_GENERATOR);

    const BITS: usize = GoldilocksField::BITS;

    fn order() -> BigUint {
        Self::ORDER.into()
    }
    fn characteristic() -> BigUint {
        Self::order()
    }

    /// Returns the inverse of the field element, using Fermat's little theorem.
    /// The inverse of `a` is computed as `a^(p-2)`, where `p` is the prime
    /// order of the field.
    ///
    /// Mathematically, this is equivalent to:
    ///                $a^(p-1)     = 1 (mod p)$
    ///                $a^(p-2) * a = 1 (mod p)$
    /// Therefore      $a^(p-2)     = a^-1 (mod p)$
    ///
    /// The following code has been adapted from
    /// winterfell/math/src/field/f64/mod.rs located at <https://github.com/facebook/winterfell>.
    fn try_inverse(&self) -> Option<Self> {
        if self.is_zero() {
            return None;
        }

        // compute base^(P - 2) using 72 multiplications
        // The exponent P - 2 is represented in binary as:
        // 0b1111111111111111111111111111111011111111111111111111111111111111

        // compute base^11
        let t2 = self.square() * *self;

        // compute base^111
        let t3 = t2.square() * *self;

        // compute base^111111 (6 ones)
        // repeatedly square t3 3 times and multiply by t3
        let t6 = exp_acc::<3>(t3, t3);

        // compute base^111111111111 (12 ones)
        // repeatedly square t6 6 times and multiply by t6
        let t12 = exp_acc::<6>(t6, t6);

        // compute base^111111111111111111111111 (24 ones)
        // repeatedly square t12 12 times and multiply by t12
        let t24 = exp_acc::<12>(t12, t12);

        // compute base^1111111111111111111111111111111 (31 ones)
        // repeatedly square t24 6 times and multiply by t6 first. then square t30 and
        // multiply by base
        let t30 = exp_acc::<6>(t24, t6);
        let t31 = t30.square() * *self;

        // compute base^111111111111111111111111111111101111111111111111111111111111111
        // repeatedly square t31 32 times and multiply by t31
        let t63 = exp_acc::<32>(t31, t31);

        // compute base^1111111111111111111111111111111011111111111111111111111111111111
        Some(t63.square() * *self)
    }

    fn from_noncanonical_biguint(n: BigUint) -> Self {
        // Biguint `mod_floor` operation - needs benchmarking
        Self(GoldilocksField::from_noncanonical_biguint(n))
    }

    #[inline(always)]
    fn from_canonical_u64(n: u64) -> Self {
        Self(GoldilocksField::from_canonical_u64(n))
    }

    fn from_noncanonical_u96((n_lo, n_hi): (u64, u32)) -> Self {
        // Contains reduction from u96 - needs benchmarking
        Self(GoldilocksField::from_noncanonical_u96((n_lo, n_hi)))
    }

    fn from_noncanonical_u128(n: u128) -> Self {
        // Contains reduction from u128 - needs benchmarking
        Self(GoldilocksField::from_noncanonical_u128(n))
    }

    #[inline]
    fn from_noncanonical_u64(n: u64) -> Self {
        Self(GoldilocksField::from_noncanonical_u64(n))
    }

    #[inline]
    fn from_noncanonical_i64(n: i64) -> Self {
        // Wrapping addition for negative numbers - needs benchmarking
        Self(GoldilocksField::from_noncanonical_i64(n))
    }

    #[inline]
    fn multiply_accumulate(&self, x: Self, y: Self) -> Self {
        // u128 multiplication + addition, followed by reduction - needs benchmarking
        Self(self.0.multiply_accumulate(x.0, y.0))
    }
}

impl PrimeField for GoldilocksFieldWrapper {
    fn to_canonical_biguint(&self) -> BigUint {
        self.0.to_canonical_biguint()
    }
}

impl Field64 for GoldilocksFieldWrapper {
    const ORDER: u64 = GoldilocksField::ORDER;

    #[inline]
    unsafe fn add_canonical_u64(&self, rhs: u64) -> Self {
        // Includes overflowing addition - needs benchmarking
        Self(self.0.add_canonical_u64(rhs))
    }

    #[inline]
    unsafe fn sub_canonical_u64(&self, rhs: u64) -> Self {
        // Includes overflowing subtraction - needs benchmarking
        Self(self.0.sub_canonical_u64(rhs))
    }
}

impl PrimeField64 for GoldilocksFieldWrapper {
    #[inline]
    fn to_canonical_u64(&self) -> u64 {
        self.0.to_canonical_u64()
    }

    #[inline(always)]
    fn to_noncanonical_u64(&self) -> u64 {
        self.0.to_noncanonical_u64()
    }
}

impl Neg for GoldilocksFieldWrapper {
    type Output = Self;

    #[inline]
    fn neg(self) -> Self {
        // Requires 1 u64 subtraction
        Self(self.0.neg())
    }
}

impl Add for GoldilocksFieldWrapper {
    type Output = Self;

    #[inline]
    #[allow(clippy::suspicious_arithmetic_impl)]
    fn add(self, rhs: Self) -> Self {
        Self(self.0 + rhs.0)
    }
}

impl AddAssign for GoldilocksFieldWrapper {
    #[inline]
    fn add_assign(&mut self, rhs: Self) {
        *self = *self + rhs;
    }
}

impl Sum for GoldilocksFieldWrapper {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        iter.fold(Self::ZERO, |acc, x| acc + x)
    }
}

impl Sub for GoldilocksFieldWrapper {
    type Output = Self;

    #[inline]
    #[allow(clippy::suspicious_arithmetic_impl)]
    fn sub(self, rhs: Self) -> Self {
        Self(self.0 - rhs.0)
    }
}

impl SubAssign for GoldilocksFieldWrapper {
    #[inline]
    fn sub_assign(&mut self, rhs: Self) {
        *self = *self - rhs;
    }
}

impl Mul for GoldilocksFieldWrapper {
    type Output = Self;

    #[inline]
    fn mul(self, rhs: Self) -> Self {
        Self(self.0 * rhs.0)
    }
}

impl MulAssign for GoldilocksFieldWrapper {
    #[inline]
    fn mul_assign(&mut self, rhs: Self) {
        *self = *self * rhs;
    }
}

impl Product for GoldilocksFieldWrapper {
    fn product<I: Iterator<Item = Self>>(iter: I) -> Self {
        iter.fold(Self::ONE, |acc, x| acc * x)
    }
}

impl Div for GoldilocksFieldWrapper {
    type Output = Self;

    #[allow(clippy::suspicious_arithmetic_impl)]
    fn div(self, rhs: Self) -> Self::Output {
        self * rhs.inverse()
    }
}

impl DivAssign for GoldilocksFieldWrapper {
    fn div_assign(&mut self, rhs: Self) {
        *self = *self / rhs;
    }
}

impl RichField for GoldilocksFieldWrapper {}

/// Squares the base N number of times and multiplies the result by the tail
/// value.
#[inline(always)]
fn exp_acc<const N: usize>(
    base: GoldilocksFieldWrapper,
    tail: GoldilocksFieldWrapper,
) -> GoldilocksFieldWrapper {
    base.exp_power_of_2(N) * tail
}

/// Goldilocks field wrapper type quadratic extension.
impl Frobenius<1> for GoldilocksFieldWrapper {}

impl Extendable<2> for GoldilocksFieldWrapper {
    type Extension = QuadraticExtension<Self>;

    // Verifiable in Sage with
    // `R.<x> = GF(p)[]; assert (x^2 - 7).is_irreducible()`.
    const W: Self = Self(<GoldilocksField as Extendable<2>>::W);

    // DTH_ROOT = W^((ORDER - 1)/2)
    const DTH_ROOT: Self = Self(<GoldilocksField as Extendable<2>>::DTH_ROOT);

    const EXT_MULTIPLICATIVE_GROUP_GENERATOR: [Self; 2] = [
        Self(<GoldilocksField as Extendable<2>>::EXT_MULTIPLICATIVE_GROUP_GENERATOR[0]),
        Self(<GoldilocksField as Extendable<2>>::EXT_MULTIPLICATIVE_GROUP_GENERATOR[1]),
    ];

    const EXT_POWER_OF_TWO_GENERATOR: [Self; 2] = [
        Self(<GoldilocksField as Extendable<2>>::EXT_POWER_OF_TWO_GENERATOR[0]),
        Self(<GoldilocksField as Extendable<2>>::EXT_POWER_OF_TWO_GENERATOR[1]),
    ];
}

/// Poseidon hash input/output.
pub type PoseidonInOut = [u64; 12];

impl Poseidon for GoldilocksFieldWrapper {
    const MDS_MATRIX_CIRC: [u64; 12] = GoldilocksField::MDS_MATRIX_CIRC;
    const MDS_MATRIX_DIAG: [u64; 12] = GoldilocksField::MDS_MATRIX_DIAG;

    const FAST_PARTIAL_FIRST_ROUND_CONSTANT: [u64; 12] =
        GoldilocksField::FAST_PARTIAL_FIRST_ROUND_CONSTANT;

    const FAST_PARTIAL_ROUND_CONSTANTS: [u64; N_PARTIAL_ROUNDS] =
        GoldilocksField::FAST_PARTIAL_ROUND_CONSTANTS;

    const FAST_PARTIAL_ROUND_VS: [[u64; 12 - 1]; N_PARTIAL_ROUNDS] =
        GoldilocksField::FAST_PARTIAL_ROUND_VS;

    const FAST_PARTIAL_ROUND_W_HATS: [[u64; 12 - 1]; N_PARTIAL_ROUNDS] =
        GoldilocksField::FAST_PARTIAL_ROUND_W_HATS;

    // NB: This is in ROW-major order to support cache-friendly pre-multiplication.
    const FAST_PARTIAL_ROUND_INITIAL_MATRIX: [[u64; 12 - 1]; 12 - 1] =
        GoldilocksField::FAST_PARTIAL_ROUND_INITIAL_MATRIX;

    #[inline(always)]
    fn mds_layer(state: &[Self; 12]) -> [Self; 12] {
        let data: [GoldilocksField; 12] = state.map(|s| s.0);
        GoldilocksField::mds_layer(&data).map(Self)
    }

    #[inline(always)]
    fn sbox_layer(state: &mut [Self; 12]) {
        let mut data: [GoldilocksField; 12] = state.map(|s| s.0);
        GoldilocksField::sbox_layer(&mut data);
        // Mutate the original state based on the mutated data
        for (s, d) in state.iter_mut().zip(data.iter()) {
            s.0 = *d;
        }
    }

    #[inline]
    fn poseidon(input: [Self; 12]) -> [Self; 12] {
        // Using the fact that `GoldilocksFieldWrapper` is a newtype around `u64`.
        let data: [u64; 12] = unsafe { core::mem::transmute(input) };

        // Using proper conversion because not every u64 number is a valid field element.
        exec::poseidon_permute(data)
            .expect("Error in permute")
            .map(Field::from_canonical_u64)
    }
}
