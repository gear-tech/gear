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

use super::*;
use alloc::vec::Vec;
use alloy_primitives::Address;
use alloy_sol_types::{SolValue, Word};

impl From<Address> for ActorId {
    fn from(value: Address) -> Self {
        let bytes: [u8; 32] = value.into_word().into();
        ActorId::from(bytes)
    }
}

impl From<ActorId> for Address {
    fn from(value: ActorId) -> Self {
        let bytes = value.into_bytes();
        Address::from_slice(&bytes[12..])
    }
}

impl SolValue for ActorId {
    type SolType = <Address as SolValue>::SolType;
}

impl ::alloy_sol_types::private::SolTypeValue<::alloy_sol_types::sol_data::Address> for ActorId {
    #[inline]
    fn stv_to_tokens(&self) -> ::alloy_sol_types::abi::token::WordToken {
        let bytes = self.into_bytes();
        ::alloy_sol_types::abi::token::WordToken(Word::from(bytes))
    }

    #[inline]
    fn stv_abi_encode_packed_to(&self, out: &mut Vec<u8>) {
        let bytes = self.into_bytes();
        out.extend_from_slice(&bytes[12..]);
    }

    #[inline]
    fn stv_eip712_data_word(&self) -> Word {
        ::alloy_sol_types::private::SolTypeValue::<::alloy_sol_types::sol_data::Address>::stv_to_tokens(self).0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::address;

    #[test]
    fn actor_id_sol_encode_decode() {
        const ADDR: Address = address!("0102030405060708090a0b0c0d0e0f1011121314");

        let actor_id: ActorId = ADDR.into();
        let address: Address = actor_id.into();

        assert_eq!(ADDR, address);

        let address_encoded = ADDR.abi_encode();
        let actor_id_encoded = actor_id.abi_encode();
        assert_eq!(address_encoded.as_slice(), actor_id_encoded.as_slice());

        let actor_id_decoded = ActorId::abi_decode(actor_id_encoded.as_slice());
        assert_eq!(Ok(actor_id), actor_id_decoded);

        let address_decoded = Address::abi_decode(actor_id_encoded.as_slice());
        assert_eq!(Ok(ADDR), address_decoded);
    }
}
