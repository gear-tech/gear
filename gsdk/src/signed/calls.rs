// This file is part of Gear.
//
// Copyright (C) 2021-2025 Gear Technologies Inc.
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

//! gear api calls
use std::{
    collections::{HashMap, HashSet},
    path::Path,
};

use crate::{
    Error, Event, GearGasNode, GearGasNodeId, IntoAccountId32, IntoSubstrate, Result, SignedApi,
    TxOutput,
    gear::{
        balances, gear, gear_eth_bridge, gear_voucher,
        runtime_types::{
            gear_common::event::{CodeChangeKind, MessageEntry},
            pallet_balances::types::{AccountData, ExtraFlags},
            pallet_gear_bank::pallet::BankAccount,
            pallet_gear_voucher::internal::{PrepaidCall, VoucherId},
            sp_weights::weight_v2::Weight,
            vara_runtime::RuntimeCall,
        },
        system, tx, utility,
    },
};
use gear_core::{
    gas::LockId,
    ids::{ActorId, CodeId, MessageId},
};
use gear_utils::{MemoryPageDump, ProgramMemoryDump};
use parity_scale_codec::Encode;
use subxt::utils::H256;
use tokio::fs;

// pallet-balances
impl SignedApi {
    /// Transfer `value` to `destination`'s account, but ensures
    /// that the original account won't be killed by the transfer
    /// transaction.
    ///
    /// Sends the
    /// [`pallet_balances::transfer_keep_alive`](https://crates.parity.io/pallet_balances/pallet/struct.Pallet.html#method.transfer_keep_alive)
    /// extrinsic.
    pub async fn transfer_keep_alive(
        &self,
        dest: impl IntoAccountId32,
        value: u128,
    ) -> Result<TxOutput> {
        let dest = dest.into_account_id();

        self.run_tx(
            tx().balances()
                .transfer_keep_alive(dest.clone().into(), value),
        )
        .await?
        .any(|event| matches!(event, Event::Balances(balances::Event::Transfer { .. })))?
        .or(|| value == 0 || self.account_id() == &dest.into_account_id().into_substrate())
        .ok_or_err()
    }

    /// Transfer `value` to `destination`'s account.
    ///
    /// Sends the
    /// [`pallet_balances::transfer`](https://crates.parity.io/pallet_balances/pallet/struct.Pallet.html#method.transfer)
    /// extrinsic.
    pub async fn transfer_allow_death(
        &self,
        dest: impl IntoAccountId32,
        value: u128,
    ) -> Result<TxOutput> {
        let dest = dest.into_account_id();

        self.run_tx(
            tx().balances()
                .transfer_allow_death(dest.clone().into(), value),
        )
        .await?
        .any(|event| matches!(event, Event::Balances(balances::Event::Transfer { .. })))?
        .or(|| value == 0 || self.account_id() == &dest.into_substrate())
        .ok_or_err()
    }

    /// Transfer the entire transferable balance from the caller to `destination`'s account.
    ///
    /// Sends the
    /// [`pallet_balances::transfer_all`](https://crates.parity.io/pallet_balances/pallet/struct.Pallet.html#method.transfer_all)
    /// extrinsic.
    pub async fn transfer_all(
        &self,
        dest: impl IntoAccountId32,
        keep_alive: bool,
    ) -> Result<TxOutput> {
        self.run_tx(
            tx().balances()
                .transfer_all(dest.into_account_id().into(), keep_alive),
        )
        .await?
        .any(|event| matches!(event, Event::Balances(balances::Event::Transfer { .. })))?
        .ok_or_err()
    }
}

// pallet-gear
impl SignedApi {
    /// Create a new program from a previously uploaded code identified by
    /// [`CodeId`](https://docs.rs/gear_core/ids/struct.CodeId.html) and
    /// initialize it with a byte slice `payload`.
    ///
    /// Sends the
    /// [`pallet_gear::create_program`](https://docs.rs/pallet_gear/pallet/struct.Pallet.html#method.create_program)
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
        salt: impl Into<Vec<u8>>,
        payload: impl Into<Vec<u8>>,
        gas_limit: u64,
        value: u128,
    ) -> Result<TxOutput<(MessageId, ActorId)>> {
        self.run_tx(tx().gear().create_program(
            code_id,
            salt.into(),
            payload.into(),
            gas_limit,
            value,
            false,
        ))
        .await?
        .find_map(|event| match event {
            Event::Gear(gear::Event::MessageQueued {
                id,
                destination,
                entry: MessageEntry::Init,
                ..
            }) => Some((id, destination)),
            _ => None,
        })?
        .ok_or_err()
    }

    /// Create a batch of programs.
    ///
    /// A batch is a set of programs to be created within one function call.
    /// Every entry of the `args` iterator is a tuple of parameters used in the
    /// [`create_program_bytes`](Self::create_program_bytes) function. It is
    /// useful when deploying a multi-program dApp.
    pub async fn create_program_bytes_batch(
        &self,
        args: impl IntoIterator<Item = (CodeId, impl Into<Vec<u8>>, impl Into<Vec<u8>>, u64, u128)>,
    ) -> Result<TxOutput<Vec<Result<(MessageId, ActorId)>>>> {
        self.run_batch(
            args.into_iter()
                .map(|(code_id, salt, payload, gas_limit, value)| {
                    RuntimeCall::Gear(gear::Call::create_program {
                        code_id,
                        salt: salt.into(),
                        init_payload: payload.into(),
                        gas_limit,
                        value,
                        keep_alive: false,
                    })
                }),
            |event| match event {
                Event::Gear(gear::Event::MessageQueued {
                    id,
                    destination,
                    entry: MessageEntry::Init,
                    ..
                }) => Some(Ok((id, destination))),
                Event::Utility(utility::Event::ItemFailed { error }) => {
                    Some(Err(self.decode_error(error).into()))
                }
                _ => None,
            },
        )
        .await
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
        salt: impl Into<Vec<u8>>,
        payload: impl Encode,
        gas_limit: u64,
        value: u128,
    ) -> Result<TxOutput<(MessageId, ActorId)>> {
        self.create_program_bytes(code_id, salt, payload.encode(), gas_limit, value)
            .await
    }

    /// Claim value from the mailbox message identified by `message_id`.
    ///
    /// Sends the
    /// [`pallet_gear::claim_value`](https://docs.rs/pallet_gear/pallet/struct.Pallet.html#method.claim_value)
    /// extrinsic.
    ///
    /// # See also
    ///
    /// - [`claim_value_batch`](Self::claim_value_batch) function claims a batch
    ///   of values from the mailbox.
    pub async fn claim_value(&self, message_id: MessageId) -> Result<TxOutput<u128>> {
        let value = self
            .mailbox_message(message_id)
            .await?
            .map(|(message, _)| message.value());

        self.run_tx(tx().gear().claim_value(message_id))
            .await?
            .any(|event| matches!(event, Event::Gear(gear::Event::UserMessageRead { .. })))?
            .then(|| value.expect("data appearance guaraenteed above"))
            .ok_or_err()
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
    ) -> Result<TxOutput<Vec<Result<u128>>>> {
        let message_ids: Vec<_> = args.clone().into_iter().collect();

        let messages =
            futures::future::try_join_all(message_ids.iter().map(|mid| self.mailbox_message(*mid)))
                .await?;

        let mut values: HashMap<_, _> = messages
            .into_iter()
            .flatten()
            .map(|(msg, _interval)| (msg.id(), msg.value()))
            .collect();

        self.run_batch(
            args.into_iter()
                .map(|message_id| RuntimeCall::Gear(gear::Call::claim_value { message_id })),
            |event| match event {
                Event::Gear(gear::Event::UserMessageRead { id, .. }) => Some(Ok(values
                    .remove(&id)
                    .expect("Data appearance guaranteed above"))),
                Event::Utility(utility::Event::ItemFailed { error }) => {
                    Some(Err(self.decode_error(error).into()))
                }
                _ => None,
            },
        )
        .await
    }

    /// Send a message containing a byte slice `payload` to the `destination`.
    ///
    /// The message also contains the maximum `gas_limit` that can be spent and
    /// the `value` to be transferred to the `destination`'s account.
    ///
    /// Sends the
    /// [`pallet_gear::send_message`](https://docs.rs/pallet_gear/pallet/struct.Pallet.html#method.send_message)
    /// extrinsic.
    ///
    /// This function returns a new message identifier.
    ///
    /// # See also
    ///
    /// - [`send_message`](Self::send_message) function sends a message with an
    ///   encoded payload.
    /// - [`send_message_bytes_batch`](Self::send_message_bytes_batch) function
    ///   sends a batch of messages.
    pub async fn send_message_bytes(
        &self,
        destination: ActorId,
        payload: impl Into<Vec<u8>>,
        gas_limit: u64,
        value: u128,
    ) -> Result<TxOutput<MessageId>> {
        self.run_tx(
            tx().gear()
                .send_message(destination, payload.into(), gas_limit, value, false),
        )
        .await?
        .find_map(|event| match event {
            Event::Gear(gear::Event::MessageQueued {
                id,
                entry: MessageEntry::Handle,
                ..
            }) => Some(id),
            _ => None,
        })?
        .ok_or_err()
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
        args: impl IntoIterator<Item = (ActorId, impl Into<Vec<u8>>, u64, u128)>,
    ) -> Result<TxOutput<Vec<Result<(MessageId, ActorId)>>>> {
        self.run_batch(
            args.into_iter()
                .map(|(destination, payload, gas_limit, value)| {
                    RuntimeCall::Gear(gear::Call::send_message {
                        destination,
                        payload: payload.into(),
                        gas_limit,
                        value,
                        keep_alive: false,
                    })
                }),
            |event| match event {
                Event::Gear(gear::Event::MessageQueued {
                    id,
                    destination,
                    entry: MessageEntry::Handle,
                    ..
                }) => Some(Ok((id, destination))),
                Event::Utility(utility::Event::ItemFailed { error }) => {
                    Some(Err(self.decode_error(error).into()))
                }
                _ => None,
            },
        )
        .await
    }

    /// Same as [`send_message_bytes`](Self::send_message_bytes), but sends a
    /// message with encoded `payload`.
    pub async fn send_message(
        &self,
        destination: ActorId,
        payload: impl Encode,
        gas_limit: u64,
        value: u128,
    ) -> Result<TxOutput<MessageId>> {
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
    /// [`pallet_gear::send_reply`](https://docs.rs/pallet_gear/pallet/struct.Pallet.html#method.send_reply)
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
        payload: impl Into<Vec<u8>>,
        gas_limit: u64,
        value: u128,
    ) -> Result<TxOutput<(MessageId, u128)>> {
        let data = self.mailbox_message(reply_to_id).await?;

        self.run_tx(
            tx().gear()
                .send_reply(reply_to_id, payload.into(), gas_limit, value, false),
        )
        .await?
        .find_map(move |event| match event {
            Event::Gear(gear::Event::MessageQueued {
                id,
                entry: MessageEntry::Reply(_),
                ..
            }) => Some(id),
            _ => None,
        })?
        .map(move |opt| {
            opt.map(|id| {
                (
                    id,
                    data.expect("data appearance guaraenteed above").0.value(),
                )
            })
        })
        .ok_or_err()
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
        args: impl IntoIterator<Item = (MessageId, impl Into<Vec<u8>>, u64, u128)> + Clone,
    ) -> Result<TxOutput<Vec<Result<(MessageId, ActorId, u128)>>>> {
        let message_ids: Vec<_> = args.clone().into_iter().map(|(mid, _, _, _)| mid).collect();

        let messages =
            futures::future::try_join_all(message_ids.iter().map(|mid| self.mailbox_message(*mid)))
                .await?;

        let mut values: HashMap<_, _> = messages
            .into_iter()
            .flatten()
            .map(|(msg, _interval)| (msg.id(), msg.value()))
            .collect();

        self.run_batch(
            args.into_iter()
                .map(|(reply_to_id, payload, gas_limit, value)| {
                    RuntimeCall::Gear(gear::Call::send_reply {
                        reply_to_id,
                        payload: payload.into(),
                        gas_limit,
                        value,
                        keep_alive: false,
                    })
                }),
            |event| match event {
                Event::Gear(gear::Event::MessageQueued {
                    id,
                    entry: MessageEntry::Reply(reply_to_id),
                    destination,
                    ..
                }) => Some(Ok((
                    id,
                    destination,
                    values
                        .remove(&reply_to_id)
                        .expect("Data appearance guaranteed above"),
                ))),
                Event::Utility(utility::Event::ItemFailed { error }) => {
                    Some(Err(self.decode_error(error).into()))
                }
                _ => None,
            },
        )
        .await
    }

    /// Same as [`send_reply_bytes`](Self::send_reply_bytes), but sends a reply
    /// with encoded `payload`.
    pub async fn send_reply(
        &self,
        reply_to_id: MessageId,
        payload: impl Encode,
        gas_limit: u64,
        value: u128,
    ) -> Result<TxOutput<(MessageId, u128)>> {
        self.send_reply_bytes(reply_to_id, payload.encode(), gas_limit, value)
            .await
    }

    /// Upload Wasm `code` to be used for creating a new program.
    ///
    /// Sends the
    /// [`pallet_gear::upload_code`](https://docs.rs/pallet_gear/pallet/struct.Pallet.html#method.upload_code)
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
    pub async fn upload_code(&self, code: impl Into<Vec<u8>>) -> Result<TxOutput<CodeId>> {
        self.run_tx(tx().gear().upload_code(code.into()))
            .await?
            .find_map(|event| match event {
                Event::Gear(gear::Event::CodeChanged {
                    id,
                    change: CodeChangeKind::Active { .. },
                }) => Some(id),
                _ => None,
            })?
            .ok_or_err()
    }

    /// Upload a batch of codes.
    ///
    /// A batch is a set of codes to be uploaded within one function call. Every
    /// entry of the `args` iterator is a byte slice used in the
    /// [`upload_code`](Self::upload_code) function. It is useful when deploying
    /// a multi-program dApp.
    pub async fn upload_code_batch(
        &self,
        args: impl IntoIterator<Item = impl Into<Vec<u8>>>,
    ) -> Result<TxOutput<Vec<Result<CodeId>>>> {
        self.run_batch(
            args.into_iter()
                .map(|code| RuntimeCall::Gear(gear::Call::upload_code { code: code.into() })),
            |event| match event {
                Event::Gear(gear::Event::CodeChanged {
                    id,
                    change: CodeChangeKind::Active { .. },
                }) => Some(Ok(id)),
                Event::Utility(utility::Event::ItemFailed { error }) => {
                    Some(Err(self.decode_error(error).into()))
                }
                _ => None,
            },
        )
        .await
    }

    /// Upload Wasm code from the file referenced by `path` to be used for
    /// creating a new program.
    ///
    /// Same as [`upload_code`](Self::upload_code), but reads the code from a
    /// file instead of using a byte vector.
    ///
    /// Works with absolute and relative paths (relative to the root dir of the
    /// repo).
    pub async fn upload_code_by_path(&self, path: impl AsRef<Path>) -> Result<TxOutput<CodeId>> {
        let code = fs::read(path).await?;
        self.upload_code(code).await
    }

    /// Upload a new program and initialize it with a byte slice `payload`.
    ///
    /// Sends the
    /// [`pallet_gear::upload_program`](https://docs.rs/pallet_gear/pallet/struct.Pallet.html#method.upload_program)
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
        code: impl Into<Vec<u8>>,
        salt: impl Into<Vec<u8>>,
        payload: impl Into<Vec<u8>>,
        gas_limit: u64,
        value: u128,
    ) -> Result<TxOutput<(MessageId, ActorId)>> {
        self.run_tx(tx().gear().upload_program(
            code.into(),
            salt.into(),
            payload.into(),
            gas_limit,
            value,
            false,
        ))
        .await?
        .find_map(|event| match event {
            Event::Gear(gear::Event::MessageQueued {
                id,
                destination,
                entry: MessageEntry::Init,
                ..
            }) => Some((id, destination)),
            _ => None,
        })?
        .ok_or_err()
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
                impl Into<Vec<u8>>,
                impl Into<Vec<u8>>,
                impl Into<Vec<u8>>,
                u64,
                u128,
            ),
        >,
    ) -> Result<TxOutput<Vec<Result<(MessageId, ActorId)>>>> {
        self.run_batch(
            args.into_iter()
                .map(|(code, salt, payload, gas_limit, value)| {
                    RuntimeCall::Gear(gear::Call::upload_program {
                        code: code.into(),
                        salt: salt.into(),
                        init_payload: payload.into(),
                        gas_limit,
                        value,
                        keep_alive: false,
                    })
                }),
            |event| match event {
                Event::Gear(gear::Event::MessageQueued {
                    id,
                    destination,
                    entry: MessageEntry::Init,
                    ..
                }) => Some(Ok((id, destination))),
                Event::Utility(utility::Event::ItemFailed { error }) => {
                    Some(Err(self.decode_error(error).into()))
                }
                _ => None,
            },
        )
        .await
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
        salt: impl Into<Vec<u8>>,
        payload: impl Into<Vec<u8>>,
        gas_limit: u64,
        value: u128,
    ) -> Result<TxOutput<(MessageId, ActorId)>> {
        let code = fs::read(path).await?;
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
        code: impl Into<Vec<u8>>,
        salt: impl Into<Vec<u8>>,
        payload: impl Encode,
        gas_limit: u64,
        value: u128,
    ) -> Result<TxOutput<(MessageId, ActorId)>> {
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
        salt: impl Into<Vec<u8>>,
        payload: impl Encode,
        gas_limit: u64,
        value: u128,
    ) -> Result<TxOutput<(MessageId, ActorId)>> {
        let code = fs::read(path).await?;
        self.upload_program(code, salt, payload, gas_limit, value)
            .await
    }

    /// Migrates an active program identified by `src_program_id` onto another
    /// node identified by `dest_node_api` and returns the migrated program
    /// identifier. All source program data is taken at the time of
    /// `src_block_hash` if it is specified or the most recent one.
    pub async fn migrate_program(
        &self,
        src_program_id: ActorId,
        src_block_hash: Option<H256>,
        dest_node_api: &SignedApi,
    ) -> Result<ActorId> {
        if dest_node_api.active_program(src_program_id).await.is_ok() {
            return Err(Error::ProgramAlreadyExists(src_program_id));
        }

        let src_block_hash = if let Some(hash) = src_block_hash {
            hash
        } else {
            self.blocks().at_latest().await?.hash()
        };

        let dest_program_id = src_program_id;

        // Collect data from the source program
        let src_program_account_data = self
            .account_data_at(src_program_id, src_block_hash)
            .await
            .or_else(|e| {
            if let Error::StorageEntryNotFound = e {
                Ok(AccountData {
                    free: 0u128,
                    reserved: 0,
                    frozen: 0,
                    flags: ExtraFlags::NEW_LOGIC,
                })
            } else {
                Err(e)
            }
        })?;

        let src_program_account_bank_data = self
            .bank_info_at(src_program_id, src_block_hash)
            .await
            .or_else(|e| {
                if let Error::StorageEntryNotFound = e {
                    Ok(BankAccount { gas: 0, value: 0 })
                } else {
                    Err(e)
                }
            })?;

        let bank_address = self.bank_address().await?;

        let src_bank_account_data = self
            .account_data_at(bank_address.clone(), src_block_hash)
            .await
            .or_else(|e| {
                if let Error::StorageEntryNotFound = e {
                    Ok(AccountData {
                        free: 0u128,
                        reserved: 0,
                        frozen: 0,
                        flags: ExtraFlags::NEW_LOGIC,
                    })
                } else {
                    Err(e)
                }
            })?;

        let mut src_program = self
            .active_program_at(src_program_id, src_block_hash)
            .await?;

        let src_program_pages = self
            .program_pages_at(src_program_id, src_block_hash)
            .await?;

        let src_program_reserved_gas_node_ids: Vec<GearGasNodeId> = src_program
            .gas_reservation_map
            .iter()
            .map(|gr| GearGasNodeId::Reservation(gr.0))
            .collect();

        let src_program_reserved_gas_nodes = self
            .gas_nodes_at(src_program_reserved_gas_node_ids, src_block_hash)
            .await?;

        let mut src_program_reserved_gas_total = 0u64;
        let mut accounts_with_reserved_funds = HashSet::new();
        for gas_node in &src_program_reserved_gas_nodes {
            if let GearGasNode::Reserved {
                id, value, lock, ..
            } = &gas_node.1
            {
                accounts_with_reserved_funds.insert(id.clone().into_substrate());
                src_program_reserved_gas_total += value + lock.0[LockId::Reservation as usize];
            } else {
                unreachable!("Unexpected gas node type");
            }
        }

        let src_code_id = src_program.code_id;

        let src_instrumented_code = self
            .instrumented_code_storage_at(src_code_id, src_block_hash)
            .await?;

        let src_code_metadata = self
            .code_metadata_storage_at(src_code_id, src_block_hash)
            .await?;

        // Apply data to the target program
        dest_node_api
            .force_set_balance(
                dest_program_id.into_account_id(),
                src_program_account_data.free,
            )
            .await?;

        dest_node_api
            .force_set_balance(bank_address, src_bank_account_data.free)
            .await?;

        dest_node_api
            .set_bank_account_storage(
                src_program_id.into_account_id(),
                src_program_account_bank_data,
            )
            .await?;

        dest_node_api
            .set_instrumented_code_storage(src_code_id, &src_instrumented_code)
            .await?;

        dest_node_api
            .set_code_metadata_storage(src_code_id, &src_code_metadata)
            .await?;

        dest_node_api
            .set_gas_nodes(&src_program_reserved_gas_nodes)
            .await?;

        for account_with_reserved_funds in accounts_with_reserved_funds {
            let src_account_bank_data = self
                .bank_info_at(account_with_reserved_funds.clone(), src_block_hash)
                .await
                .or_else(|e| {
                    if let Error::StorageEntryNotFound = e {
                        Ok(BankAccount { gas: 0, value: 0 })
                    } else {
                        Err(e)
                    }
                })?;

            let dest_account_data = dest_node_api
                .account_data(account_with_reserved_funds.clone())
                .await
                .or_else(|e| {
                    if let Error::StorageEntryNotFound = e {
                        Ok(AccountData {
                            free: 0u128,
                            reserved: 0,
                            frozen: 0,
                            flags: ExtraFlags::NEW_LOGIC,
                        })
                    } else {
                        Err(e)
                    }
                })?;
            let dest_account_bank_data = self
                .bank_info_at(account_with_reserved_funds.clone(), None)
                .await
                .or_else(|e| {
                    if let Error::StorageEntryNotFound = e {
                        Ok(BankAccount { gas: 0, value: 0 })
                    } else {
                        Err(e)
                    }
                })?;

            dest_node_api
                .force_set_balance(
                    account_with_reserved_funds.clone().into_account_id(),
                    dest_account_data.free,
                )
                .await?;

            dest_node_api
                .set_bank_account_storage(
                    account_with_reserved_funds.into_account_id(),
                    BankAccount {
                        gas: src_account_bank_data
                            .gas
                            .saturating_add(dest_account_bank_data.gas),
                        value: src_account_bank_data
                            .value
                            .saturating_add(dest_account_bank_data.value),
                    },
                )
                .await?;
        }

        let dest_gas_total_issuance = dest_node_api.total_issuance().await.or_else(|e| {
            if let Error::StorageEntryNotFound = e {
                Ok(0)
            } else {
                Err(e)
            }
        })?;

        dest_node_api
            .set_total_issuance(
                dest_gas_total_issuance.saturating_add(src_program_reserved_gas_total),
            )
            .await?;

        dest_node_api
            .set_program_pages(dest_program_id, &src_program_pages)
            .await?;

        src_program.expiration_block = dest_node_api.blocks().at_latest().await?.number();
        dest_node_api
            .set_program(dest_program_id, src_program)
            .await?;

        Ok(dest_program_id)
    }

    /// Save program (identified by `program_id`) memory dump to the file for
    /// further restoring in gclient/gtest. Program memory dumped at the
    /// time of `block_hash` if presented or the most recent block.
    pub async fn save_program_memory_dump_at<P: AsRef<Path>>(
        &self,
        program_id: ActorId,
        block_hash: Option<H256>,
        file_path: P,
    ) -> Result<()> {
        let program_pages = self
            .program_pages_at(program_id, block_hash)
            .await?
            .into_iter()
            .map(|(page, page_buf)| MemoryPageDump::new(page, page_buf))
            .collect();

        let program_account_data =
            self.account_data_at(program_id, block_hash)
                .await
                .or_else(|e| {
                    if let Error::StorageEntryNotFound = e {
                        Ok(AccountData {
                            free: 0u128,
                            reserved: 0,
                            frozen: 0,
                            flags: ExtraFlags::NEW_LOGIC,
                        })
                    } else {
                        Err(e)
                    }
                })?;

        let dump = ProgramMemoryDump {
            pages: program_pages,
            balance: program_account_data.free,
            reserved_balance: program_account_data.reserved,
        };
        fs::write(file_path, serde_json::to_vec(&dump)?).await?;

        Ok(())
    }

    /// Replace entire program memory with one saved earlier in gclient/gtest
    pub async fn replace_program_memory<P: AsRef<Path>>(
        &self,
        program_id: ActorId,
        file_path: P,
    ) -> Result<()> {
        let contents = fs::read(file_path).await?;
        let memory_dump: ProgramMemoryDump = serde_json::from_slice(&contents)?;

        let pages = memory_dump
            .pages
            .into_iter()
            .map(|page| page.into_gear_page())
            .collect();

        self.force_set_balance(program_id, memory_dump.balance)
            .await?;

        self.set_program_pages(program_id, &pages).await?;

        Ok(())
    }

    /// Upgrade the runtime with the `code` containing the Wasm code of the new
    /// runtime.
    ///
    /// Sends the
    /// [`pallet_system::set_code`](https://crates.parity.io/frame_system/pallet/struct.Pallet.html#method.set_code)
    /// extrinsic.
    pub async fn set_code(&self, code: impl Into<Vec<u8>>) -> Result<TxOutput> {
        self.sudo_unchecked_weight(
            RuntimeCall::System(system::Call::set_code { code: code.into() }),
            Weight {
                ref_time: 0,
                proof_size: 0,
            },
        )
        .await?
        .find_map(|event| match event {
            Event::System(system::Event::CodeUpdated) => Some(()),
            _ => None,
        })?
        .ok_or_err()
    }

    /// Upgrade the runtime by reading the code from the file located at the
    /// `path`.
    ///
    /// Same as [`set_code`](Self::set_code), but reads the runtime code from a
    /// file instead of using a byte vector.
    pub async fn set_code_by_path(&self, path: impl AsRef<Path>) -> Result<TxOutput> {
        let code = fs::read(path).await?;
        self.set_code(code).await
    }

    /// Upgrade the runtime with the `code` containing the Wasm code of the new
    /// runtime but **without** checks.
    ///
    /// Sends the
    /// [`pallet_system::set_code_without_checks`](https://crates.parity.io/frame_system/pallet/struct.Pallet.html#method.set_code_without_checks)
    /// extrinsic.
    pub async fn set_code_without_checks(&self, code: impl Into<Vec<u8>>) -> Result<TxOutput> {
        self.sudo_unchecked_weight(
            RuntimeCall::System(system::Call::set_code_without_checks { code: code.into() }),
            Weight {
                ref_time: 0,
                proof_size: 0,
            },
        )
        .await?
        .any(|event| matches!(event, Event::System(system::Event::CodeUpdated)))?
        .ok_or_err()
    }

    /// Upgrade the runtime by reading the code from the file located at the
    /// `path`.
    ///
    /// Same as [`set_code_without_checks`](Self::set_code_without_checks), but
    /// reads the runtime code from a file instead of using a byte vector.
    pub async fn set_code_without_checks_by_path(
        &self,
        path: impl AsRef<Path>,
    ) -> Result<TxOutput> {
        let code = fs::read(path).await?;
        self.set_code_without_checks(code).await
    }
}

impl SignedApi {
    /// Sends the `pallet-gear-eth-bridge::reset_overflowed_queue` extrinsic.
    ///
    /// This function returns a hash of the block with the transaction.
    pub async fn reset_overflowed_queue(
        &self,
        encoded_finality_proof: Vec<u8>,
    ) -> Result<TxOutput> {
        self.run_tx(
            tx().gear_eth_bridge()
                .reset_overflowed_queue(encoded_finality_proof),
        )
        .await?
        .any(|event| {
            matches!(
                event,
                Event::GearEthBridge(gear_eth_bridge::Event::QueueReset)
            )
        })?
        .ok_or_err()
    }
}

// pallet-utility
impl SignedApi {
    /// Sends `pallet_utility::force_batch` extrinsic.
    pub async fn force_batch(&self, calls: Vec<RuntimeCall>) -> Result<TxOutput> {
        self.run_tx(tx().utility().force_batch(calls)).await
    }

    /// Submits a batch of extrinsics and extracts their outputs from events.
    ///
    /// This is a more high-level wrapper around [`Self::force_batch`].
    ///
    /// Returns a vector of extrinsic results and the hash of
    /// its block.
    pub async fn run_batch<O, F>(
        &self,
        calls: impl IntoIterator<Item = RuntimeCall>,
        f: F,
    ) -> Result<TxOutput<Vec<O>>>
    where
        F: FnMut(Event) -> Option<O>,
    {
        let calls = calls.into_iter().collect::<Vec<_>>();
        let expected = calls.len();

        let output = self.force_batch(calls).await?.filter_map(f)?;

        let found = output.as_ref().len();
        if found == expected {
            Ok(output)
        } else {
            Err(Error::IncompleteBatchResult { expected, found })
        }
    }
}

// pallet-sudo
impl SignedApi {
    /// Submits `pallet_sudo::sudo_unchecked_weight` extrinsic.
    pub async fn sudo_unchecked_weight(
        &self,
        call: RuntimeCall,
        weight: Weight,
    ) -> Result<TxOutput> {
        self.sudo_run_tx(tx().sudo().sudo_unchecked_weight(call, weight))
            .await
    }

    /// Set the free balance of the `to` account to `new_free`.
    ///
    /// Sends the [`pallet_balances::set_balance`](https://crates.parity.io/pallet_balances/pallet/struct.Pallet.html#method.set_balance) extrinsic.
    pub async fn force_set_balance(
        &self,
        to: impl IntoAccountId32,
        new_free: u128,
    ) -> Result<TxOutput> {
        self.sudo_unchecked_weight(
            RuntimeCall::Balances(balances::Call::force_set_balance {
                who: to.into_account_id().into(),
                new_free,
            }),
            Weight {
                ref_time: 0,
                // # TODO
                //
                // Check this field
                proof_size: Default::default(),
            },
        )
        .await
    }
}

// pallet-gear-voucher
impl SignedApi {
    /// Issue a new voucher.
    ///
    /// Returns issued `voucher_id` at specified `at_block_hash`.
    ///
    /// Arguments:
    /// * spender:  user id that is eligible to use the voucher;
    /// * balance:  voucher balance could be used for transactions fees and gas;
    /// * programs: pool of programs spender can interact with, if None - means
    ///   any program, limited by Config param;
    /// * code_uploading: allow voucher to be used as payer for `upload_code`
    ///   transactions fee;
    /// * duration: amount of blocks voucher could be used by spender and
    ///   couldn't be revoked by owner. Must be out in [MinDuration;
    ///   MaxDuration] constants. Expiration block of the voucher calculates as:
    ///   current bn (extrinsic exec bn) + duration + 1.
    pub async fn issue_voucher(
        &self,
        spender: impl IntoAccountId32,
        balance: u128,
        programs: Option<Vec<ActorId>>,
        code_uploading: bool,
        duration: u32,
    ) -> Result<TxOutput<VoucherId>> {
        self.run_tx(tx().gear_voucher().issue(
            spender.into_account_id(),
            balance,
            programs,
            code_uploading,
            duration,
        ))
        .await?
        .find_map(|event| match event {
            Event::GearVoucher(gear_voucher::Event::VoucherIssued { voucher_id, .. }) => {
                Some(voucher_id)
            }
            _ => None,
        })?
        .ok_or_err()
    }

    /// Update existing voucher.
    ///
    /// This extrinsic updates existing voucher: it can only extend vouchers
    /// rights in terms of balance, validity or programs to interact pool.
    ///
    /// Can only be called by the voucher owner.
    ///
    /// Arguments:
    /// * spender:          account id of the voucher spender;
    /// * voucher_id:       voucher id to be updated;
    /// * move_ownership:   optionally moves ownership to another account;
    /// * balance_top_up:   optionally top ups balance of the voucher from
    ///   origins balance;
    /// * append_programs:  optionally extends pool of programs by
    ///   `Some(programs_set)` passed or allows it to interact with any program
    ///   by `None` passed;
    /// * code_uploading:   optionally allows voucher to be used to pay fees for
    ///   `upload_code` extrinsics;
    /// * prolong_duration: optionally increases expiry block number. If voucher
    ///   is expired, prolongs since current bn. Validity prolongation (since
    ///   current block number for expired or since storage written expiry)
    ///   should be in [MinDuration; MaxDuration], in other words voucher
    ///   couldn't have expiry greater than current block number + MaxDuration.
    #[allow(clippy::too_many_arguments)]
    pub async fn update_voucher(
        &self,
        spender: impl IntoAccountId32,
        voucher_id: VoucherId,
        move_ownership: Option<impl IntoAccountId32>,
        balance_top_up: Option<u128>,
        append_programs: Option<Option<Vec<ActorId>>>,
        code_uploading: Option<bool>,
        prolong_duration: Option<u32>,
    ) -> Result<TxOutput<VoucherId>> {
        self.run_tx(tx().gear_voucher().update(
            spender.into_account_id(),
            voucher_id,
            move_ownership.map(|id| id.into_account_id()),
            balance_top_up,
            append_programs,
            code_uploading,
            prolong_duration,
        ))
        .await?
        .find_map(|event| match event {
            Event::GearVoucher(gear_voucher::Event::VoucherUpdated { voucher_id, .. }) => {
                Some(voucher_id)
            }
            _ => None,
        })?
        .ok_or_err()
    }

    /// Revoke existing voucher.
    ///
    /// This extrinsic revokes existing voucher, if current block is greater
    /// than expiration block of the voucher (it is no longer valid).
    ///
    /// Currently it means sending of all balance from voucher account to
    /// voucher owner without voucher removal from storage map, but this
    /// behavior may change in future, as well as the origin validation:
    /// only owner is able to revoke voucher now.
    ///
    /// Arguments:
    /// * spender:    account id of the voucher spender;
    /// * voucher_id: voucher id to be revoked.
    pub async fn revoke_voucher(
        &self,
        spender: impl IntoAccountId32,
        voucher_id: VoucherId,
    ) -> Result<TxOutput<VoucherId>> {
        self.run_tx(
            tx().gear_voucher()
                .revoke(spender.into_account_id(), voucher_id),
        )
        .await?
        .find_map(|event| match event {
            Event::GearVoucher(gear_voucher::Event::VoucherRevoked { voucher_id, .. }) => {
                Some(voucher_id)
            }
            _ => None,
        })?
        .ok_or_err()
    }

    /// Decline existing and not expired voucher.
    ///
    /// This extrinsic expires voucher of the caller, if it's still active,
    /// allowing it to be revoked.
    ///
    /// Arguments:
    /// * voucher_id:   voucher id to be declined.
    pub async fn decline_voucher(&self, voucher_id: VoucherId) -> Result<TxOutput<VoucherId>> {
        self.run_tx(tx().gear_voucher().decline(voucher_id))
            .await?
            .find_map(|event| match event {
                Event::GearVoucher(gear_voucher::Event::VoucherDeclined { voucher_id, .. }) => {
                    Some(voucher_id)
                }
                _ => None,
            })?
            .ok_or_err()
    }

    /// Same as [`Self::upload_code`], but using a voucher.
    pub async fn upload_code_with_voucher(
        &self,
        voucher_id: VoucherId,
        code: impl Into<Vec<u8>>,
    ) -> Result<TxOutput<CodeId>> {
        self.run_tx(
            tx().gear_voucher()
                .call(voucher_id, PrepaidCall::UploadCode { code: code.into() }),
        )
        .await?
        .find_map(|event| match event {
            Event::Gear(gear::Event::CodeChanged {
                id,
                change: CodeChangeKind::Active { .. },
            }) => Some(id),
            _ => None,
        })?
        .ok_or_err()
    }

    /// Same as [`Self::send_message_bytes`], but using a voucher.
    pub async fn send_message_bytes_with_voucher(
        &self,
        voucher_id: VoucherId,
        destination: ActorId,
        payload: impl Into<Vec<u8>>,
        gas_limit: u64,
        value: u128,
        keep_alive: bool,
    ) -> Result<TxOutput<MessageId>> {
        self.run_tx(tx().gear_voucher().call(
            voucher_id,
            PrepaidCall::SendMessage {
                destination,
                payload: payload.into(),
                gas_limit,
                value,
                keep_alive,
            },
        ))
        .await?
        .find_map(|event| match event {
            Event::Gear(gear::Event::MessageQueued {
                id,
                entry: MessageEntry::Handle,
                ..
            }) => Some(id),
            _ => None,
        })?
        .ok_or_err()
    }

    /// Same as [`Self::send_message`], but using a voucher.
    pub async fn send_message_with_voucher(
        &self,
        voucher_id: VoucherId,
        destination: ActorId,
        payload: impl Encode,
        gas_limit: u64,
        value: u128,
        keep_alive: bool,
    ) -> Result<TxOutput<MessageId>> {
        self.send_message_bytes_with_voucher(
            voucher_id,
            destination,
            payload.encode(),
            gas_limit,
            value,
            keep_alive,
        )
        .await
    }

    /// Same as [`Self::send_reply_bytes`], but using a voucher.
    pub async fn send_reply_bytes_with_voucher(
        &self,
        voucher_id: VoucherId,
        reply_to_id: MessageId,
        payload: impl Into<Vec<u8>>,
        gas_limit: u64,
        value: u128,
        keep_alive: bool,
    ) -> Result<TxOutput<(MessageId, u128)>> {
        let data = self.mailbox_message(reply_to_id).await?;

        self.run_tx(tx().gear_voucher().call(
            voucher_id,
            PrepaidCall::SendReply {
                reply_to_id,
                payload: payload.into(),
                gas_limit,
                value,
                keep_alive,
            },
        ))
        .await?
        .find_map(|event| match event {
            Event::Gear(gear::Event::MessageQueued {
                id,
                entry: MessageEntry::Reply(_),
                ..
            }) => Some(id),
            _ => None,
        })?
        .map(move |opt| {
            opt.map(move |id| {
                (
                    id,
                    data.expect("Data appearance guaranteed above").0.value(),
                )
            })
        })
        .ok_or_err()
    }

    /// Same as [`Self::send_reply`], but using a voucher.
    pub async fn send_reply_with_voucher(
        &self,
        voucher_id: VoucherId,
        reply_to_id: MessageId,
        payload: impl Encode,
        gas_limit: u64,
        value: u128,
        keep_alive: bool,
    ) -> Result<TxOutput<(MessageId, u128)>> {
        self.send_reply_bytes_with_voucher(
            voucher_id,
            reply_to_id,
            payload.encode(),
            gas_limit,
            value,
            keep_alive,
        )
        .await
    }

    /// Decline existing and not expired voucher via call pre-paid with a voucher.
    ///
    /// See [`Self::decline_voucher`] for details.
    pub async fn decline_voucher_with_voucher(
        &self,
        voucher_id: VoucherId,
    ) -> Result<TxOutput<VoucherId>> {
        self.run_tx(
            tx().gear_voucher()
                .call(voucher_id, PrepaidCall::DeclineVoucher),
        )
        .await?
        .find_map(|event| match event {
            Event::GearVoucher(gear_voucher::Event::VoucherDeclined { voucher_id, .. }) => {
                Some(voucher_id)
            }
            _ => None,
        })?
        .ok_or_err()
    }
}
