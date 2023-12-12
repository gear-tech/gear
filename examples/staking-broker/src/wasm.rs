// This file is part of Gear.
//
// Copyright (C) 2021-2023 Gear Technologies Inc.
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
//
// This is a very trivial implementation of a staking broker that would accept
// users messages and bond, unbond or nominate some validators on their behalf.
// It is not concerned with some important constraints like unbonding period etc.

use gbuiltin::{staking::*, Response};
use gstd::{debug, msg, prelude::*, ActorId};
use hashbrown::HashMap;
use hex_literal::hex;
use parity_scale_codec::Encode;

// Staking proxy built-in actor id (hardcoded for all runtimes)
const BUILT_IN: ActorId = ActorId::new(hex!(
    "9d765baea1938d17096421e4f881af7dc4ce5c15bb5022f409fc0d6265d97c3a"
));

// Runtime AccountId type
pub type AccountId = [u8; 32];

#[derive(Debug, Default)]
struct StakingBroker {
    /// Has bonded any amount yet
    has_bonded_any: bool,
    /// Total debit
    total_debit: u128,
    /// Registry of bonded deposits
    bonded: HashMap<ActorId, u128>,
    /// Reward payee account id
    reward_account: AccountId,
}

static mut STATE: Option<StakingBroker> = None;

/// Do the actual message sending and reply handling.
async fn do_send_message<E: Encode>(payload: E, mut on_success: impl FnMut()) {
    let response_bytes = match msg::send_for_reply(BUILT_IN, payload, 0, 0)
        .expect("Error sending message")
        .await
    {
        Ok(response_bytes) => {
            debug!("[StakingBroker] Reply received: {response_bytes:?}");
            response_bytes
        }
        Err(error_bytes) => {
            debug!("[StakingBroker] Error reply received: {error_bytes:?}");
            msg::reply_bytes(b"Error reply received", 0).expect("Failed to send reply");
            return;
        }
    };

    // Decoding the reply
    let Ok(response) = Response::decode(&mut &response_bytes[..]) else {
        debug!("[StakingBroker] Failed to decode response");
        msg::reply_bytes(b"Failed to decode reply message", 0).expect("Failed to send reply");
        return;
    };
    debug!("[StakingBroker] Decoded reply: {response:?}");

    match response {
        Response::Success => {
            on_success();
            msg::reply_bytes(b"Success", 0).expect("Failed to send reply");
        }
        Response::Failure(reason) => {
            let err_message = reason.to_string();
            debug!("[StakingBroker] DispatchError: {err_message:?}");
            msg::reply_bytes(b"Error in dispatchable call", 0).expect("Failed to send reply");
        }
    }
}

impl StakingBroker {
    /// Add bonded amount for the contract as both stash and controller.
    async fn bond(&mut self, value: u128, payee: Option<RewardAccount>) {
        // Prepare a message to the built-in actor
        // Checking the flag to decide whether to use `Bond` or `BondExtra`
        // Note: this is not how you'd do it in a real application, given the
        // Staking pallet `unbonding` logic, but it's enough for the example.
        let payload = if !self.has_bonded_any {
            Request::V1(RequestV1::Bond { value, payee })
        } else {
            Request::V1(RequestV1::BondExtra { value })
        };
        debug!(
            "[StakingBroker] Sending `bond` message {:?} at broker's state {:?}",
            payload, self
        );
        do_send_message(payload, || {
            // Update local state to account for value transfer in pallet
            self.bonded
                .entry(msg::source())
                .and_modify(|old| *old += value)
                .or_insert(value);
            self.total_debit += value;
            self.has_bonded_any = true;
            self.reward_account = match payee {
                Some(RewardAccount::Program) | None => msg::source().into(),
                Some(RewardAccount::Custom(account_id)) => account_id,
            };
        })
        .await
    }

    async fn unbond(&mut self, value: u128) {
        let source = msg::source();

        // The sender can unbond only so much as they have bonded
        let value = self.bonded.get(&source).map_or(0, |v| (*v).min(value));
        if value == 0 {
            debug!("[StakingBroker::unbond] No bonded amount");
            msg::reply_bytes(b"No bonded amount", 0).expect("Failed to send reply");
            return;
        }

        // Prepare a message to the built-in actor
        let payload = Request::V1(RequestV1::Unbond { value });
        debug!(
            "[StakingBroker] Sending `unbond` message {:?} at broker's state {:?}",
            payload, self
        );
        do_send_message(payload, || {
            // Update local state
            if let Some(old) = self.bonded.get_mut(&source) {
                *old = old.saturating_sub(value);
            }
            self.total_debit = self.total_debit.saturating_sub(value);
        })
        .await
    }

    async fn nominate(&mut self, targets: Vec<AccountId>) {
        // Prepare a message to the built-in actor
        let payload = Request::V1(RequestV1::Nominate { targets });
        debug!(
            "[StakingBroker] Sending `nominate` message {:?} at broker's state {:?}",
            payload, self
        );
        do_send_message(payload, || {}).await
    }

    async fn rebond(&mut self, value: u128) {
        let source = msg::source();

        // Prepare a message to the built-in actor
        let payload = Request::V1(RequestV1::Rebond { value });
        debug!(
            "[StakingBroker] Sending `rebond` message {:?} at broker's state {:?}",
            payload, self
        );
        do_send_message(payload, || {
            // Update local state
            if let Some(old) = self.bonded.get_mut(&source) {
                *old = old.saturating_add(value);
            }
            self.total_debit = self.total_debit.saturating_add(value);
        })
        .await
    }

    async fn witdraw_unbonded(&mut self) {
        let _sender = msg::source();

        // Prepare a message to the built-in actor
        let payload = Request::V1(RequestV1::WithdrawUnbonded {
            num_slashing_spans: 0,
        });
        debug!(
            "[StakingBroker] Sending `withdraw_unbonded` message {:?} at broker's state {:?}",
            payload, self
        );
        do_send_message(payload, || {
            // TODO: send a part of withdrawn amount to the sender and/or
            // some other users who requested unbonding earlier
        })
        .await
    }

    async fn set_payee(&mut self, payee: RewardAccount) {
        // Prepare a message to the built-in actor
        let payload = Request::V1(RequestV1::SetPayee { payee });
        debug!(
            "[StakingBroker] Sending `set_payee` message {:?} at broker's state {:?}",
            payload, self
        );
        do_send_message(payload, || {
            self.reward_account = match payee {
                RewardAccount::Program => msg::source().into(),
                RewardAccount::Custom(account_id) => account_id,
            }
        })
        .await
    }

    async fn payout_stakers(&mut self, validator_stash: AccountId, era: u32) {
        // Prepare a message to the built-in actor
        let payload = Request::V1(RequestV1::PayoutStakers {
            validator_stash,
            era,
        });
        debug!(
            "[StakingBroker] Sending `payout_stakers` message {:?} at broker's state {:?}",
            payload, self
        );
        do_send_message(payload, || {
            // TODO: transfer fraction of rewards to nominators of the `validator_stash`
        })
        .await
    }
}

#[gstd::async_main]
async fn main() {
    let broker = unsafe { STATE.get_or_insert(Default::default()) };

    let payload = msg::load().expect("Expecting a valid payload");
    match payload {
        // Handle the V1 staking requests
        Request::V1(request) => match request {
            RequestV1::Bond { value, payee } => {
                broker.bond(msg::value().min(value), payee).await;
            }
            RequestV1::BondExtra { value } => {
                broker.bond(msg::value().min(value), None).await;
            }
            RequestV1::Unbond { value } => {
                broker.unbond(value).await;
            }
            RequestV1::WithdrawUnbonded { .. } => {
                broker.witdraw_unbonded().await;
            }
            RequestV1::Nominate { targets } => {
                broker.nominate(targets).await;
            }
            RequestV1::PayoutStakers {
                validator_stash,
                era,
            } => {
                broker.payout_stakers(validator_stash, era).await;
            }
            RequestV1::Rebond { value } => {
                broker.rebond(value).await;
            }
            RequestV1::SetPayee { payee } => {
                broker.set_payee(payee).await;
            }
        },
        // More arms can be added in the future
    }
}

#[no_mangle]
extern "C" fn init() {
    let sb: StakingBroker = Default::default();
    unsafe { STATE = Some(sb) };
}
