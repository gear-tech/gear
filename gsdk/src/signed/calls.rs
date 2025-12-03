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
use std::{collections::HashMap, path::Path};

use crate::{
    AsGear, Error, Event, IntoAccountId32, Result, SignedApi, TxEvents, TxInBlock,
    gear::{
        self,
        runtime_types::{
            gear_common::event::{CodeChangeKind, MessageEntry},
            pallet_gear_voucher::internal::{PrepaidCall, VoucherId},
            sp_weights::weight_v2::Weight,
            vara_runtime::RuntimeCall,
        },
    },
    utils,
};
use gear_core::ids::{ActorId, CodeId, MessageId};
use parity_scale_codec::Encode;
use subxt::utils::H256;

// pallet-balances
impl SignedApi {
    /// Transfer `value` to `destination`'s account, but ensures
    /// that the original account won't be killed by the transfer
    /// transaction.
    ///
    /// Sends the
    /// [`pallet_balances::transfer_keep_alive`](https://crates.parity.io/pallet_balances/pallet/struct.Pallet.html#method.transfer_keep_alive)
    /// extrinsic.
    ///
    /// This function returns a hash of the block with the transfer transaction.
    pub async fn transfer_keep_alive(
        &self,
        dest: impl IntoAccountId32,
        value: u128,
    ) -> Result<H256> {
        let tx = self
            .run_tx(
                gear::tx()
                    .balances()
                    .transfer_keep_alive(dest.into_account_id().into(), value),
            )
            .await?;

        for event in tx.wait_for_success().await?.iter() {
            if let Event::Balances(gear::balances::Event::Transfer { .. }) = event?.as_gear()? {
                return Ok(tx.block_hash());
            }
        }

        // Sending zero value is a no-op, so now event occurs.
        if value == 0 {
            return Ok(tx.block_hash());
        }

        Err(Error::EventNotFound)
    }

    /// Transfer `value` to `destination`'s account.
    ///
    /// Sends the
    /// [`pallet_balances::transfer`](https://crates.parity.io/pallet_balances/pallet/struct.Pallet.html#method.transfer)
    /// extrinsic.
    ///
    /// This function returns a hash of the block with the transfer transaction.
    pub async fn transfer_allow_death(
        &self,
        dest: impl IntoAccountId32,
        value: u128,
    ) -> Result<H256> {
        let tx = self
            .run_tx(
                gear::tx()
                    .balances()
                    .transfer_allow_death(dest.into_account_id().into(), value),
            )
            .await?;

        for event in tx.wait_for_success().await?.iter() {
            if let Event::Balances(gear::balances::Event::Transfer { .. }) = event?.as_gear()? {
                return Ok(tx.block_hash());
            }
        }

        // Sending zero value is a no-op, so now event occurs.
        if value == 0 {
            return Ok(tx.block_hash());
        }

        Err(Error::EventNotFound)
    }

    /// Transfer the entire transferable balance from the caller to `destination`'s account.
    ///
    /// Sends the
    /// [`pallet_balances::transfer_all`](https://crates.parity.io/pallet_balances/pallet/struct.Pallet.html#method.transfer_all)
    /// extrinsic.
    ///
    /// This function returns a hash of the block with the transfer transaction.
    pub async fn transfer_all(&self, dest: impl IntoAccountId32, keep_alive: bool) -> Result<H256> {
        let tx = self
            .run_tx(
                gear::tx()
                    .balances()
                    .transfer_all(dest.into_account_id().into(), keep_alive),
            )
            .await?;

        for event in tx.wait_for_success().await?.iter() {
            if let Event::Balances(gear::balances::Event::Transfer { .. }) = event?.as_gear()? {
                return Ok(tx.block_hash());
            }
        }

        Err(Error::EventNotFound)
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
    ) -> Result<(MessageId, ActorId, H256)> {
        let salt = salt.into();
        let payload = payload.into();

        let tx = self
            .run_tx(
                gear::tx()
                    .gear()
                    .create_program(code_id, salt, payload, gas_limit, value, false),
            )
            .await?;

        for event in tx.wait_for_success().await?.iter() {
            if let Event::Gear(gear::gear::Event::MessageQueued {
                id,
                destination,
                entry: MessageEntry::Init,
                ..
            }) = event?.as_gear()?
            {
                return Ok((id, destination, tx.block_hash()));
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
        args: impl IntoIterator<Item = (CodeId, impl Into<Vec<u8>>, impl Into<Vec<u8>>, u64, u128)>,
    ) -> Result<(Vec<Result<(MessageId, ActorId)>>, H256)> {
        self.run_batch(
            args.into_iter()
                .map(|(code_id, salt, payload, gas_limit, value)| {
                    RuntimeCall::Gear(gear::gear::Call::create_program {
                        code_id,
                        salt: salt.into(),
                        init_payload: payload.into(),
                        gas_limit,
                        value,
                        keep_alive: false,
                    })
                }),
            |event| match event {
                Event::Gear(gear::gear::Event::MessageQueued {
                    id,
                    destination,
                    entry: MessageEntry::Init,
                    ..
                }) => Some(Ok((id, destination))),
                Event::Utility(gear::utility::Event::ItemFailed { error }) => {
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
    ) -> Result<(MessageId, ActorId, H256)> {
        self.create_program_bytes(code_id, salt, payload.encode(), gas_limit, value)
            .await
    }

    /// Claim value from the mailbox message identified by `message_id`.
    ///
    /// Sends the
    /// [`pallet_gear::claim_value`](https://docs.rs/pallet_gear/pallet/struct.Pallet.html#method.claim_value)
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
            .mailbox_message(message_id)
            .await?
            .map(|(message, _interval)| message.value());

        let tx = self
            .run_tx(gear::tx().gear().claim_value(message_id))
            .await?;

        for event in tx.wait_for_success().await?.iter() {
            if let Event::Gear(gear::gear::Event::UserMessageRead { .. }) = event?.as_gear()? {
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
                .map(|message_id| RuntimeCall::Gear(gear::gear::Call::claim_value { message_id })),
            |event| match event {
                Event::Gear(gear::gear::Event::UserMessageRead { id, .. }) => Some(Ok(values
                    .remove(&id)
                    .expect("Data appearance guaranteed above"))),
                Event::Utility(gear::utility::Event::ItemFailed { error }) => {
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
        destination: ActorId,
        payload: impl Into<Vec<u8>>,
        gas_limit: u64,
        value: u128,
    ) -> Result<(MessageId, H256)> {
        let tx = self
            .run_tx(gear::tx().gear().send_message(
                destination,
                payload.into(),
                gas_limit,
                value,
                false,
            ))
            .await?;

        for event in tx.wait_for_success().await?.iter() {
            if let Event::Gear(gear::gear::Event::MessageQueued {
                id,
                entry: MessageEntry::Handle,
                ..
            }) = event?.as_gear()?
            {
                return Ok((id, tx.block_hash()));
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
        args: impl IntoIterator<Item = (ActorId, impl Into<Vec<u8>>, u64, u128)>,
    ) -> Result<(Vec<Result<(MessageId, ActorId)>>, H256)> {
        self.run_batch(
            args.into_iter()
                .map(|(destination, payload, gas_limit, value)| {
                    RuntimeCall::Gear(gear::gear::Call::send_message {
                        destination,
                        payload: payload.into(),
                        gas_limit,
                        value,
                        keep_alive: false,
                    })
                }),
            |event| match event {
                Event::Gear(gear::gear::Event::MessageQueued {
                    id,
                    destination,
                    entry: MessageEntry::Handle,
                    ..
                }) => Some(Ok((id, destination))),
                Event::Utility(gear::utility::Event::ItemFailed { error }) => {
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
    ) -> Result<(MessageId, u128, H256)> {
        let data = self.mailbox_message(reply_to_id).await?;

        let tx = self
            .run_tx(gear::tx().gear().send_reply(
                reply_to_id,
                payload.into(),
                gas_limit,
                value,
                false,
            ))
            .await?;

        let events = tx.wait_for_success().await?;

        let (message, _interval) = data.expect("Data appearance guaranteed above");

        for event in events.iter() {
            if let Event::Gear(gear::gear::Event::MessageQueued {
                id,
                entry: MessageEntry::Reply(_),
                ..
            }) = event?.as_gear()?
            {
                return Ok((id, message.value(), tx.block_hash()));
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
        args: impl IntoIterator<Item = (MessageId, impl Into<Vec<u8>>, u64, u128)> + Clone,
    ) -> Result<(Vec<Result<(MessageId, ActorId, u128)>>, H256)> {
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
                    RuntimeCall::Gear(gear::gear::Call::send_reply {
                        reply_to_id,
                        payload: payload.into(),
                        gas_limit,
                        value,
                        keep_alive: false,
                    })
                }),
            |event| match event {
                Event::Gear(gear::gear::Event::MessageQueued {
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
                Event::Utility(gear::utility::Event::ItemFailed { error }) => {
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
    ) -> Result<(MessageId, u128, H256)> {
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
    pub async fn upload_code(&self, code: impl Into<Vec<u8>>) -> Result<(CodeId, H256)> {
        let tx = self
            .run_tx(gear::tx().gear().upload_code(code.into()))
            .await?;

        for event in tx.wait_for_success().await?.iter() {
            if let Event::Gear(gear::gear::Event::CodeChanged {
                id,
                change: CodeChangeKind::Active { .. },
            }) = event?.as_gear()?
            {
                return Ok((id, tx.block_hash()));
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
        args: impl IntoIterator<Item = impl Into<Vec<u8>>>,
    ) -> Result<(Vec<Result<CodeId>>, H256)> {
        self.run_batch(
            args.into_iter()
                .map(|code| RuntimeCall::Gear(gear::gear::Call::upload_code { code: code.into() })),
            |event| match event {
                Event::Gear(gear::gear::Event::CodeChanged {
                    id,
                    change: CodeChangeKind::Active { .. },
                }) => Some(Ok(id)),
                Event::Utility(gear::utility::Event::ItemFailed { error }) => {
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
    pub async fn upload_code_by_path(&self, path: impl AsRef<Path>) -> Result<(CodeId, H256)> {
        let code = utils::read_wasm_file(path).await?;
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
    ) -> Result<(MessageId, ActorId, H256)> {
        let tx = self
            .run_tx(gear::tx().gear().upload_program(
                code.into(),
                salt.into(),
                payload.into(),
                gas_limit,
                value,
                false,
            ))
            .await?;

        for event in tx.wait_for_success().await?.iter() {
            if let Event::Gear(gear::gear::Event::MessageQueued {
                id,
                destination,
                entry: MessageEntry::Init,
                ..
            }) = event?.as_gear()?
            {
                return Ok((id, destination, tx.block_hash()));
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
                impl Into<Vec<u8>>,
                impl Into<Vec<u8>>,
                impl Into<Vec<u8>>,
                u64,
                u128,
            ),
        >,
    ) -> Result<(Vec<Result<(MessageId, ActorId)>>, H256)> {
        self.run_batch(
            args.into_iter()
                .map(|(code, salt, payload, gas_limit, value)| {
                    RuntimeCall::Gear(gear::gear::Call::upload_program {
                        code: code.into(),
                        salt: salt.into(),
                        init_payload: payload.into(),
                        gas_limit,
                        value,
                        keep_alive: false,
                    })
                }),
            |event| match event {
                Event::Gear(gear::gear::Event::MessageQueued {
                    id,
                    destination,
                    entry: MessageEntry::Init,
                    ..
                }) => Some(Ok((id, destination))),
                Event::Utility(gear::utility::Event::ItemFailed { error }) => {
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
    ) -> Result<(MessageId, ActorId, H256)> {
        let code = utils::read_wasm_file(path).await?;
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
    ) -> Result<(MessageId, ActorId, H256)> {
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
    ) -> Result<(MessageId, ActorId, H256)> {
        let code = utils::read_wasm_file(path).await?;
        self.upload_program(code, salt, payload, gas_limit, value)
            .await
    }
}

impl SignedApi {
    /// Sends the `pallet-gear-eth-bridge::reset_overflowed_queue` extrinsic.
    ///
    /// This function returns a hash of the block with the transaction.
    pub async fn reset_overflowed_queue(&self, encoded_finality_proof: Vec<u8>) -> Result<H256> {
        let tx = self
            .run_tx(
                gear::tx()
                    .gear_eth_bridge()
                    .reset_overflowed_queue(encoded_finality_proof),
            )
            .await?;

        for event in tx.wait_for_success().await?.iter() {
            if let Event::GearEthBridge(gear::gear_eth_bridge::Event::QueueReset) =
                event?.as_gear()?
            {
                return Ok(tx.block_hash());
            }
        }

        Err(Error::EventNotFound)
    }
}

// pallet-utility
impl SignedApi {
    /// Sends `pallet_utility::force_batch` extrinsic.
    pub async fn force_batch(&self, calls: Vec<RuntimeCall>) -> Result<TxInBlock> {
        self.run_tx(gear::tx().utility().force_batch(calls)).await
    }

    /// Submits a batch of extrinsics and extracts their outputs from events.
    ///
    /// This is a more high-level wrapper around [`Self::force_batch`].
    ///
    /// Returns a vector of extrinsic results and the hash of
    /// its block.
    pub async fn run_batch<O, F: FnMut(Event) -> Option<O>>(
        &self,
        calls: impl IntoIterator<Item = RuntimeCall>,
        mut f: F,
    ) -> Result<(Vec<O>, H256)> {
        let calls = calls.into_iter().collect::<Vec<_>>();
        let expected = calls.len();

        let tx = self.force_batch(calls).await?;

        tx.wait_for_success()
            .await?
            .iter()
            .map(|event| Ok(f(event?.as_gear()?)))
            .filter_map(|res| res.transpose())
            .collect::<Result<Vec<_>>>()
            .and_then(|results| {
                if results.len() == expected {
                    Ok((results, tx.block_hash()))
                } else {
                    Err(Error::IncompleteBatchResult {
                        expected,
                        found: results.len(),
                    })
                }
            })
    }
}

// pallet-sudo
impl SignedApi {
    /// Submits `pallet_sudo::sudo_unchecked_weight` extrinsic.
    pub async fn sudo_unchecked_weight(
        &self,
        call: RuntimeCall,
        weight: Weight,
    ) -> Result<TxEvents> {
        self.sudo_run_tx(gear::tx().sudo().sudo_unchecked_weight(call, weight))
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
    ) -> Result<(VoucherId, H256)> {
        let tx = self
            .run_tx(gear::tx().gear_voucher().issue(
                spender.into_account_id(),
                balance,
                programs,
                code_uploading,
                duration,
            ))
            .await?;

        for event in tx.wait_for_success().await?.iter() {
            if let Event::GearVoucher(gear::gear_voucher::Event::VoucherIssued {
                voucher_id, ..
            }) = event?.as_gear()?
            {
                return Ok((voucher_id, tx.block_hash()));
            }
        }

        Err(Error::EventNotFound)
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
    ) -> Result<(VoucherId, H256)> {
        let tx = self
            .run_tx(gear::tx().gear_voucher().update(
                spender.into_account_id(),
                voucher_id,
                move_ownership.map(|id| id.into_account_id()),
                balance_top_up,
                append_programs,
                code_uploading,
                prolong_duration,
            ))
            .await?;

        for event in tx.wait_for_success().await?.iter() {
            if let Event::GearVoucher(gear::gear_voucher::Event::VoucherUpdated {
                voucher_id,
                ..
            }) = event?.as_gear()?
            {
                return Ok((voucher_id, tx.block_hash()));
            }
        }

        Err(Error::EventNotFound)
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
    ) -> Result<(VoucherId, H256)> {
        let tx = self
            .run_tx(
                gear::tx()
                    .gear_voucher()
                    .revoke(spender.into_account_id(), voucher_id),
            )
            .await?;

        for event in tx.wait_for_success().await?.iter() {
            if let Event::GearVoucher(gear::gear_voucher::Event::VoucherRevoked {
                voucher_id,
                ..
            }) = event?.as_gear()?
            {
                return Ok((voucher_id, tx.block_hash()));
            }
        }

        Err(Error::EventNotFound)
    }

    /// Decline existing and not expired voucher.
    ///
    /// This extrinsic expires voucher of the caller, if it's still active,
    /// allowing it to be revoked.
    ///
    /// Arguments:
    /// * voucher_id:   voucher id to be declined.
    pub async fn decline_voucher(&self, voucher_id: VoucherId) -> Result<(VoucherId, H256)> {
        let tx = self
            .run_tx(gear::tx().gear_voucher().decline(voucher_id))
            .await?;

        for event in tx.wait_for_success().await?.iter() {
            if let Event::GearVoucher(gear::gear_voucher::Event::VoucherDeclined {
                voucher_id,
                ..
            }) = event?.as_gear()?
            {
                return Ok((voucher_id, tx.block_hash()));
            }
        }

        Err(Error::EventNotFound)
    }

    /// Uploads Wasm code via call pre-paid with a voucher.
    ///
    /// See [`Self::upload_code`] for details.
    pub async fn upload_code_with_voucher(
        &self,
        voucher_id: VoucherId,
        code: impl Into<Vec<u8>>,
    ) -> Result<(CodeId, H256)> {
        let tx = self
            .run_tx(
                gear::tx()
                    .gear_voucher()
                    .call(voucher_id, PrepaidCall::UploadCode { code: code.into() }),
            )
            .await?;

        for event in tx.wait_for_success().await?.iter() {
            if let Event::Gear(gear::gear::Event::CodeChanged {
                id,
                change: CodeChangeKind::Active { .. },
            }) = event?.as_gear()?
            {
                return Ok((id, tx.block_hash()));
            }
        }

        Err(Error::EventNotFound)
    }

    /// Sends a message via call pre-paid with a voucher.
    ///
    /// See [`Self::send_message`] for details.
    pub async fn send_message_with_voucher(
        &self,
        voucher_id: VoucherId,
        destination: ActorId,
        payload: impl Into<Vec<u8>>,
        gas_limit: u64,
        value: u128,
        keep_alive: bool,
    ) -> Result<(MessageId, H256)> {
        let tx = self
            .run_tx(gear::tx().gear_voucher().call(
                voucher_id,
                PrepaidCall::SendMessage {
                    destination,
                    payload: payload.into(),
                    gas_limit,
                    value,
                    keep_alive,
                },
            ))
            .await?;

        for event in tx.wait_for_success().await?.iter() {
            if let Event::Gear(gear::gear::Event::MessageQueued {
                id,
                entry: MessageEntry::Handle,
                ..
            }) = event?.as_gear()?
            {
                return Ok((id, tx.block_hash()));
            }
        }

        Err(Error::EventNotFound)
    }

    /// Sends a reply via call pre-paid with a voucher.
    ///
    /// See [`Self::send_reply`] for details.
    pub async fn send_reply_with_voucher(
        &self,
        voucher_id: VoucherId,
        reply_to_id: MessageId,
        payload: impl Into<Vec<u8>>,
        gas_limit: u64,
        value: u128,
        keep_alive: bool,
    ) -> Result<(MessageId, u128, H256)> {
        let data = self.mailbox_message(reply_to_id).await?;

        let tx = self
            .run_tx(gear::tx().gear_voucher().call(
                voucher_id,
                PrepaidCall::SendReply {
                    reply_to_id,
                    payload: payload.into(),
                    gas_limit,
                    value,
                    keep_alive,
                },
            ))
            .await?;

        let events = tx.wait_for_success().await?;

        let (message, _interval) = data.expect("Data appearance guaranteed above");

        for event in events.iter() {
            if let Event::Gear(gear::gear::Event::MessageQueued {
                id,
                entry: MessageEntry::Reply(_),
                ..
            }) = event?.as_gear()?
            {
                return Ok((id, message.value(), tx.block_hash()));
            }
        }

        Err(Error::EventNotFound)
    }

    /// Decline existing and not expired voucher via call pre-paid with a voucher.
    ///
    /// See [`Self::decline_voucher`] for details.
    pub async fn decline_voucher_with_voucher(
        &self,
        voucher_id: VoucherId,
    ) -> Result<(VoucherId, H256)> {
        let tx = self
            .run_tx(
                gear::tx()
                    .gear_voucher()
                    .call(voucher_id, PrepaidCall::DeclineVoucher),
            )
            .await?;

        for event in tx.wait_for_success().await?.iter() {
            if let Event::GearVoucher(gear::gear_voucher::Event::VoucherDeclined {
                voucher_id,
                ..
            }) = event?.as_gear()?
            {
                return Ok((voucher_id, tx.block_hash()));
            }
        }

        Err(Error::EventNotFound)
    }
}
