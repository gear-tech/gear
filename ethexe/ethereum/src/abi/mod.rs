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

use alloy::sol;

mod events;
mod gear;

pub use middleware_abi::*;
pub use mirror_abi::*;

// TODO (breathx): remove this dummy hack to avoid reentrancy issues with
// the `sol!` macro, dealing with internal libraries (e.g. 'Gear').
mod mirror_abi {
    alloy::sol!(
        #[sol(rpc)]
        IMirror,
        "Mirror.json"
    );
}

mod middleware_abi {
    alloy::sol!(
        #[sol(rpc)]
        IMiddleware,
        "Middleware.json"
    );
}

sol!(
    #[allow(clippy::too_many_arguments)]
    #[sol(rpc)]
    IRouter,
    "Router.json"
);

sol!(
    #[sol(rpc)]
    ITransparentUpgradeableProxy,
    "TransparentUpgradeableProxy.json"
);

sol!(
    #[allow(clippy::too_many_arguments)]
    #[sol(rpc)]
    IWrappedVara,
    "WrappedVara.json"
);

pub(crate) mod utils {
    use alloy::primitives::{FixedBytes, Uint};
    use gprimitives::{ActorId, CodeId, H256, MessageId, U256};

    pub use alloy::primitives::Bytes;

    pub type Bytes32 = FixedBytes<32>;
    pub type Uint256 = Uint<256, 4>;
    pub type Uint48 = Uint<48, 1>;

    pub fn actor_id_to_address_lossy(actor_id: ActorId) -> alloy::primitives::Address {
        actor_id.to_address_lossy().to_fixed_bytes().into()
    }

    pub fn address_to_actor_id(address: alloy::primitives::Address) -> ActorId {
        (*address.into_word()).into()
    }

    pub fn bytes32_to_code_id(bytes: Bytes32) -> CodeId {
        bytes.0.into()
    }

    pub fn bytes32_to_h256(bytes: Bytes32) -> H256 {
        bytes.0.into()
    }

    pub fn bytes32_to_message_id(bytes: Bytes32) -> MessageId {
        bytes.0.into()
    }

    pub fn code_id_to_bytes32(code_id: CodeId) -> Bytes32 {
        code_id.into_bytes().into()
    }

    pub fn message_id_to_bytes32(message_id: MessageId) -> Bytes32 {
        message_id.into_bytes().into()
    }

    pub fn h256_to_bytes32(h256: H256) -> Bytes32 {
        h256.0.into()
    }

    pub fn u64_to_uint48_lossy(value: u64) -> Uint48 {
        Uint48::try_from(value).unwrap_or(Uint48::MAX)
    }

    pub fn uint256_to_u128_lossy(value: Uint256) -> u128 {
        let [low, high, ..] = value.into_limbs();

        ((high as u128) << 64) | (low as u128)
    }

    pub fn u256_to_uint256(value: U256) -> Uint256 {
        let mut bytes = [0u8; Uint256::BYTES];
        value.to_little_endian(&mut bytes);
        Uint256::from_le_bytes(bytes)
    }

    pub fn uint256_to_u256(value: Uint256) -> U256 {
        let bytes: [u8; Uint256::BYTES] = value.to_le_bytes();
        U256::from_little_endian(&bytes)
    }

    #[test]
    fn casts_are_correct() {
        use rand::Rng;

        let mut rng = rand::thread_rng();

        // uint256 -> u128
        assert_eq!(uint256_to_u128_lossy(Uint256::MAX), u128::MAX);

        for _ in 0..10 {
            let val: u128 = rng.r#gen();
            let uint256 = Uint256::from(val);

            assert_eq!(uint256_to_u128_lossy(uint256), val);
        }

        // u64 -> uint48
        assert_eq!(u64_to_uint48_lossy(u64::MAX), Uint48::MAX);

        for _ in 0..10 {
            let val = rng.gen_range(0..=Uint48::MAX.into_limbs()[0]);
            let uint48 = Uint48::from(val);

            assert_eq!(u64_to_uint48_lossy(val), uint48);

            assert_eq!(
                ethexe_common::u64_into_uint48_be_bytes_lossy(val),
                uint48.to_be_bytes()
            );
        }
    }
}
