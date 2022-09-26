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
        gear_common::event::{CodeChangeKind, Entry, Reason, UserMessageReadRuntimeReason},
        gear_runtime::RuntimeCall,
        pallet_gear::pallet::Call as GearCall,
        sp_weights::weight_v2::Weight,
    },
    utility::Event as UtilityEvent,
    Event,
};
use parity_scale_codec::Encode;
use std::path::PathBuf;
use subxt::sp_core::H256;

impl GearApi {
    /// `pallet_balances::transfer`
    pub async fn transfer(&self, destination: ProgramId, value: u128) -> Result<H256> {
        let destination: [u8; 32] = destination.into();

        let tx = self.0.transfer(destination, value).await?;

        let expected_event = Event::Balances(BalancesEvent::Transfer {
            from: self.0.account_id().clone(),
            to: destination.into(),
            amount: value,
        });

        for event in tx.wait_for_success().await?.iter() {
            if event?.event == expected_event {
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

        let expected_source = self.0.account_id().clone();

        for event in tx.wait_for_success().await?.iter() {
            if let Event::Gear(GearEvent::MessageEnqueued {
                id,
                source,
                destination,
                entry: Entry::Init,
            }) = event?.event
            {
                if source == expected_source {
                    return Ok((id.into(), destination.into(), tx.block_hash()));
                }
            }
        }

        Err(Error::EventNotFound)
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
        let data = self
            .get_from_mailbox(self.0.account_id().clone(), message_id)
            .await?;

        let tx = self.0.claim_value(message_id).await?;

        let events = tx.wait_for_success().await?;

        let (message, _interval) = data.expect("Data appearance guaranteed above");

        for event in events.iter() {
            if let Event::Gear(GearEvent::UserMessageRead { id, reason }) = event?.event {
                if MessageId::from(id) == message.id()
                    && reason == Reason::Runtime(UserMessageReadRuntimeReason::MessageClaimed)
                {
                    return Ok((message.value(), tx.block_hash()));
                }
            }
        }

        Err(Error::EventNotFound)
    }

    /// `pallet_gear::reset`
    pub async fn reset(&self) -> Result<H256> {
        let tx = self.0.reset().await?;

        for event in tx.wait_for_success().await?.iter() {
            if let Event::Gear(GearEvent::DatabaseWiped) = event?.event {
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

        let expected_source = self.0.account_id();
        let expected_destination = destination;

        for event in tx.wait_for_success().await?.iter() {
            if let Event::Gear(GearEvent::MessageEnqueued {
                id,
                source,
                destination,
                entry: Entry::Handle,
            }) = event?.event
            {
                if &source == expected_source
                    && ProgramId::from(destination) == expected_destination
                {
                    return Ok((id.into(), tx.block_hash()));
                }
            }
        }

        Err(Error::EventNotFound)
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

        let expected_source = self.0.account_id();

        let data = self
            .get_from_mailbox(expected_source.clone(), reply_to_id)
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
                source,
                entry: Entry::Reply(replied_on),
                ..
            }) = event?.event
            {
                if &source == expected_source && MessageId::from(replied_on) == reply_to_id {
                    return Ok((id.into(), message.value(), tx.block_hash()));
                }
            }
        }

        Err(Error::EventNotFound)
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
        let code = code.as_ref();
        let expected_code_id = CodeId::generate(code);
        let tx = self.0.upload_code(code.to_vec()).await?;

        for event in tx.wait_for_success().await?.iter() {
            if let Event::Gear(GearEvent::CodeChanged {
                id,
                change: CodeChangeKind::Active { .. },
            }) = event?.event
            {
                if CodeId::from(id) == expected_code_id {
                    return Ok((expected_code_id, tx.block_hash()));
                }
            }
        }

        Err(Error::EventNotFound)
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

        tx.block_hash();

        for event in tx.wait_for_success().await?.iter() {
            if let Event::Gear(GearEvent::MessageEnqueued {
                id,
                source,
                destination,
                entry: Entry::Init,
            }) = event?.event
            {
                if &source == self.0.account_id() {
                    return Ok((id.into(), destination.into(), tx.block_hash()));
                }
            }
        }

        Err(Error::EventNotFound)
    }

    /// `pallet_gear::upload_program` with bytes in payload.
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

        let ex = self.0.tx().utility().force_batch(calls)?;
        let tx = self.0.process(ex).await?;

        let account_id = self.0.account_id();

        let mut res = Vec::with_capacity(amount);

        for event in tx.wait_for_success().await?.iter() {
            match event?.event {
                Event::Gear(GearEvent::MessageEnqueued {
                    id,
                    source,
                    destination,
                    entry: Entry::Init,
                }) => {
                    if &source == account_id {
                        res.push(Ok((id.into(), destination.into())));
                    }
                }
                Event::Utility(UtilityEvent::ItemFailed { error }) => res.push(Err(
                    subxt::GenericError::Runtime(subxt::RuntimeError(error)).into(),
                )),
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
        let ex = self.0.tx().sudo().sudo_unchecked_weight(
            RuntimeCall::System(SystemCall::set_code {
                code: code.as_ref().to_vec(),
            }),
            Weight { ref_time: 0 },
        )?;

        let tx = self.0.process(ex).await?;

        Ok(tx.wait_for_success().await?.block_hash())
    }

    /// `pallet_sudo` && `pallet_system` runtime upgrade by path to runtime.
    pub async fn set_code_by_path(&self, path: impl Into<PathBuf>) -> Result<H256> {
        let code = utils::code_from_os(path)?;
        self.set_code(code).await
    }
}
