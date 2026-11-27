// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Custom origins for governance interventions.

#![allow(clippy::manual_inspect)]

pub use pallet_custom_origins::*;

#[frame_support::pallet]
pub mod pallet_custom_origins {
    use frame_support::pallet_prelude::*;

    use crate::{Balance, UNITS};

    #[pallet::config]
    pub trait Config: frame_system::Config {}

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    #[derive(PartialEq, Eq, Clone, MaxEncodedLen, Encode, Decode, TypeInfo, RuntimeDebug)]
    #[pallet::origin]
    pub enum Origin {
        /// Origin for cancelling slashes and managing election provider.
        StakingAdmin,
        /// Origin able to spend up to 100,000,000 VARA from the treasury at once.
        Treasurer,
        /// Origin for managing the composition of the fellowship.
        FellowshipAdmin,
        /// Origin for managing the registrar.
        GeneralAdmin,
        /// Origin able to cancel referenda.
        ReferendumCanceller,
        /// Origin able to kill referenda.
        ReferendumKiller,
        /// Origin able to spend up to 1,000 VARA from the treasury at once.
        SmallTipper,
        /// Origin able to spend up to 5,000 VARA from the treasury at once.
        BigTipper,
        /// Origin able to spend up to 500,000 VARA from the treasury at once.
        SmallSpender,
        /// Origin able to spend up to 10,000,000 VARA from the treasury at once.
        MediumSpender,
        /// Origin able to spend up to 50,000,000 VARA from the treasury at once.
        BigSpender,
        /// Origin able to dispatch a whitelisted call.
        WhitelistedCaller,
        /// Origin commanded by any members of the Vara Fellowship (no grade needed).
        FellowshipInitiates,
        /// Origin commanded by Vara Fellows (1st grade or greater).
        Fellows,
        /// Origin commanded by Vara Experts (2nd grade or greater).
        FellowshipExperts,
        /// Origin commanded by Vara Masters (3rd grade of greater).
        FellowshipMasters,
        /// Origin commanded by rank 1 of the Vara Fellowship and with a success of 1.
        Fellowship1Dan,
        /// Origin commanded by rank 2 of the Vara Fellowship and with a success of 2.
        Fellowship2Dan,
        /// Origin commanded by rank 3 of the Vara Fellowship and with a success of 3.
        Fellowship3Dan,
        /// Origin commanded by rank 4 of the Vara Fellowship and with a success of 4.
        Fellowship4Dan,
        /// Origin commanded by rank 5 of the Vara Fellowship and with a success of 5.
        Fellowship5Dan,
        /// Origin commanded by rank 6 of the Vara Fellowship and with a success of 6.
        Fellowship6Dan,
        /// Origin commanded by rank 7 of the Vara Fellowship and with a success of 7.
        Fellowship7Dan,
        /// Origin commanded by rank 8 of the Vara Fellowship and with a success of 8.
        Fellowship8Dan,
        /// Origin commanded by rank 9 of the Vara Fellowship and with a success of 9.
        Fellowship9Dan,
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
        Fellows: u16 = 3,
        FellowshipExperts: u16 = 5,
        FellowshipMasters: u16 = 7,
    );

    macro_rules! decl_ensure {
        (
            $vis:vis type $name:ident: EnsureOrigin<Success = $success_type:ty> {
                $( $item:ident = $success:expr, )*
            }
        ) => {
            $vis struct $name;
            impl<O: Into<Result<Origin, O>> + From<Origin>>
                EnsureOrigin<O> for $name
            {
                type Success = $success_type;
                fn try_origin(o: O) -> Result<Self::Success, O> {
                    o.into().and_then(|o| match o {
                        $(
                            Origin::$item => Ok($success),
                        )*
                        r => Err(O::from(r)),
                    })
                }
                #[cfg(feature = "runtime-benchmarks")]
                fn try_successful_origin() -> Result<O, ()> {
                    // By convention the more privileged origins go later, so for greatest chance
                    // of success, we want the last one.
                    let _result: Result<O, ()> = Err(());
                    $(
                        let _result: Result<O, ()> = Ok(O::from(Origin::$item));
                    )*
                    _result
                }
            }
        }
    }

    decl_ensure! {
        pub type Spender: EnsureOrigin<Success = Balance> {
            SmallTipper = 1_000 * UNITS,
            BigTipper = 5_000 * UNITS,
            SmallSpender = 500_000 * UNITS,
            MediumSpender = 10_000_000 * UNITS,
            BigSpender = 50_000_000 * UNITS,
            Treasurer = 100_000_000 * UNITS,
        }
    }

    decl_ensure! {
        pub type EnsureFellowship: EnsureOrigin<Success = u16> {
            Fellowship1Dan = 1,
            Fellowship2Dan = 2,
            Fellowship3Dan = 3,
            Fellowship4Dan = 4,
            Fellowship5Dan = 5,
            Fellowship6Dan = 6,
            Fellowship7Dan = 7,
            Fellowship8Dan = 8,
            Fellowship9Dan = 9,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Origin;
    use crate::{
        Balance, Runtime, RuntimeCall, RuntimeOrigin, System, UNITS, governance::TracksInfo,
    };
    use frame_support::{assert_noop, assert_ok, traits::OriginTrait};
    use pallet_referenda::TracksInfo as _;
    use parity_scale_codec::Encode;
    use sp_keyring::AccountKeyring;
    use sp_runtime::traits::Dispatchable;

    #[test]
    fn treasury_spend_limits_match_origins_and_tracks() {
        sp_io::TestExternalities::default().execute_with(|| {
            System::set_block_number(1);
            let spend = |amount| {
                RuntimeCall::Treasury(pallet_treasury::Call::spend {
                    asset_kind: Box::new(()),
                    amount,
                    beneficiary: Box::new(AccountKeyring::Alice.to_account_id()),
                    valid_from: None,
                })
            };
            for (origin, codec_index, track_id, cap) in [
                (Origin::SmallTipper, 6, 30, 1_000),
                (Origin::BigTipper, 7, 31, 5_000),
                (Origin::SmallSpender, 8, 32, 500_000),
                (Origin::MediumSpender, 9, 33, 10_000_000),
                (Origin::BigSpender, 10, 34, 50_000_000),
                (Origin::Treasurer, 1, 11, 100_000_000),
            ] {
                assert_eq!(origin.encode(), [codec_index]);
                let origin: RuntimeOrigin = origin.into();
                assert_eq!(TracksInfo::track_for(origin.caller()), Ok(track_id));
                assert_ok!(spend(1).dispatch(origin.clone()));
                assert_ok!(spend(cap * UNITS - 1).dispatch(origin.clone()));
                assert_ok!(spend(cap * UNITS).dispatch(origin.clone()));
                assert_noop!(
                    spend(cap * UNITS + 1).dispatch(origin),
                    pallet_treasury::Error::<Runtime>::InsufficientPermission
                );
            }
            let root = RuntimeOrigin::root();
            assert_eq!(TracksInfo::track_for(root.caller()), Ok(0));
            assert_ok!(spend(Balance::MAX).dispatch(root));
        });
    }
}
