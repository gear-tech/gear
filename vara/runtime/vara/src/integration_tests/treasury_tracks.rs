// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use super::*;
use crate::governance::{TracksInfo, pallet_custom_origins::Origin as CustomOrigin};
use frame_support::traits::{OriginTrait, StorePreimage, schedule::DispatchTime};
use pallet_conviction_voting::{AccountVote, Conviction, Vote};
use pallet_referenda::{
    DecidingCount, ReferendumInfo, ReferendumInfoFor, TrackQueue, TracksInfo as _,
};
use pallet_treasury::Spends;
use sp_runtime::traits::Header as _;

fn block_driver() -> impl FnMut(BlockNumber, &dyn Fn()) {
    // ExtBuilder opened block 1. Skip idle heights but run the full hooks at each
    // referendum transition, with consecutive BABE slots.
    on_finalize(1);
    let mut parent = System::finalize().hash();
    let mut slot = 1u64;
    move |number, check| {
        slot += 1;
        let header = Header::new(
            number,
            Default::default(),
            Default::default(),
            parent,
            Digest {
                logs: vec![DigestItem::PreRuntime(
                    BABE_ENGINE_ID,
                    PreDigest::SecondaryPlain(SecondaryPlainPreDigest {
                        slot: Slot::from(slot),
                        authority_index: 0,
                    })
                    .encode(),
                )],
            },
        );
        Executive::initialize_block(&header);
        assert_ok!(Timestamp::set(RuntimeOrigin::none(), slot * SLOT_DURATION));
        check();
        assert_ok!(Gear::run(RuntimeOrigin::none(), None));
        parent = Executive::finalize_block().hash();
    }
}

#[test]
fn treasury_tracks_restarted_confirmation_enacts_after_the_decision_period() {
    let voter = AccountKeyring::Alice.to_account_id();
    let beneficiary = AccountKeyring::Bob.to_account_id();
    ExtBuilder::default()
        .initial_authorities(vec![bridge_test_authority(
            AccountKeyring::Charlie,
            "Charlie",
        )])
        .stash(STASH)
        .endowed_accounts(vec![voter.clone()])
        .endowment(ENDOWMENT)
        .build()
        .execute_with(|| {
            let origin = RuntimeOrigin::signed(voter);
            let proposal_origin = RuntimeOrigin::from(CustomOrigin::SmallTipper).into_caller();
            let track_id = TracksInfo::track_for(&proposal_origin).unwrap();
            let track = TracksInfo::info(track_id).unwrap();
            let amount = 10 * UNITS;
            let call = RuntimeCall::Treasury(pallet_treasury::Call::spend {
                asset_kind: Box::new(()),
                amount,
                beneficiary: Box::new(beneficiary.clone()),
                valid_from: None,
            });
            assert_ok!(Referenda::submit(
                origin.clone(),
                Box::new(proposal_origin),
                Preimage::bound(call).unwrap(),
                DispatchTime::After(0),
            ));
            assert_ok!(Referenda::place_decision_deposit(origin.clone(), 0));
            let vote = |aye| {
                assert_ok!(ConvictionVoting::vote(
                    origin.clone(),
                    0,
                    AccountVote::Standard {
                        vote: Vote {
                            aye,
                            conviction: Conviction::Locked1x,
                        },
                        balance: ENDOWMENT * 3 / 4,
                    },
                ));
            };
            let ongoing = || Referenda::ensure_ongoing(0).unwrap();

            vote(true);
            let first = 1 + track.prepare_period;
            let first_deadline = first + track.confirm_period;
            let decision_end = first + track.decision_period;
            let restart = decision_end - 1;
            let confirmed = restart + track.confirm_period;
            let enacted = confirmed + track.min_enactment_period;

            let mut block = block_driver();

            block(2, &|| assert!(ongoing().deciding.is_none()));
            block(first, &|| {
                let deciding = ongoing().deciding.unwrap();
                assert_eq!(deciding.since, first);
                assert_eq!(deciding.confirming, Some(first_deadline));
                vote(false);
            });
            block(first + 1, &|| {
                assert_eq!(ongoing().deciding.unwrap().confirming, None);
            });
            block(first_deadline, &|| {
                assert_eq!(ongoing().deciding.unwrap().confirming, None);
                assert!(!Spends::<Runtime>::contains_key(0));
            });
            block(restart - 1, &|| vote(true));
            block(restart, &|| {
                assert_eq!(ongoing().deciding.unwrap().confirming, Some(confirmed));
            });
            block(decision_end, &|| {
                assert_eq!(ongoing().deciding.unwrap().confirming, Some(confirmed));
                assert!(!Spends::<Runtime>::contains_key(0));
            });
            block(confirmed - 1, &|| {
                assert_eq!(ongoing().deciding.unwrap().confirming, Some(confirmed));
            });
            block(confirmed, &|| {
                assert!(matches!(
                    ReferendumInfoFor::<Runtime>::get(0),
                    Some(ReferendumInfo::Approved(at, ..)) if at == confirmed
                ));
                assert!(!Spends::<Runtime>::contains_key(0));
            });
            block(confirmed + 1, &|| {
                assert_eq!(DecidingCount::<Runtime>::get(track_id), 0);
            });
            block(enacted - 1, &|| {
                assert!(!Spends::<Runtime>::contains_key(0))
            });
            block(enacted, &|| {
                assert!(Spends::<Runtime>::contains_key(0));
                System::assert_has_event(RuntimeEvent::Treasury(
                    pallet_treasury::Event::AssetSpendApproved {
                        index: 0,
                        asset_kind: (),
                        amount,
                        beneficiary: beneficiary.clone(),
                        valid_from: enacted,
                        expire_at: enacted + PayoutSpendPeriod::get(),
                    },
                ));
            });
        });
}

#[test]
fn treasury_tracks_scheduler_refills_a_full_root_track_on_the_next_block() {
    let voter = AccountKeyring::Alice.to_account_id();
    let proposal_origin = RuntimeOrigin::root().into_caller();
    let track_id = TracksInfo::track_for(&proposal_origin).unwrap();
    let track = TracksInfo::info(track_id).unwrap();
    ExtBuilder::default()
        .initial_authorities(vec![bridge_test_authority(
            AccountKeyring::Charlie,
            "Charlie",
        )])
        .stash(STASH)
        .endowed_accounts(vec![voter.clone()])
        .endowment(2 * track.decision_deposit + ENDOWMENT)
        .build()
        .execute_with(|| {
            let origin = RuntimeOrigin::signed(voter);
            for index in 0..2 {
                let call = RuntimeCall::System(frame_system::Call::remark { remark: vec![] });
                assert_ok!(Referenda::submit(
                    origin.clone(),
                    Box::new(proposal_origin.clone()),
                    Preimage::bound(call).unwrap(),
                    DispatchTime::After(0),
                ));
                assert_ok!(Referenda::place_decision_deposit(origin.clone(), index));
            }
            let first = 1 + track.prepare_period;
            let mut block = block_driver();
            let queued = core::cell::Cell::new(None);
            block(first, &|| {
                assert_eq!(DecidingCount::<Runtime>::get(track_id), 1);
                let queue = TrackQueue::<Runtime>::get(track_id);
                let &[(waiting, _)] = queue.as_slice() else {
                    panic!("expected one queued referendum, got {queue:?}");
                };
                queued.set(Some(waiting));
                let active = 1 - waiting;
                assert!(
                    Referenda::ensure_ongoing(active)
                        .unwrap()
                        .deciding
                        .is_some()
                );
                let status = Referenda::ensure_ongoing(waiting).unwrap();
                assert!(status.in_queue);
                assert!(status.deciding.is_none());

                assert_ok!(Referenda::cancel(
                    CustomOrigin::ReferendumCanceller.into(),
                    active,
                ));
                assert!(
                    Referenda::ensure_ongoing(waiting)
                        .unwrap()
                        .deciding
                        .is_none()
                );
                assert_eq!(TrackQueue::<Runtime>::get(track_id), queue);
            });
            block(first + 1, &|| {
                let status = Referenda::ensure_ongoing(queued.get().unwrap()).unwrap();
                assert!(!status.in_queue);
                assert_eq!(status.deciding.unwrap().since, first + 1);
                assert!(TrackQueue::<Runtime>::get(track_id).is_empty());
                assert_eq!(DecidingCount::<Runtime>::get(track_id), 1);
            });
        });
}
