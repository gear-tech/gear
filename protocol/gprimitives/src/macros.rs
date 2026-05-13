// This file is part of Gear.

// Copyright (C) 2021-2025 Gear Technologies Inc.
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
        impl From<H256> for $ty {
            fn from(h256: H256) -> Self {
                Self::new(h256.to_fixed_bytes())
            }
        }
    };
    (@into_h256 $ty:ty) => {
        impl From<$ty> for H256 {
            fn from(value: $ty) -> Self {
                Self(value.0)
            }
        }
    };
    (@from_str $ty:ty) => {
        impl FromStr for $ty {
            type Err = ConversionError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                match s.strip_prefix("0x") {
                    Some(s) if s.len() == 64 => {
                        let mut id = Self::zero();
                        hex::decode_to_slice(s, &mut id.0)
                            .map_err(|_| ConversionError::InvalidHexString)?;
                        Ok(id)
                    }
                    _ => Err(ConversionError::InvalidHexString),
                }
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
                let byte_array = utils::ByteSliceFormatter::Array(&self.0);

                let is_alternate = f.alternate();
                if is_alternate {
                    f.write_str(concat!(stringify!($ty), "("))?;
                }

                byte_array.fmt(f)?;

                if is_alternate {
                    f.write_str(")")?;
                }

                Ok(())
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
    (@serde $ty:ty) => {
        #[cfg(feature = "serde")]
        impl Serialize for $ty {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                serializer.collect_str(self)
            }
        }

        #[cfg(feature = "serde")]
        impl<'de> Deserialize<'de> for $ty {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                deserializer.deserialize_identifier(utils::HexStrVisitor::new())
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
