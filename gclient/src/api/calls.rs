// This file is part of Gear.

// Copyright (C) 2022 Gear Technologies Inc.
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

use super::{GearApi, Result};
use crate::{utils, Error};
use gear_core::ids::*;
use gp::api::generated::api::{
    balances::Event as BalancesEvent,
    gear::Event as GearEvent,
    runtime_types::{
        frame_system::pallet::Call as SystemCall,
        gear_common::event::{CodeChangeKind, Entry},
        gear_runtime::RuntimeCall,
        pallet_gear::pallet::Call as GearCall,
        sp_weights::weight_v2::Weight,
    },
    tx,
    utility::Event as UtilityEvent,
    Event,
};
use parity_scale_codec::Encode;
use std::{collections::BTreeMap, path::PathBuf};
use subxt::{events::Phase, ext::sp_core::H256};

impl GearApi {
    /// `pallet_balances::transfer`
    pub async fn transfer(&self, destination: ProgramId, value: u128) -> Result<H256> {
        let destination: [u8; 32] = destination.into();

        let tx = self.0.transfer(destination, value).await?;

        for event in tx.wait_for_success().await?.iter() {
            if let Event::Balances(BalancesEvent::Transfer { .. }) =
                event?.as_root_event::<(Phase, Event)>()?.1
            {
                return Ok(tx.block_hash());
            }
        }

        Err(Error::EventNotFound)
    }

    /// `pallet_gear::create_program` with bytes in payload.
    pub async fn create_program_bytes(
        &self,
        code_id: CodeId,
        salt: impl AsRef<[u8]>,
        payload: impl AsRef<[u8]>,
        gas_limit: u64,
        value: u128,
    ) -> Result<(MessageId, ProgramId, H256)> {
        let salt = salt.as_ref().to_vec();
        let payload = payload.as_ref().to_vec();

        let tx = self
            .0
            .create_program(code_id, salt, payload, gas_limit, value)
            .await?;

        for event in tx.wait_for_success().await?.iter() {
            if let Event::Gear(GearEvent::MessageEnqueued {
                id,
                destination,
                entry: Entry::Init,
                ..
            }) = event?.as_root_event::<(Phase, Event)>()?.1
            {
                return Ok((id.into(), destination.into(), tx.block_hash()));
            }
        }

        Err(Error::EventNotFound)
    }

    /// `pallet_gear::create_program` with bytes in payload batched.
    pub async fn create_program_bytes_batch(
        &self,
        args: impl IntoIterator<Item = (CodeId, impl AsRef<[u8]>, impl AsRef<[u8]>, u64, u128)>,
    ) -> Result<(Vec<Result<(MessageId, ProgramId)>>, H256)> {
        let calls: Vec<_> = args
            .into_iter()
            .map(|(code_id, salt, payload, gas_limit, value)| {
                RuntimeCall::Gear(GearCall::create_program {
                    code_id: code_id.into(),
                    salt: salt.as_ref().to_vec(),
                    init_payload: payload.as_ref().to_vec(),
                    gas_limit,
                    value,
                })
            })
            .collect();

        let amount = calls.len();

        let ex = tx().utility().force_batch(calls);
        let tx = self.0.process(ex, "gear", "create_program").await?;

        let mut res = Vec::with_capacity(amount);

        for event in tx.wait_for_success().await?.iter() {
            match event?.as_root_event::<(Phase, Event)>()?.1 {
                Event::Gear(GearEvent::MessageEnqueued {
                    id,
                    destination,
                    entry: Entry::Init,
                    ..
                }) => res.push(Ok((id.into(), destination.into()))),
                Event::Utility(UtilityEvent::ItemFailed { error }) => {
                    res.push(Err(self.0.decode_error(error).into()))
                }
                _ => (),
            }
        }

        if res.len() == amount {
            Ok((res, tx.block_hash()))
        } else {
            Err(Error::IncompleteBatchResult(res.len(), amount))
        }
    }

    /// `pallet_gear::create_program` with `impl Encode` type in payload.
    pub async fn create_program(
        &self,
        code_id: CodeId,
        salt: impl AsRef<[u8]>,
        payload: impl Encode,
        gas_limit: u64,
        value: u128,
    ) -> Result<(MessageId, ProgramId, H256)> {
        self.create_program_bytes(code_id, salt, payload.encode(), gas_limit, value)
            .await
    }

    /// `pallet_gear::claim_value`
    pub async fn claim_value(&self, message_id: MessageId) -> Result<(u128, H256)> {
        let value = self
            .get_from_mailbox(self.0.account_id(), message_id)
            .await?
            .map(|(message, _interval)| message.value());

        let tx = self.0.claim_value(message_id).await?;

        for event in tx.wait_for_success().await?.iter() {
            if let Event::Gear(GearEvent::UserMessageRead { .. }) =
                event?.as_root_event::<(Phase, Event)>()?.1
            {
                return Ok((
                    value.expect("Data appearance guaranteed above"),
                    tx.block_hash(),
                ));
            }
        }

        Err(Error::EventNotFound)
    }

    /// `pallet_gear::claim_value` batched.
    pub async fn claim_value_batch(
        &self,
        args: impl IntoIterator<Item = MessageId>,
    ) -> Result<(Vec<Result<u128>>, H256)> {
        let mut message_ids = args.into_iter();
        let mut values = BTreeMap::new();

        for message_id in message_ids.by_ref() {
            values.insert(
                message_id,
                self.get_from_mailbox(self.0.account_id(), message_id)
                    .await?
                    .map(|(message, _interval)| message.value()),
            );
        }

        let calls: Vec<_> = message_ids
            .map(|message_id| {
                RuntimeCall::Gear(GearCall::claim_value {
                    message_id: message_id.into(),
                })
            })
            .collect();

        let amount = calls.len();

        let ex = tx().utility().force_batch(calls);
        let tx = self.0.process(ex, "gear", "claim_value").await?;

        let mut res = Vec::with_capacity(amount);

        for event in tx.wait_for_success().await?.iter() {
            match event?.as_root_event::<(Phase, Event)>()?.1 {
                Event::Gear(GearEvent::UserMessageRead { id, .. }) => res.push(Ok(values
                    .remove(&id.into())
                    .flatten()
                    .expect("Data appearance guaranteed above"))),
                Event::Utility(UtilityEvent::ItemFailed { error }) => {
                    res.push(Err(self.0.decode_error(error).into()))
                }
                _ => (),
            }
        }

        if res.len() == amount {
            Ok((res, tx.block_hash()))
        } else {
            Err(Error::IncompleteBatchResult(res.len(), amount))
        }
    }

    /// `pallet_gear::reset`
    pub async fn reset(&self) -> Result<H256> {
        let tx = self.0.reset().await?;

        for event in tx.wait_for_success().await?.iter() {
            if let Event::Gear(GearEvent::DatabaseWiped) =
                event?.as_root_event::<(Phase, Event)>()?.1
            {
                return Ok(tx.block_hash());
            }
        }

        Err(Error::EventNotFound)
    }

    /// `pallet_gear::send_message` with bytes in payload.
    pub async fn send_message_bytes(
        &self,
        destination: ProgramId,
        payload: impl AsRef<[u8]>,
        gas_limit: u64,
        value: u128,
    ) -> Result<(MessageId, H256)> {
        let payload = payload.as_ref().to_vec();

        let tx = self
            .0
            .send_message(destination, payload, gas_limit, value)
            .await?;

        for event in tx.wait_for_success().await?.iter() {
            if let Event::Gear(GearEvent::MessageEnqueued {
                id,
                entry: Entry::Handle,
                ..
            }) = event?.as_root_event::<(Phase, Event)>()?.1
            {
                return Ok((id.into(), tx.block_hash()));
            }
        }

        Err(Error::EventNotFound)
    }

    /// `pallet_gear::send_message` with bytes in payload batched.
    pub async fn send_message_bytes_batch(
        &self,
        args: impl IntoIterator<Item = (ProgramId, impl AsRef<[u8]>, u64, u128)>,
    ) -> Result<(Vec<Result<(MessageId, ProgramId)>>, H256)> {
        let calls: Vec<_> = args
            .into_iter()
            .map(|(destination, payload, gas_limit, value)| {
                RuntimeCall::Gear(GearCall::send_message {
                    destination: destination.into(),
                    payload: payload.as_ref().to_vec(),
                    gas_limit,
                    value,
                })
            })
            .collect();

        let amount = calls.len();

        let ex = tx().utility().force_batch(calls);
        let tx = self.0.process(ex, "gear", "send_message").await?;

        let mut res = Vec::with_capacity(amount);

        for event in tx.wait_for_success().await?.iter() {
            match event?.as_root_event::<(Phase, Event)>()?.1 {
                Event::Gear(GearEvent::MessageEnqueued {
                    id,
                    destination,
                    entry: Entry::Handle,
                    ..
                }) => res.push(Ok((id.into(), destination.into()))),
                Event::Utility(UtilityEvent::ItemFailed { error }) => {
                    res.push(Err(self.0.decode_error(error).into()))
                }
                _ => (),
            }
        }

        if res.len() == amount {
            Ok((res, tx.block_hash()))
        } else {
            Err(Error::IncompleteBatchResult(res.len(), amount))
        }
    }

    /// `pallet_gear::send_message` with `impl Encode` type in payload.
    pub async fn send_message(
        &self,
        destination: ProgramId,
        payload: impl Encode,
        gas_limit: u64,
        value: u128,
    ) -> Result<(MessageId, H256)> {
        self.send_message_bytes(destination, payload.encode(), gas_limit, value)
            .await
    }

    /// `pallet_gear::send_reply` with bytes in payload.
    pub async fn send_reply_bytes(
        &self,
        reply_to_id: MessageId,
        payload: impl AsRef<[u8]>,
        gas_limit: u64,
        value: u128,
    ) -> Result<(MessageId, u128, H256)> {
        let payload = payload.as_ref().to_vec();

        let data = self
            .get_from_mailbox(self.0.account_id(), reply_to_id)
            .await?;

        let tx = self
            .0
            .send_reply(reply_to_id, payload, gas_limit, value)
            .await?;

        let events = tx.wait_for_success().await?;

        let (message, _interval) = data.expect("Data appearance guaranteed above");

        for event in events.iter() {
            if let Event::Gear(GearEvent::MessageEnqueued {
                id,
                entry: Entry::Reply(_),
                ..
            }) = event?.as_root_event::<(Phase, Event)>()?.1
            {
                return Ok((id.into(), message.value(), tx.block_hash()));
            }
        }

        Err(Error::EventNotFound)
    }

    /// `pallet_gear::send_reply` with bytes in payload.
    pub async fn send_reply_bytes_batch(
        &self,
        args: impl IntoIterator<Item = (MessageId, impl AsRef<[u8]>, u64, u128)>,
    ) -> Result<(Vec<Result<(MessageId, u128)>>, H256)> {
        let mut args = args.into_iter();
        let mut values = BTreeMap::new();

        for (message_id, _, _, _) in args.by_ref() {
            values.insert(
                message_id,
                self.get_from_mailbox(self.0.account_id(), message_id)
                    .await?
                    .map(|(message, _interval)| message.value()),
            );
        }

        let calls: Vec<_> = args
            .map(|(reply_to_id, payload, gas_limit, value)| {
                RuntimeCall::Gear(GearCall::send_reply {
                    reply_to_id: reply_to_id.into(),
                    payload: payload.as_ref().to_vec(),
                    gas_limit,
                    value,
                })
            })
            .collect();

        let amount = calls.len();

        let ex = tx().utility().force_batch(calls);
        let tx = self.0.process(ex, "gear", "send_reply_bytes").await?;

        let mut res = Vec::with_capacity(amount);

        for event in tx.wait_for_success().await?.iter() {
            match event?.as_root_event::<(Phase, Event)>()?.1 {
                Event::Gear(GearEvent::MessageEnqueued {
                    id,
                    entry: Entry::Reply(reply_to_id),
                    ..
                }) => res.push(Ok((
                    id.into(),
                    values
                        .remove(&reply_to_id.into())
                        .flatten()
                        .expect("Data appearance guaranteed above"),
                ))),
                Event::Utility(UtilityEvent::ItemFailed { error }) => {
                    res.push(Err(self.0.decode_error(error).into()))
                }
                _ => (),
            }
        }

        if res.len() == amount {
            Ok((res, tx.block_hash()))
        } else {
            Err(Error::IncompleteBatchResult(res.len(), amount))
        }
    }

    /// `pallet_gear::send_reply` with `impl Encode` type in payload.
    pub async fn send_reply(
        &self,
        reply_to_id: MessageId,
        payload: impl Encode,
        gas_limit: u64,
        value: u128,
    ) -> Result<(MessageId, u128, H256)> {
        self.send_reply_bytes(reply_to_id, payload.encode(), gas_limit, value)
            .await
    }

    /// `pallet_gear::upload_code`
    pub async fn upload_code(&self, code: impl AsRef<[u8]>) -> Result<(CodeId, H256)> {
        let tx = self.0.upload_code(code.as_ref().to_vec()).await?;

        for event in tx.wait_for_success().await?.iter() {
            if let Event::Gear(GearEvent::CodeChanged {
                id,
                change: CodeChangeKind::Active { .. },
            }) = event?.as_root_event::<(Phase, Event)>()?.1
            {
                return Ok((id.into(), tx.block_hash()));
            }
        }

        Err(Error::EventNotFound)
    }

    /// `pallet_gear::upload_code` batched.
    pub async fn upload_code_batch(
        &self,
        args: impl IntoIterator<Item = impl AsRef<[u8]>>,
    ) -> Result<(Vec<Result<CodeId>>, H256)> {
        let calls: Vec<_> = args
            .into_iter()
            .map(|code| {
                RuntimeCall::Gear(GearCall::upload_code {
                    code: code.as_ref().to_vec(),
                })
            })
            .collect();

        let amount = calls.len();

        let ex = tx().utility().force_batch(calls);
        let tx = self.0.process(ex, "gear", "upload_code").await?;

        let mut res = Vec::with_capacity(amount);

        for event in tx.wait_for_success().await?.iter() {
            match event?.as_root_event::<(Phase, Event)>()?.1 {
                Event::Gear(GearEvent::CodeChanged {
                    id,
                    change: CodeChangeKind::Active { .. },
                }) => {
                    res.push(Ok(id.into()));
                }
                Event::Utility(UtilityEvent::ItemFailed { error }) => {
                    res.push(Err(self.0.decode_error(error).into()))
                }
                _ => (),
            }
        }

        if res.len() == amount {
            Ok((res, tx.block_hash()))
        } else {
            Err(Error::IncompleteBatchResult(res.len(), amount))
        }
    }

    /// `pallet_gear::upload_code` from os path.
    ///
    /// This works with absolute and relative paths (relatively root dir of the repo).
    pub async fn upload_code_by_path(&self, path: impl Into<PathBuf>) -> Result<(CodeId, H256)> {
        let code = utils::code_from_os(path)?;
        self.upload_code(code).await
    }

    /// `pallet_gear::upload_program` with bytes in payload.
    pub async fn upload_program_bytes(
        &self,
        code: impl AsRef<[u8]>,
        salt: impl AsRef<[u8]>,
        payload: impl AsRef<[u8]>,
        gas_limit: u64,
        value: u128,
    ) -> Result<(MessageId, ProgramId, H256)> {
        let code = code.as_ref().to_vec();
        let salt = salt.as_ref().to_vec();
        let payload = payload.as_ref().to_vec();

        let tx = self
            .0
            .upload_program(code, salt, payload, gas_limit, value)
            .await?;

        for event in tx.wait_for_success().await?.iter() {
            if let Event::Gear(GearEvent::MessageEnqueued {
                id,
                destination,
                entry: Entry::Init,
                ..
            }) = event?.as_root_event::<(Phase, Event)>()?.1
            {
                return Ok((id.into(), destination.into(), tx.block_hash()));
            }
        }

        Err(Error::EventNotFound)
    }

    /// `pallet_gear::upload_program` with bytes in payload batched.
    pub async fn upload_program_bytes_batch(
        &self,
        args: impl IntoIterator<
            Item = (
                impl AsRef<[u8]>,
                impl AsRef<[u8]>,
                impl AsRef<[u8]>,
                u64,
                u128,
            ),
        >,
    ) -> Result<(Vec<Result<(MessageId, ProgramId)>>, H256)> {
        let calls: Vec<_> = args
            .into_iter()
            .map(|(code, salt, payload, gas_limit, value)| {
                RuntimeCall::Gear(GearCall::upload_program {
                    code: code.as_ref().to_vec(),
                    salt: salt.as_ref().to_vec(),
                    init_payload: payload.as_ref().to_vec(),
                    gas_limit,
                    value,
                })
            })
            .collect();

        let amount = calls.len();

        let ex = tx().utility().force_batch(calls);
        let tx = self.0.process(ex, "gear", "upload_program").await?;

        let mut res = Vec::with_capacity(amount);

        for event in tx.wait_for_success().await?.iter() {
            match event?.as_root_event::<(Phase, Event)>()?.1 {
                Event::Gear(GearEvent::MessageEnqueued {
                    id,
                    destination,
                    entry: Entry::Init,
                    ..
                }) => res.push(Ok((id.into(), destination.into()))),
                Event::Utility(UtilityEvent::ItemFailed { error }) => {
                    res.push(Err(self.0.decode_error(error).into()))
                }
                _ => (),
            }
        }

        if res.len() == amount {
            Ok((res, tx.block_hash()))
        } else {
            Err(Error::IncompleteBatchResult(res.len(), amount))
        }
    }

    /// `pallet_gear::upload_program` with bytes in payload and code from os.
    pub async fn upload_program_bytes_by_path(
        &self,
        path: impl Into<PathBuf>,
        salt: impl AsRef<[u8]>,
        payload: impl AsRef<[u8]>,
        gas_limit: u64,
        value: u128,
    ) -> Result<(MessageId, ProgramId, H256)> {
        let code = utils::code_from_os(path)?;
        self.upload_program_bytes(code, salt, payload, gas_limit, value)
            .await
    }

    /// `pallet_gear::upload_program` with `impl Encode` type in payload.
    pub async fn upload_program(
        &self,
        code: impl AsRef<[u8]>,
        salt: impl AsRef<[u8]>,
        payload: impl Encode,
        gas_limit: u64,
        value: u128,
    ) -> Result<(MessageId, ProgramId, H256)> {
        self.upload_program_bytes(code, salt, payload.encode(), gas_limit, value)
            .await
    }

    /// `pallet_gear::upload_program` with `impl Encode` type in payload and code from os.
    pub async fn upload_program_by_path(
        &self,
        path: impl Into<PathBuf>,
        salt: impl AsRef<[u8]>,
        payload: impl Encode,
        gas_limit: u64,
        value: u128,
    ) -> Result<(MessageId, ProgramId, H256)> {
        let code = utils::code_from_os(path)?;
        self.upload_program(code, salt, payload, gas_limit, value)
            .await
    }

    /// `pallet_sudo` && `pallet_system` runtime upgrade.
    pub async fn set_code(&self, code: impl AsRef<[u8]>) -> Result<H256> {
        let ex = tx().sudo().sudo_unchecked_weight(
            RuntimeCall::System(SystemCall::set_code {
                code: code.as_ref().to_vec(),
            }),
            Weight {
                ref_time: 0,
                // # TODO
                //
                // Check this field
                proof_size: Default::default(),
            },
        );

        let tx = self.0.process(ex, "sudo", "set_code").await?;

        Ok(tx.wait_for_success().await?.block_hash())
    }

    /// `pallet_sudo` && `pallet_system` runtime upgrade by path to runtime.
    pub async fn set_code_by_path(&self, path: impl Into<PathBuf>) -> Result<H256> {
        let code = utils::code_from_os(path)?;
        self.set_code(code).await
    }
}
