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

//! Identifier mapping helpers for DKG protocol.

use super::protocol::DkgProtocol;
use ethexe_common::{
    Address,
    crypto::{DkgIdentifier, DkgSessionId},
};

impl DkgProtocol {
    /// Returns the current DKG session id.
    pub fn session(&self) -> DkgSessionId {
        self.config.session
    }

    /// Returns the ordered participant list for this session.
    pub fn participants(&self) -> &[Address] {
        &self.config.participants
    }

    /// Returns the DKG identifier for a given address.
    pub fn identifier_for(&self, address: Address) -> Option<DkgIdentifier> {
        self.identifiers.get(&address).copied()
    }

    /// Returns the address for a given DKG identifier.
    pub fn address_for_identifier(&self, identifier: DkgIdentifier) -> Option<Address> {
        self.addresses.get(&identifier).copied()
    }

    /// Returns the local participant identifier.
    pub fn self_identifier(&self) -> DkgIdentifier {
        self.identifiers[&self.config.self_address]
    }

    /// Returns a flat address -> identifier map for persistence.
    pub fn identifier_map(&self) -> Vec<(Address, DkgIdentifier)> {
        self.identifiers
            .iter()
            .map(|(addr, identifier)| (*addr, *identifier))
            .collect()
    }
}
