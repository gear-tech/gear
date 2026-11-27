// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use super::*;
use frame_support::dispatch::DispatchResult;
use pallet_nomination_pools::{self as pools, BondExtra, PoolMembers};
use sp_runtime::traits::Hash as _;

const OPERATOR: ProxyType = ProxyType::TreasuryPoolOperator;
const POOL: pools::PoolId = 1;
const BOND: Balance = 10 * UNITS;

#[test]
fn treasury_pool_operator_preserves_proxy_scale_and_supersets() {
    use ProxyType::*;

    let types = [
        Any,
        NonTransfer,
        Governance,
        Staking,
        IdentityJudgement,
        CancelProxy,
        TreasuryPoolOperator,
    ];
    // Each bit identifies a type in the corresponding row's superset relation.
    let supersets = [
        0b1111111, 0b1111110, 0b0000100, 0b0001000, 0b0010000, 0b0100000, 0b1000000,
    ];
    for (i, proxy_type) in types.iter().enumerate() {
        for (j, other) in types.iter().enumerate() {
            assert_eq!(
                proxy_type.is_superset(other),
                supersets[i] & (1 << j) != 0,
                "{proxy_type:?} vs {other:?}",
            );
        }
    }

    let old_indices = 0..6u8;
    let delay = 0x1234_5678u32;
    let deposit = 0x0123_4567_89ab_cdefu128;
    // A SCALE Proxies entry: compact length, old definitions, and balance deposit.
    let mut old_bytes = vec![6 << 2];
    for index in old_indices.clone() {
        old_bytes.extend([index + 1; 32]);
        old_bytes.push(index);
        old_bytes.extend(delay.to_le_bytes());
    }
    old_bytes.extend(deposit.to_le_bytes());

    sp_io::TestExternalities::default().execute_with(|| {
        let real = Treasury::account_id();
        let key = pallet_proxy::Proxies::<Runtime>::hashed_key_for(&real);
        sp_io::storage::set(&key, &old_bytes);
        let (defs, held) = Proxy::proxies(real);
        assert_eq!(defs.len(), 6);
        assert_eq!(held, deposit);
        for (def, index) in defs.iter().zip(old_indices) {
            assert_eq!(def.delegate, AccountId::new([index + 1; 32]));
            assert_eq!(def.proxy_type, types[usize::from(index)]);
            assert_eq!(def.delay, delay);
        }
        assert_eq!((defs, held).encode(), old_bytes);
    });

    assert_eq!(OPERATOR.encode(), [6]);
    assert_eq!(ProxyType::decode(&mut &[6][..]).unwrap(), OPERATOR);

    use gbuiltin_proxy::Request;
    let delegate = [0x42; 32].into();
    let proxy_type = BuiltinProxyType::CancelProxy;
    assert_eq!(ProxyType::from(proxy_type), CancelProxy);
    for (index, request) in [
        (
            0,
            Request::AddProxy {
                delegate,
                proxy_type,
            },
        ),
        (
            1,
            Request::RemoveProxy {
                delegate,
                proxy_type,
            },
        ),
    ] {
        let mut wire = vec![index];
        wire.extend([0x42; 32]);
        wire.push(5);
        assert_eq!(request.encode(), wire);
        assert_eq!(Request::decode(&mut &wire[..]).unwrap(), request);
        wire[33] = 6;
        assert!(Request::decode(&mut &wire[..]).is_err());
    }
}

fn with_pool(test: impl FnOnce(AccountId, AccountId, AccountId, AccountId)) {
    let treasury = Treasury::account_id();
    let depositor = AccountKeyring::Alice.to_account_id();
    let operator = AccountKeyring::Bob.to_account_id();
    let delayed_operator = AccountKeyring::Charlie.to_account_id();
    ExtBuilder::default()
        .initial_authorities(vec![bridge_test_authority(AccountKeyring::Dave, "Dave")])
        .stash(STASH)
        .endowed_accounts(vec![
            treasury.clone(),
            depositor.clone(),
            operator.clone(),
            delayed_operator.clone(),
        ])
        .endowment(ENDOWMENT)
        .build()
        .execute_with(|| {
            pallet_staking::CurrentEra::<Runtime>::put(0);
            assert_ok!(NominationPools::create(
                RuntimeOrigin::signed(depositor.clone()),
                BOND,
                treasury.clone().into(),
                depositor.clone().into(),
                depositor.clone().into(),
            ));
            assert_ok!(NominationPools::join(
                RuntimeOrigin::signed(treasury.clone()),
                BOND,
                POOL,
            ));
            test(treasury, depositor, operator, delayed_operator);
        });
}

fn register(treasury: &AccountId, delegate: &AccountId, proxy_type: ProxyType, delay: BlockNumber) {
    assert_ok!(Proxy::add_proxy(
        RuntimeOrigin::signed(treasury.clone()),
        delegate.clone().into(),
        proxy_type,
        delay,
    ));
}

fn proxy(
    treasury: &AccountId,
    operator: &AccountId,
    proxy_type: ProxyType,
    call: RuntimeCall,
) -> DispatchResult {
    Proxy::proxy(
        RuntimeOrigin::signed(operator.clone()),
        treasury.clone().into(),
        Some(proxy_type),
        Box::new(call),
    )
}

fn assert_proxy_executed(result: DispatchResult) {
    System::assert_last_event(RuntimeEvent::Proxy(pallet_proxy::Event::ProxyExecuted {
        result,
    }));
}

fn bond_extra() -> RuntimeCall {
    RuntimeCall::NominationPools(pools::Call::bond_extra {
        extra: BondExtra::FreeBalance(BOND),
    })
}

fn unbond(member_account: Address, unbonding_points: Balance) -> RuntimeCall {
    RuntimeCall::NominationPools(pools::Call::unbond {
        member_account,
        unbonding_points,
    })
}

fn withdraw(member_account: Address) -> RuntimeCall {
    RuntimeCall::NominationPools(pools::Call::withdraw_unbonded {
        member_account,
        num_slashing_spans: 0,
    })
}

#[test]
fn treasury_pool_operator_filters_calls_and_force_selects_exactly() {
    with_pool(|treasury, depositor, operator, _| {
        register(&treasury, &operator, ProxyType::Any, 0);
        register(&treasury, &operator, OPERATOR, 0);

        let state = || {
            (
                Balances::free_balance(&treasury),
                Balances::free_balance(&operator),
                Balances::free_balance(NominationPools::generate_bonded_account(POOL)),
                PoolMembers::<Runtime>::get(&treasury),
                PoolMembers::<Runtime>::get(&depositor),
                pools::Metadata::<Runtime>::get(POOL),
                Proxy::proxies(treasury.clone()),
            )
        };
        let before = state();
        for call in [
            unbond(depositor.clone().into(), BOND),
            unbond(Address::Index(()), BOND),
            withdraw(depositor.clone().into()),
            withdraw(Address::Index(())),
            RuntimeCall::Utility(pallet_utility::Call::batch_all {
                calls: vec![bond_extra()],
            }),
            RuntimeCall::Balances(pallet_balances::Call::transfer_keep_alive {
                dest: operator.clone().into(),
                value: BOND,
            }),
            RuntimeCall::NominationPools(pools::Call::set_metadata {
                pool_id: POOL,
                metadata: b"not authorized".to_vec(),
            }),
            RuntimeCall::NominationPools(pools::Call::pool_withdraw_unbonded {
                pool_id: POOL,
                num_slashing_spans: 0,
            }),
            RuntimeCall::NominationPools(pools::Call::bond_extra_other {
                member: treasury.clone().into(),
                extra: BondExtra::FreeBalance(BOND),
            }),
            RuntimeCall::NominationPools(pools::Call::claim_payout_other {
                other: treasury.clone(),
            }),
            RuntimeCall::Staking(pallet_staking::Call::chill {}),
            RuntimeCall::Proxy(pallet_proxy::Call::add_proxy {
                delegate: depositor.clone().into(),
                proxy_type: OPERATOR,
                delay: 0,
            }),
            RuntimeCall::Proxy(pallet_proxy::Call::remove_proxy {
                delegate: operator.clone().into(),
                proxy_type: OPERATOR,
                delay: 0,
            }),
            RuntimeCall::Proxy(pallet_proxy::Call::remove_proxies {}),
        ] {
            assert_ok!(proxy(&treasury, &operator, OPERATOR, call));
            assert_proxy_executed(Err(frame_system::Error::<Runtime>::CallFiltered.into()));
            assert_eq!(state(), before);
        }

        let remark =
            || RuntimeCall::System(frame_system::Call::remark_with_event { remark: vec![] });
        assert_noop!(
            proxy(&treasury, &operator, ProxyType::NonTransfer, remark()),
            pallet_proxy::Error::<Runtime>::NotProxy,
        );
        assert_ok!(proxy(&treasury, &operator, ProxyType::Any, remark()));
        assert_proxy_executed(Ok(()));
        assert_eq!(state(), before);
    });
}

#[test]
fn treasury_pool_operator_keeps_rewards_and_withdrawn_principal_in_treasury() {
    with_pool(|treasury, submitter, operator, delayed_operator| {
        let delay = 2;
        register(&treasury, &operator, OPERATOR, 0);
        register(&treasury, &delayed_operator, ProxyType::Any, delay);
        register(&treasury, &delayed_operator, OPERATOR, delay);
        let reward_account = NominationPools::generate_reward_account(POOL);
        let bonded_account = NominationPools::generate_bonded_account(POOL);
        let seed_rewards = |value| {
            assert_ok!(Balances::transfer_keep_alive(
                RuntimeOrigin::signed(submitter.clone()),
                reward_account.clone().into(),
                value,
            ));
        };
        let announce = |call: &RuntimeCall| {
            assert_ok!(Proxy::announce(
                RuntimeOrigin::signed(delayed_operator.clone()),
                treasury.clone().into(),
                BlakeTwo256::hash_of(call),
            ));
        };
        let submit = |proxy_type, call: RuntimeCall| {
            Proxy::proxy_announced(
                RuntimeOrigin::signed(submitter.clone()),
                delayed_operator.clone().into(),
                treasury.clone().into(),
                Some(proxy_type),
                Box::new(call),
            )
        };
        let held = Balances::reserved_balance(&delayed_operator);
        let assert_announcement_released = || {
            let (pending, deposit) = Proxy::announcements(delayed_operator.clone());
            assert!(pending.is_empty());
            assert_eq!(deposit, 0);
            assert_eq!(Balances::reserved_balance(&delayed_operator), held);
        };
        let other_balances = || {
            (
                Balances::free_balance(&operator),
                Balances::free_balance(&delayed_operator),
                Balances::free_balance(&submitter),
            )
        };
        let treasury_before = Balances::free_balance(&treasury);
        let member_before = PoolMembers::<Runtime>::get(&treasury);

        // Equal initial points make Treasury's first share exactly half the reward.
        seed_rewards(4 * BOND);
        let rewards_before = Balances::free_balance(&reward_account);
        let others_before = other_balances();
        let claim = RuntimeCall::NominationPools(pools::Call::claim_payout {});
        assert_eq!(
            NominationPools::api_pending_rewards(treasury.clone()),
            Some(2 * BOND),
        );
        assert_ok!(proxy(&treasury, &operator, OPERATOR, claim.clone()));
        assert_proxy_executed(Err(frame_system::Error::<Runtime>::CallFiltered.into()));
        assert_noop!(
            proxy(&treasury, &delayed_operator, OPERATOR, claim.clone()),
            pallet_proxy::Error::<Runtime>::Unannounced,
        );
        announce(&claim);
        let (pending, deposit) = Proxy::announcements(delayed_operator.clone());
        assert_eq!(pending.len(), 1);
        assert_eq!(
            deposit,
            AnnouncementDepositBase::get() + AnnouncementDepositFactor::get(),
        );
        assert_eq!(
            Balances::reserved_balance(&delayed_operator),
            held + deposit
        );
        let eligible_at = System::block_number() + delay;
        System::set_block_number(eligible_at - 1);
        assert_noop!(
            submit(OPERATOR, claim.clone()),
            pallet_proxy::Error::<Runtime>::Unannounced,
        );
        System::set_block_number(eligible_at);
        assert_noop!(
            submit(ProxyType::NonTransfer, claim.clone()),
            pallet_proxy::Error::<Runtime>::NotProxy,
        );
        assert_ok!(submit(OPERATOR, claim.clone()));
        assert_proxy_executed(Err(frame_system::Error::<Runtime>::CallFiltered.into()));
        assert_announcement_released();
        assert_noop!(
            submit(OPERATOR, claim),
            pallet_proxy::Error::<Runtime>::Unannounced,
        );
        assert_eq!(PoolMembers::<Runtime>::get(&treasury), member_before);
        assert_eq!(Balances::free_balance(&reward_account), rewards_before);
        assert_eq!(Balances::free_balance(&treasury), treasury_before);
        assert_eq!(other_balances(), others_before);

        let bonded_before = Balances::free_balance(&bonded_account);
        let submitter_before = PoolMembers::<Runtime>::get(&submitter);
        for denied in [
            unbond(Address::Index(()), BOND),
            unbond(submitter.clone().into(), BOND),
            withdraw(Address::Index(())),
            withdraw(submitter.clone().into()),
            RuntimeCall::Utility(pallet_utility::Call::batch_all {
                calls: vec![bond_extra()],
            }),
        ] {
            announce(&denied);
            System::set_block_number(System::block_number() + delay);
            assert_ok!(submit(OPERATOR, denied));
            assert_proxy_executed(Err(frame_system::Error::<Runtime>::CallFiltered.into()));
            assert_announcement_released();
            assert_eq!(PoolMembers::<Runtime>::get(&treasury), member_before);
            assert_eq!(PoolMembers::<Runtime>::get(&submitter), submitter_before);
            assert_eq!(Balances::free_balance(&treasury), treasury_before);
            assert_eq!(Balances::free_balance(&bonded_account), bonded_before);
            assert_eq!(Balances::free_balance(&reward_account), rewards_before);
            assert_eq!(other_balances(), others_before);
        }

        assert_ok!(proxy(&treasury, &operator, OPERATOR, bond_extra()));
        assert_proxy_executed(Ok(()));
        assert_eq!(
            PoolMembers::<Runtime>::get(&treasury).unwrap().points,
            2 * BOND
        );
        assert_eq!(Balances::free_balance(&bonded_account), 3 * BOND);
        assert_eq!(
            Balances::free_balance(&reward_account),
            rewards_before - 2 * BOND
        );
        assert_eq!(Balances::free_balance(&treasury), treasury_before + BOND);
        assert_eq!(other_balances(), others_before);

        // Treasury now owns two thirds of the points before unbonding its entire position.
        seed_rewards(3 * BOND);
        let rewards_before = Balances::free_balance(&reward_account);
        let others_before = other_balances();
        let principal = 2 * BOND;
        assert_ok!(proxy(
            &treasury,
            &operator,
            OPERATOR,
            unbond(treasury.clone().into(), principal),
        ));
        assert_proxy_executed(Ok(()));
        let maturity = BondingDuration::get();
        let member = PoolMembers::<Runtime>::get(&treasury).unwrap();
        assert_eq!(member.points, 0);
        assert_eq!(member.unbonding_eras.get(&maturity), Some(&principal));
        let liquid_after_unbond = treasury_before + 3 * BOND;
        assert_eq!(Balances::free_balance(&treasury), liquid_after_unbond);
        assert_eq!(
            Balances::free_balance(&reward_account),
            rewards_before - 2 * BOND
        );
        assert_eq!(Balances::free_balance(&bonded_account), 3 * BOND);
        assert_eq!(other_balances(), others_before);

        pallet_staking::CurrentEra::<Runtime>::put(maturity - 1);
        let withdraw = withdraw(treasury.clone().into());
        assert_ok!(proxy(&treasury, &operator, OPERATOR, withdraw.clone()));
        assert_proxy_executed(Err(pools::Error::<Runtime>::CannotWithdrawAny.into()));
        assert_eq!(Balances::free_balance(&treasury), liquid_after_unbond);
        assert_eq!(PoolMembers::<Runtime>::get(&treasury), Some(member));

        pallet_staking::CurrentEra::<Runtime>::put(maturity);
        announce(&withdraw);
        System::set_block_number(System::block_number() + delay);
        assert_ok!(submit(OPERATOR, withdraw));
        assert_proxy_executed(Ok(()));
        assert!(PoolMembers::<Runtime>::get(&treasury).is_none());
        assert_eq!(Balances::free_balance(&bonded_account), BOND);
        assert_eq!(
            Balances::free_balance(&reward_account),
            rewards_before - 2 * BOND
        );
        assert_eq!(
            Balances::free_balance(&treasury),
            liquid_after_unbond + principal
        );
        assert_eq!(other_balances(), others_before);
        assert_announcement_released();
    });
}
