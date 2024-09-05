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

//! Gear builtin primitives.

use crate::ActorId;
#[cfg(feature = "testing")]
pub use tests::generate_actor_id;

/// Seed for generating builtin actor ids
pub const SEED: [u8; 8] = *b"built/in";

/// Gear builtin actor
#[derive(Clone, Copy)]
#[repr(u64)]
pub enum BuiltinActor {
    /// Librar [`gbuiltin_bls318`]
    Bls12_381,
    /// library [`gbuiltin_staking`]
    Staking,
    /// library [`gbuiltin_eth_bridge`]
    EthBridge,
    /// Customized ids
    #[cfg(feature = "testing")]
    Other(u64),
}

impl BuiltinActor {
    /// Get the library index
    #[cfg(not(feature = "testing"))]
    pub const fn id(&self) -> u64 {
        *self as u64 + 1
    }

    /// Get the library index
    #[cfg(feature = "testing")]
    pub const fn id(&self) -> u64 {
        match self {
            Self::Bls12_381 => 1,
            Self::Staking => 2,
            Self::EthBridge => 3,
            Self::Other(id) => *id,
        }
    }

    /// Get actor id
    pub fn actor_id(&self) -> ActorId {
        match self {
            Self::Bls12_381 => BLS12_381,
            Self::Staking => STAKING,
            Self::EthBridge => ETH_BRIDGE,
            #[cfg(feature = "testing")]
            Self::Other(id) => generate_actor_id(*id),
        }
    }
}

/// Convert library index to actor ID
pub fn to_actor_id(idx: u64) -> ActorId {
    match idx {
        b if b == BuiltinActor::Bls12_381.id() => BLS12_381,
        b if b == BuiltinActor::EthBridge.id() => ETH_BRIDGE,
        b if b == BuiltinActor::Staking.id() => STAKING,
        #[cfg(feature = "testing")]
        _ => generate_actor_id(idx),
        #[cfg(not(feature = "testing"))]
        _ => panic!("Unsupported builtin library"),
    }
}

/// Actor ID of builtin library [`gbuiltin_bls318`]
pub const BLS12_381: ActorId = ActorId([
    107, 110, 41, 44, 56, 41, 69, 232, 11, 245, 26, 242, 186, 127, 233, 244, 88, 220, 255, 129,
    174, 96, 117, 196, 111, 144, 149, 225, 187, 236, 220, 55,
]);

/// Actor ID of builtin library [`gbuiltin_staking`]
pub const STAKING: ActorId = ActorId([
    119, 246, 94, 241, 144, 225, 27, 254, 203, 143, 200, 151, 15, 211, 116, 158, 148, 190, 214,
    106, 35, 236, 47, 122, 54, 35, 231, 133, 208, 129, 103, 97,
]);

/// Actor ID of builtin library [`gbuiltin_eth_bridge`]
pub const ETH_BRIDGE: ActorId = ActorId([
    242, 129, 108, 237, 11, 21, 116, 149, 149, 57, 45, 58, 24, 181, 162, 54, 61, 111, 239, 229,
    179, 182, 21, 55, 57, 242, 24, 21, 27, 122, 205, 191,
]);

#[cfg(feature = "testing")]
mod tests {
    use crate::{builtin::SEED, ActorId};
    use blake2::{digest::typenum::U32, Blake2b, Digest};
    use parity_scale_codec::Encode;

    /// Generate actor id from number
    pub fn generate_actor_id(id: u64) -> ActorId {
        ActorId(hash((SEED, id).encode().as_slice()))
    }

    /// Blake2 hash
    fn hash(data: &[u8]) -> [u8; 32] {
        let mut ctx = Blake2b::<U32>::new();
        ctx.update(data);
        ctx.finalize().into()
    }

    #[test]
    fn actor_ids_matched() {
        use crate::builtin::BuiltinActor;

        assert_eq!(
            generate_actor_id(BuiltinActor::Bls12_381.id()),
            BuiltinActor::Bls12_381.actor_id()
        );
        assert_eq!(
            generate_actor_id(BuiltinActor::EthBridge.id()),
            BuiltinActor::EthBridge.actor_id()
        );
        assert_eq!(
            generate_actor_id(BuiltinActor::Staking.id()),
            BuiltinActor::Staking.actor_id()
        );
    }
}
