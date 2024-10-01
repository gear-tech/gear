// This file is part of Gear.
//
// Copyright (C) 2024 Gear Technologies Inc.
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

use ethexe_common::router::{OutgoingMessage, StateTransition, ValueClaim};
use gprimitives::{ActorId, CodeId, H256};
use parity_scale_codec::{Decode, Encode};
use std::collections::{btree_map::Iter, BTreeMap};

#[derive(Default)]
pub struct InBlockTransitions {
    states: BTreeMap<ActorId, H256>,
    modifications: BTreeMap<ActorId, NonFinalTransition>,
}

impl InBlockTransitions {
    pub fn new(states: BTreeMap<ActorId, H256>) -> Self {
        Self {
            states,
            ..Default::default()
        }
    }

    pub fn state_of(&self, actor_id: &ActorId) -> Option<H256> {
        self.states.get(actor_id).cloned()
    }

    pub fn states_amount(&self) -> usize {
        self.states.len()
    }

    pub fn states_iter(&self) -> Iter<ActorId, H256> {
        self.states.iter()
    }

    pub fn current_messages(&self) -> Vec<(ActorId, OutgoingMessage)> {
        self.modifications
            .iter()
            .flat_map(|(id, trans)| trans.messages.iter().map(|message| (*id, message.clone())))
            .collect()
    }

    pub fn register_new(&mut self, actor_id: ActorId) {
        self.states.insert(actor_id, H256::zero());
        self.modifications.insert(actor_id, Default::default());
    }

    pub fn modify_state(&mut self, actor_id: ActorId, new_state_hash: H256) -> Option<()> {
        self.modify_state_with(
            actor_id,
            new_state_hash,
            0,
            Default::default(),
            Default::default(),
        )
    }

    pub fn modify_state_with(
        &mut self,
        actor_id: ActorId,
        new_state_hash: H256,
        extra_value_to_receive: u128,
        extra_claims: Vec<ValueClaim>,
        extra_messages: Vec<OutgoingMessage>,
    ) -> Option<()> {
        let initial_state = self.states.insert(actor_id, new_state_hash)?;

        let transition = self
            .modifications
            .entry(actor_id)
            .or_insert(NonFinalTransition {
                initial_state,
                ..Default::default()
            });

        transition.value_to_receive += extra_value_to_receive;
        transition.claims.extend(extra_claims);
        transition.messages.extend(extra_messages);

        Some(())
    }

    pub fn finalize(self) -> (Vec<StateTransition>, BTreeMap<ActorId, H256>) {
        let Self {
            states,
            modifications,
        } = self;

        let mut res = Vec::with_capacity(modifications.len());

        for (actor_id, modification) in modifications {
            let new_state_hash = states
                .get(&actor_id)
                .cloned()
                .expect("failed to find state record for modified state");

            if !modification.is_noop(new_state_hash) {
                res.push(StateTransition {
                    actor_id,
                    new_state_hash,
                    value_to_receive: modification.value_to_receive,
                    value_claims: modification.claims,
                    messages: modification.messages,
                });
            }
        }

        (res, states)
    }
}

#[derive(Default)]
pub struct NonFinalTransition {
    initial_state: H256,
    pub value_to_receive: u128,
    pub claims: Vec<ValueClaim>,
    pub messages: Vec<OutgoingMessage>,
}

impl NonFinalTransition {
    pub fn is_noop(&self, current_state: H256) -> bool {
        // check if just created program (always op)
        !self.initial_state.is_zero()
            // check if state hash changed at final (always op)
            && current_state == self.initial_state
            // check if with unchanged state needs commitment (op)
            && (self.value_to_receive == 0 && self.claims.is_empty() && self.messages.is_empty())
    }
}
