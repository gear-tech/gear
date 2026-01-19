use std::collections::BTreeSet;

use alloy::{network::NetworkWallet, primitives::Address, providers::WalletProvider};
use anyhow::Result;
use ethexe_common::gear::Message;
use ethexe_ethereum::Ethereum;
use gear_call_gen::Seed;
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
        _ => None,
    });

    Ok(BTreeSet::from_iter(mailbox_messages))
}
