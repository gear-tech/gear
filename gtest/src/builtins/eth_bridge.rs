// This file is part of Gear.
//
// Copyright (C) 2025 Gear Technologies Inc.
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

//! Eth-bridge builtin actor implementation.
//!
//! The main function the module is `process_eth_bridge_dispatch` which
//! processes incoming dispatches to the eth-bridge builtin actor.

pub use builtins_common::eth_bridge::{Request as EthBridgeRequest, Response as EthBridgeResponse};

use crate::state::bridge::BridgeBuiltinStorage;
use builtins_common::{BuiltinActorError, eth_bridge};
use gear_common::Origin;
use gear_core::{ids::ActorId, message::StoredDispatch};
use gprimitives::{H160, H256, U256};
use parity_scale_codec::Decode;

/// The id of the ETH bridge builtin actor.
pub const ETH_BRIDGE_ID: ActorId = ActorId::new(*b"modl/bia/eth-bridge/v-\x01\0/\0\0\0\0\0\0\0");

/// Processes a dispatch message sent to the Eth-bridge builtin actor.
pub(crate) fn process_eth_bridge_dispatch(
    dispatch: &StoredDispatch,
) -> Result<EthBridgeResponse, BuiltinActorError> {
    let source = dispatch.source();
    let mut payload = dispatch.payload_bytes();
    let request =
        EthBridgeRequest::decode(&mut payload).map_err(|_| BuiltinActorError::DecodingError)?;

    match request {
        EthBridgeRequest::SendEthMessage {
            destination,
            payload,
        } => {
            let (nonce, hash) = create_bridge_call_output(source, destination, payload);

            Ok(EthBridgeResponse::EthMessageQueued { nonce, hash })
        }
    }
}

fn create_bridge_call_output(source: ActorId, destination: H160, payload: Vec<u8>) -> (U256, H256) {
    let nonce = BridgeBuiltinStorage::fetch_nonce();
    let hash = eth_bridge::bridge_call_hash(
        nonce,
        source.cast(),
        destination,
        &payload,
        eth_bridge::keccak256_hash,
    );

    (nonce, hash)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DEFAULT_USER_ALICE, Log, Program, System};
    use demo_constructor::{Arg, Call, Calls, Scheme, WASM_BINARY};
    use parity_scale_codec::Encode;

    #[test]
    fn test_eth_bridge_builtin() {
        let sys = System::new();

        let alice_actor_id = ActorId::from(DEFAULT_USER_ALICE);
        let proxy_program_id = ActorId::new([3; 32]);

        // Create destination address and payload for the bridge message
        let destination = H160::from_slice(&[1u8; 20]);
        let bridge_payload = b"test bridge message".to_vec();

        // Calculate expected hash and nonce using the same function as the builtin
        let expected_nonce = U256::zero();
        let expected_hash = eth_bridge::bridge_call_hash(
            expected_nonce,
            proxy_program_id.cast(),
            destination,
            &bridge_payload,
            eth_bridge::keccak256_hash,
        );

        // Create the bridge request
        let bridge_request = EthBridgeRequest::SendEthMessage {
            destination,
            payload: bridge_payload,
        };

        let proxy_scheme = Scheme::predefined(
            // init: do nothing
            Calls::builder().noop(),
            // handle: send message to eth bridge builtin
            Calls::builder().add_call(Call::Send(
                Arg::new(ETH_BRIDGE_ID.into_bytes()),
                Arg::new(bridge_request.encode()),
                None,
                Arg::new(0u128),
                Arg::new(0u32),
            )),
            // handle_reply: load reply payload and forward it to original sender
            Calls::builder()
                .add_call(Call::LoadBytes)
                .add_call(Call::StoreVec("reply_payload".to_string()))
                .add_call(Call::Send(
                    Arg::new(alice_actor_id.into_bytes()),
                    Arg::get("reply_payload"),
                    Some(Arg::new(0)),
                    Arg::new(0u128),
                    Arg::new(0u32),
                )),
            // handle_signal: noop
            Calls::builder(),
        );

        let proxy_program = Program::from_binary_with_id(&sys, proxy_program_id, WASM_BINARY);

        // Initialize proxy with the scheme
        let init_mid = proxy_program.send(alice_actor_id, proxy_scheme);
        let res = sys.run_next_block();
        assert!(res.succeed.contains(&init_mid));

        // Send a message to the proxy to trigger the bridge interaction
        let mid = proxy_program.send_bytes(alice_actor_id, b"");
        let res = sys.run_next_block();
        assert!(res.succeed.contains(&mid));

        // Verify that Alice received a response from the proxy
        assert!(
            res.contains(
                &Log::builder()
                    .source(proxy_program.id())
                    .dest(alice_actor_id)
            )
        );

        let mut logs = res.decoded_log();
        let response = logs.pop().expect("no log found");

        let EthBridgeResponse::EthMessageQueued { nonce, hash } = response.payload();

        assert_eq!(nonce, &expected_nonce);
        assert_eq!(hash, &expected_hash);
    }
}
