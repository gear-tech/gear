// This file is part of Gear.

// Copyright (C) 2022-2025 Gear Technologies Inc.
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
use crate::{Error, utils};
use gear_core::{
    gas::LockId,
    ids::{ActorId, CodeId, MessageId},
    memory::PageBuf,
};
use gear_utils::{MemoryPageDump, ProgramMemoryDump};
use gsdk::{
    AsGear, GearGasNode, GearGasNodeId, IntoAccountId32, IntoSubstrate,
    config::GearConfig,
    ext::subxt::{blocks::ExtrinsicEvents, utils::H256},
    gear::{
        Event,
        gear::Event as GearEvent,
        runtime_types::{
            frame_system::pallet::Call as SystemCall,
            gear_common::event::{CodeChangeKind, MessageEntry},
            pallet_balances::{
                pallet::Call as BalancesCall,
                types::{AccountData, ExtraFlags},
            },
            pallet_gear::pallet::Call as GearCall,
            pallet_gear_bank::pallet::BankAccount,
            pallet_gear_voucher::internal::VoucherId,
            sp_weights::weight_v2::Weight,
            vara_runtime::RuntimeCall,
        },
        system::Event as SystemEvent,
        utility::Event as UtilityEvent,
    },
};
use hex::ToHex;
use parity_scale_codec::{Decode, Encode};
use std::{
    collections::{BTreeMap, HashSet},
    path::Path,
};

impl GearApi {
    /// Migrates an active program identified by `src_program_id` onto another
    /// node identified by `dest_node_api` and returns the migrated program
    /// identifier. All source program data is taken at the time of
    /// `src_block_hash` if it is specified or the most recent one.
    pub async fn migrate_program(
        &self,
        src_program_id: ActorId,
        src_block_hash: Option<H256>,
        dest_node_api: &GearApi,
    ) -> Result<ActorId> {
        if dest_node_api
            .0
            .api()
            .active_program(src_program_id)
            .await
            .is_ok()
        {
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
            .0
            .api()
            .account_data_at(src_program_id, src_block_hash)
            .await
            .or_else(|e| {
                if let gsdk::Error::StorageEntryNotFound = e {
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
            .0
            .api()
            .bank_info_at(src_program_id, src_block_hash)
            .await
            .or_else(|e| {
                if let gsdk::Error::StorageEntryNotFound = e {
                    Ok(BankAccount { gas: 0, value: 0 })
                } else {
                    Err(e)
                }
            })?;

        let bank_address = self.0.api().bank_address().await?;

        let src_bank_account_data = self
            .0
            .api()
            .account_data_at(bank_address.clone(), src_block_hash)
            .await
            .or_else(|e| {
                if let gsdk::Error::StorageEntryNotFound = e {
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
            .0
            .api()
            .active_program_at(src_program_id, src_block_hash)
            .await?;

        let src_program_pages = self
            .0
            .api()
            .program_pages_at(src_program_id, src_block_hash)
            .await?;

        let src_program_reserved_gas_node_ids: Vec<GearGasNodeId> = src_program
            .gas_reservation_map
            .iter()
            .map(|gr| GearGasNodeId::Reservation(gr.0))
            .collect();

        let src_program_reserved_gas_nodes = self
            .0
            .api()
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
            .0
            .api()
            .instrumented_code_storage_at(src_code_id, src_block_hash)
            .await?;

        let src_code_metadata = self
            .0
            .api()
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
            .0
            .storage()
            .set_bank_account_storage(
                src_program_id.into_account_id(),
                src_program_account_bank_data,
            )
            .await?;

        dest_node_api
            .0
            .storage()
            .set_instrumented_code_storage(src_code_id, &src_instrumented_code)
            .await?;

        dest_node_api
            .0
            .storage()
            .set_code_metadata_storage(src_code_id, &src_code_metadata)
            .await?;

        dest_node_api
            .0
            .storage()
            .set_gas_nodes(&src_program_reserved_gas_nodes)
            .await?;

        for account_with_reserved_funds in accounts_with_reserved_funds {
            let src_account_bank_data = self
                .0
                .api()
                .bank_info_at(account_with_reserved_funds.clone(), src_block_hash)
                .await
                .or_else(|e| {
                    if let gsdk::Error::StorageEntryNotFound = e {
                        Ok(BankAccount { gas: 0, value: 0 })
                    } else {
                        Err(e)
                    }
                })?;

            let dest_account_data = dest_node_api
                .0
                .api()
                .account_data(account_with_reserved_funds.clone())
                .await
                .or_else(|e| {
                    if let gsdk::Error::StorageEntryNotFound = e {
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
                .0
                .api()
                .bank_info_at(account_with_reserved_funds.clone(), None)
                .await
                .or_else(|e| {
                    if let gsdk::Error::StorageEntryNotFound = e {
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
                .0
                .storage()
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

        let dest_gas_total_issuance =
            dest_node_api.0.api().total_issuance().await.or_else(|e| {
                if let gsdk::Error::StorageEntryNotFound = e {
                    Ok(0)
                } else {
                    Err(e)
                }
            })?;

        dest_node_api
            .0
            .storage()
            .set_total_issuance(
                dest_gas_total_issuance.saturating_add(src_program_reserved_gas_total),
            )
            .await?;

        dest_node_api
            .0
            .storage()
            .set_gpages(dest_program_id, &src_program_pages)
            .await?;

        src_program.expiration_block = dest_node_api.last_block_number().await?;
        dest_node_api
            .0
            .storage()
            .set_gprog(dest_program_id, src_program)
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
    ) -> Result {
        let program_pages = self
            .0
            .api()
            .program_pages_at(program_id, block_hash)
            .await?
            .into_iter()
            .map(|(page, data)| {
                MemoryPageDump::new(
                    page,
                    PageBuf::decode(&mut &*data).expect("Couldn't decode PageBuf"),
                )
            })
            .collect();

        let program_account_data = self
            .0
            .api()
            .account_data_at(program_id, block_hash)
            .await
            .or_else(|e| {
                if let gsdk::Error::StorageEntryNotFound = e {
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

        ProgramMemoryDump {
            pages: program_pages,
            balance: program_account_data.free,
            reserved_balance: program_account_data.reserved,
        }
        .save_to_file(file_path);

        Ok(())
    }

    /// Replace entire program memory with one saved earlier in gclient/gtest
    pub async fn replace_program_memory<P: AsRef<Path>>(
        &self,
        program_id: ActorId,
        file_path: P,
    ) -> Result {
        let memory_dump = ProgramMemoryDump::load_from_file(file_path);
        let pages = memory_dump
            .pages
            .into_iter()
            .map(|page| page.into_gear_page())
            .collect();

        self.force_set_balance(program_id, memory_dump.balance)
            .await?;

        self.0.storage().set_gpages(program_id, &pages).await?;

        Ok(())
    }

    fn process_set_code(&self, events: &ExtrinsicEvents<GearConfig>) -> Result<()> {
        for event in events.iter() {
            let event = event?.as_gear()?;
            if let Event::System(SystemEvent::CodeUpdated) = event {
                return Ok(());
            }
        }

        Err(Error::EventNotFound)
    }

    /// Upgrade the runtime with the `code` containing the Wasm code of the new
    /// runtime.
    ///
    /// Sends the
    /// [`pallet_system::set_code`](https://crates.parity.io/frame_system/pallet/struct.Pallet.html#method.set_code)
    /// extrinsic.
    pub async fn set_code(&self, code: impl AsRef<[u8]>) -> Result<H256> {
        let (block_hash, events) = self
            .0
            .calls()
            .sudo_unchecked_weight(
                RuntimeCall::System(SystemCall::set_code {
                    code: code.as_ref().to_vec(),
                }),
                Weight {
                    ref_time: 0,
                    proof_size: 0,
                },
            )
            .await?;
        self.process_set_code(&events)?;
        Ok(block_hash)
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

    /// Upgrade the runtime with the `code` containing the Wasm code of the new
    /// runtime but **without** checks.
    ///
    /// Sends the
    /// [`pallet_system::set_code_without_checks`](https://crates.parity.io/frame_system/pallet/struct.Pallet.html#method.set_code_without_checks)
    /// extrinsic.
    pub async fn set_code_without_checks(&self, code: impl AsRef<[u8]>) -> Result<H256> {
        let (block_hash, events) = self
            .0
            .calls()
            .sudo_unchecked_weight(
                RuntimeCall::System(SystemCall::set_code_without_checks {
                    code: code.as_ref().to_vec(),
                }),
                Weight {
                    ref_time: 0,
                    proof_size: 0,
                },
            )
            .await?;
        self.process_set_code(&events)?;
        Ok(block_hash)
    }

    /// Upgrade the runtime by reading the code from the file located at the
    /// `path`.
    ///
    /// Same as [`set_code_without_checks`](Self::set_code_without_checks), but
    /// reads the runtime code from a file instead of using a byte vector.
    pub async fn set_code_without_checks_by_path(&self, path: impl AsRef<Path>) -> Result<H256> {
        let code = utils::code_from_os(path)?;
        self.set_code_without_checks(code).await
    }

    /// Set the free balance of the `to` account to `new_free`.
    ///
    /// Sends the [`pallet_balances::set_balance`](https://crates.parity.io/pallet_balances/pallet/struct.Pallet.html#method.set_balance) extrinsic.
    pub async fn force_set_balance(
        &self,
        to: impl IntoAccountId32,
        new_free: u128,
    ) -> Result<H256> {
        let events = self
            .0
            .calls()
            .sudo_unchecked_weight(
                RuntimeCall::Balances(BalancesCall::force_set_balance {
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
            .await?;
        Ok(events.0)
    }

    /// Same as [`upload_code`](Self::upload_code), but upload code
    /// using voucher.
    pub async fn upload_code_with_voucher(
        &self,
        voucher_id: VoucherId,
        code: impl AsRef<[u8]>,
    ) -> Result<(CodeId, H256)> {
        let tx = self
            .0
            .calls()
            .upload_code_with_voucher(voucher_id, code.as_ref().to_vec())
            .await?;

        for event in tx.wait_for_success().await?.iter() {
            if let Event::Gear(GearEvent::CodeChanged {
                id,
                change: CodeChangeKind::Active { .. },
            }) = event?.as_gear()?
            {
                return Ok((id, tx.block_hash()));
            }
        }

        Err(Error::EventNotFound)
    }

    /// Same as [`send_message_bytes`](Self::send_message_bytes), but sends a
    /// message using voucher.
    pub async fn send_message_bytes_with_voucher(
        &self,
        voucher_id: VoucherId,
        destination: ActorId,
        payload: impl AsRef<[u8]>,
        gas_limit: u64,
        value: u128,
        keep_alive: bool,
    ) -> Result<(MessageId, H256)> {
        let payload = payload.as_ref().to_vec();

        let tx = self
            .0
            .calls()
            .send_message_with_voucher(
                voucher_id,
                destination,
                payload,
                gas_limit,
                value,
                keep_alive,
            )
            .await?;

        for event in tx.wait_for_success().await?.iter() {
            if let Event::Gear(GearEvent::MessageQueued {
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

    /// Same as [`send_message_bytes_with_voucher`](Self::send_message_bytes_with_voucher), but sends a
    /// message with encoded `payload`.
    pub async fn send_message_with_voucher(
        &self,
        voucher_id: VoucherId,
        destination: ActorId,
        payload: impl Encode,
        gas_limit: u64,
        value: u128,
        keep_alive: bool,
    ) -> Result<(MessageId, H256)> {
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

    /// Same as [`send_reply_bytes`](Self::send_reply_bytes), but sends a reply
    /// using voucher.
    pub async fn send_reply_bytes_with_voucher(
        &self,
        voucher_id: VoucherId,
        reply_to_id: MessageId,
        payload: impl AsRef<[u8]>,
        gas_limit: u64,
        value: u128,
        keep_alive: bool,
    ) -> Result<(MessageId, u128, H256)> {
        let payload = payload.as_ref().to_vec();

        let data = self.0.storage().mailbox_message(reply_to_id).await?;

        let tx = self
            .0
            .calls()
            .send_reply_with_voucher(
                voucher_id,
                reply_to_id,
                payload,
                gas_limit,
                value,
                keep_alive,
            )
            .await?;

        let events = tx.wait_for_success().await?;

        let (message, _interval) = data.expect("Data appearance guaranteed above");

        for event in events.iter() {
            if let Event::Gear(GearEvent::MessageQueued {
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

    /// Same as [`send_reply_bytes_with_voucher`](Self::send_reply_bytes_with_voucher), but sends a reply
    /// with encoded `payload`.
    pub async fn send_reply_with_voucher(
        &self,
        voucher_id: VoucherId,
        reply_to_id: MessageId,
        payload: impl Encode,
        gas_limit: u64,
        value: u128,
        keep_alive: bool,
    ) -> Result<(MessageId, u128, H256)> {
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
}
