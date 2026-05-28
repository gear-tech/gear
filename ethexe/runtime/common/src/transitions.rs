// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use alloc::{
    collections::{BTreeMap, btree_map::Iter},
    vec::Vec,
};
use anyhow::{Result, anyhow};
use core::num::NonZero;
use ethexe_common::{
    ProgramStates, Schedule, ScheduledTask, StateHashWithQueueSize,
    gear::{Message, StateTransition, ValueClaim},
};
use gprimitives::{ActorId, CodeId, H256};

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
    block_height: u32,
    states: ProgramStates,
    schedule: Schedule,
    modifications: BTreeMap<ActorId, NonFinalTransition>,
    program_creations: BTreeMap<ActorId, CodeId>,
}

#[derive(Debug, Clone, Default)]
pub struct FinalizedBlockTransitions {
    pub transitions: Vec<StateTransition>,
    pub states: ProgramStates,
    pub schedule: Schedule,
    pub program_creations: Vec<(ActorId, CodeId)>,
}

impl InBlockTransitions {
    pub fn new(block_height: u32, states: ProgramStates, schedule: Schedule) -> Self {
        Self {
            block_height,
            states,
            schedule,
            ..Default::default()
        }
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

    pub fn modifications_len(&self) -> usize {
        self.modifications.len()
    }

    /// Drain every scheduled task whose deadline is at or before
    /// `block_height` and return them in chronological order
    /// (oldest height first; within a height, BTreeSet `Ord`).
    pub fn take_actual_tasks(&mut self) -> Vec<ScheduledTask> {
        let cutoff = self.block_height.saturating_add(1);
        let kept = self.schedule.split_off(&cutoff);
        let due = core::mem::replace(&mut self.schedule, kept);
        due.into_values().flatten().collect()
    }

    pub fn schedule_task(&mut self, in_blocks: NonZero<u32>, task: ScheduledTask) -> u32 {
        let scheduled_block = self.block_height + u32::from(in_blocks);

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

    pub fn register_new(&mut self, actor_id: ActorId, code_id: CodeId) {
        self.states.insert(actor_id, StateHashWithQueueSize::zero());
        self.modifications.insert(actor_id, Default::default());
        self.program_creations.insert(actor_id, code_id);
    }

    pub fn registered_programs(&self) -> &BTreeMap<ActorId, CodeId> {
        &self.program_creations
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

    pub fn claim_value(&mut self, actor_id: ActorId, claim: ValueClaim) {
        self.modify(actor_id, |_state, transition| {
            transition.value_to_receive = transition
                .value_to_receive
                .checked_add(
                    i128::try_from(claim.value).expect("claimed_value doesn't fit in i128"),
                )
                .expect("Overflow in transition.value_to_receive += claimed_value");

            transition.claims.push(claim);
        });
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

    pub fn finalize(self) -> FinalizedBlockTransitions {
        let Self {
            states,
            schedule,
            modifications,
            program_creations,
            ..
        } = self;

        let mut transitions = Vec::with_capacity(modifications.len());

        for (actor_id, modification) in modifications {
            let new_state = states
                .get(&actor_id)
                .cloned()
                .expect("failed to find state record for modified state");

            if !modification.is_noop(new_state.hash) {
                transitions.push(StateTransition {
                    actor_id,
                    new_state_hash: new_state.hash,
                    exited: modification.inheritor.is_some(),
                    inheritor: modification.inheritor.unwrap_or_default(),
                    value_to_receive: modification.value_to_receive.unsigned_abs(),
                    value_to_receive_negative_sign: modification.value_to_receive < 0,
                    value_claims: modification.claims,
                    messages: modification.messages,
                });
            }
        }

        FinalizedBlockTransitions {
            transitions,
            states,
            schedule,
            program_creations: program_creations.into_iter().collect(),
        }
    }

    pub fn block_height(&self) -> u32 {
        self.block_height
    }

    #[cfg(any(test, feature = "mock"))]
    pub fn from_parts(
        block_height: u32,
        states: ProgramStates,
        schedule: Schedule,
        modifications: BTreeMap<ActorId, NonFinalTransition>,
        program_creations: BTreeMap<ActorId, CodeId>,
    ) -> Self {
        Self {
            block_height,
            states,
            schedule,
            modifications,
            program_creations,
        }
    }

    #[cfg(any(test, feature = "mock"))]
    pub fn modifications_mut(&mut self) -> &mut BTreeMap<ActorId, NonFinalTransition> {
        &mut self.modifications
    }

    #[cfg(any(test, feature = "mock"))]
    pub fn block_height_mut(&mut self) -> &mut u32 {
        &mut self.block_height
    }
}

#[derive(Debug, Default, Clone)]
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

    #[cfg(any(test, feature = "mock"))]
    pub fn initial_state(&self) -> H256 {
        self.initial_state
    }

    #[cfg(any(test, feature = "mock"))]
    pub fn new(
        initial_state: H256,
        inheritor: Option<ActorId>,
        value_to_receive: i128,
        claims: Vec<ValueClaim>,
        messages: Vec<Message>,
    ) -> Self {
        Self {
            initial_state,
            inheritor,
            value_to_receive,
            claims,
            messages,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::collections::BTreeSet;
    use ethexe_common::ScheduledTask;
    use gprimitives::MessageId;

    fn wake(actor: u8, msg: u8) -> ScheduledTask {
        ScheduledTask::WakeMessage(ActorId::from([actor; 32]), MessageId::from([msg; 32]))
    }

    fn transitions_with_schedule(block_height: u32, schedule: Schedule) -> InBlockTransitions {
        InBlockTransitions::new(block_height, ProgramStates::default(), schedule)
    }

    #[test]
    fn take_actual_tasks_single_height() {
        let mut schedule = Schedule::new();
        schedule
            .entry(10)
            .or_default()
            .extend([wake(1, 1), wake(2, 2)]);
        let mut t = transitions_with_schedule(10, schedule);

        let drained = t.take_actual_tasks();
        let drained: BTreeSet<_> = drained.into_iter().collect();
        assert_eq!(drained, BTreeSet::from([wake(1, 1), wake(2, 2)]));
        assert!(t.schedule.is_empty(), "all due heights drained");
    }

    /// Tasks left over from earlier MBs (heights < current) must fire on the
    /// next pass — that's the MB-driven invariant. Future heights stay put.
    #[test]
    fn take_actual_tasks_drains_past_heights_keeps_future() {
        let mut schedule = Schedule::new();
        schedule.entry(5).or_default().insert(wake(1, 1));
        schedule.entry(8).or_default().insert(wake(2, 2));
        schedule.entry(10).or_default().insert(wake(3, 3));
        schedule.entry(15).or_default().insert(wake(4, 4));
        schedule.entry(20).or_default().insert(wake(5, 5));
        let mut t = transitions_with_schedule(10, schedule);

        let drained = t.take_actual_tasks();
        // Past-and-current drained.
        assert_eq!(drained, vec![wake(1, 1), wake(2, 2), wake(3, 3)]);
        // Future preserved.
        assert_eq!(t.schedule.len(), 2);
        assert!(t.schedule.contains_key(&15));
        assert!(t.schedule.contains_key(&20));
    }

    /// Chronological ordering across heights — height-major, BTreeSet `Ord`
    /// within a height. Validators must agree on this order.
    #[test]
    fn take_actual_tasks_ordering_is_height_major() {
        let mut schedule = Schedule::new();
        // Inserted out of height order; insertion order in BTreeSet doesn't matter.
        schedule.entry(20).or_default().insert(wake(0, 9));
        schedule
            .entry(5)
            .or_default()
            .extend([wake(2, 2), wake(1, 1)]);
        schedule.entry(15).or_default().insert(wake(3, 3));
        let mut t = transitions_with_schedule(20, schedule);

        let drained = t.take_actual_tasks();
        // Height 5 first (Ord-sorted within), then 15, then 20.
        assert_eq!(
            drained,
            vec![wake(1, 1), wake(2, 2), wake(3, 3), wake(0, 9)]
        );
        assert!(t.schedule.is_empty());
    }

    /// Empty schedule → no tasks, no panic.
    #[test]
    fn take_actual_tasks_empty() {
        let mut t = transitions_with_schedule(42, Schedule::new());
        assert!(t.take_actual_tasks().is_empty());
    }

    /// `block_height = 0` should still drain height-0 tasks.
    #[test]
    fn take_actual_tasks_at_genesis() {
        let mut schedule = Schedule::new();
        schedule.entry(0).or_default().insert(wake(1, 1));
        schedule.entry(1).or_default().insert(wake(2, 2));
        let mut t = transitions_with_schedule(0, schedule);

        assert_eq!(t.take_actual_tasks(), vec![wake(1, 1)]);
        assert!(t.schedule.contains_key(&1));
    }
}
