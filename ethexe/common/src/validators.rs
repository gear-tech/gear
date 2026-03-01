// This file is part of Gear.
//
// Copyright (C) 2024-2025 Gear Technologies Inc.
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

//! Non-empty validator list wrapper used across ethexe.

use crate::Address;
use alloc::vec::Vec;
use derive_more::{Deref, DerefMut, Display, IntoIterator};
use nonempty::NonEmpty;
use parity_scale_codec::{Decode, Encode};
use scale_info::{TypeInfo, build::Fields};

/// [`ValidatorsVec`] is a wrapper over non-empty vector of [`Address`].
/// It is needed because `NonEmpty` does not implement `Encode` and `Decode`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Deref, DerefMut, IntoIterator)]
pub struct ValidatorsVec(NonEmpty<Address>);

impl Encode for ValidatorsVec {
    fn encode(&self) -> Vec<u8> {
        Into::<Vec<_>>::into(self.0.clone()).encode()
    }
}

impl Decode for ValidatorsVec {
    fn decode<I: parity_scale_codec::Input>(
        input: &mut I,
    ) -> Result<Self, parity_scale_codec::Error> {
        let inner: Vec<Address> = Decode::decode(input)?;
        NonEmpty::from_vec(inner)
            .map(Self)
            .ok_or(parity_scale_codec::Error::from(
                "Failed to decode ValidatorsVec: empty vector",
            ))
    }
}

impl TypeInfo for ValidatorsVec {
    type Identity = Self;

    fn type_info() -> scale_info::Type {
        scale_info::Type::builder()
            .path(scale_info::Path::new("ValidatorsVec", module_path!()))
            .composite(Fields::unnamed().field(|f| f.ty::<Vec<Address>>()))
    }
}

#[derive(Debug, Display)]
#[display("ValidatorsVec cannot be created from an empty collection")]
pub struct EmptyValidatorsError;

#[cfg(feature = "std")]
impl std::error::Error for EmptyValidatorsError {}

impl TryFrom<Vec<Address>> for ValidatorsVec {
    type Error = EmptyValidatorsError;

    fn try_from(value: Vec<Address>) -> Result<Self, Self::Error> {
        NonEmpty::from_vec(value)
            .map(Self)
            .ok_or(EmptyValidatorsError)
    }
}

impl TryFrom<Vec<alloy_primitives::Address>> for ValidatorsVec {
    type Error = EmptyValidatorsError;

    fn try_from(value: Vec<alloy_primitives::Address>) -> Result<Self, Self::Error> {
        let vec: Vec<Address> = value.into_iter().map(Into::into).collect();
        NonEmpty::from_vec(vec)
            .map(Self)
            .ok_or(EmptyValidatorsError)
    }
}

impl From<NonEmpty<Address>> for ValidatorsVec {
    fn from(value: NonEmpty<Address>) -> Self {
        Self(value)
    }
}

impl From<ValidatorsVec> for Vec<Address> {
    fn from(value: ValidatorsVec) -> Self {
        value.0.into()
    }
}

impl From<ValidatorsVec> for Vec<gear_core::ids::ActorId> {
    fn from(value: ValidatorsVec) -> Self {
        value.into_iter().map(Address::into).collect()
    }
}
