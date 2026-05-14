// This file is part of Gear.
//
// Copyright (C) 2024-2025 Gear Technologies Inc.
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

//! This is a very basic implementation of a staking broker program for demo purpose only.
//! It accepts users messages and bond, unbond or nominate some validators on their behalf.
//! It is not concerned with some important constraints like unbonding period etc., so a real
//! implementation of a liquid staking contract, for example, would be more complex.

use gbuiltin_staking::*;
use gstd::{ActorId, debug, errors::Error, msg, prelude::*};
use hashbrown::HashMap;
use hex_literal::hex;
use parity_scale_codec::Encode;

// Staking proxy builtin actor program id (hardcoded for all runtimes)
const BUILTIN_ADDRESS: ActorId = ActorId::new(hex!(
    "77f65ef190e11bfecb8fc8970fd3749e94bed66a23ec2f7a3623e785d0816761"
));

#[derive(Debug, Default)]
struct StakingBroker {
    /// Has bonded any amount yet
    has_bonded_any: bool,
    /// Total debit
    total_debit: u128,
    /// Registry of bonded deposits
    bonded: HashMap<ActorId, u128>,
    /// Reward payee account id
    reward_account: ActorId,
}

static mut STATE: Option<StakingBroker> = None;

/// Do the actual message sending and reply handling.
async fn do_send_message<E: Encode>(payload: E, mut on_success: impl FnMut()) {
    match msg::send_for_reply(BUILTIN_ADDRESS, payload, 0, 0)
        .expect("Error sending message")
        .await
    {
        Ok(_) => {
            debug!("[StakingBroker] Success reply from builtin actor received");
            on_success();
            msg::reply_bytes(b"Success", 0).expect("Failed to send reply");
        }
        Err(e) => {
            debug!("[StakingBroker] Error reply from builtin actor received: {e:?}");
            match e {
                Error::ErrorReply(payload, _reason) => {
                    panic!("{}", payload);
                }
                _ => panic!("Error in upstream program"),
            }
        }
    };
}

impl StakingBroker {
    /// Add bonded amount for the contract as both stash and controller.
    async fn bond(&mut self, value: u128, payee: Option<RewardAccount>) {
        // Prepare a message to the built-in actor
        // Checking the flag to decide whether to use `Bond` or `BondExtra`
        // Note: this is not how you'd do it in a real application, given the
        // Staking pallet `unbonding` logic, but it's enough for the example.
        let payload = if !self.has_bonded_any {
            Request::Bond {
                value,
                payee: payee.unwrap_or(RewardAccount::Program),
            }
        } else {
            Request::BondExtra { value }
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
                Some(RewardAccount::Custom(account_id)) => account_id,
                _ => msg::source(),
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
        let payload = Request::Unbond { value };
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

    async fn nominate(&mut self, targets: Vec<ActorId>) {
        // Prepare a message to the built-in actor
        let payload = Request::Nominate { targets };
        debug!(
            "[StakingBroker] Sending `nominate` message {:?} at broker's state {:?}",
            payload, self
        );
        do_send_message(payload, || {}).await
    }

    async fn chill(&mut self) {
        // Prepare a message to the built-in actor
        let payload = Request::Chill {};
        debug!(
            "[StakingBroker] Sending `chill` message {:?} at broker's state {:?}",
            payload, self
        );
        do_send_message(payload, || {}).await
    }

    async fn rebond(&mut self, value: u128) {
        let source = msg::source();

        // Prepare a message to the built-in actor
        let payload = Request::Rebond { value };
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

    async fn withdraw_unbonded(&mut self) {
        let _sender = msg::source();

        // Prepare a message to the built-in actor
        let payload = Request::WithdrawUnbonded {
            num_slashing_spans: 0,
        };
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
        let payload = Request::SetPayee { payee };
        debug!(
            "[StakingBroker] Sending `set_payee` message {:?} at broker's state {:?}",
            payload, self
        );
        do_send_message(payload, || {
            self.reward_account = match payee {
                RewardAccount::Custom(account_id) => account_id,
                _ => msg::source(),
            }
        })
        .await
    }

    async fn payout_stakers(&mut self, validator_stash: ActorId, era: u32) {
        // Prepare a message to the built-in actor
        let payload = Request::PayoutStakers {
            validator_stash,
            era,
        };
        debug!(
            "[StakingBroker] Sending `payout_stakers` message {:?} at broker's state {:?}",
            payload, self
        );
        do_send_message(payload, || {
            // TODO: transfer fraction of rewards to nominators of the `validator_stash`
        })
        .await
    }

    async fn active_era(&mut self) {
        debug!(
            "[StakingBroker] Sending `active_era` message at broker's state {:?}",
            self
        );

        match msg::send_for_reply(BUILTIN_ADDRESS, Request::ActiveEra, 0, 0)
            .expect("Error sending message")
            .await
        {
            Ok(reply) => {
                debug!("[StakingBroker] ActiveEra reply from builtin actor received");
                // Forward the ActiveEra response back to the user
                msg::reply_bytes(reply, 0).expect("Failed to send reply");
            }
            Err(e) => {
                debug!("[StakingBroker] Error reply from builtin actor received: {e:?}");
                match e {
                    Error::ErrorReply(payload, _reason) => {
                        panic!("{}", payload);
                    }
                    _ => panic!("Error in upstream program"),
                }
            }
        }
    }
}

#[gstd::async_main]
async fn main() {
    let broker = unsafe { static_mut!(STATE).get_or_insert(Default::default()) };

    let request: Request = msg::load().expect("Expecting a valid payload");
    match request {
        Request::Bond { value, payee } => {
            broker.bond(msg::value().min(value), Some(payee)).await;
        }
        Request::BondExtra { value } => {
            broker.bond(msg::value().min(value), None).await;
        }
        Request::Unbond { value } => {
            broker.unbond(value).await;
        }
        Request::WithdrawUnbonded { .. } => {
            broker.withdraw_unbonded().await;
        }
        Request::Nominate { targets } => {
            broker.nominate(targets).await;
        }
        Request::Chill => {
            broker.chill().await;
        }
        Request::PayoutStakers {
            validator_stash,
            era,
        } => {
            broker.payout_stakers(validator_stash, era).await;
        }
        Request::Rebond { value } => {
            broker.rebond(value).await;
        }
        Request::SetPayee { payee } => {
            broker.set_payee(payee).await;
        }
        Request::ActiveEra => {
            broker.active_era().await;
        }
    }
}

#[unsafe(no_mangle)]
extern "C" fn init() {
    let sb: StakingBroker = Default::default();
    unsafe { STATE = Some(sb) };
}
