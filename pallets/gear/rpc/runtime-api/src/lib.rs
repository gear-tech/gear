// This file is part of Gear.

// Copyright (C) 2021-2023 Gear Technologies Inc.
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

#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet_gear::{manager::HandleKind, GasInfo};
use sp_core::H256;
use sp_runtime::traits::Block as BlockT;
use sp_std::vec::Vec;

sp_api::decl_runtime_apis! {
    pub trait GearApi {
        #[allow(clippy::too_many_arguments)]
        fn calculate_gas_info(source: H256, kind: HandleKind, payload: Vec<u8>, value: u128, allow_other_panics: bool, initial_gas: Option<u64>,) -> Result<GasInfo, Vec<u8>>;

        /// Generate inherent-like extrinsic that runs message queue processing.
        fn gear_run_extrinsic() -> <Block as BlockT>::Extrinsic;

        fn read_state(program_id: H256, payload: Vec<u8>) -> Result<Vec<u8>, Vec<u8>>;

        fn read_state_using_wasm(program_id: H256, fn_name: Vec<u8>, wasm: Vec<u8>, argument: Option<Vec<u8>>) -> Result<Vec<u8>, Vec<u8>>;

        fn read_metahash(program_id: H256) -> Result<H256, Vec<u8>>;
    }
}
