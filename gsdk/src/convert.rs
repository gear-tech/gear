// This file is part of Gear.
//
// Copyright (C) 2025 Gear Technologies Inc.
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

//! This module provides conversion traits between [`subxt`] types and Substrate types.

use gear_core::pages::GearPage;
use subxt::{error::ModuleError, events::EventDetails};

use crate::{GearConfig, Result, gear::runtime_types::gear_core::pages::Page};

/// Trait for Substrate types convertible to their
/// [`subxt`] counterpart.
pub trait IntoSubxt {
    /// Type's [`subxt`] counterpart.
    type Target;

    /// Convert to [`subxt`] type.
    fn into_subxt(self) -> Self::Target;
}

/// Trait for [`subxt`] types convertible to their
/// Substrate counterpart.
pub trait IntoSubstrate {
    /// Type's Substrate counterpart.
    type Target;

    /// Convert to Substrate type.
    fn into_substrate(self) -> Self::Target;
}

/// Helper traits to decoding [`subxt`] types into Gear-specific types.
pub trait AsGear {
    /// Gear-specific counterpart of the type.
    type Target;

    fn as_gear(&self) -> Result<Self::Target>;
}

impl IntoSubxt for sp_runtime::AccountId32 {
    type Target = subxt::utils::AccountId32;

    fn into_subxt(self) -> Self::Target {
        subxt::utils::AccountId32(self.into())
    }
}

impl IntoSubxt for gear_core::ids::ActorId {
    type Target = subxt::utils::AccountId32;

    fn into_subxt(self) -> Self::Target {
        subxt::utils::AccountId32(self.into_bytes())
    }
}

impl IntoSubstrate for subxt::utils::AccountId32 {
    type Target = sp_runtime::AccountId32;

    fn into_substrate(self) -> Self::Target {
        self.0.into()
    }
}

impl<A: IntoSubstrate, B> IntoSubstrate for subxt::utils::MultiAddress<A, B> {
    type Target = sp_runtime::MultiAddress<A::Target, B>;

    fn into_substrate(self) -> Self::Target {
        match self {
            Self::Id(id) => Self::Target::Id(id.into_substrate()),
            Self::Index(index) => Self::Target::Index(index),
            Self::Raw(items) => Self::Target::Raw(items),
            Self::Address32(address) => Self::Target::Address32(address),
            Self::Address20(address) => Self::Target::Address20(address),
        }
    }
}

impl<A: IntoSubxt, B> IntoSubxt for sp_runtime::MultiAddress<A, B> {
    type Target = subxt::utils::MultiAddress<A::Target, B>;

    fn into_subxt(self) -> Self::Target {
        match self {
            Self::Id(id) => Self::Target::Id(id.into_subxt()),
            Self::Index(index) => Self::Target::Index(index),
            Self::Raw(items) => Self::Target::Raw(items),
            Self::Address32(address) => Self::Target::Address32(address),
            Self::Address20(address) => Self::Target::Address20(address),
        }
    }
}

impl AsGear for EventDetails<GearConfig> {
    type Target = crate::Event;

    fn as_gear(&self) -> Result<Self::Target> {
        Ok(self.as_root_event()?)
    }
}

impl AsGear for ModuleError {
    type Target = crate::RuntimeError;

    fn as_gear(&self) -> Result<Self::Target> {
        Ok(self.as_root_error()?)
    }
}

impl From<GearPage> for Page {
    fn from(page: GearPage) -> Page {
        Page(page.into())
    }
}
