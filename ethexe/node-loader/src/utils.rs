use std::collections::BTreeSet;

use alloy::{primitives::Address, providers::WalletProvider};
use anyhow::Result;
use ethexe_ethereum::Ethereum;
use gear_call_gen::Seed;
use gear_core_errors::ReplyCode;
use gear_wasm_gen::{
    EntryPointsSet, InvocableSyscall, RegularParamType, StandardGearWasmConfigsBundle, SyscallName,
    SyscallsInjectionTypes, SyscallsParamsConfig,
};
use gprimitives::{ActorId, MessageId};

use crate::batch::{Event, EventKind};

/// Returns configs bundle with a gear wasm generator config, which logs `seed`.
pub fn get_wasm_gen_config(
    seed: Seed,
    _existing_programs: impl Iterator<Item = ActorId>,
) -> StandardGearWasmConfigsBundle {
    let initial_pages = 2;
    let mut injection_types = SyscallsInjectionTypes::all_once();
    injection_types.set_multiple(
        [
            (SyscallName::Leave, 0..=0),
            (SyscallName::Panic, 0..=0),
            (SyscallName::OomPanic, 0..=0),
            (SyscallName::EnvVars, 0..=0),
            (SyscallName::Send, 10..=15),
            (SyscallName::Exit, 0..=1),
            (SyscallName::Alloc, 3..=6),
            (SyscallName::Free, 3..=6),
        ]
        .map(|(syscall, range)| (InvocableSyscall::Loose(syscall), range))
        .into_iter(),
    );

    let params_config = SyscallsParamsConfig::new()
        .with_default_regular_config()
        .with_rule(RegularParamType::Alloc, (1..=10).into())
        .with_rule(
            RegularParamType::Free,
            (initial_pages..=initial_pages + 50).into(),
        );

    StandardGearWasmConfigsBundle {
        log_info: Some(format!("Gear program seed = '{seed}'")),
        entry_points_set: EntryPointsSet::InitHandleHandleReply,
        injection_types,
        params_config,
        initial_pages: initial_pages as u32,
        ..Default::default()
    }
}

pub async fn capture_mailbox_messages(
    api: &Ethereum,
    event_source: &[Event],
) -> Result<BTreeSet<MessageId>> {
    let to: Address = api.provider().default_signer_address();
    let mailbox_messages = event_source.iter().filter_map(|event| match event.kind {
        EventKind::Message(ref msg) if msg.destination == to => Some(MessageId::new(msg.id.0)),
        EventKind::MessageQueueingRequested(ref msg) if msg.source == to => {
            Some(MessageId::new(msg.id.0))
        }
        _ => None,
    });

    Ok(BTreeSet::from_iter(mailbox_messages))
}

/// Check whether processing batch of messages identified by corresponding
/// `message_ids` resulted in errors or has been successful.
///
/// This function returns a vector of statuses with an associated message
/// identifier ([`MessageId`]). Each status can be an error message in case
/// of an error.
pub async fn err_waited_or_succeed_batch(
    event_source: &mut [Event],
    message_ids: impl IntoIterator<Item = MessageId>,
) -> Vec<(MessageId, Option<String>)> {
    let message_ids: Vec<MessageId> = message_ids.into_iter().collect();
    let mut caught_ids = Vec::with_capacity(message_ids.len());

    event_source
        .iter_mut()
        .filter_map(|e| match &e.kind {
            EventKind::Reply(reply) if message_ids.contains(&MessageId::new(reply.replyTo.0)) => {
                caught_ids.push(MessageId::new(reply.replyTo.0));
                Some(vec![(
                    MessageId::new(reply.replyTo.0),
                    (!ReplyCode::from_bytes(reply.replyCode.0).is_success())
                        .then(|| String::from_utf8(reply.payload.to_vec()).expect("Infallible")),
                )])
            }
            EventKind::MessageCallFailed(call)
                if message_ids.contains(&MessageId::new(call.id.0)) =>
            {
                Some(vec![(
                    MessageId::new(call.id.0),
                    Some(format!(
                        "Call to {} failed (value={})",
                        call.destination, call.value
                    )),
                )])
            }

            EventKind::ReplyCallFailed(call) => Some(vec![(
                MessageId::new(call.replyTo.0),
                Some(format!(
                    "Reply failed with: '{}'",
                    ReplyCode::from_bytes(call.replyCode.0)
                )),
            )]),

            EventKind::MessageQueueingRequested(msg)
                if message_ids.contains(&MessageId::new(msg.id.0)) =>
            {
                Some(vec![(MessageId::new(msg.id.0), None)])
            }
            EventKind::ReplyQueueingRequested(msg)
                if message_ids.contains(&MessageId::new(msg.repliedTo.0)) =>
            {
                Some(vec![(MessageId::new(msg.repliedTo.0), None)])
            }
            _ => None,
        })
        .flatten()
        .collect()
}
