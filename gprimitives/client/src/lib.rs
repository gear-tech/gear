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

//! Mock definitions and implementations for Goldilocs field and Poseidon hash

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub mod field {
    pub mod goldilocks_field {
        use super::types::{Field, PrimeField64};
        use crate::hash::poseidon::Poseidon;

        use core::fmt::{self, Debug, Display, Formatter};

        #[derive(Copy, Clone, Default)]
        #[repr(transparent)]
        pub struct GoldilocksField(pub u64);

        impl Display for GoldilocksField {
            fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
                Display::fmt(&self.to_canonical_u64(), f)
            }
        }

        impl Debug for GoldilocksField {
            fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
                Debug::fmt(&self.to_canonical_u64(), f)
            }
        }

        impl Field for GoldilocksField {
            const ORDER: u64 = 0xFFFFFFFF00000001;

            #[inline(always)]
            fn from_canonical_u64(n: u64) -> Self {
                debug_assert!(n < Self::ORDER);
                Self(n)
            }
        }

        impl PrimeField64 for GoldilocksField {
            #[inline]
            fn to_canonical_u64(&self) -> u64 {
                let mut c = self.0;
                // We only need one condition subtraction, since 2 * ORDER would not fit in a u64.
                if c >= Self::ORDER {
                    c -= Self::ORDER;
                }
                c
            }
        }

        impl Poseidon for GoldilocksField {}
    }

    pub mod types {
        pub trait Field {
            const ORDER: u64;

            fn from_canonical_u64(n: u64) -> Self;
        }

        pub trait PrimeField64 {
            fn to_canonical_u64(&self) -> u64;
        }
    }
}

pub mod hash {
    pub mod poseidon {
        pub const SPONGE_RATE: usize = 8;
        pub const SPONGE_CAPACITY: usize = 4;
        pub const SPONGE_WIDTH: usize = SPONGE_RATE + SPONGE_CAPACITY;

        use crate::field::types::PrimeField64;

        pub trait Poseidon: PrimeField64 {
            // Mocked default implementation: does nothing.
            #[inline]
            fn poseidon(input: [Self; SPONGE_WIDTH]) -> [Self; SPONGE_WIDTH]
            where
                Self: Sized,
            {
                input
            }
        }
    }
}
