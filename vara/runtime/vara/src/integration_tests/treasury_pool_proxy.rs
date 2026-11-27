// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use super::*;
use frame_support::dispatch::DispatchResult;
use pallet_nomination_pools::{self as pools, BondExtra, PoolMembers};
use sp_runtime::traits::Hash as _;

const OPERATOR: ProxyType = ProxyType::TreasuryPoolOperator;
const WITHDRAW: ProxyType = ProxyType::TreasuryPoolWithdraw;
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
        TreasuryPoolWithdraw,
    ];
    // Each bit identifies a type in the corresponding row's superset relation.
    let supersets = [
        0b1111_1111u8,
        0b1111_1110,
        0b0000_0100,
        0b0000_1000,
        0b0001_0000,
        0b0010_0000,
        0b1100_0000,
        0b1000_0000,
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

    let old_indices = 0..7u8;
    let delay = 0x1234_5678u32;
    let deposit = 0x0123_4567_89ab_cdefu128;
    // A SCALE Proxies entry: compact length, old definitions, and balance deposit.
    let mut old_bytes = vec![7 << 2];
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
        assert_eq!(defs.len(), 7);
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
    assert_eq!(WITHDRAW.encode(), [7]);
    assert_eq!(ProxyType::decode(&mut &[7][..]).unwrap(), WITHDRAW);

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
        for runtime_only in [6, 7] {
            wire[33] = runtime_only;
            assert!(Request::decode(&mut &wire[..]).is_err());
        }
    }
}

fn with_accounts(test: impl FnOnce(AccountId, AccountId, AccountId, AccountId)) {
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
            test(treasury, depositor, operator, delayed_operator);
        });
}

fn with_pool(test: impl FnOnce(AccountId, AccountId, AccountId, AccountId)) {
    with_accounts(|treasury, depositor, operator, delayed_operator| {
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
fn treasury_pool_proxies_filter_calls_and_force_select_exactly() {
    with_pool(|treasury, depositor, operator, delayed_operator| {
        register(&treasury, &operator, ProxyType::Any, 0);
        register(&treasury, &operator, OPERATOR, 0);
        register(&treasury, &operator, WITHDRAW, 0);
        let delay = 2;
        for role in [ProxyType::Any, OPERATOR, WITHDRAW] {
            register(&treasury, &delayed_operator, role, delay);
        }
        let bonded_account = NominationPools::generate_bonded_account(POOL);
        let reward_account = NominationPools::generate_reward_account(POOL);

        let state = || {
            (
                Balances::free_balance(&treasury),
                Balances::free_balance(&operator),
                Balances::free_balance(&delayed_operator),
                Balances::free_balance(&bonded_account),
                Balances::free_balance(&reward_account),
                PoolMembers::<Runtime>::get(&treasury),
                PoolMembers::<Runtime>::get(&depositor),
                pools::Metadata::<Runtime>::get(POOL),
                Proxy::proxies(treasury.clone()),
            )
        };
        let before = state();
        let held = Balances::reserved_balance(&delayed_operator);
        let assert_denied = |role, call: RuntimeCall| {
            let filtered = Err(frame_system::Error::<Runtime>::CallFiltered.into());
            assert_ok!(proxy(&treasury, &operator, role, call.clone()));
            assert_proxy_executed(filtered);
            assert_eq!(state(), before);

            assert_ok!(Proxy::announce(
                RuntimeOrigin::signed(delayed_operator.clone()),
                treasury.clone().into(),
                BlakeTwo256::hash_of(&call),
            ));
            System::set_block_number(System::block_number() + delay);
            let submit = |proxy_type, call| {
                Proxy::proxy_announced(
                    RuntimeOrigin::signed(operator.clone()),
                    delayed_operator.clone().into(),
                    treasury.clone().into(),
                    Some(proxy_type),
                    Box::new(call),
                )
            };
            assert_noop!(
                submit(ProxyType::NonTransfer, call.clone()),
                pallet_proxy::Error::<Runtime>::NotProxy,
            );
            assert_ok!(submit(role, call));
            assert_proxy_executed(filtered);
            let (pending, deposit) = Proxy::announcements(delayed_operator.clone());
            assert!(pending.is_empty());
            assert_eq!(deposit, 0);
            assert_eq!(Balances::reserved_balance(&delayed_operator), held);
            assert_eq!(state(), before);
        };
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
            RuntimeCall::NominationPools(pools::Call::claim_payout {}),
            RuntimeCall::NominationPools(pools::Call::nominate {
                pool_id: POOL,
                validators: vec![],
            }),
            RuntimeCall::NominationPools(pools::Call::update_roles {
                pool_id: POOL,
                new_root: pools::ConfigOp::Set(operator.clone()),
                new_nominator: pools::ConfigOp::Noop,
                new_bouncer: pools::ConfigOp::Noop,
            }),
            RuntimeCall::NominationPools(pools::Call::set_commission {
                pool_id: POOL,
                new_commission: None,
            }),
            RuntimeCall::NominationPools(pools::Call::set_state {
                pool_id: POOL,
                state: pools::PoolState::Destroying,
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
            for role in [WITHDRAW, OPERATOR] {
                assert_denied(role, call.clone());
            }
        }
        for call in [
            bond_extra(),
            RuntimeCall::NominationPools(pools::Call::bond_extra {
                extra: BondExtra::Rewards,
            }),
            unbond(treasury.clone().into(), BOND),
        ] {
            assert_denied(WITHDRAW, call);
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

#[test]
fn treasury_pool_withdraw_releases_only_mature_depositor_funds_and_can_be_revoked() {
    for delay in [0, 2] {
        with_accounts(|treasury, submitter, operator, bot| {
            let minimum = NominationPools::depositor_min_bond();
            assert_ok!(NominationPools::create(
                RuntimeOrigin::signed(treasury.clone()),
                minimum + 2 * BOND,
                treasury.clone().into(),
                operator.clone().into(),
                treasury.clone().into(),
            ));
            assert_ok!(NominationPools::set_state(
                RuntimeOrigin::signed(treasury.clone()),
                POOL,
                pools::PoolState::Blocked,
            ));
            register(&treasury, &operator, OPERATOR, 0);
            register(&treasury, &bot, WITHDRAW, delay);

            let announce = |call: &RuntimeCall| {
                Proxy::announce(
                    RuntimeOrigin::signed(bot.clone()),
                    treasury.clone().into(),
                    BlakeTwo256::hash_of(call),
                )
            };
            let submit = |call| {
                Proxy::proxy_announced(
                    RuntimeOrigin::signed(submitter.clone()),
                    bot.clone().into(),
                    treasury.clone().into(),
                    Some(WITHDRAW),
                    Box::new(call),
                )
            };
            let bot_dispatch = |call: RuntimeCall| {
                if delay == 0 {
                    return proxy(&treasury, &bot, WITHDRAW, call);
                }
                assert_ok!(announce(&call));
                System::set_block_number(System::block_number() + delay);
                let result = submit(call);
                let (pending, deposit) = Proxy::announcements(bot.clone());
                assert!(pending.is_empty());
                assert_eq!(deposit, 0);
                result
            };
            let take_chunk = || unbond(treasury.clone().into(), BOND);
            let release = || withdraw(treasury.clone().into());
            let pool_account = NominationPools::generate_bonded_account(POOL);
            let balances = || {
                (
                    Balances::free_balance(&treasury),
                    Balances::free_balance(&pool_account),
                    Balances::free_balance(&bot),
                    Balances::free_balance(&submitter),
                )
            };
            let (liquid, bonded, bot_balance, submitter_balance) = balances();
            assert_ok!(bot_dispatch(take_chunk()));
            assert_proxy_executed(Err(frame_system::Error::<Runtime>::CallFiltered.into()));
            assert_ok!(proxy(&treasury, &operator, OPERATOR, take_chunk()));
            assert_proxy_executed(Ok(()));
            pallet_staking::CurrentEra::<Runtime>::put(1);
            assert_ok!(proxy(&treasury, &operator, OPERATOR, take_chunk()));
            assert_proxy_executed(Ok(()));
            assert_ok!(proxy(
                &treasury,
                &operator,
                OPERATOR,
                unbond(treasury.clone().into(), minimum)
            ));
            assert_proxy_executed(Err(pools::Error::<Runtime>::MinimumBondNotMet.into()));

            let maturity = BondingDuration::get();
            pallet_staking::CurrentEra::<Runtime>::put(maturity - 1);
            assert_ok!(bot_dispatch(release()));
            assert_proxy_executed(Err(pools::Error::<Runtime>::CannotWithdrawAny.into()));
            assert_eq!(balances(), (liquid, bonded, bot_balance, submitter_balance));
            pallet_staking::CurrentEra::<Runtime>::put(maturity);
            assert_noop!(
                NominationPools::withdraw_unbonded(
                    RuntimeOrigin::signed(bot.clone()),
                    treasury.clone().into(),
                    0
                ),
                pools::Error::<Runtime>::NotKickerOrDestroying
            );
            assert_ok!(bot_dispatch(release()));
            assert_proxy_executed(Ok(()));
            System::assert_has_event(RuntimeEvent::NominationPools(pools::Event::Withdrawn {
                member: treasury.clone(),
                pool_id: POOL,
                points: BOND,
                balance: BOND,
            }));
            assert_ok!(bot_dispatch(release()));
            assert_proxy_executed(Err(pools::Error::<Runtime>::CannotWithdrawAny.into()));
            assert_eq!(
                balances(),
                (liquid + BOND, bonded - BOND, bot_balance, submitter_balance)
            );

            if delay > 0 {
                assert_ok!(announce(&release()));
            }
            assert_ok!(Proxy::remove_proxy(
                RuntimeOrigin::signed(treasury.clone()),
                bot.clone().into(),
                WITHDRAW,
                delay
            ));
            pallet_staking::CurrentEra::<Runtime>::put(maturity + 1);
            // Removing the grant also releases its deposit into Treasury's free balance.
            let after_revocation = balances();
            if delay > 0 {
                System::set_block_number(System::block_number() + delay);
                assert_noop!(submit(release()), pallet_proxy::Error::<Runtime>::NotProxy);
            } else {
                assert_noop!(
                    proxy(&treasury, &bot, WITHDRAW, release()),
                    pallet_proxy::Error::<Runtime>::NotProxy
                );
            }
            assert_eq!(balances(), after_revocation);
            let (liquid_after_revocation, _, bot_after_announcement, _) = after_revocation;
            assert_ok!(proxy(&treasury, &operator, OPERATOR, release()));
            assert_proxy_executed(Ok(()));
            assert_eq!(
                balances(),
                (
                    liquid_after_revocation + BOND,
                    bonded - 2 * BOND,
                    bot_after_announcement,
                    submitter_balance
                )
            );
            let member = PoolMembers::<Runtime>::get(&treasury).unwrap();
            assert_eq!(member.points, minimum);
            assert!(member.unbonding_eras.is_empty());
        });
    }
}

#[test]
fn treasury_pool_withdraw_redeems_slashed_points_to_treasury() {
    with_pool(|treasury, submitter, operator, bot| {
        register(&treasury, &operator, OPERATOR, 0);
        register(&treasury, &bot, WITHDRAW, 0);
        assert_ok!(proxy(
            &treasury,
            &operator,
            OPERATOR,
            unbond(treasury.clone().into(), BOND),
        ));
        assert_proxy_executed(Ok(()));

        let bonded = NominationPools::generate_bonded_account(POOL);
        let mut imbalance = Default::default();
        pallet_staking::slashing::do_slash::<Runtime>(&bonded, BOND, &mut 0, &mut imbalance, 0);
        drop(imbalance);
        let maturity = BondingDuration::get();
        System::assert_has_event(RuntimeEvent::NominationPools(
            pools::Event::UnbondingPoolSlashed {
                era: maturity,
                pool_id: POOL,
                balance: BOND / 2,
            },
        ));
        let member = PoolMembers::<Runtime>::get(&treasury).unwrap();
        assert_eq!(member.unbonding_eras.get(&maturity), Some(&BOND));
        let balances = || {
            (
                Balances::free_balance(&treasury),
                Balances::free_balance(&bonded),
                Balances::free_balance(&bot),
                Balances::free_balance(&submitter),
            )
        };
        let before = balances();
        let (liquid, bonded_balance, bot_balance, submitter_balance) = before;
        pallet_staking::CurrentEra::<Runtime>::put(maturity);
        assert_ok!(NominationPools::pool_withdraw_unbonded(
            RuntimeOrigin::signed(submitter.clone()),
            POOL,
            0,
        ));
        assert_eq!(PoolMembers::<Runtime>::get(&treasury), Some(member));
        assert_eq!(balances(), before);

        assert_ok!(proxy(
            &treasury,
            &bot,
            WITHDRAW,
            withdraw(treasury.clone().into()),
        ));
        assert_proxy_executed(Ok(()));
        assert_eq!(
            balances(),
            (
                liquid + BOND / 2,
                bonded_balance - BOND / 2,
                bot_balance,
                submitter_balance,
            ),
        );
        assert!(!PoolMembers::<Runtime>::contains_key(&treasury));
    });
}

#[test]
fn treasury_pool_operator_can_compound_and_direct_claims_follow_member_permissions() {
    with_pool(|treasury, depositor, operator, bot| {
        register(&treasury, &operator, OPERATOR, 0);
        let treasury_origin = RuntimeOrigin::signed(treasury.clone());
        let bot_origin = RuntimeOrigin::signed(bot.clone());
        let reward_account = NominationPools::generate_reward_account(POOL);
        assert_ok!(Balances::transfer_keep_alive(
            RuntimeOrigin::signed(depositor),
            reward_account.clone().into(),
            4 * BOND,
        ));
        assert_ok!(NominationPools::set_claim_permission(
            treasury_origin.clone(),
            pools::ClaimPermission::Permissioned,
        ));
        assert_noop!(
            NominationPools::claim_payout_other(bot_origin.clone(), treasury.clone()),
            pools::Error::<Runtime>::DoesNotHavePermission,
        );

        let treasury_balance = Balances::free_balance(&treasury);
        let bot_balance = Balances::free_balance(&bot);
        let operator_balance = Balances::free_balance(&operator);
        assert_ok!(proxy(
            &treasury,
            &operator,
            OPERATOR,
            RuntimeCall::NominationPools(pools::Call::bond_extra {
                extra: BondExtra::Rewards,
            }),
        ));
        assert_proxy_executed(Ok(()));
        assert_eq!(
            PoolMembers::<Runtime>::get(&treasury).unwrap().points,
            3 * BOND
        );
        assert_eq!(Balances::free_balance(&treasury), treasury_balance);
        assert_eq!(Balances::free_balance(&operator), operator_balance);

        // Treasury now owns three quarters of the pool points.
        assert_ok!(Balances::transfer_keep_alive(
            RuntimeOrigin::signed(operator.clone()),
            reward_account.into(),
            4 * BOND,
        ));
        assert_ok!(NominationPools::set_claim_permission(
            treasury_origin,
            pools::ClaimPermission::PermissionlessWithdraw,
        ));
        assert_ok!(NominationPools::claim_payout_other(
            bot_origin,
            treasury.clone()
        ));
        assert_eq!(
            Balances::free_balance(&treasury),
            treasury_balance + 3 * BOND
        );
        assert_eq!(Balances::free_balance(&bot), bot_balance);
        assert_eq!(
            Balances::free_balance(&operator),
            operator_balance - 4 * BOND
        );
        assert_eq!(NominationPools::api_pending_rewards(treasury), Some(0));
    });
}
