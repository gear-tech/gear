// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use super::*;
use crate::governance::pallet_custom_origins::Origin as CustomOrigin;
use frame_support::traits::{
    OriginTrait, StorePreimage,
    schedule::{DispatchTime, v3::Anon},
};
use pallet_bounties::{BountyApprovals, BountyStatus, Call as BountyCall};
use pallet_treasury::{
    Call as TreasuryCall, Error as TreasuryError, Event as TreasuryEvent, Spends,
};
use pallet_utility::{Call as UtilityCall, Event as UtilityEvent};
use sp_runtime::{DispatchError, DispatchResult, MultiAddress, traits::Hash as _};

const AMOUNT: Balance = 10 * UNITS;

fn ext() -> sp_io::TestExternalities {
    ExtBuilder::default()
        .initial_authorities(vec![bridge_test_authority(
            AccountKeyring::Charlie,
            "Charlie",
        )])
        .stash(STASH)
        .endowed_accounts(vec![AccountKeyring::Alice.to_account_id()])
        .endowment(ENDOWMENT)
        .build()
}

fn signed() -> RuntimeOrigin {
    RuntimeOrigin::signed(AccountKeyring::Alice.to_account_id())
}

fn spender() -> RuntimeOrigin {
    CustomOrigin::SmallTipper.into()
}

fn beneficiary() -> AccountId {
    AccountKeyring::Bob.to_account_id()
}

fn dispatch(call: RuntimeCall, origin: RuntimeOrigin) -> DispatchResult {
    call.dispatch(origin).map(|_| ()).map_err(|err| err.error)
}

fn propose_bounty() {
    assert_ok!(dispatch(
        RuntimeCall::Bounties(BountyCall::propose_bounty {
            value: BountyValueMinimum::get(),
            description: vec![],
        }),
        signed(),
    ));
}

fn legacy_calls(bounty_id: u32) -> [RuntimeCall; 2] {
    [
        RuntimeCall::Treasury(TreasuryCall::spend_local {
            amount: AMOUNT,
            beneficiary: MultiAddress::Id(beneficiary()),
        }),
        RuntimeCall::Bounties(BountyCall::approve_bounty { bounty_id }),
    ]
}

fn spend(amount: Balance, valid_from: Option<BlockNumber>) -> RuntimeCall {
    RuntimeCall::Treasury(TreasuryCall::spend {
        asset_kind: Box::new(()),
        amount,
        beneficiary: Box::new(beneficiary()),
        valid_from,
    })
}

fn batch(calls: Vec<RuntimeCall>) -> RuntimeCall {
    RuntimeCall::Utility(UtilityCall::batch { calls })
}

fn assert_legacy_unchanged() {
    assert_eq!(Treasury::proposal_count(), 0);
    assert!(Treasury::approvals().is_empty());
    assert!(Treasury::proposals(0).is_none());
    assert!(BountyApprovals::<Runtime>::get().is_empty());
    assert_eq!(
        pallet_bounties::Bounties::<Runtime>::get(0)
            .unwrap()
            .get_status(),
        BountyStatus::Proposed,
    );
}

fn fund_treasury(amount: Balance) {
    assert_ok!(Balances::transfer_keep_alive(
        signed(),
        MultiAddress::Id(Treasury::account_id()),
        amount,
    ));
}

#[test]
fn legacy_calls_are_filtered_directly_and_through_nested_utility() {
    ext().execute_with(|| {
        propose_bounty();
        let filtered: DispatchError = frame_system::Error::<Runtime>::CallFiltered.into();
        for (index, legacy) in legacy_calls(0).into_iter().enumerate() {
            assert_noop!(dispatch(legacy.clone(), spender()), filtered);
            let calls = || vec![spend(AMOUNT, None), legacy.clone(), spend(AMOUNT, None)];

            assert_ok!(dispatch(
                batch(vec![RuntimeCall::Utility(UtilityCall::batch_all {
                    calls: calls(),
                })]),
                spender(),
            ));
            System::assert_last_event(RuntimeEvent::Utility(UtilityEvent::BatchInterrupted {
                index: 0,
                error: filtered,
            }));
            assert_eq!(Spends::<Runtime>::iter_keys().count(), index);

            System::reset_events();
            assert_ok!(dispatch(batch(vec![batch(calls())]), spender()));
            System::assert_has_event(RuntimeEvent::Utility(UtilityEvent::BatchInterrupted {
                index: 1,
                error: filtered,
            }));
            assert_eq!(Spends::<Runtime>::iter_keys().count(), index + 1);
        }

        System::reset_events();
        let [local, bounty] = legacy_calls(0);
        assert_ok!(dispatch(
            batch(vec![RuntimeCall::Utility(UtilityCall::force_batch {
                calls: vec![spend(AMOUNT, None), local, bounty, spend(AMOUNT, None)],
            })]),
            spender(),
        ));
        let failures = System::events()
            .into_iter()
            .filter_map(|record| match record.event {
                RuntimeEvent::Utility(UtilityEvent::ItemFailed { error }) => Some(error),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(failures, [filtered, filtered]);
        System::assert_has_event(RuntimeEvent::Utility(
            UtilityEvent::BatchCompletedWithErrors,
        ));
        assert_eq!(Spends::<Runtime>::iter_keys().count(), 4);
        assert_legacy_unchanged();
    });
}

#[test]
fn legacy_calls_are_filtered_through_proxy_and_scheduler() {
    ext().execute_with(|| {
        propose_bounty();
        let filtered = frame_system::Error::<Runtime>::CallFiltered.into();
        assert_ok!(Proxy::add_proxy(
            signed(),
            MultiAddress::Id(beneficiary()),
            ProxyType::Any,
            0,
        ));
        let calls = legacy_calls(0);
        for call in &calls {
            assert_ok!(dispatch(
                RuntimeCall::Proxy(pallet_proxy::Call::proxy {
                    real: MultiAddress::Id(AccountKeyring::Alice.to_account_id()),
                    force_proxy_type: Some(ProxyType::Any),
                    call: Box::new(call.clone()),
                }),
                RuntimeOrigin::signed(beneficiary()),
            ));
            System::assert_last_event(RuntimeEvent::Proxy(pallet_proxy::Event::ProxyExecuted {
                result: Err(filtered),
            }));
        }

        let [local, bounty] = calls;
        let tasks = [local, bounty, spend(AMOUNT, None)].map(|call| {
            <Scheduler as Anon<BlockNumber, RuntimeCall, OriginCaller>>::schedule(
                DispatchTime::At(2),
                None,
                0,
                spender().into_caller(),
                Preimage::bound(call).unwrap(),
            )
            .unwrap()
        });
        System::set_block_number(2);
        Scheduler::on_initialize(2);
        for (task, result) in tasks
            .into_iter()
            .zip([Err(filtered), Err(filtered), Ok(())])
        {
            System::assert_has_event(RuntimeEvent::Scheduler(
                pallet_scheduler::Event::Dispatched {
                    task,
                    id: None,
                    result,
                },
            ));
        }
        assert!(Spends::<Runtime>::contains_key(0));
        assert_legacy_unchanged();
    });
}

#[test]
fn root_and_whitelist_enact_legacy_calls_and_existing_approvals_are_funded() {
    ext().execute_with(|| {
        for _ in 0..3 {
            propose_bounty();
        }
        // Seed old approvals through the pallet methods and prove the custom origin satisfies them.
        assert_ok!(Treasury::spend_local(
            spender(),
            AMOUNT,
            MultiAddress::Id(beneficiary()),
        ));
        assert_ok!(Bounties::approve_bounty(spender(), 0));

        for call in legacy_calls(1) {
            assert_ok!(dispatch(call, RuntimeOrigin::root()));
        }
        for call in legacy_calls(2) {
            let call_hash = BlakeTwo256::hash_of(&call);
            assert_ok!(dispatch(
                RuntimeCall::Whitelist(pallet_whitelist::Call::whitelist_call { call_hash }),
                RuntimeOrigin::root(),
            ));
            let enact = RuntimeCall::Whitelist(
                pallet_whitelist::Call::dispatch_whitelisted_call_with_preimage {
                    call: Box::new(call),
                },
            );
            assert_noop!(dispatch(enact.clone(), signed()), DispatchError::BadOrigin);
            let origin = CustomOrigin::WhitelistedCaller;
            assert_ok!(dispatch(enact.clone(), origin.clone().into()));
            assert!(matches!(
                System::events().last().unwrap().event,
                RuntimeEvent::Whitelist(pallet_whitelist::Event::WhitelistedCallDispatched {
                    call_hash: enacted,
                    result: Ok(_),
                }) if enacted == call_hash
            ));
            assert_noop!(
                dispatch(enact, origin.into()),
                pallet_whitelist::Error::<Runtime>::CallIsNotWhitelisted,
            );
        }

        assert_ok!(Treasury::spend_local(
            spender(),
            AMOUNT,
            MultiAddress::Id(beneficiary()),
        ));
        assert_ok!(dispatch(
            RuntimeCall::Treasury(TreasuryCall::remove_approval { proposal_id: 3 }),
            CustomOrigin::Treasurer.into(),
        ));
        assert_eq!(Treasury::approvals().as_slice(), &[0, 1, 2]);
        assert_eq!(BountyApprovals::<Runtime>::get().as_slice(), &[0, 1, 2]);
        assert!(Balances::reserved_balance(AccountKeyring::Alice.to_account_id()) > 0);
        let bounty_value = BountyValueMinimum::get();
        fund_treasury(3 * (AMOUNT + bounty_value));
        assert!(!pallet_staking::Bonded::<Runtime>::contains_key(
            Treasury::account_id()
        ));
        assert!(
            !pallet_nomination_pools::PoolMembers::<Runtime>::contains_key(Treasury::account_id())
        );
        let period = SpendPeriod::get();
        System::set_block_number(period);
        Treasury::on_initialize(period);

        assert_eq!(Treasury::pot(), 0);
        assert!(Treasury::approvals().is_empty());
        assert_eq!(Balances::free_balance(beneficiary()), 3 * AMOUNT);
        assert_eq!(
            Balances::reserved_balance(AccountKeyring::Alice.to_account_id()),
            0
        );
        assert!(BountyApprovals::<Runtime>::get().is_empty());
        for index in 0..3 {
            assert!(Treasury::proposals(index).is_none());
            assert_eq!(
                pallet_bounties::Bounties::<Runtime>::get(index)
                    .unwrap()
                    .get_status(),
                BountyStatus::Funded,
            );
            assert_eq!(
                Balances::free_balance(Bounties::bounty_account_id(index)),
                bounty_value
            );
        }
        assert_ok!(dispatch(
            RuntimeCall::Bounties(BountyCall::close_bounty { bounty_id: 0 }),
            CustomOrigin::Treasurer.into(),
        ));
        assert!(!pallet_bounties::Bounties::<Runtime>::contains_key(0));
        assert_eq!(Treasury::pot(), bounty_value);
    });
}

#[test]
fn staking_lock_prevents_debit_for_legacy_treasury_settlement() {
    ext().execute_with(|| {
        let treasury = Treasury::account_id();
        fund_treasury(AMOUNT);
        assert_ok!(Staking::bond(
            RuntimeOrigin::signed(treasury.clone()),
            AMOUNT,
            pallet_staking::RewardDestination::Staked,
        ));
        assert_ok!(Treasury::spend_local(
            spender(),
            AMOUNT,
            MultiAddress::Id(beneficiary()),
        ));
        let issuance = Balances::total_issuance();
        let treasury_balance = Balances::free_balance(&treasury);
        let period = SpendPeriod::get();
        System::set_block_number(period);
        Treasury::on_initialize(period);

        // Legacy settlement credits the beneficiary before trying to debit the locked pot.
        assert_eq!(Balances::free_balance(beneficiary()), AMOUNT);
        assert_eq!(Balances::free_balance(&treasury), treasury_balance);
        assert_eq!(Balances::total_issuance(), issuance + AMOUNT);
        assert!(Treasury::approvals().is_empty());
        assert!(Treasury::proposals(0).is_none());
    });
}

#[test]
fn modern_spends_share_the_origin_cap_with_partial_or_atomic_batch_semantics() {
    ext().execute_with(|| {
        let calls = || {
            vec![
                spend(600 * UNITS, None),
                spend(400 * UNITS, None),
                spend(UNITS, None),
            ]
        };
        assert_noop!(
            dispatch(
                RuntimeCall::Utility(UtilityCall::batch_all { calls: calls() }),
                spender(),
            ),
            TreasuryError::<Runtime>::InsufficientPermission,
        );
        assert_ok!(dispatch(batch(calls()), spender()));
        System::assert_last_event(RuntimeEvent::Utility(UtilityEvent::BatchInterrupted {
            index: 2,
            error: TreasuryError::<Runtime>::InsufficientPermission.into(),
        }));
        assert!(Spends::<Runtime>::contains_key(0));
        assert!(Spends::<Runtime>::contains_key(1));
        assert!(!Spends::<Runtime>::contains_key(2));
        assert_ok!(dispatch(spend(UNITS, None), spender()));
        assert!(Spends::<Runtime>::contains_key(2));

        System::reset_events();
        let scheduled = <Scheduler as Anon<BlockNumber, RuntimeCall, OriginCaller>>::schedule(
            DispatchTime::At(2),
            None,
            0,
            spender().into_caller(),
            Preimage::bound(batch(calls())).unwrap(),
        );
        assert_ok!(scheduled);
        System::set_block_number(2);
        Scheduler::on_initialize(2);
        System::assert_has_event(RuntimeEvent::Utility(UtilityEvent::BatchInterrupted {
            index: 2,
            error: TreasuryError::<Runtime>::InsufficientPermission.into(),
        }));
        assert_eq!(Spends::<Runtime>::iter_keys().count(), 5);
    });
}

#[test]
fn modern_payout_can_retry_after_funding_but_pays_only_once() {
    ext().execute_with(|| {
        let payout = || RuntimeCall::Treasury(TreasuryCall::payout { index: 0 });
        let status = || RuntimeCall::Treasury(TreasuryCall::check_status { index: 0 });
        assert_ok!(dispatch(spend(AMOUNT, None), spender()));
        assert_noop!(
            dispatch(payout(), signed()),
            TreasuryError::<Runtime>::PayoutError
        );
        assert_noop!(
            dispatch(status(), signed()),
            TreasuryError::<Runtime>::NotAttempted
        );
        assert_eq!(Balances::free_balance(beneficiary()), 0);

        fund_treasury(AMOUNT);
        assert_ok!(dispatch(payout(), signed()));
        System::assert_last_event(RuntimeEvent::Treasury(TreasuryEvent::Paid {
            index: 0,
            payment_id: (),
        }));
        assert_noop!(
            dispatch(payout(), signed()),
            TreasuryError::<Runtime>::AlreadyAttempted
        );
        assert_noop!(
            dispatch(
                RuntimeCall::Treasury(TreasuryCall::void_spend { index: 0 }),
                CustomOrigin::Treasurer.into(),
            ),
            TreasuryError::<Runtime>::AlreadyAttempted,
        );
        assert_ok!(dispatch(status(), signed()));
        System::assert_last_event(RuntimeEvent::Treasury(TreasuryEvent::SpendProcessed {
            index: 0,
        }));
        assert!(!Spends::<Runtime>::contains_key(0));
        assert_noop!(
            dispatch(payout(), signed()),
            TreasuryError::<Runtime>::InvalidIndex
        );
        assert_eq!(Balances::free_balance(beneficiary()), AMOUNT);
        assert_eq!(Treasury::pot(), 0);
    });
}

#[test]
fn modern_spends_keep_their_validity_window_and_can_be_voided() {
    ext().execute_with(|| {
        let payout = |index| RuntimeCall::Treasury(TreasuryCall::payout { index });
        let valid_from = 3;
        assert_ok!(dispatch(spend(AMOUNT, Some(valid_from)), spender()));
        assert_noop!(
            dispatch(payout(0), signed()),
            TreasuryError::<Runtime>::EarlyPayout
        );
        System::set_block_number(valid_from);
        assert_noop!(
            dispatch(payout(0), signed()),
            TreasuryError::<Runtime>::PayoutError
        );

        let expires = valid_from + PayoutSpendPeriod::get();
        System::set_block_number(expires);
        assert_noop!(
            dispatch(payout(0), signed()),
            TreasuryError::<Runtime>::SpendExpired
        );
        System::set_block_number(expires + 1);
        assert_ok!(dispatch(
            RuntimeCall::Treasury(TreasuryCall::check_status { index: 0 }),
            signed(),
        ));
        System::assert_last_event(RuntimeEvent::Treasury(TreasuryEvent::SpendProcessed {
            index: 0,
        }));
        assert!(!Spends::<Runtime>::contains_key(0));

        assert_ok!(dispatch(spend(AMOUNT, None), spender()));
        let void = RuntimeCall::Treasury(TreasuryCall::void_spend { index: 1 });
        assert_noop!(dispatch(void.clone(), signed()), DispatchError::BadOrigin);
        assert_ok!(dispatch(void, CustomOrigin::Treasurer.into()));
        System::assert_last_event(RuntimeEvent::Treasury(TreasuryEvent::AssetSpendVoided {
            index: 1,
        }));
        assert!(!Spends::<Runtime>::contains_key(1));
        assert_noop!(
            dispatch(payout(1), signed()),
            TreasuryError::<Runtime>::InvalidIndex
        );
    });
}
