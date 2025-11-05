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

use alloc::{
    collections::{BTreeMap, BTreeSet, btree_map::Iter},
    vec::Vec,
};
use anyhow::{Result, anyhow};
use core::num::NonZero;
use ethexe_common::{
    BlockHeader, ProgramStates, Schedule, ScheduledTask, StateHashWithQueueSize,
    gear::{Message, StateTransition, ValueClaim},
};
use gprimitives::{ActorId, H256};

/// In-memory store for the state transitions
/// that are going to be applied in the current block.
///
/// The type is instantiated with states taken from the parent
/// block, as parent block stores latest states to be possibly
/// updated in the current block.
///
/// The type actually stores latest state transitions, which are going to be
/// applied in the current block.
#[derive(Debug, Default)]
pub struct InBlockTransitions {
    header: BlockHeader,
    states: ProgramStates,
    schedule: Schedule,
    modifications: BTreeMap<ActorId, NonFinalTransition>,
}

impl InBlockTransitions {
    pub fn new(header: BlockHeader, states: ProgramStates, schedule: Schedule) -> Self {
        Self {
            header,
            states,
            schedule,
            ..Default::default()
        }
    }

    pub fn header(&self) -> &BlockHeader {
        &self.header
    }

    pub fn is_program(&self, actor_id: &ActorId) -> bool {
        self.states.contains_key(actor_id)
    }

    pub fn state_of(&self, actor_id: &ActorId) -> Option<StateHashWithQueueSize> {
        self.states.get(actor_id).copied()
    }

    pub fn states_amount(&self) -> usize {
        self.states.len()
    }

    pub fn states_iter(&self) -> Iter<'_, ActorId, StateHashWithQueueSize> {
        self.states.iter()
    }

    pub fn known_programs(&self) -> Vec<ActorId> {
        self.states.keys().copied().collect()
    }

    pub fn current_messages(&self) -> Vec<(ActorId, Message)> {
        self.modifications
            .iter()
            .flat_map(|(id, trans)| trans.messages.iter().map(|message| (*id, message.clone())))
            .collect()
    }

    pub fn take_actual_tasks(&mut self) -> BTreeSet<ScheduledTask> {
        self.schedule
            .remove(&self.header.height)
            .unwrap_or_default()
    }

    pub fn schedule_task(&mut self, in_blocks: NonZero<u32>, task: ScheduledTask) -> u32 {
        let scheduled_block = self.header.height + u32::from(in_blocks);

        self.schedule
            .entry(scheduled_block)
            .or_default()
            .insert(task);

        scheduled_block
    }

    pub fn remove_task(&mut self, expiry: u32, task: &ScheduledTask) -> Result<()> {
        let block_tasks = self
            .schedule
            .get_mut(&expiry)
            .ok_or_else(|| anyhow!("No tasks found scheduled for a given block"))?;

        block_tasks
            .remove(task)
            .then_some(())
            .ok_or_else(|| anyhow!("Requested task wasn't found scheduled for a given block"))?;

        if block_tasks.is_empty() {
            self.schedule.remove(&expiry);
        }

        Ok(())
    }

    pub fn register_new(&mut self, actor_id: ActorId) {
        self.states.insert(actor_id, StateHashWithQueueSize::zero());
        self.modifications.insert(actor_id, Default::default());
    }

    pub fn modify_state(
        &mut self,
        actor_id: ActorId,
        new_state_hash: H256,
        canonical_queue_size: u8,
        injected_queue_size: u8,
    ) {
        self.modify(actor_id, |state, _transition| {
            state.hash = new_state_hash;
            state.canonical_queue_size = canonical_queue_size;
            state.injected_queue_size = injected_queue_size;
        })
    }

    pub fn modify_transition<T>(
        &mut self,
        actor_id: ActorId,
        f: impl FnOnce(&mut NonFinalTransition) -> T,
    ) -> T {
        self.modify(actor_id, |_state, transition| f(transition))
    }

    pub fn modify<T>(
        &mut self,
        actor_id: ActorId,
        f: impl FnOnce(&mut StateHashWithQueueSize, &mut NonFinalTransition) -> T,
    ) -> T {
        let initial_state = self
            .states
            .get_mut(&actor_id)
            .expect("couldn't modify transition for unknown actor");

        let transition = self
            .modifications
            .entry(actor_id)
            .or_insert(NonFinalTransition {
                initial_state: initial_state.hash,
                ..Default::default()
            });

        f(initial_state, transition)
    }

    pub fn finalize(self) -> (Vec<StateTransition>, ProgramStates, Schedule) {
        let Self {
            states,
            schedule,
            modifications,
            ..
        } = self;

        let mut res = Vec::with_capacity(modifications.len());

        for (actor_id, modification) in modifications {
            let new_state = states
                .get(&actor_id)
                .cloned()
                .expect("failed to find state record for modified state");

            let (value_to_receive, value_to_receive_negative_sign) =
                if modification.value_to_receive >= 0 {
                    (modification.value_to_receive as u128, false)
                } else {
                    (modification.value_to_receive.unsigned_abs(), true)
                };

            if !modification.is_noop(new_state.hash) {
                res.push(StateTransition {
                    actor_id,
                    new_state_hash: new_state.hash,
                    exited: modification.inheritor.is_some(),
                    inheritor: modification.inheritor.unwrap_or_default(),
                    value_to_receive,
                    value_to_receive_negative_sign,
                    value_claims: modification.claims,
                    messages: modification.messages,
                });
            }
        }

        (res, states, schedule)
    }
}

#[derive(Debug, Default)]
pub struct NonFinalTransition {
    initial_state: H256,
    pub inheritor: Option<ActorId>,
    pub value_to_receive: i128,
    pub claims: Vec<ValueClaim>,
    pub messages: Vec<Message>,
}

impl NonFinalTransition {
    pub fn is_noop(&self, current_state: H256) -> bool {
        // check if just created program (always op)
        !self.initial_state.is_zero()
            // check if state hash changed at final (always op)
            && current_state == self.initial_state
            // check if with unchanged state needs commitment (op)
            && (self.inheritor.is_none() && self.value_to_receive == 0 && self.claims.is_empty() && self.messages.is_empty())
    }
}
