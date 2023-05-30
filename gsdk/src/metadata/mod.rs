// This file is part of Gear.
//
// Copyright (C) 2023 Gear Technologies Inc.
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
//! TODO: clean this file after #2668
#![allow(dead_code, unused_imports, non_camel_case_types)]
#![allow(clippy::all)]
#![allow(unused)]

pub mod errors;
mod generated;
mod impls;

pub use self::{
    errors::ModuleError,
    generated::runtime_types::runtime_types::{
        self, vara_runtime as gear_runtime, vara_runtime::RuntimeEvent as Event,
    },
};

pub mod system {
    pub use super::runtime_types::frame_system::pallet::Event;
}

pub mod grandpa {
    pub use super::runtime_types::pallet_grandpa::pallet::Event;
}

pub mod balances {
    pub use super::runtime_types::pallet_balances::pallet::Event;
}

pub mod vesting {
    pub use super::runtime_types::pallet_vesting::pallet::Event;
}

pub mod transaction_payment {
    pub use super::runtime_types::pallet_transaction_payment::pallet::Event;
}

pub mod bags_list {
    pub use super::runtime_types::pallet_bags_list::pallet::Event;
}

pub mod im_online {
    pub use super::runtime_types::pallet_im_online::pallet::Event;
}

pub mod staking {
    pub use super::runtime_types::pallet_staking::pallet::pallet::Event;
}

pub mod session {
    pub use super::runtime_types::pallet_session::pallet::Event;
}

pub mod treasury {
    pub use super::runtime_types::pallet_treasury::pallet::Event;
}

pub mod conviction_voting {
    pub use super::runtime_types::pallet_conviction_voting::pallet::Event;
}

pub mod referenda {
    pub use super::runtime_types::pallet_referenda::pallet::Event;
}

pub mod fellowship_collective {
    pub use super::runtime_types::pallet_ranked_collective::pallet::Event;
}

pub mod fellowship_referenda {
    pub use super::runtime_types::pallet_ranked_collective::pallet::Event;
}

pub mod whitelist {
    pub use super::runtime_types::pallet_whitelist::pallet::Event;
}

pub mod sudo {
    pub use super::runtime_types::pallet_sudo::pallet::Event;
}

pub mod scheduler {
    pub use super::runtime_types::pallet_scheduler::pallet::Event;
}

pub mod preimage {
    pub use super::runtime_types::pallet_preimage::pallet::Event;
}

pub mod identity {
    pub use super::runtime_types::pallet_identity::pallet::Event;
}
pub mod utility {
    pub use super::runtime_types::pallet_utility::pallet::Event;
}

pub mod gear {
    pub use super::runtime_types::pallet_gear::pallet::Event;
}

pub mod staking_rewards {
    pub use super::runtime_types::pallet_gear_staking_rewards::pallet::Event;
}

pub mod airdrop {
    pub use super::runtime_types::pallet_airdrop::pallet::Event;
}

pub mod gear_debug {
    pub use super::runtime_types::pallet_gear_debug::pallet::Event;
}

pub type DispatchError = runtime_types::sp_runtime::DispatchError;
