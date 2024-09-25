// This file is part of Gear.

// Copyright (C) 2021-2024 Gear Technologies Inc.
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
use frame_support::assert_ok;
use gbuiltin_proxy::{ProxyType, Request};
use gear_core::ids::{prelude::*, CodeId, ProgramId};
use pallet_proxy::Event as ProxyEvent;
use parity_scale_codec::Encode;

#[test]
fn add_remove_proxy_works() {
    init_logger();
    new_test_ext().execute_with(|| {
        let proxy_pid = utils::upload_and_initialize_broker();

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(SIGNER),
            proxy_pid,
            Request::AddProxy {
                delegate: SIGNER.cast(),
                proxy_type: ProxyType::Any,
            }
            .encode(),
            10_000_000_000,
            0,
            false,
        ));
        run_to_next_block();

        System::assert_has_event(RuntimeEvent::Proxy(ProxyEvent::ProxyAdded {
            delegator: proxy_pid.cast(),
            delegatee: SIGNER,
            proxy_type: ProxyType::Any.into(),
            delay: 0
        }));

        System::reset_events();

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(SIGNER),
            proxy_pid,
            Request::RemoveProxy {
                delegate: SIGNER.cast(),
                proxy_type: ProxyType::Any,
            }
            .encode(),
            10_000_000_000,
            0,
            false,
        ));
        run_to_next_block();

        System::assert_has_event(RuntimeEvent::Proxy(ProxyEvent::ProxyRemoved {
            delegator: proxy_pid.cast(),
            delegatee: SIGNER,
            proxy_type: ProxyType::Any.into(),
            delay: 0,
        }));

        // todo add proxy call
    })
}

mod utils {
    use super::*;

    pub(super) fn upload_and_initialize_broker() -> ProgramId {
        let code = WASM_BINARY;
        let salt = b"proxy_broker";
        let pid = ProgramId::generate_from_user(CodeId::generate(code), salt);
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

        assert_ok!(<Balances as frame_support::traits::Currency<_>>::transfer(
            &1,
            &pid.cast(),
            2000,
            frame_support::traits::ExistenceRequirement::AllowDeath
        ));

        pid
    }
}
