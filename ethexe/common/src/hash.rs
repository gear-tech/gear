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

use alloc::string::{String, ToString};
use anyhow::Result;
use core::{
    any::Any,
    cmp::Ordering,
    fmt::Debug,
    hash::{Hash, Hasher},
    marker::PhantomData,
};
use gprimitives::H256;
use parity_scale_codec::{Decode, Encode};

fn option_string<T: ToString>(value: &Option<T>) -> String {
    value
        .as_ref()
        .map(|v| v.to_string())
        .unwrap_or_else(|| "<none>".to_string())
}

fn shortname<T: Any>() -> &'static str {
    core::any::type_name::<T>()
        .split("::")
        .last()
        .expect("name is empty")
}

#[derive(Encode, Decode, derive_more::Into, derive_more::Display)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "std", serde(transparent))]
#[display("{hash}")]
pub struct HashOf<T: 'static> {
    hash: H256,
    #[into(ignore)]
    #[codec(skip)]
    #[cfg_attr(feature = "std", serde(skip))]
    _phantom: PhantomData<T>,
}

impl<T> Debug for HashOf<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "HashOf<{}>({:?})", shortname::<T>(), self.hash)
    }
}

impl<T> PartialEq for HashOf<T> {
    fn eq(&self, other: &Self) -> bool {
        self.hash.eq(&other.hash)
    }
}

impl<T> Eq for HashOf<T> {}

impl<T> PartialOrd for HashOf<T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<T> Ord for HashOf<T> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.hash.cmp(&other.hash)
    }
}

impl<T> Clone for HashOf<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for HashOf<T> {}

impl<T> Hash for HashOf<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.hash.hash(state)
    }
}

impl<T> AsRef<[u8]> for HashOf<T> {
    fn as_ref(&self) -> &[u8] {
        self.hash.as_ref()
    }
}

impl<T> HashOf<T> {
    /// # Safety
    /// Use it only for low-level storage implementations or tests.
    pub unsafe fn new(hash: H256) -> Self {
        Self {
            hash,
            _phantom: PhantomData,
        }
    }

    /// Note: previous named `hash()`, but renamed to `inner()` to avoid confusion with `Hash` trait.
    pub fn inner(self) -> H256 {
        self.hash
    }

    pub fn zero() -> Self {
        Self {
            hash: H256::zero(),
            _phantom: PhantomData,
        }
    }

    #[cfg(feature = "mock")]
    pub fn random() -> Self {
        Self {
            hash: H256::random(),
            _phantom: PhantomData,
        }
    }
}

impl<T> Default for HashOf<T> {
    fn default() -> Self {
        Self::zero()
    }
}

#[derive(
    Encode, Decode, PartialEq, Eq, derive_more::Into, derive_more::From, derive_more::Display,
)]
#[cfg_attr(
    feature = "std",
    derive(serde::Serialize, serde::Deserialize),
    serde(bound = "")
)]
#[display("{}", option_string(_0))]
pub struct MaybeHashOf<T: 'static>(Option<HashOf<T>>);

impl<T> Debug for MaybeHashOf<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let option_string = option_string(&Self::to_inner(*self).map(HashOf::inner));
        write!(f, "MaybeHashOf<{}>({})", shortname::<T>(), option_string)
    }
}

impl<T> Clone for MaybeHashOf<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for MaybeHashOf<T> {}

impl<T> Hash for MaybeHashOf<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.hash(state)
    }
}

impl<T> MaybeHashOf<T> {
    pub const fn empty() -> Self {
        Self(None)
    }

    pub const fn is_empty(&self) -> bool {
        self.0.is_none()
    }

    pub fn from_inner(inner: Option<HashOf<T>>) -> Self {
        Self(inner)
    }

    pub fn to_inner(self) -> Option<HashOf<T>> {
        self.0
    }

    pub fn map<U>(&self, f: impl FnOnce(HashOf<T>) -> U) -> Option<U> {
        self.to_inner().map(f)
    }

    pub fn map_or_default<U: Default>(&self, f: impl FnOnce(HashOf<T>) -> U) -> U {
        self.map(f).unwrap_or_default()
    }

    pub fn try_map_or_default<U: Default>(
        &self,
        f: impl FnOnce(HashOf<T>) -> Result<U>,
    ) -> Result<U> {
        self.map(f).unwrap_or_else(|| Ok(Default::default()))
    }

    pub fn replace(&mut self, other: Option<Self>) {
        if let Some(other) = other {
            *self = other;
        }
    }
}

impl<T> From<HashOf<T>> for MaybeHashOf<T> {
    fn from(value: HashOf<T>) -> Self {
        Self(Some(value))
    }
}
