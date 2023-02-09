// This file is part of Gear.

// Copyright (C) 2021-2022 Gear Technologies Inc.
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

//! Governance configuration for the Vara runtime.

use super::*;
use frame_support::{
    parameter_types,
    traits::{ConstU16, EitherOf},
};
use frame_system::EnsureRootWithSuccess;

mod origins;
pub use origins::{
    pallet_custom_origins, Fellows, FellowshipAdmin, FellowshipExperts, FellowshipInitiates,
    FellowshipMasters, GeneralAdmin, ReferendumCanceller, ReferendumKiller, StakingAdmin,
    Treasurer, WhitelistedCaller,
};
mod tracks;
pub use tracks::TracksInfo;
mod fellowship;
pub use fellowship::{FellowshipCollectiveInstance, FellowshipReferendaInstance};

parameter_types! {
    pub const VoteLockingPeriod: BlockNumber = 7 * DAYS;
}

impl pallet_conviction_voting::Config for Runtime {
    type WeightInfo = pallet_conviction_voting::weights::SubstrateWeight<Self>;
    type RuntimeEvent = RuntimeEvent;
    type Currency = Balances;
    type VoteLockingPeriod = VoteLockingPeriod;
    type MaxVotes = ConstU32<512>;
    type MaxTurnout =
        frame_support::traits::tokens::currency::ActiveIssuanceOf<Balances, Self::AccountId>;
    type Polls = Referenda;
}

parameter_types! {
    pub const AlarmInterval: BlockNumber = 1;
    pub const SubmissionDeposit: Balance = 10 * DOLLARS;
    pub const UndecidingTimeout: BlockNumber = 14 * DAYS;
}

impl pallet_custom_origins::Config for Runtime {}

impl pallet_whitelist::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type RuntimeCall = RuntimeCall;
    type WhitelistOrigin =
        EitherOf<EnsureRootWithSuccess<Self::AccountId, ConstU16<65535>>, Fellows>;
    type DispatchWhitelistedOrigin = EitherOf<EnsureRoot<Self::AccountId>, WhitelistedCaller>;
    type Preimages = Preimage;
    type WeightInfo = pallet_whitelist::weights::SubstrateWeight<Runtime>;
}

impl pallet_referenda::Config for Runtime {
    type WeightInfo = pallet_referenda::weights::SubstrateWeight<Self>;
    type RuntimeCall = RuntimeCall;
    type RuntimeEvent = RuntimeEvent;
    type Scheduler = Scheduler;
    type Currency = Balances;
    type SubmitOrigin = frame_system::EnsureSigned<AccountId>;
    type CancelOrigin = ReferendumCanceller;
    type KillOrigin = ReferendumKiller;
    type Slash = ();
    type Votes = pallet_conviction_voting::VotesOf<Runtime>;
    type Tally = pallet_conviction_voting::TallyOf<Runtime>;
    type SubmissionDeposit = SubmissionDeposit;
    type MaxQueued = ConstU32<100>;
    type UndecidingTimeout = UndecidingTimeout;
    type AlarmInterval = AlarmInterval;
    type Tracks = TracksInfo;
    type Preimages = Preimage;
}
