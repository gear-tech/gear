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

//! Custom origins for governance interventions.

pub use pallet_custom_origins::*;

#[frame_support::pallet]
pub mod pallet_custom_origins {
    use frame_support::pallet_prelude::*;

    #[pallet::config]
    pub trait Config: frame_system::Config {}

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    #[derive(PartialEq, Eq, Clone, MaxEncodedLen, Encode, Decode, TypeInfo, RuntimeDebug)]
    #[pallet::origin]
    pub enum Origin {
        /// Origin for cancelling slashes.
        StakingAdmin,
        /// Origin for spending (any amount of) funds.
        Treasurer,
        /// Origin for managing the composition of the fellowship.
        FellowshipAdmin,
        /// Origin for managing the registrar.
        GeneralAdmin,
        /// Origin able to cancel referenda.
        ReferendumCanceller,
        /// Origin able to kill referenda.
        ReferendumKiller,
        /// Origin able to dispatch a whitelisted call.
        WhitelistedCaller,
        /// Origin commanded by any members of the Gear Fellowship (no grade needed).
        FellowshipInitiates,
        /// Origin commanded by Gear Fellows (1st grade or greater).
        Fellows,
        /// Origin commanded by Gear Experts (2nd grade or greater).
        FellowshipExperts,
        /// Origin commanded by Gear Masters (3rd grade of greater).
        FellowshipMasters,
    }

    macro_rules! decl_unit_ensures {
		( $name:ident: $success_type:ty = $success:expr ) => {
			pub struct $name;
			impl<O: Into<Result<Origin, O>> + From<Origin>>
				EnsureOrigin<O> for $name
			{
				type Success = $success_type;
				fn try_origin(o: O) -> Result<Self::Success, O> {
					o.into().and_then(|o| match o {
						Origin::$name => Ok($success),
						r => Err(O::from(r)),
					})
				}
				#[cfg(feature = "runtime-benchmarks")]
				fn try_successful_origin() -> Result<O, ()> {
					Ok(O::from(Origin::$name))
				}
			}
		};
		( $name:ident ) => { decl_unit_ensures! { $name : () = () } };
		( $name:ident: $success_type:ty = $success:expr, $( $rest:tt )* ) => {
			decl_unit_ensures! { $name: $success_type = $success }
			decl_unit_ensures! { $( $rest )* }
		};
		( $name:ident, $( $rest:tt )* ) => {
			decl_unit_ensures! { $name }
			decl_unit_ensures! { $( $rest )* }
		};
		() => {}
	}
    decl_unit_ensures!(
        StakingAdmin,
        Treasurer,
        FellowshipAdmin,
        GeneralAdmin,
        ReferendumCanceller,
        ReferendumKiller,
        WhitelistedCaller,
        FellowshipInitiates: u16 = 0,
        Fellows: u16 = 1,
        FellowshipExperts: u16 = 2,
        FellowshipMasters: u16 = 3,
    );
}
