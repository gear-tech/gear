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

impl IntoSubxt for sp_runtime::AccountId32 {
    type Target = subxt::utils::AccountId32;

    fn into_subxt(self) -> Self::Target {
        subxt::utils::AccountId32(self.into())
    }
}

impl IntoSubstrate for subxt::utils::AccountId32 {
    type Target = sp_runtime::AccountId32;

    fn into_substrate(self) -> Self::Target {
        self.0.into()
    }
}
