// This file is part of Gear.

// Copyright (C) 2021-2025 Gear Technologies Inc.
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

//! Proxy builtin tests.

use super::basic::init_logger;
use crate::mock::*;
use common::Origin;
use demo_proxy_broker::WASM_BINARY;
use frame_support::{assert_err, assert_ok, dispatch::GetDispatchInfo};
use gbuiltin_proxy::{ProxyType, Request};
use gear_core::ids::{prelude::*, ActorId, CodeId};
use pallet_balances::Call as BalancesCall;
use pallet_proxy::{Error as ProxyError, Event as ProxyEvent};
use parity_scale_codec::Encode;
use sp_runtime::traits::StaticLookup;

#[test]
fn add_remove_proxy_works() {
    init_logger();
    new_test_ext().execute_with(|| {
        let proxy_pid = utils::upload_and_initialize_broker();

        // Add proxy
        let add_proxy_req = Request::AddProxy {
            delegate: SIGNER.cast(),
            proxy_type: ProxyType::Any,
        };
        utils::send_proxy_request(proxy_pid, add_proxy_req);

        System::assert_has_event(RuntimeEvent::Proxy(ProxyEvent::ProxyAdded {
            delegator: proxy_pid.cast(),
            delegatee: SIGNER,
            proxy_type: ProxyType::Any.into(),
            delay: 0,
        }));

        System::reset_events();

        // Remove proxy
        let remove_proxy_req = Request::RemoveProxy {
            delegate: SIGNER.cast(),
            proxy_type: ProxyType::Any,
        };
        utils::send_proxy_request(proxy_pid, remove_proxy_req);

        System::assert_has_event(RuntimeEvent::Proxy(ProxyEvent::ProxyRemoved {
            delegator: proxy_pid.cast(),
            delegatee: SIGNER,
            proxy_type: ProxyType::Any.into(),
            delay: 0,
        }));

        // Execute proxy
        let dest = 42;
        let value = EXISTENTIAL_DEPOSIT * 3;
        let call = RuntimeCall::Balances(BalancesCall::transfer_allow_death { dest, value });

        assert_err!(
            Proxy::proxy(
                RuntimeOrigin::signed(SIGNER),
                proxy_pid.cast(),
                None,
                Box::new(call)
            ),
            ProxyError::<Test>::NotProxy,
        );
    })
}

#[test]
fn add_execute_proxy_works() {
    init_logger();
    new_test_ext().execute_with(|| {
        let proxy_pid = utils::upload_and_initialize_broker();

        // Add proxy
        let add_proxy_req = Request::AddProxy {
            delegate: SIGNER.cast(),
            proxy_type: ProxyType::Any,
        };
        utils::send_proxy_request(proxy_pid, add_proxy_req);

        System::assert_has_event(RuntimeEvent::Proxy(ProxyEvent::ProxyAdded {
            delegator: proxy_pid.cast(),
            delegatee: SIGNER,
            proxy_type: ProxyType::Any.into(),
            delay: 0,
        }));

        // Execute proxy
        let dest = 42;
        let value = EXISTENTIAL_DEPOSIT * 3;
        let call = RuntimeCall::Balances(BalancesCall::transfer_allow_death { dest, value });

        assert_ok!(Proxy::proxy(
            RuntimeOrigin::signed(SIGNER),
            proxy_pid.cast(),
            None,
            Box::new(call)
        ));
        assert_eq!(Balances::free_balance(dest), value);
    })
}

#[test]
fn gas_allowance_respected() {
    init_logger();
    new_test_ext().execute_with(|| {
        let proxy_pid = utils::upload_and_initialize_broker();

        // Add proxy request
        let add_proxy_req = Request::AddProxy {
            delegate: SIGNER.cast(),
            proxy_type: ProxyType::Any,
        };
        // Everything works if the gas allowance is sufficient
        utils::send_proxy_request(proxy_pid, add_proxy_req);
        System::assert_has_event(RuntimeEvent::Proxy(ProxyEvent::ProxyAdded {
            delegator: proxy_pid.cast(),
            delegatee: SIGNER,
            proxy_type: ProxyType::Any.into(),
            delay: 0,
        }));

        let remove_proxy_req = Request::RemoveProxy {
            delegate: SIGNER.cast(),
            proxy_type: ProxyType::Any,
        };

        // Estimate the cost of the proxy call
        let gas_cost_proxy_message = pallet_proxy::Call::<Test>::remove_proxy {
            delegate: <Test as frame_system::Config>::Lookup::unlookup(SIGNER.cast()),
            proxy_type: ProxyType::Any.into(),
            delay: 0_u64,
        }
        .get_dispatch_info()
        .call_weight
        .ref_time();

        let gas_cost_send_message_to_broker = 680_000_000; // Heuristic value

        System::reset_events();

        // With insufficient gas allowance, the dispatch `is not processed
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(SIGNER),
            proxy_pid,
            remove_proxy_req.encode(),
            10_000_000_000,
            0,
            false,
        ));
        run_for_n_blocks(
            1,
            Some(gas_cost_send_message_to_broker + gas_cost_proxy_message),
        );

        // The dispatch is still in the queue
        assert!(!message_queue_empty());

        // Message is pushed through if the gas allowance is sufficient
        run_to_next_block();
        System::assert_has_event(RuntimeEvent::Proxy(ProxyEvent::ProxyRemoved {
            delegator: proxy_pid.cast(),
            delegatee: SIGNER,
            proxy_type: ProxyType::Any.into(),
            delay: 0,
        }));

        // Message queue is now empty
        assert!(message_queue_empty());
    })
}

mod utils {
    use super::*;

    pub(super) fn upload_and_initialize_broker() -> ActorId {
        let code = WASM_BINARY;
        let salt = b"proxy_broker";
        let pid = ActorId::generate_from_user(CodeId::generate(code), salt);
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(SIGNER),
            code.to_vec(),
            salt.to_vec(),
            Default::default(),
            10_000_000_000,
            0,
            false,
        ));
        run_to_next_block();

        // Top-up balance of the proxy so it can pay adding proxy deposit
        assert_ok!(<Balances as frame_support::traits::Currency<_>>::transfer(
            &1,
            &pid.cast(),
            10 * EXISTENTIAL_DEPOSIT,
            frame_support::traits::ExistenceRequirement::AllowDeath
        ));

        pid
    }

    pub(super) fn send_proxy_request(proxy_pid: ActorId, req: Request) {
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(SIGNER),
            proxy_pid,
            req.encode(),
            10_000_000_000,
            0,
            false,
        ));
        run_to_next_block();
    }
}
