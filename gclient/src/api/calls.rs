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
use std::{collections::BTreeMap, path::Path};
use subxt::{events::Phase, ext::sp_core::H256};

impl GearApi {
    /// Transfer `value` to `destination`'s account.
    ///
    /// Sends the
    /// [`pallet_balances::transfer`](https://crates.parity.io/pallet_balances/pallet/struct.Pallet.html#method.transfer)
    /// extrinsic.
    ///
    /// This function returns a hash of the block with the transfer transaction.
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

    /// Create a new program from a previously uploaded code identified by
    /// [`CodeId`](https://docs.gear.rs/gear_core/ids/struct.CodeId.html) and
    /// initialize it with a byte slice `payload`.
    ///
    /// Sends the
    /// [`pallet_gear::create_program`](https://docs.gear.rs/pallet_gear/pallet/struct.Pallet.html#method.create_program)
    /// extrinsic.
    ///
    /// Parameters:
    ///
    /// - `code_id` is the code identifier that can be obtained by calling the
    ///   [`upload_code`](Self::upload_code) function;
    /// - `salt` is the arbitrary data needed to generate an address for a new
    ///   program (control of salt uniqueness is entirely on the function
    ///   caller’s side);
    /// - `payload` vector contains data to be processed in the `init` function
    ///   of the newly deployed "child" program;
    /// - `gas_limit` is the maximum gas amount allowed to spend for the program
    ///   creation and initialization;
    /// - `value` to be transferred to the program's account during
    ///   initialization.
    ///
    /// This function returns a tuple with an init message identifier, newly
    /// created program identifier, and a hash of the block with the message
    /// enqueuing transaction.
    ///
    /// # See also
    ///
    /// - [`create_program`](Self::create_program) function initializes a newly
    ///   created program with an encoded payload.
    /// - [`create_program_bytes_batch`](Self::create_program_bytes_batch)
    ///   function creates a batch of programs and initializes them.
    /// - [`upload_code`](Self::upload_code) function uploads a code and returns
    ///   its identifier.
    /// - [`upload_program_bytes`](Self::upload_program) function uploads a new
    ///   program and initialize it.
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

    /// Create a batch of programs.
    ///
    /// Same as [`create_program_bytes`](Self::create_program_bytes), but sends
    /// a batch of extrinsics.
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
        let tx = self.0.process(ex).await?;

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

    /// Create a new program from a previously uploaded code identified by
    /// [`CodeId`](https://docs.gear.rs/gear_core/ids/struct.CodeId.html) and
    /// initialize it with an encoded `payload`.
    ///
    /// Sends the
    /// [`pallet_gear::create_program`](https://docs.gear.rs/pallet_gear/pallet/struct.Pallet.html#method.create_program)
    /// extrinsic.
    ///
    /// Parameters:
    ///
    /// - `code_id` is the code identifier that can be obtained by calling the
    ///   [`upload_code`](Self::upload_code) function;
    /// - `salt` is the arbitrary data needed to generate an address for a new
    ///   program (control of salt uniqueness is entirely on the function
    ///   caller’s side);
    /// - `payload` contains encoded data to be processed in the `init` function
    ///   of the newly deployed "child" program;
    /// - `gas_limit` is the maximum gas amount allowed to spend for the program
    ///   creation and initialization;
    /// - `value` to be transferred to the program's account during
    ///   initialization.
    ///
    /// # See also
    ///
    /// - [`create_program_bytes`](Self::create_program_bytes) function
    ///   initializes a newly created program with a byte slice payload.
    /// - [`upload_code`](Self::upload_code) function uploads a code and returns
    ///   its identifier.
    /// - [`upload_program`](Self::upload_program) function uploads a new
    ///   program and initialize it.
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

    /// Claim value from the mailbox message identified by `message_id`.
    ///
    /// Sends the
    /// [`pallet_gear::claim_value`](https://docs.gear.rs/pallet_gear/pallet/struct.Pallet.html#method.claim_value)
    /// extrinsic.
    ///
    /// This function returns a tuple with value and block hash containing the
    /// corresponding transaction.
    pub async fn claim_value(&self, message_id: MessageId) -> Result<(u128, H256)> {
        let value = self
            .get_from_mailbox(message_id)
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

    /// Claim a batch of values from the mailbox.
    ///
    /// Same as [`claim_value`](Self::claim_value), but sends a batch of
    /// extrinsics.
    pub async fn claim_value_batch(
        &self,
        args: impl IntoIterator<Item = MessageId>,
    ) -> Result<(Vec<Result<u128>>, H256)> {
        let mut message_ids = args.into_iter();
        let mut values = BTreeMap::new();

        for message_id in message_ids.by_ref() {
            values.insert(
                message_id,
                self.get_from_mailbox(message_id)
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
        let tx = self.0.process(ex).await?;

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

    /// Clear data from all pallet storages.
    ///
    /// Sends the
    /// [`pallet_gear::reset`](https://docs.gear.rs/pallet_gear/pallet/struct.Pallet.html#method.reset)
    /// extrinsic.
    ///
    /// This function returns a hash of the block with the reset transaction.
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

    /// Send a message containing a byte slice `payload` to the `destination`.
    ///
    /// The message also contains the maximum `gas_limit` that can be spent and
    /// the `value` to be transferred to the `destination`'s account.
    ///
    /// Sends the
    /// [`pallet_gear::send_message`](https://docs.gear.rs/pallet_gear/pallet/struct.Pallet.html#method.send_message)
    /// extrinsic.
    ///
    /// This function returns a tuple with a new message identifier and a hash
    /// of the block with the message enqueuing transaction.
    ///
    /// # See also
    ///
    /// - [`send_message`](Self::send_message) function sends a message with an
    ///   encoded payload.
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

    /// Send a batch of messages.
    ///
    /// Same as [`send_message_bytes`](Self::send_message_bytes), but sends a
    /// batch of extrinsics.
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
        let tx = self.0.process(ex).await?;

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

    /// Send a message containing an encoded `payload` to the `destination`.
    ///
    /// The message also contains the maximum `gas_limit` that can be spent and
    /// the `value` to be transferred to the `destination`'s account.
    ///
    /// Sends the
    /// [`pallet_gear::send_message`](https://docs.gear.rs/pallet_gear/pallet/struct.Pallet.html#method.send_message)
    /// extrinsic.
    ///
    /// This function returns a tuple with a new message identifier and a hash
    /// of the block with the message enqueuing transaction.
    ///
    /// # See also
    ///
    /// - [`send_message_bytes`](Self::send_message_bytes) function sends a
    ///   message with a byte slice payload.
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

    /// Send a reply containing a byte slice `payload` to the message identified
    /// by `reply_to_id`.
    ///
    /// The reply also contains the maximum `gas_limit` that can be spent and
    /// the `value` to be transferred to the destination's account.
    ///
    /// Sends the
    /// [`pallet_gear::send_reply`](https://docs.gear.rs/pallet_gear/pallet/struct.Pallet.html#method.send_reply)
    /// extrinsic.
    ///
    /// This function returns a tuple with a new message identifier, transferred
    /// value, and a hash of the block with the message enqueuing transaction.
    ///
    /// # See also
    ///
    /// - [`send_reply`](Self::send_reply) function sends a reply with an
    ///   encoded payload.
    pub async fn send_reply_bytes(
        &self,
        reply_to_id: MessageId,
        payload: impl AsRef<[u8]>,
        gas_limit: u64,
        value: u128,
    ) -> Result<(MessageId, u128, H256)> {
        let payload = payload.as_ref().to_vec();

        let data = self.get_from_mailbox(reply_to_id).await?;

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

    /// Send a batch of replies.
    ///
    /// Same as [`send_reply_bytes`](Self::send_reply_bytes), but sends a batch
    /// of extrinsics.
    pub async fn send_reply_bytes_batch(
        &self,
        args: impl IntoIterator<Item = (MessageId, impl AsRef<[u8]>, u64, u128)>,
    ) -> Result<(Vec<Result<(MessageId, u128)>>, H256)> {
        let mut args = args.into_iter();
        let mut values = BTreeMap::new();

        for (message_id, _, _, _) in args.by_ref() {
            values.insert(
                message_id,
                self.get_from_mailbox(message_id)
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
        let tx = self.0.process(ex).await?;

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

    /// Send a reply containing an encoded `payload` to the message identified
    /// by `reply_to_id`.
    ///
    /// The reply also contains the maximum `gas_limit` that can be spent and
    /// the `value` to be transferred to the destination's account.
    ///
    /// Sends the
    /// [`pallet_gear::send_reply`](https://docs.gear.rs/pallet_gear/pallet/struct.Pallet.html#method.send_reply)
    /// extrinsic.
    ///
    /// This function returns a tuple with a new message identifier, transferred
    /// value, and a hash of the block with the message enqueuing transaction.
    ///
    /// # See also
    ///
    /// - [`send_reply_bytes`](Self::send_reply_bytes) function sends a reply
    ///   with a byte slice payload.
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

    /// Upload Wasm `code` to be used for creating a new program.
    ///
    /// Sends the
    /// [`pallet_gear::upload_code`](https://docs.gear.rs/pallet_gear/pallet/struct.Pallet.html#method.upload_code)
    /// extrinsic.
    ///
    /// This function returns a tuple with a code identifier and a hash of the
    /// block with the code uploading transaction. The code identifier can be
    /// used when creating a program using the
    /// [`create_program`](Self::create_program) function.
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

    /// Upload a batch of codes.
    ///
    /// Same as [`upload_code`](Self::upload_code), but sends a batch of
    /// extrinsics.
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
        let tx = self.0.process(ex).await?;

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

    /// Upload Wasm code from the file referenced by `path` to be used for
    /// creating a new program.
    ///
    /// Same as [`upload_code`](Self::upload_code), but reads the code from a
    /// file instead of using a byte vector.
    ///
    /// Works with absolute and relative paths (relative to the root dir of the
    /// repo).
    pub async fn upload_code_by_path(&self, path: impl AsRef<Path>) -> Result<(CodeId, H256)> {
        let code = utils::code_from_os(path)?;
        self.upload_code(code).await
    }

    /// Upload a new program and initialize it with a byte slice `payload`.
    ///
    /// Sends the
    /// [`pallet_gear::upload_program`](https://docs.gear.rs/pallet_gear/pallet/struct.Pallet.html#method.upload_program)
    /// extrinsic.
    ///
    /// Parameters:
    ///
    /// - `code` is the byte slice containing a binary Wasm code of the program;
    /// - `salt` is the arbitrary data needed to generate an address for a new
    ///   program (control of salt uniqueness is entirely on the function
    ///   caller’s side);
    /// - `payload` vector contains data to be processed in the `init` function
    ///   of the newly deployed "child" program;
    /// - `gas_limit` is the maximum gas amount allowed to spend for the program
    ///   creation and initialization;
    /// - `value` to be transferred to the program's account during
    ///   initialization.
    ///
    /// # See also
    ///
    /// - [`create_program_bytes`](Self::create_program_bytes) function creates
    ///   a program from a previously uploaded code.
    /// - [`upload_code`](Self::upload_code) function uploads a code and returns
    ///   its identifier.
    /// - [`upload_program`](Self::upload_program) function uploads a program
    ///   and initializes it with an encoded payload.
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

    /// Upload a batch of programs.
    ///
    /// Same as [`upload_program_bytes`](Self::upload_program_bytes), but sends
    /// a batch of extrinsics.
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
        let tx = self.0.process(ex).await?;

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

    /// Upload a new program from the file referenced by `path` and initialize
    /// it with a byte slice `payload`.
    ///
    /// Same as [`upload_program_bytes`](Self::upload_program_bytes), but reads
    /// the program from a file instead of using a byte vector.
    ///
    /// Works with absolute and relative paths (relative to the root dir of the
    /// repo).
    pub async fn upload_program_bytes_by_path(
        &self,
        path: impl AsRef<Path>,
        salt: impl AsRef<[u8]>,
        payload: impl AsRef<[u8]>,
        gas_limit: u64,
        value: u128,
    ) -> Result<(MessageId, ProgramId, H256)> {
        let code = utils::code_from_os(path)?;
        self.upload_program_bytes(code, salt, payload, gas_limit, value)
            .await
    }

    /// Upload a new program and initialize it with an encoded `payload`.
    ///
    /// Sends the
    /// [`pallet_gear::upload_program`](https://docs.gear.rs/pallet_gear/pallet/struct.Pallet.html#method.upload_program)
    /// extrinsic.
    ///
    /// Parameters:
    ///
    /// - `code` is the byte slice containing a binary Wasm code of the program;
    /// - `salt` is the arbitrary data needed to generate an address for a new
    ///   program (control of salt uniqueness is entirely on the function
    ///   caller’s side);
    /// - `payload` contains encoded data to be processed in the `init` function
    ///   of the newly deployed "child" program;
    /// - `gas_limit` is the maximum gas amount allowed to spend for the program
    ///   creation and initialization;
    /// - `value` to be transferred to the program's account during
    ///   initialization.
    ///
    /// # See also
    ///
    /// - [`create_program_bytes`](Self::create_program_bytes) function creates
    ///   a program from a previously uploaded code.
    /// - [`upload_code`](Self::upload_code) function uploads a code and returns
    ///   its identifier.
    /// - [`upload_program_bytes`](Self::upload_program_bytes) function uploads
    ///   a program and initializes it with a byte slice payload.
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

    /// Upload a new program from the file referenced by `path` and initialize
    /// it with an encoded `payload`.
    ///
    /// Same as [`upload_program`](Self::upload_program), but reads the program
    /// from a file instead of using a byte vector.
    ///
    /// Works with absolute and relative paths (relative to the root dir of the
    /// repo).
    pub async fn upload_program_by_path(
        &self,
        path: impl AsRef<Path>,
        salt: impl AsRef<[u8]>,
        payload: impl Encode,
        gas_limit: u64,
        value: u128,
    ) -> Result<(MessageId, ProgramId, H256)> {
        let code = utils::code_from_os(path)?;
        self.upload_program(code, salt, payload, gas_limit, value)
            .await
    }

    /// Upgrade the runtime with the `code` containing the Wasm code of the new
    /// runtime.
    ///
    /// Sends the
    /// [`pallet_system::set_code`](https://crates.parity.io/frame_system/pallet/struct.Pallet.html#method.set_code)
    /// extrinsic.
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

        let tx = self.0.process(ex).await?;

        Ok(tx.wait_for_success().await?.block_hash())
    }

    /// Upgrade the runtime by reading the code from the file located at the
    /// `path`.
    ///
    /// Sends the
    /// [`pallet_system::set_code`](https://crates.parity.io/frame_system/pallet/struct.Pallet.html#method.set_code)
    /// extrinsic.
    pub async fn set_code_by_path(&self, path: impl AsRef<Path>) -> Result<H256> {
        let code = utils::code_from_os(path)?;
        self.set_code(code).await
    }
}
