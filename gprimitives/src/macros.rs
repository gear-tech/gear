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

//! A utility module with macros.

macro_rules! impl_primitive {
    (@new $ty:ty) => {
        impl $ty {
            #[doc = concat!("Creates a new `", stringify!($ty), "` from a 32-byte array.")]
            pub const fn new(array: [u8; 32]) -> Self {
                Self(array)
            }
        }
    };
    (@zero $ty:ty) => {
        impl $ty {
            #[doc = concat!("Creates a new zero `", stringify!($ty), "`.")]
            pub const fn zero() -> Self {
                Self([0; 32])
            }

            #[doc = concat!("Checks whether `", stringify!($ty), "` is zero.")]
            pub fn is_zero(&self) -> bool {
                self == &Self::zero()
            }
        }
    };
    (@into_bytes $ty:ty) => {
        impl $ty {
            #[doc = concat!("Returns `", stringify!($ty), "`as bytes array.")]
            pub fn into_bytes(self) -> [u8; 32] {
                self.0
            }
        }
    };
    (@from_u64 $ty:ty) => {
        impl From<u64> for $ty {
            fn from(value: u64) -> Self {
                let mut id = Self::zero();
                id.0[..8].copy_from_slice(&value.to_le_bytes()[..]);
                id
            }
        }
    };
    (@from_h256 $ty:ty) => {
        #[cfg(feature = "codec")]
        impl From<H256> for $ty {
            fn from(h256: H256) -> Self {
                Self::new(h256.to_fixed_bytes())
            }
        }
    };
    (@try_from_slice $ty:ty) => {
        impl TryFrom<&[u8]> for $ty {
            type Error = ConversionError;

            fn try_from(slice: &[u8]) -> Result<Self, Self::Error> {
                if slice.len() != 32 {
                    return Err(ConversionError::InvalidSliceLength);
                }

                let mut ret = Self([0; 32]);
                ret.as_mut().copy_from_slice(slice);

                Ok(ret)
            }
        }
    };
    (@display $ty:ty) => {
        impl fmt::Display for $ty {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                const LEN: usize = 32;
                const MEDIAN: usize = (LEN + 1) / 2;

                let mut e1 = MEDIAN;
                let mut s2 = MEDIAN;

                if let Some(precision) = f.precision() {
                    if precision < MEDIAN {
                        e1 = precision;
                        s2 = LEN - precision;
                    }
                }

                let mut out1 = [0; MEDIAN * 2];
                let mut out2 = [0; MEDIAN * 2];

                let _ = hex::encode_to_slice(&self.0[..e1], &mut out1[..e1 * 2]);
                let _ = hex::encode_to_slice(&self.0[s2..], &mut out2[..(LEN - s2) * 2]);

                let p1 = unsafe { str::from_utf8_unchecked(&out1[..e1 * 2]) };
                let p2 = unsafe { str::from_utf8_unchecked(&out2[..(LEN - s2) * 2]) };
                let sep = e1.ne(&s2).then_some("..").unwrap_or_default();

                if f.alternate() {
                    write!(f, "{}(0x{p1}{sep}{p2})", stringify!($ty))
                } else {
                    write!(f, "0x{p1}{sep}{p2}")
                }
            }
        }
    };
    (@debug $ty:ty) => {
        impl fmt::Debug for $ty {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                fmt::Display::fmt(self, f)
            }
        }
    };
    ($($feature:ident)*, $ty:ty) => {
        $(
            macros::impl_primitive!(@$feature $ty);
        )*
    };
}

pub(crate) use impl_primitive;
