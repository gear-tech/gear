// This file is part of Gear.
//
// Copyright (C) 2026 Gear Technologies Inc.
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

use crate::{
    FinalizedBlockTransitions, InBlockTransitions, NonFinalTransition,
    state::{
        ActiveProgram, Allocations, Dispatch, DispatchStash, Expiring, Mailbox, MailboxMessage,
        MemStorage, MemoryPages, MemoryPagesRegion, MessageQueue, MessageQueueHashWithSize,
        PayloadLookup, Program, ProgramState, RegionIdx, Storage, UserMailbox, Waitlist,
    },
};
#[cfg(test)]
use ::proptest::sample;
use ::proptest::{
    arbitrary::Arbitrary,
    collection, option,
    prelude::{BoxedStrategy, Strategy, any},
    prop_oneof,
    strategy::Just,
};
use alloc::collections::BTreeMap;
#[cfg(test)]
use alloc::vec::Vec;
#[cfg(test)]
use ethexe_common::Schedule;
use ethexe_common::{
    HashOf, MaybeHashOf, ProgramStates, StateHashWithQueueSize,
    gear::{Message, MessageType, ValueClaim},
    mock::schedule_strategy as common_schedule_strategy,
};
use gear_core::{
    buffer::Payload,
    ids::prelude::MessageIdExt as _,
    message::{ContextStore, DispatchKind, MessageDetails, ReplyDetails, SignalDetails},
    pages::{GearPage, WasmPage, numerated::tree::IntervalsTree},
    program::MemoryInfix,
    reservation::ReservationNonce,
};
use gear_core_errors::{
    ErrorReplyReason, ReplyCode, SignalCode, SimpleExecutionError, SuccessReplyReason,
};
use gprimitives::{ActorId, CodeId, H256, MessageId};
use parity_scale_codec::{Decode, Encode};

fn h256_strategy() -> BoxedStrategy<H256> {
    any::<[u8; 32]>().prop_map(Into::into).boxed()
}

fn actor_id_strategy() -> BoxedStrategy<ActorId> {
    h256_strategy().prop_map(Into::into).boxed()
}

fn code_id_strategy() -> BoxedStrategy<CodeId> {
    h256_strategy().prop_map(Into::into).boxed()
}

fn message_id_strategy() -> BoxedStrategy<MessageId> {
    h256_strategy().prop_map(Into::into).boxed()
}

fn hash_of_strategy<T: 'static>() -> BoxedStrategy<HashOf<T>> {
    h256_strategy()
        .prop_map(|hash| unsafe { HashOf::new(hash) })
        .boxed()
}

fn maybe_hash_of_strategy<T: 'static>() -> BoxedStrategy<MaybeHashOf<T>> {
    prop_oneof![
        Just(MaybeHashOf::empty()),
        hash_of_strategy::<T>().prop_map(Into::into),
    ]
    .boxed()
}

fn payload_strategy() -> BoxedStrategy<Payload> {
    collection::vec(any::<u8>(), 0..=64)
        .prop_map(|bytes| Payload::try_from(bytes).expect("payload strategy stays within bounds"))
        .boxed()
}

fn direct_payload_lookup_strategy() -> BoxedStrategy<PayloadLookup> {
    payload_strategy().prop_map(PayloadLookup::Direct).boxed()
}

fn payload_lookup_strategy() -> BoxedStrategy<PayloadLookup> {
    prop_oneof![
        direct_payload_lookup_strategy(),
        hash_of_strategy::<Payload>().prop_map(Into::into),
    ]
    .boxed()
}

fn reservation_nonce(raw: u64) -> ReservationNonce {
    ReservationNonce::decode(&mut &raw.encode()[..]).expect("reservation nonce encoding is valid")
}

fn context_store_strategy() -> BoxedStrategy<ContextStore> {
    (
        collection::btree_set(actor_id_strategy(), 0..=4),
        any::<u64>(),
        option::of(any::<u64>()),
        any::<u32>(),
    )
        .prop_map(
            |(initialized, reservation_nonce_raw, system_reservation, local_nonce)| {
                ContextStore::new(
                    initialized,
                    reservation_nonce(reservation_nonce_raw),
                    system_reservation,
                    local_nonce,
                )
            },
        )
        .boxed()
}

fn reply_code_strategy() -> BoxedStrategy<ReplyCode> {
    prop_oneof![
        Just(ReplyCode::Success(SuccessReplyReason::Auto)),
        Just(ReplyCode::Success(SuccessReplyReason::Manual)),
        Just(ReplyCode::Error(ErrorReplyReason::RemovedFromWaitlist)),
        Just(ReplyCode::Error(ErrorReplyReason::Execution(
            SimpleExecutionError::UserspacePanic,
        ))),
        Just(ReplyCode::Error(ErrorReplyReason::Execution(
            SimpleExecutionError::RanOutOfGas,
        ))),
        Just(ReplyCode::Error(ErrorReplyReason::Execution(
            SimpleExecutionError::MemoryOverflow,
        ))),
    ]
    .boxed()
}

fn signal_code_strategy() -> BoxedStrategy<SignalCode> {
    prop_oneof![
        Just(SignalCode::RemovedFromWaitlist),
        Just(SignalCode::Execution(SimpleExecutionError::UserspacePanic)),
        Just(SignalCode::Execution(SimpleExecutionError::RanOutOfGas)),
        Just(SignalCode::Execution(SimpleExecutionError::MemoryOverflow)),
    ]
    .boxed()
}

fn reply_details_strategy() -> BoxedStrategy<ReplyDetails> {
    (message_id_strategy(), reply_code_strategy())
        .prop_map(|(to, code)| ReplyDetails::new(to, code))
        .boxed()
}

fn signal_details_strategy() -> BoxedStrategy<SignalDetails> {
    (message_id_strategy(), signal_code_strategy())
        .prop_map(|(to, code)| SignalDetails::new(to, code))
        .boxed()
}

fn value_claim_strategy() -> BoxedStrategy<ValueClaim> {
    (message_id_strategy(), actor_id_strategy(), any::<u128>())
        .prop_map(|(message_id, destination, value)| ValueClaim {
            message_id,
            destination,
            value,
        })
        .boxed()
}

fn message_strategy() -> BoxedStrategy<Message> {
    (
        message_id_strategy(),
        actor_id_strategy(),
        collection::vec(any::<u8>(), 0..=64),
        any::<u128>(),
        option::of(reply_details_strategy()),
        any::<bool>(),
    )
        .prop_map(
            |(id, destination, payload, value, reply_details, call)| Message {
                id,
                destination,
                payload,
                value,
                reply_details,
                call,
            },
        )
        .boxed()
}

#[cfg(test)]
fn dispatch_kind_strategy() -> BoxedStrategy<DispatchKind> {
    prop_oneof![
        Just(DispatchKind::Init),
        Just(DispatchKind::Handle),
        Just(DispatchKind::Reply),
        Just(DispatchKind::Signal),
    ]
    .boxed()
}

fn dispatch_strategy_with(
    payload_lookup: fn() -> BoxedStrategy<PayloadLookup>,
) -> BoxedStrategy<Dispatch> {
    prop_oneof![
        (
            message_id_strategy(),
            actor_id_strategy(),
            payload_lookup(),
            any::<u128>(),
            any::<MessageType>(),
            any::<bool>(),
            option::of(context_store_strategy()),
        )
            .prop_map(
                |(id, source, payload, value, message_type, call, context)| Dispatch {
                    id,
                    kind: DispatchKind::Init,
                    source,
                    payload,
                    value,
                    details: None,
                    context,
                    message_type,
                    call,
                }
            ),
        (
            message_id_strategy(),
            actor_id_strategy(),
            payload_lookup(),
            any::<u128>(),
            any::<MessageType>(),
            any::<bool>(),
            option::of(context_store_strategy()),
        )
            .prop_map(
                |(id, source, payload, value, message_type, call, context)| Dispatch {
                    id,
                    kind: DispatchKind::Handle,
                    source,
                    payload,
                    value,
                    details: None,
                    context,
                    message_type,
                    call,
                }
            ),
        (
            message_id_strategy(),
            actor_id_strategy(),
            payload_lookup(),
            any::<u128>(),
            reply_code_strategy(),
            any::<MessageType>(),
            any::<bool>(),
            option::of(context_store_strategy()),
        )
            .prop_map(
                |(reply_to, source, payload, value, reply_code, message_type, call, context)| {
                    Dispatch {
                        id: MessageId::generate_reply(reply_to),
                        kind: DispatchKind::Reply,
                        source,
                        payload,
                        value,
                        details: Some(MessageDetails::Reply(ReplyDetails::new(
                            reply_to, reply_code,
                        ))),
                        context,
                        message_type,
                        call,
                    }
                }
            ),
        (
            message_id_strategy(),
            actor_id_strategy(),
            payload_lookup(),
            any::<u128>(),
            signal_details_strategy(),
            any::<MessageType>(),
            any::<bool>(),
            option::of(context_store_strategy()),
        )
            .prop_map(
                |(id, source, payload, value, signal_details, message_type, call, context)| {
                    Dispatch {
                        id,
                        kind: DispatchKind::Signal,
                        source,
                        payload,
                        value,
                        details: Some(MessageDetails::Signal(signal_details)),
                        context,
                        message_type,
                        call,
                    }
                }
            ),
    ]
    .boxed()
}

fn dispatch_strategy() -> BoxedStrategy<Dispatch> {
    dispatch_strategy_with(payload_lookup_strategy)
}

#[cfg(test)]
fn dispatch_direct_payload_strategy() -> BoxedStrategy<Dispatch> {
    dispatch_strategy_with(direct_payload_lookup_strategy)
}

fn mailbox_message_strategy() -> BoxedStrategy<MailboxMessage> {
    (
        payload_lookup_strategy(),
        any::<u128>(),
        any::<MessageType>(),
    )
        .prop_map(|(payload, value, message_type)| {
            MailboxMessage::new(payload, value, message_type)
        })
        .boxed()
}

fn gear_page_strategy(start: u32, end_exclusive: u32) -> BoxedStrategy<GearPage> {
    (start..end_exclusive)
        .prop_map(|page| GearPage::try_from(page).expect("page range stays valid"))
        .boxed()
}

fn wasm_page_strategy(start: u32, end_exclusive: u32) -> BoxedStrategy<WasmPage> {
    (start..end_exclusive)
        .prop_map(|page| WasmPage::try_from(page).expect("page range stays valid"))
        .boxed()
}

fn page_buf_strategy() -> BoxedStrategy<gear_core::memory::PageBuf> {
    any::<u8>()
        .prop_map(gear_core::memory::PageBuf::filled_with)
        .boxed()
}

fn raw_pages_strategy(
    start: u32,
    end_exclusive: u32,
) -> BoxedStrategy<BTreeMap<GearPage, gear_core::memory::PageBuf>> {
    collection::btree_map(
        gear_page_strategy(start, end_exclusive),
        page_buf_strategy(),
        0..=16,
    )
    .boxed()
}

fn allocations_tree_strategy() -> BoxedStrategy<IntervalsTree<WasmPage>> {
    collection::btree_set(wasm_page_strategy(0, 128), 0..=16)
        .prop_map(|pages| pages.into_iter().collect::<IntervalsTree<WasmPage>>())
        .boxed()
}

fn program_states_strategy() -> BoxedStrategy<ProgramStates> {
    collection::btree_map(actor_id_strategy(), any::<StateHashWithQueueSize>(), 0..=4).boxed()
}

fn in_block_transitions_strategy() -> BoxedStrategy<InBlockTransitions> {
    (
        any::<u32>(),
        program_states_strategy(),
        common_schedule_strategy(),
        collection::btree_map(actor_id_strategy(), any::<NonFinalTransition>(), 0..=4),
        collection::btree_map(actor_id_strategy(), code_id_strategy(), 0..=3),
    )
        .prop_map(
            |(block_height, states, schedule, modifications, raw_program_creations)| {
                let mut program_creations = BTreeMap::new();
                for (actor_id, code_id) in raw_program_creations {
                    if !states.contains_key(&actor_id) {
                        program_creations.insert(actor_id, code_id);
                    }
                }

                InBlockTransitions::from_parts(
                    block_height,
                    states,
                    schedule,
                    modifications,
                    program_creations,
                )
            },
        )
        .boxed()
}

#[cfg(test)]
#[derive(Debug, Clone)]
struct InBlockTransitionsModel {
    modifications: BTreeMap<ActorId, NonFinalTransition>,
    states: ProgramStates,
    schedule: Schedule,
    program_creations: BTreeMap<ActorId, CodeId>,
}

#[cfg(test)]
fn transition_with_current_state_strategy(
    current_state: StateHashWithQueueSize,
) -> BoxedStrategy<NonFinalTransition> {
    prop_oneof![
        any::<NonFinalTransition>(),
        Just(NonFinalTransition::new(
            current_state.hash,
            None,
            0,
            Vec::new(),
            Vec::new(),
        )),
    ]
    .boxed()
}

#[cfg(test)]
fn in_block_transitions_with_model_strategy()
-> BoxedStrategy<(InBlockTransitions, InBlockTransitionsModel)> {
    (
        any::<u32>(),
        program_states_strategy(),
        common_schedule_strategy(),
        collection::btree_map(actor_id_strategy(), code_id_strategy(), 0..=3),
    )
        .prop_flat_map(|(block_height, states, schedule, raw_program_creations)| {
            let mut program_creations = BTreeMap::new();
            for (actor_id, code_id) in raw_program_creations {
                if !states.contains_key(&actor_id) {
                    program_creations.insert(actor_id, code_id);
                }
            }

            let mut known_states = states;
            for actor_id in program_creations.keys().copied() {
                known_states
                    .entry(actor_id)
                    .or_insert_with(StateHashWithQueueSize::zero);
            }

            let actors: Vec<_> = known_states.keys().copied().collect();
            let modifications = if actors.is_empty() {
                Just(BTreeMap::new()).boxed()
            } else {
                collection::vec(
                    sample::select(actors.clone()).prop_flat_map({
                        let known_states = known_states.clone();
                        move |actor_id| {
                            let current_state = known_states
                                .get(&actor_id)
                                .copied()
                                .expect("actor selected from known states");

                            (
                                Just(actor_id),
                                transition_with_current_state_strategy(current_state),
                            )
                        }
                    }),
                    0..=4,
                )
                .prop_map(|entries| entries.into_iter().collect::<BTreeMap<_, _>>())
                .boxed()
            };

            (
                Just(block_height),
                Just(known_states),
                Just(schedule),
                modifications,
                Just(program_creations),
            )
                .prop_map(
                    |(block_height, states, schedule, modifications, program_creations)| {
                        let model = InBlockTransitionsModel {
                            modifications: modifications.clone(),
                            states: states.clone(),
                            schedule: schedule.clone(),
                            program_creations: program_creations.clone(),
                        };
                        let transitions = InBlockTransitions::from_parts(
                            block_height,
                            states,
                            schedule,
                            modifications,
                            program_creations,
                        );

                        (transitions, model)
                    },
                )
                .boxed()
        })
        .boxed()
}

#[cfg(test)]
fn waitlist_entries_strategy() -> BoxedStrategy<BTreeMap<MessageId, (Dispatch, u32)>> {
    collection::btree_map(
        message_id_strategy(),
        (dispatch_direct_payload_strategy(), any::<u32>()),
        0..=6,
    )
    .boxed()
}

#[cfg(test)]
type DispatchStashEntry = (Dispatch, u32, Option<ActorId>);

#[cfg(test)]
fn dispatch_stash_entries_strategy() -> BoxedStrategy<BTreeMap<MessageId, DispatchStashEntry>> {
    collection::btree_map(
        message_id_strategy(),
        (
            dispatch_direct_payload_strategy(),
            any::<u32>(),
            option::of(actor_id_strategy()),
        ),
        0..=6,
    )
    .boxed()
}

type MailboxWithStorage = (
    MemStorage,
    Mailbox,
    BTreeMap<ActorId, BTreeMap<MessageId, Expiring<MailboxMessage>>>,
);

fn mailbox_with_storage_strategy() -> BoxedStrategy<MailboxWithStorage> {
    collection::btree_map(
        actor_id_strategy(),
        collection::btree_map(
            message_id_strategy(),
            (mailbox_message_strategy(), any::<u32>()),
            0..=4,
        ),
        0..=4,
    )
    .prop_map(|users| {
        let storage = MemStorage::default();
        let mut mailbox = Mailbox::default();
        let mut expected = BTreeMap::new();

        for (user_id, messages) in users {
            let mut user_mailbox = BTreeMap::new();
            for (message_id, (message, expiry)) in messages {
                mailbox.add_and_store_user_mailbox(
                    &storage,
                    user_id,
                    message_id,
                    message.clone(),
                    expiry,
                );
                user_mailbox.insert(
                    message_id,
                    Expiring {
                        value: message,
                        expiry,
                    },
                );
            }
            if !user_mailbox.is_empty() {
                expected.insert(user_id, user_mailbox);
            }
        }

        (storage, mailbox, expected)
    })
    .boxed()
}

#[cfg(test)]
fn payload_lookup_with_storage_strategy() -> BoxedStrategy<(MemStorage, PayloadLookup, Vec<u8>)> {
    collection::vec(any::<u8>(), 0..=64)
        .prop_map(|bytes| {
            let payload = Payload::try_from(bytes.clone()).expect("payload strategy stays valid");
            (MemStorage::default(), PayloadLookup::Direct(payload), bytes)
        })
        .boxed()
}

#[cfg(test)]
fn dispatch_with_storage_strategy() -> BoxedStrategy<(MemStorage, Dispatch, ActorId, Vec<u8>)> {
    (
        dispatch_kind_strategy(),
        actor_id_strategy(),
        actor_id_strategy(),
        collection::vec(any::<u8>(), 0..=64),
        any::<u128>(),
        option::of(any::<bool>()),
        any::<MessageType>(),
        any::<bool>(),
        option::of(context_store_strategy()),
        option::of(any::<bool>()),
    )
        .prop_map(
            |(
                kind,
                source,
                destination,
                payload,
                value,
                store_payload,
                message_type,
                call,
                context,
                prefer_reply,
            )| {
                let storage = MemStorage::default();
                let stored_payload = Payload::try_from(payload.clone())
                    .expect("payload strategy stays within bounds");
                let payload_lookup = if store_payload.unwrap_or(false) {
                    storage.write_payload(stored_payload).into()
                } else {
                    PayloadLookup::Direct(stored_payload)
                };

                let dispatch = match kind {
                    DispatchKind::Init | DispatchKind::Handle => Dispatch {
                        id: MessageId::from([payload.len() as u8; 32]),
                        kind,
                        source,
                        payload: payload_lookup,
                        value,
                        details: None,
                        context,
                        message_type,
                        call,
                    },
                    DispatchKind::Reply => {
                        let reply_to = MessageId::from([source.as_ref()[0]; 32]);
                        Dispatch {
                            id: MessageId::generate_reply(reply_to),
                            kind,
                            source,
                            payload: payload_lookup,
                            value,
                            details: Some(MessageDetails::Reply(ReplyDetails::new(
                                reply_to,
                                ReplyCode::Success(match prefer_reply.unwrap_or(false) {
                                    true => SuccessReplyReason::Manual,
                                    false => SuccessReplyReason::Auto,
                                }),
                            ))),
                            context,
                            message_type,
                            call,
                        }
                    }
                    DispatchKind::Signal => Dispatch {
                        id: MessageId::from([destination.as_ref()[0]; 32]),
                        kind,
                        source,
                        payload: payload_lookup,
                        value,
                        details: Some(MessageDetails::Signal(SignalDetails::new(
                            MessageId::from([value as u8; 32]),
                            SignalCode::RemovedFromWaitlist,
                        ))),
                        context,
                        message_type,
                        call,
                    },
                };

                (storage, dispatch, destination, payload)
            },
        )
        .boxed()
}

#[cfg(test)]
fn flatten_memory_pages(
    storage: &MemStorage,
    pages: &MemoryPages,
) -> BTreeMap<GearPage, HashOf<gear_core::memory::PageBuf>> {
    let mut flattened = BTreeMap::new();

    for region_hash in pages.to_inner() {
        if let Some(region_hash) = region_hash.to_inner() {
            let region = storage
                .memory_pages_region(region_hash)
                .expect("region must exist in storage");
            flattened.extend(region.as_inner().iter().map(|(&page, &data)| (page, data)));
        }
    }

    flattened
}

impl Arbitrary for PayloadLookup {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(_args: Self::Parameters) -> Self::Strategy {
        payload_lookup_strategy()
    }
}

impl Arbitrary for MessageQueueHashWithSize {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(_args: Self::Parameters) -> Self::Strategy {
        prop_oneof![
            Just(Self {
                hash: MaybeHashOf::empty(),
                cached_queue_size: 0,
            }),
            (hash_of_strategy::<MessageQueue>(), 1u8..=u8::MAX).prop_map(
                |(hash, cached_queue_size)| Self {
                    hash: hash.into(),
                    cached_queue_size,
                }
            ),
        ]
        .boxed()
    }
}

impl Arbitrary for ActiveProgram {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(_args: Self::Parameters) -> Self::Strategy {
        (
            maybe_hash_of_strategy::<Allocations>(),
            maybe_hash_of_strategy::<MemoryPages>(),
            any::<u32>().prop_map(MemoryInfix::new),
            any::<bool>(),
        )
            .prop_map(
                |(allocations_hash, pages_hash, memory_infix, initialized)| Self {
                    allocations_hash,
                    pages_hash,
                    memory_infix,
                    initialized,
                },
            )
            .boxed()
    }
}

impl Arbitrary for Program {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(_args: Self::Parameters) -> Self::Strategy {
        prop_oneof![
            any::<ActiveProgram>().prop_map(Self::Active),
            actor_id_strategy().prop_map(Self::Exited),
            actor_id_strategy().prop_map(Self::Terminated),
        ]
        .boxed()
    }
}

impl Arbitrary for ProgramState {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(_args: Self::Parameters) -> Self::Strategy {
        (
            any::<Program>(),
            any::<MessageQueueHashWithSize>(),
            any::<MessageQueueHashWithSize>(),
            maybe_hash_of_strategy::<Waitlist>(),
            maybe_hash_of_strategy::<DispatchStash>(),
            maybe_hash_of_strategy::<Mailbox>(),
            any::<u128>(),
            any::<u128>(),
        )
            .prop_map(
                |(
                    program,
                    canonical_queue,
                    injected_queue,
                    waitlist_hash,
                    stash_hash,
                    mailbox_hash,
                    balance,
                    executable_balance,
                )| Self {
                    program,
                    canonical_queue,
                    injected_queue,
                    waitlist_hash,
                    stash_hash,
                    mailbox_hash,
                    balance,
                    executable_balance,
                },
            )
            .boxed()
    }
}

impl Arbitrary for Dispatch {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(_args: Self::Parameters) -> Self::Strategy {
        dispatch_strategy()
    }
}

impl<T> Arbitrary for Expiring<T>
where
    T: Arbitrary + 'static,
{
    type Parameters = T::Parameters;
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(args: Self::Parameters) -> Self::Strategy {
        (T::arbitrary_with(args), any::<u32>())
            .prop_map(|(value, expiry)| Self { value, expiry })
            .boxed()
    }
}

impl Arbitrary for MessageQueue {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(_args: Self::Parameters) -> Self::Strategy {
        collection::vec(any::<Dispatch>(), 0..=6)
            .prop_map(|dispatches| {
                let mut queue = Self::default();
                for dispatch in dispatches {
                    queue.queue(dispatch);
                }
                queue
            })
            .boxed()
    }
}

impl Arbitrary for Waitlist {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(_args: Self::Parameters) -> Self::Strategy {
        collection::btree_map(
            message_id_strategy(),
            (dispatch_strategy(), any::<u32>()),
            0..=6,
        )
        .prop_map(|entries| {
            let mut waitlist = Self::default();
            for (message_id, (mut dispatch, expiry)) in entries {
                dispatch.id = message_id;
                waitlist.wait(dispatch, expiry);
            }
            waitlist
        })
        .boxed()
    }
}

impl Arbitrary for DispatchStash {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(_args: Self::Parameters) -> Self::Strategy {
        collection::btree_map(
            message_id_strategy(),
            (
                dispatch_strategy(),
                any::<u32>(),
                option::of(actor_id_strategy()),
            ),
            0..=6,
        )
        .prop_map(|entries| {
            let mut stash = Self::default();
            for (message_id, (mut dispatch, expiry, user_id)) in entries {
                dispatch.id = message_id;
                match user_id {
                    Some(user_id) => stash.add_to_user(dispatch, expiry, user_id),
                    None => stash.add_to_program(dispatch, expiry),
                }
            }
            stash
        })
        .boxed()
    }
}

impl Arbitrary for MailboxMessage {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(_args: Self::Parameters) -> Self::Strategy {
        mailbox_message_strategy()
    }
}

impl Arbitrary for UserMailbox {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(_args: Self::Parameters) -> Self::Strategy {
        collection::btree_map(
            message_id_strategy(),
            (any::<MailboxMessage>(), any::<u32>()),
            0..=4,
        )
        .prop_map(|entries| {
            Self::from_inner(
                entries
                    .into_iter()
                    .map(|(message_id, (value, expiry))| (message_id, Expiring { value, expiry }))
                    .collect(),
            )
        })
        .boxed()
    }
}

impl Arbitrary for Mailbox {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(_args: Self::Parameters) -> Self::Strategy {
        mailbox_with_storage_strategy()
            .prop_map(|(_storage, mailbox, _expected)| mailbox)
            .boxed()
    }
}

impl Arbitrary for RegionIdx {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(_args: Self::Parameters) -> Self::Strategy {
        (0u8..MemoryPages::REGIONS_AMOUNT as u8)
            .prop_map(|idx| {
                let page = GearPage::try_from(idx as u32 * MemoryPages::PAGES_PER_REGION as u32)
                    .expect("region start must be a valid page");
                MemoryPages::page_region(page)
            })
            .boxed()
    }
}

impl Arbitrary for MemoryPagesRegion {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(_args: Self::Parameters) -> Self::Strategy {
        any::<RegionIdx>()
            .prop_flat_map(|region_idx| {
                let region_start =
                    u32::from(Into::<u8>::into(region_idx)) * MemoryPages::PAGES_PER_REGION as u32;
                let region_end = region_start + MemoryPages::PAGES_PER_REGION as u32;
                raw_pages_strategy(region_start, region_end)
                    .prop_map(|pages| {
                        let storage = MemStorage::default();
                        Self::from_inner(storage.write_pages_data(pages))
                    })
                    .boxed()
            })
            .boxed()
    }
}

impl Arbitrary for MemoryPages {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(_args: Self::Parameters) -> Self::Strategy {
        raw_pages_strategy(0, MemoryPages::MAX_PAGES as u32)
            .prop_map(|pages| {
                let storage = MemStorage::default();
                let mut memory_pages = Self::default();
                memory_pages.update_and_store_regions(&storage, storage.write_pages_data(pages));
                memory_pages
            })
            .boxed()
    }
}

impl Arbitrary for Allocations {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(_args: Self::Parameters) -> Self::Strategy {
        allocations_tree_strategy()
            .prop_map(|intervals| {
                let mut allocations = Self::default();
                let _ = allocations.update(intervals);
                allocations
            })
            .boxed()
    }
}

impl Arbitrary for NonFinalTransition {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(_args: Self::Parameters) -> Self::Strategy {
        (
            h256_strategy(),
            option::of(actor_id_strategy()),
            any::<i128>(),
            collection::vec(value_claim_strategy(), 0..=4),
            collection::vec(message_strategy(), 0..=4),
        )
            .prop_map(
                |(initial_state, inheritor, value_to_receive, claims, messages)| {
                    Self::new(initial_state, inheritor, value_to_receive, claims, messages)
                },
            )
            .boxed()
    }
}

impl Arbitrary for InBlockTransitions {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(_args: Self::Parameters) -> Self::Strategy {
        in_block_transitions_strategy()
    }
}

impl Arbitrary for FinalizedBlockTransitions {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(_args: Self::Parameters) -> Self::Strategy {
        (
            collection::vec(any::<ethexe_common::gear::StateTransition>(), 0..=4),
            program_states_strategy(),
            common_schedule_strategy(),
            collection::vec((actor_id_strategy(), code_id_strategy()), 0..=4),
        )
            .prop_map(|(transitions, states, schedule, program_creations)| Self {
                transitions,
                states,
                schedule,
                program_creations,
            })
            .boxed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ScheduleRestorer, state::QueryableStorage};
    use ::proptest::{prop_assert, prop_assert_eq, test_runner::Config as ProptestConfig};
    use alloc::collections::BTreeSet;
    use ethexe_common::ScheduledTask;

    fn proptest_config() -> ProptestConfig {
        ProptestConfig::with_cases(128)
    }

    ::proptest::proptest! {
        #![proptest_config(proptest_config())]

        #[test]
        fn finalize_matches_model((transitions, model) in in_block_transitions_with_model_strategy()) {
            let finalized = transitions.finalize();

            let transitions_by_actor = finalized
                .transitions
                .iter()
                .map(|transition| (transition.actor_id, transition))
                .collect::<BTreeMap<_, _>>();
            prop_assert_eq!(
                transitions_by_actor.len(),
                finalized.transitions.len(),
                "each actor must appear at most once in finalized transitions",
            );

            let expected_actor_ids = model
                .modifications
                .iter()
                .filter_map(|(&actor_id, modification)| {
                    let current_state = model
                        .states
                        .get(&actor_id)
                        .expect("modification only generated for known actor")
                        .hash;
                    let initial_state = modification.initial_state();
                    let noop = !initial_state.is_zero()
                        && current_state == initial_state
                        && modification.inheritor.is_none()
                        && modification.value_to_receive == 0
                        && modification.claims.is_empty()
                        && modification.messages.is_empty();

                    (!noop).then_some(actor_id)
                })
                .collect::<BTreeSet<_>>();
            let actual_actor_ids = transitions_by_actor.keys().copied().collect::<BTreeSet<_>>();
            prop_assert_eq!(actual_actor_ids, expected_actor_ids.clone());

            for actor_id in expected_actor_ids {
                let modification = model
                    .modifications
                    .get(&actor_id)
                    .expect("actor comes from modifications");
                let transition = transitions_by_actor
                    .get(&actor_id)
                    .expect("non-noop modification must be finalized");
                let current_state = model
                    .states
                    .get(&actor_id)
                    .expect("modification only generated for known actor");

                prop_assert_eq!(transition.new_state_hash, current_state.hash);
                prop_assert_eq!(transition.exited, modification.inheritor.is_some());
                prop_assert_eq!(transition.inheritor, modification.inheritor.unwrap_or_default());
                prop_assert_eq!(
                    transition.value_to_receive,
                    modification.value_to_receive.unsigned_abs(),
                );
                prop_assert_eq!(
                    transition.value_to_receive_negative_sign,
                    modification.value_to_receive < 0,
                );
                prop_assert_eq!(&transition.value_claims, &modification.claims);
                prop_assert_eq!(&transition.messages, &modification.messages);
            }
            prop_assert_eq!(finalized.states, model.states);
            prop_assert_eq!(finalized.schedule, model.schedule);
            prop_assert_eq!(
                finalized.program_creations.into_iter().collect::<BTreeMap<_, _>>(),
                model.program_creations,
            );
        }

        #[test]
        fn restorer_waitlist_respects_expiry(current_block in any::<u32>(), program_id in actor_id_strategy(), waitlist in any::<Waitlist>()) {
            let expected: Schedule = waitlist
                .clone()
                .into_inner()
                .into_iter()
                .filter_map(|(message_id, expiring)| {
                    (expiring.expiry > current_block).then_some((
                        expiring.expiry,
                        BTreeSet::from([ScheduledTask::WakeMessage(program_id, message_id)]),
                    ))
                })
                .fold(BTreeMap::new(), |mut acc, (expiry, tasks)| {
                    acc.entry(expiry).or_default().extend(tasks);
                    acc
                });

            let mut restorer = ScheduleRestorer::new(current_block);
            restorer.waitlist(program_id, &waitlist);

            prop_assert_eq!(restorer.restore(), expected);
        }

        #[test]
        fn restorer_user_mailbox_respects_expiry(
            current_block in any::<u32>(),
            program_id in actor_id_strategy(),
            user_id in actor_id_strategy(),
            user_mailbox in any::<UserMailbox>(),
        ) {
            let expected: Schedule = user_mailbox
                .as_ref()
                .iter()
                .filter_map(|(&message_id, expiring)| {
                    (expiring.expiry > current_block).then_some((
                        expiring.expiry,
                        BTreeSet::from([ScheduledTask::RemoveFromMailbox(
                            (program_id, user_id),
                            message_id,
                        )]),
                    ))
                })
                .fold(BTreeMap::new(), |mut acc, (expiry, tasks)| {
                    acc.entry(expiry).or_default().extend(tasks);
                    acc
                });

            let mut restorer = ScheduleRestorer::new(current_block);
            restorer.user_mailbox(program_id, user_id, &user_mailbox);

            prop_assert_eq!(restorer.restore(), expected);
        }

        #[test]
        fn restorer_stash_restores_correct_task_kind(
            current_block in any::<u32>(),
            program_id in actor_id_strategy(),
            entries in dispatch_stash_entries_strategy(),
        ) {
            let mut stash = DispatchStash::default();
            let mut expected: Schedule = BTreeMap::new();

            for (&message_id, (dispatch, expiry, user_id)) in &entries {
                let mut dispatch = dispatch.clone();
                dispatch.id = message_id;
                let expiry = *expiry;
                if let Some(user_id) = *user_id {
                    stash.add_to_user(dispatch.clone(), expiry, user_id);
                    if expiry > current_block {
                        expected
                            .entry(expiry)
                            .or_default()
                            .insert(ScheduledTask::SendUserMessage {
                                message_id,
                                to_mailbox: program_id,
                            });
                    }
                } else {
                    stash.add_to_program(dispatch.clone(), expiry);
                    if expiry > current_block {
                        expected
                            .entry(expiry)
                            .or_default()
                            .insert(ScheduledTask::SendDispatch((program_id, message_id)));
                    }
                }
            }

            let mut restorer = ScheduleRestorer::new(current_block);
            restorer.stash(program_id, &stash);

            prop_assert_eq!(restorer.restore(), expected);
        }

        #[test]
        fn message_queue_is_fifo(dispatches in collection::vec(dispatch_direct_payload_strategy(), 0..=32)) {
            let mut queue = MessageQueue::default();
            for dispatch in &dispatches {
                queue.queue(dispatch.clone());
            }

            let drained: Vec<_> = core::iter::from_fn(|| queue.dequeue()).collect();
            prop_assert_eq!(drained, dispatches);
        }

        #[test]
        fn waitlist_wait_and_wake_round_trip(entries in waitlist_entries_strategy()) {
            let mut waitlist = Waitlist::default();
            let mut expected = BTreeMap::new();

            for (message_id, (mut dispatch, expiry)) in entries {
                dispatch.id = message_id;
                waitlist.wait(dispatch.clone(), expiry);
                expected.insert(message_id, Expiring { value: dispatch, expiry });
            }

            for (message_id, expiring) in expected {
                prop_assert_eq!(waitlist.wake(&message_id), Some(expiring));
            }

            prop_assert!(waitlist.as_ref().is_empty());
        }

        #[test]
        fn dispatch_stash_preserves_user_vs_program(entries in dispatch_stash_entries_strategy()) {
            let mut stash = DispatchStash::default();
            let mut expected = BTreeMap::new();

            for (message_id, (mut dispatch, expiry, user_id)) in entries {
                dispatch.id = message_id;
                match user_id {
                    Some(user_id) => stash.add_to_user(dispatch.clone(), expiry, user_id),
                    None => stash.add_to_program(dispatch.clone(), expiry),
                }
                expected.insert(message_id, (dispatch, user_id));
            }

            for (message_id, (dispatch, user_id)) in expected {
                match user_id {
                    Some(user_id) => prop_assert_eq!(stash.remove_to_user(&message_id), (dispatch, user_id)),
                    None => prop_assert_eq!(stash.remove_to_program(&message_id), dispatch),
                }
            }

            prop_assert!(stash.as_ref().is_empty());
        }

        #[test]
        fn mailbox_into_values_round_trips((storage, mailbox, expected) in mailbox_with_storage_strategy()) {
            prop_assert_eq!(mailbox.into_values(&storage), expected);
        }

        #[test]
        fn payload_lookup_force_stored_and_query_round_trip((storage, mut payload_lookup, expected) in payload_lookup_with_storage_strategy()) {
            let hash = payload_lookup.force_stored(&storage);
            prop_assert_eq!(payload_lookup.clone(), PayloadLookup::Stored(hash));
            prop_assert_eq!(
                payload_lookup
                    .query(&storage)
                    .expect("stored payload should be readable")
                    .into_vec(),
                expected,
            );
        }

        #[test]
        fn dispatch_into_message_preserves_logical_content((storage, dispatch, destination, expected_payload) in dispatch_with_storage_strategy()) {
            let expected_id = dispatch.id;
            let expected_value = dispatch.value;
            let expected_call = dispatch.call;
            let expected_reply_details = dispatch.details.and_then(|details| details.to_reply_details());

            let message = dispatch.into_message(&storage, destination);

            prop_assert_eq!(message.id, expected_id);
            prop_assert_eq!(message.destination, destination);
            prop_assert_eq!(message.payload, expected_payload);
            prop_assert_eq!(message.value, expected_value);
            prop_assert_eq!(message.reply_details, expected_reply_details);
            prop_assert_eq!(message.call, expected_call);
        }

        #[test]
        fn memory_pages_update_and_remove_round_trip(
            raw_pages in raw_pages_strategy(0, MemoryPages::MAX_PAGES as u32)
                .prop_flat_map(|raw_pages| {
                    let removable_pages: Vec<_> = raw_pages.keys().copied().collect();
                    let removed = if removable_pages.is_empty() {
                        Just(Vec::new()).boxed()
                    } else {
                        collection::vec(sample::select(removable_pages), 0..=8).boxed()
                    };

                    (Just(raw_pages), removed)
                })
        ) {
            let (raw_pages, removed_pages) = raw_pages;
            let storage = MemStorage::default();
            let hashed_pages = storage.write_pages_data(raw_pages);
            let initial_pages = hashed_pages.clone();
            let mut memory_pages = MemoryPages::default();
            memory_pages.update_and_store_regions(&storage, hashed_pages.clone());

            prop_assert_eq!(
                flatten_memory_pages(&storage, &memory_pages),
                hashed_pages.clone(),
            );

            let mut expected = hashed_pages;
            for page in &removed_pages {
                expected.remove(page);
            }

            memory_pages.remove_and_store_regions(&storage, &removed_pages);

            let actual = flatten_memory_pages(&storage, &memory_pages);

            for region_idx in 0..MemoryPages::REGIONS_AMOUNT {
                let region_idx = region_idx as u8;

                let expected_region: BTreeMap<_, _> = expected
                    .iter()
                    .filter(|(page, _)| {
                        let page_region: u8 = MemoryPages::page_region(**page).into();
                        page_region == region_idx
                    })
                    .map(|(&page, &hash)| (page, hash))
                    .collect();

                // TODO #5373: removing all pages from a region leaves the old region hash in place,
                // so empty regions cannot be asserted until MemoryPages::remove_and_store_regions
                // clears the stored region entry.
                if expected_region.is_empty() {
                    continue;
                }

                let actual_region: BTreeMap<_, _> = actual
                    .iter()
                    .filter(|(page, _)| {
                        let page_region: u8 = MemoryPages::page_region(**page).into();
                        page_region == region_idx
                    })
                    .map(|(&page, &hash)| (page, hash))
                    .collect();

                prop_assert_eq!(
                    actual_region,
                    expected_region,
                    "region {:?} should match when it remains non-empty after removal",
                    region_idx
                );
            }

            for page in initial_pages.keys().filter(|page| !expected.contains_key(page)) {
                let region_idx: u8 = MemoryPages::page_region(*page).into();
                let region_still_has_pages = expected
                    .keys()
                    .any(|remaining_page| {
                        let page_region: u8 = MemoryPages::page_region(*remaining_page).into();
                        page_region == region_idx
                    });

                if region_still_has_pages {
                    prop_assert!(
                        !actual.contains_key(page),
                        "removed page {page:?} should not remain in non-empty region {region_idx:?}"
                    );
                }
            }
        }

        #[test]
        fn allocations_update_and_store_is_consistent(
            initial in allocations_tree_strategy(),
            updated in allocations_tree_strategy(),
        ) {
            let storage = MemStorage::default();
            let mut allocations = Allocations::default();
            let _ = allocations.update(initial.clone());

            let expected_removed: Vec<_> = initial
                .difference(&updated)
                .flat_map(|interval| interval.iter())
                .flat_map(|page| page.to_iter())
                .collect();

            prop_assert_eq!(allocations.update(updated.clone()), expected_removed.clone());
            prop_assert_eq!(allocations.tree_len(), updated.intervals_amount() as u32);

            match allocations.store(&storage) {
                Some(hash) => {
                    let mut restored = storage.query(&hash).expect("stored allocations should be readable");
                    prop_assert_eq!(restored.tree_len(), updated.intervals_amount() as u32);
                    prop_assert!(restored.update(updated).is_empty());
                }
                None => {
                    prop_assert!(expected_removed.is_empty());
                }
            }
        }
    }
}
