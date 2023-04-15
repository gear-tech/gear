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
use crate::{api::storage::account_id::IntoAccountId32, utils, Error};
use gear_core::ids::*;
use gsdk::{
    ext::{
        sp_core::H256,
        sp_runtime::{AccountId32, MultiAddress},
    },
    metadata::{
        balances::Event as BalancesEvent,
        gear::Event as GearEvent,
        runtime_types::{
            frame_system::pallet::Call as SystemCall,
            gear_common::event::{CodeChangeKind, MessageEntry},
            gear_runtime::RuntimeCall,
            pallet_balances::{pallet::Call as BalancesCall, AccountData},
            pallet_gear::pallet::Call as GearCall,
            sp_weights::weight_v2::Weight,
        },
        utility::Event as UtilityEvent,
        Event,
    },
    types, Error as GsdkError,
};
use hex::ToHex;
use parity_scale_codec::Encode;
use std::{
    collections::{BTreeMap, HashSet},
    path::Path,
};

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
                event?.as_root_event::<Event>()?
            {
                return Ok(tx.block_hash());
            }
        }

        // Sending zero value is a no-op, so now event occurres.
        if value == 0 {
            return Ok(tx.block_hash());
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
    /// - [`upload_program_bytes`](Self::upload_program_bytes) function uploads
    ///   a new program and initialize it.
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
            if let Event::Gear(GearEvent::MessageQueued {
                id,
                destination,
                entry: MessageEntry::Init,
                ..
            }) = event?.as_root_event::<Event>()?
            {
                return Ok((id.into(), destination.into(), tx.block_hash()));
            }
        }

        Err(Error::EventNotFound)
    }

    /// Create a batch of programs.
    ///
    /// A batch is a set of programs to be created within one function call.
    /// Every entry of the `args` iterator is a tuple of parameters used in the
    /// [`create_program_bytes`](Self::create_program_bytes) function. It is
    /// useful when deploying a multi-program dApp.
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

        let tx = self.0.force_batch(calls).await?;
        let mut res = Vec::with_capacity(amount);

        for event in tx.wait_for_success().await?.iter() {
            match event?.as_root_event::<Event>()? {
                Event::Gear(GearEvent::MessageQueued {
                    id,
                    destination,
                    entry: MessageEntry::Init,
                    ..
                }) => res.push(Ok((id.into(), destination.into()))),
                Event::Utility(UtilityEvent::ItemFailed { error }) => {
                    res.push(Err(self.0.api().decode_error(error).into()))
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

    /// Same as [`create_program_bytes`](Self::create_program_bytes), but
    /// initializes a newly created program with an encoded `payload`.
    ///
    /// # See also
    ///
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

    /// Migrates an active program identified by `src_program_id` onto another
    /// node identified by `dest_node_api` and returns the migrated program
    /// identifier. All source program data is taken at the time of
    /// `src_block_hash` if it is specified or the most recent one.
    pub async fn migrate_program(
        &self,
        src_program_id: ProgramId,
        src_block_hash: Option<H256>,
        dest_node_api: &GearApi,
    ) -> Result<ProgramId> {
        if dest_node_api.0.api().gprog(src_program_id).await.is_ok() {
            return Err(Error::ProgramAlreadyExists(
                src_program_id.as_ref().encode_hex(),
            ));
        }

        let mut src_block_hash = src_block_hash;
        if src_block_hash.is_none() {
            src_block_hash = Some(self.last_block_hash().await?);
        }

        let dest_program_id = src_program_id;

        // Collect data from the source program
        let src_program_account_data = self
            .account_data_at(src_program_id, src_block_hash)
            .await
            .or_else(|e| {
            if let Error::GearSDK(GsdkError::StorageNotFound) = e {
                Ok(AccountData {
                    free: 0u128,
                    reserved: 0,
                    misc_frozen: 0,
                    fee_frozen: 0,
                })
            } else {
                Err(e)
            }
        })?;

        let src_program = self
            .0
            .api()
            .gprog_at(src_program_id, src_block_hash)
            .await?;

        let src_program_pages = self
            .0
            .api()
            .gpages_at(src_program_id, &src_program, src_block_hash)
            .await?;

        let src_program_reserved_gas_node_ids: Vec<types::GearGasNodeId> = src_program
            .gas_reservation_map
            .iter()
            .map(|gr| gr.0.into())
            .collect();

        let src_program_reserved_gas_nodes = self
            .0
            .api()
            .gas_nodes_at(&src_program_reserved_gas_node_ids, src_block_hash)
            .await?;

        let mut src_program_reserved_gas_total = 0u64;
        let mut accounts_with_reserved_funds = HashSet::new();
        for gas_node in &src_program_reserved_gas_nodes {
            if let types::GearGasNode::Reserved {
                id, value, lock, ..
            } = &gas_node.1
            {
                accounts_with_reserved_funds.insert(id);
                src_program_reserved_gas_total += value + lock;
            } else {
                unreachable!("Unexpected gas node type");
            }
        }

        let src_code_id = src_program.code_hash.0.into();

        let src_code_len = self
            .0
            .api()
            .code_len_storage_at(src_code_id, src_block_hash)
            .await?;

        let src_code = self
            .0
            .api()
            .code_storage_at(src_code_id, src_block_hash)
            .await?;

        // Apply data to the target program
        dest_node_api
            .set_balance(
                dest_program_id.into_account_id(),
                src_program_account_data.free,
                src_program_account_data.reserved,
            )
            .await?;

        dest_node_api
            .0
            .set_code_storage(src_code_id, &src_code)
            .await?;

        dest_node_api
            .0
            .set_code_len_storage(src_code_id, src_code_len)
            .await?;

        dest_node_api
            .0
            .set_gas_nodes(&src_program_reserved_gas_nodes)
            .await?;

        for account_with_reserved_funds in accounts_with_reserved_funds {
            let src_account_data = self
                .account_data_at(account_with_reserved_funds, src_block_hash)
                .await?;
            let dest_account_data = dest_node_api
                .account_data(account_with_reserved_funds)
                .await
                .or_else(|e| {
                    if let Error::GearSDK(GsdkError::StorageNotFound) = e {
                        Ok(AccountData {
                            free: 0u128,
                            reserved: 0,
                            misc_frozen: 0,
                            fee_frozen: 0,
                        })
                    } else {
                        Err(e)
                    }
                })?;
            dest_node_api
                .set_balance(
                    account_with_reserved_funds.into_account_id(),
                    dest_account_data.free,
                    dest_account_data
                        .reserved
                        .saturating_add(src_account_data.reserved),
                )
                .await?;
        }

        let dest_gas_total_issuance =
            dest_node_api.0.api().total_issuance().await.or_else(|e| {
                if let GsdkError::StorageNotFound = e {
                    Ok(0)
                } else {
                    Err(e)
                }
            })?;

        dest_node_api
            .0
            .set_total_issuance(
                dest_gas_total_issuance.saturating_add(src_program_reserved_gas_total),
            )
            .await?;

        dest_node_api
            .0
            .set_gpages(dest_program_id, &src_program_pages)
            .await?;

        dest_node_api
            .0
            .set_gprog(
                dest_program_id,
                src_program,
                dest_node_api.last_block_number().await?,
            )
            .await?;

        Ok(dest_program_id)
    }

    /// Claim value from the mailbox message identified by `message_id`.
    ///
    /// Sends the
    /// [`pallet_gear::claim_value`](https://docs.gear.rs/pallet_gear/pallet/struct.Pallet.html#method.claim_value)
    /// extrinsic.
    ///
    /// This function returns a tuple with value and block hash containing the
    /// corresponding transaction.
    ///
    /// # See also
    ///
    /// - [`claim_value_batch`](Self::claim_value_batch) function claims a batch
    ///   of values from the mailbox.
    pub async fn claim_value(&self, message_id: MessageId) -> Result<(u128, H256)> {
        let value = self
            .get_mailbox_message(message_id)
            .await?
            .map(|(message, _interval)| message.value());

        let tx = self.0.claim_value(message_id).await?;

        for event in tx.wait_for_success().await?.iter() {
            if let Event::Gear(GearEvent::UserMessageRead { .. }) =
                event?.as_root_event::<Event>()?
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
    /// A batch is a set of requests to be executed within one function call.
    /// Every entry of the `args` iterator is a message identifier used in the
    /// [`claim_value`](Self::claim_value) function. It is useful when
    /// processing multiple replies in the mailbox at once.
    pub async fn claim_value_batch(
        &self,
        args: impl IntoIterator<Item = MessageId> + Clone,
    ) -> Result<(Vec<Result<u128>>, H256)> {
        let mut values = BTreeMap::new();
        for message_id in args.clone().into_iter() {
            values.insert(
                message_id,
                self.get_mailbox_message(message_id)
                    .await?
                    .map(|(message, _interval)| message.value()),
            );
        }

        let calls: Vec<_> = args
            .into_iter()
            .map(|message_id| {
                RuntimeCall::Gear(GearCall::claim_value {
                    message_id: message_id.into(),
                })
            })
            .collect();

        let amount = calls.len();

        let tx = self.0.force_batch(calls).await?;
        let mut res = Vec::with_capacity(amount);

        for event in tx.wait_for_success().await?.iter() {
            match event?.as_root_event::<Event>()? {
                Event::Gear(GearEvent::UserMessageRead { id, .. }) => res.push(Ok(values
                    .remove(&id.into())
                    .flatten()
                    .expect("Data appearance guaranteed above"))),
                Event::Utility(UtilityEvent::ItemFailed { error }) => {
                    res.push(Err(self.0.api().decode_error(error).into()))
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
    /// - [`send_message_bytes_batch`](Self::send_message_bytes_batch) function
    ///   sends a batch of messages.
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
            if let Event::Gear(GearEvent::MessageQueued {
                id,
                entry: MessageEntry::Handle,
                ..
            }) = event?.as_root_event::<Event>()?
            {
                return Ok((id.into(), tx.block_hash()));
            }
        }

        Err(Error::EventNotFound)
    }

    /// Send a batch of messages.
    ///
    /// A batch is a set of messages to be sent within one function call. Every
    /// entry of the `args` iterator is a tuple of parameters used in the
    /// [`send_message_bytes`](Self::send_message_bytes) function. It is useful
    /// when invoking several programs at once or sending a sequence of messages
    /// to one program.
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

        let tx = self.0.force_batch(calls).await?;
        let mut res = Vec::with_capacity(amount);

        for event in tx.wait_for_success().await?.iter() {
            match event?.as_root_event::<Event>()? {
                Event::Gear(GearEvent::MessageQueued {
                    id,
                    destination,
                    entry: MessageEntry::Handle,
                    ..
                }) => res.push(Ok((id.into(), destination.into()))),
                Event::Utility(UtilityEvent::ItemFailed { error }) => {
                    res.push(Err(self.0.api().decode_error(error).into()))
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

    /// Same as [`send_message_bytes`](Self::send_message_bytes), but sends a
    /// message with encoded `payload`.
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
    /// - [`send_reply_bytes_batch`](Self::send_reply_bytes_batch) function send
    ///   a batch of replies.
    pub async fn send_reply_bytes(
        &self,
        reply_to_id: MessageId,
        payload: impl AsRef<[u8]>,
        gas_limit: u64,
        value: u128,
    ) -> Result<(MessageId, u128, H256)> {
        let payload = payload.as_ref().to_vec();

        let data = self.get_mailbox_message(reply_to_id).await?;

        let tx = self
            .0
            .send_reply(reply_to_id, payload, gas_limit, value)
            .await?;

        let events = tx.wait_for_success().await?;

        let (message, _interval) = data.expect("Data appearance guaranteed above");

        for event in events.iter() {
            if let Event::Gear(GearEvent::MessageQueued {
                id,
                entry: MessageEntry::Reply(_),
                ..
            }) = event?.as_root_event::<Event>()?
            {
                return Ok((id.into(), message.value(), tx.block_hash()));
            }
        }

        Err(Error::EventNotFound)
    }

    /// Send a batch of replies.
    ///
    /// A batch is a set of replies to be sent within one function call. Every
    /// entry of the `args` iterator is a tuple of parameters used in the
    /// [`send_reply_bytes`](Self::send_reply_bytes) function. It is useful when
    /// replying to several programs at once.
    ///
    /// The output for each call slightly differs from
    /// [`send_reply_bytes`](Self::send_reply_bytes) as the destination
    /// program id is also returned in the resulting tuple.
    pub async fn send_reply_bytes_batch(
        &self,
        args: impl IntoIterator<Item = (MessageId, impl AsRef<[u8]>, u64, u128)> + Clone,
    ) -> Result<(Vec<Result<(MessageId, ProgramId, u128)>>, H256)> {
        let mut values = BTreeMap::new();
        for (message_id, _, _, _) in args.clone().into_iter() {
            values.insert(
                message_id,
                self.get_mailbox_message(message_id)
                    .await?
                    .map(|(message, _interval)| message.value()),
            );
        }

        let calls: Vec<_> = args
            .into_iter()
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

        let tx = self.0.force_batch(calls).await?;
        let mut res = Vec::with_capacity(amount);

        for event in tx.wait_for_success().await?.iter() {
            match event?.as_root_event::<Event>()? {
                Event::Gear(GearEvent::MessageQueued {
                    id,
                    entry: MessageEntry::Reply(reply_to_id),
                    destination,
                    ..
                }) => res.push(Ok((
                    id.into(),
                    destination.into(),
                    values
                        .remove(&reply_to_id.into())
                        .flatten()
                        .expect("Data appearance guaranteed above"),
                ))),
                Event::Utility(UtilityEvent::ItemFailed { error }) => {
                    res.push(Err(self.0.api().decode_error(error).into()))
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

    /// Same as [`send_reply_bytes`](Self::send_reply_bytes), but sends a reply
    /// with encoded `payload`.
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
    ///
    /// # See also
    ///
    /// - [`create_program`](Self::create_program) function creates a program
    ///   from a previously uploaded code and initializes it.
    /// - [`upload_program`](Self::upload_program) function uploads a new
    ///   program and initialize it.
    pub async fn upload_code(&self, code: impl AsRef<[u8]>) -> Result<(CodeId, H256)> {
        let tx = self.0.upload_code(code.as_ref().to_vec()).await?;

        for event in tx.wait_for_success().await?.iter() {
            if let Event::Gear(GearEvent::CodeChanged {
                id,
                change: CodeChangeKind::Active { .. },
            }) = event?.as_root_event::<Event>()?
            {
                return Ok((id.into(), tx.block_hash()));
            }
        }

        Err(Error::EventNotFound)
    }

    /// Upload a batch of codes.
    ///
    /// A batch is a set of codes to be uploaded within one function call. Every
    /// entry of the `args` iterator is a byte slice used in the
    /// [`upload_code`](Self::upload_code) function. It is useful when deploying
    /// a multi-program dApp.
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

        let tx = self.0.force_batch(calls).await?;
        let mut res = Vec::with_capacity(amount);

        for event in tx.wait_for_success().await?.iter() {
            match event?.as_root_event::<Event>()? {
                Event::Gear(GearEvent::CodeChanged {
                    id,
                    change: CodeChangeKind::Active { .. },
                }) => {
                    res.push(Ok(id.into()));
                }
                Event::Utility(UtilityEvent::ItemFailed { error }) => {
                    res.push(Err(self.0.api().decode_error(error).into()))
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
            if let Event::Gear(GearEvent::MessageQueued {
                id,
                destination,
                entry: MessageEntry::Init,
                ..
            }) = event?.as_root_event::<Event>()?
            {
                return Ok((id.into(), destination.into(), tx.block_hash()));
            }
        }

        Err(Error::EventNotFound)
    }

    /// Upload a batch of programs.
    ///
    /// A batch is a set of programs to be uploaded within one function call.
    /// Every entry of the `args` iterator is a tuple used in the
    /// [`upload_program_bytes`](Self::upload_program_bytes) function. It is
    /// useful when deploying a multi-program dApp.
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

        let tx = self.0.force_batch(calls).await?;
        let mut res = Vec::with_capacity(amount);

        for event in tx.wait_for_success().await?.iter() {
            match event?.as_root_event::<Event>()? {
                Event::Gear(GearEvent::MessageQueued {
                    id,
                    destination,
                    entry: MessageEntry::Init,
                    ..
                }) => res.push(Ok((id.into(), destination.into()))),
                Event::Utility(UtilityEvent::ItemFailed { error }) => {
                    res.push(Err(self.0.api().decode_error(error).into()))
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

    /// Same as [`upload_program_bytes`](Self::upload_program_bytes), but
    /// initializes a newly uploaded program with an encoded `payload`.
    ///
    /// # See also
    ///
    /// - [`create_program`](Self::create_program) function creates a program
    ///   from a previously uploaded code.
    /// - [`upload_code`](Self::upload_code) function uploads a code and returns
    ///   its identifier.
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
        let tx = self
            .0
            .sudo_unchecked_weight(
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
            )
            .await?;
        Ok(tx.wait_for_success().await?.block_hash())
    }

    /// Upgrade the runtime by reading the code from the file located at the
    /// `path`.
    ///
    /// Same as [`set_code`](Self::set_code), but reads the runtime code from a
    /// file instead of using a byte vector.
    pub async fn set_code_by_path(&self, path: impl AsRef<Path>) -> Result<H256> {
        let code = utils::code_from_os(path)?;
        self.set_code(code).await
    }

    /// Set the free and reserved balance of the `to` account to `new_free` and
    /// `new_reserved` respectively.
    ///
    /// Sends the [`pallet_balances::set_balance`](https://crates.parity.io/pallet_balances/pallet/struct.Pallet.html#method.set_balance) extrinsic.
    pub async fn set_balance(
        &self,
        to: impl Into<MultiAddress<AccountId32, ()>>,
        new_free: u128,
        new_reserved: u128,
    ) -> Result<H256> {
        let tx = self
            .0
            .sudo_unchecked_weight(
                RuntimeCall::Balances(BalancesCall::set_balance {
                    who: to.into(),
                    new_free,
                    new_reserved,
                }),
                Weight {
                    ref_time: 0,
                    // # TODO
                    //
                    // Check this field
                    proof_size: Default::default(),
                },
            )
            .await?;
        Ok(tx.wait_for_success().await?.block_hash())
    }
}
