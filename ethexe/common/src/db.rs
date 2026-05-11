// This file is part of Gear.
//
// Copyright (C) 2024-2025 Gear Technologies Inc.
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

//! Common db types and traits.

use crate::{
    Address, BlockHeader, CodeBlobInfo, Digest, HashOf, ProgramStates, ProtocolTimelines, Schedule,
    SimpleBlockData, ValidatorsVec,
    events::BlockEvent,
    gear::StateTransition,
    injected::{InjectedTransaction, Promise, SignedCompactPromise, SignedInjectedTransaction},
    mb::Transactions,
};
use alloc::{
    collections::{BTreeSet, VecDeque},
    vec::Vec,
};
use gear_core::{
    code::{CodeMetadata, InstrumentedCode},
    ids::{ActorId, CodeId},
};
use gprimitives::H256;
use parity_scale_codec::{Decode, Encode};
use scale_info::TypeInfo;

/// Ethexe metadata associated with an on-chain block.
#[derive(Clone, Debug, Default, Encode, Decode, TypeInfo, PartialEq, Eq, Hash)]
pub struct BlockMeta {
    pub prepared: bool,
    pub codes_queue: Option<VecDeque<CodeId>>,
    pub last_committed_batch: Option<Digest>,
    /// Last committed MB hash visible from this Eth block.
    pub last_committed_mb: Option<H256>,
    // +_+_+ rename to `last_committed_eb`
    /// Eth block hash from the last committed `ChainCommitment::last_advanced_eth_block`.
    pub last_committed_advanced_eth_block: Option<H256>,
    pub latest_era_validators_committed: Option<u64>,
}

#[auto_impl::auto_impl(&, Box)]
pub trait HashStorageRO {
    fn read_by_hash(&self, hash: H256) -> Option<Vec<u8>>;
}

#[auto_impl::auto_impl(&, Box)]
pub trait BlockMetaStorageRO {
    /// NOTE: if `BlockMeta` doesn't exist in the database, it will return the default value.
    fn block_meta(&self, block_hash: H256) -> BlockMeta;
}

#[auto_impl::auto_impl(&)]
pub trait BlockMetaStorageRW: BlockMetaStorageRO {
    /// NOTE: if `BlockMeta` doesn't exist in the database,
    /// it will be created with default values and then will be mutated.
    fn mutate_block_meta(&self, block_hash: H256, f: impl FnOnce(&mut BlockMeta));
}

#[auto_impl::auto_impl(&, Box)]
pub trait CodesStorageRO {
    fn original_code_exists(&self, code_id: CodeId) -> bool;
    fn original_code(&self, code_id: CodeId) -> Option<Vec<u8>>;
    fn program_code_id(&self, program_id: ActorId) -> Option<CodeId>;
    fn instrumented_code_exists(&self, runtime_id: u32, code_id: CodeId) -> bool;
    fn instrumented_code(&self, runtime_id: u32, code_id: CodeId) -> Option<InstrumentedCode>;
    fn code_metadata(&self, code_id: CodeId) -> Option<CodeMetadata>;
    fn code_valid(&self, code_id: CodeId) -> Option<bool>;
    fn valid_codes(&self) -> BTreeSet<CodeId>;
}

#[auto_impl::auto_impl(&)]
pub trait CodesStorageRW: CodesStorageRO {
    fn set_original_code(&self, code: &[u8]) -> CodeId;
    fn set_program_code_id(&self, program_id: ActorId, code_id: CodeId);
    fn set_instrumented_code(&self, runtime_id: u32, code_id: CodeId, code: InstrumentedCode);
    fn set_code_metadata(&self, code_id: CodeId, code_metadata: CodeMetadata);
    fn set_code_valid(&self, code_id: CodeId, valid: bool);
}

#[auto_impl::auto_impl(&, Box)]
pub trait OnChainStorageRO {
    fn block_header(&self, block_hash: H256) -> Option<BlockHeader>;
    fn block_events(&self, block_hash: H256) -> Option<Vec<BlockEvent>>;
    fn code_blob_info(&self, code_id: CodeId) -> Option<CodeBlobInfo>;
    fn block_synced(&self, block_hash: H256) -> bool;
    fn validators(&self, era_index: u64) -> Option<ValidatorsVec>;

    fn block_simple_data(&self, block_hash: H256) -> Option<SimpleBlockData> {
        self.block_header(block_hash).map(|header| SimpleBlockData {
            hash: block_hash,
            header,
        })
    }
}

#[auto_impl::auto_impl(&)]
pub trait OnChainStorageRW: OnChainStorageRO {
    fn set_block_header(&self, block_hash: H256, header: BlockHeader);
    fn set_block_events(&self, block_hash: H256, events: &[BlockEvent]);
    fn set_code_blob_info(&self, code_id: CodeId, code_info: CodeBlobInfo);
    fn set_validators(&self, era_index: u64, validator_set: ValidatorsVec);
    fn set_block_synced(&self, block_hash: H256);
}

#[auto_impl::auto_impl(&)]
pub trait InjectedStorageRO {
    /// Returns the transactions by its hash.
    fn injected_transaction(
        &self,
        hash: HashOf<InjectedTransaction>,
    ) -> Option<SignedInjectedTransaction>;

    /// Returns the promise by its transaction hash.
    fn promise(&self, hash: HashOf<InjectedTransaction>) -> Option<Promise>;

    /// Returns the compact promise by its transaction hash.
    fn compact_promise(&self, hash: HashOf<InjectedTransaction>) -> Option<SignedCompactPromise>;
}

#[auto_impl::auto_impl(&)]
pub trait InjectedStorageRW: InjectedStorageRO {
    fn set_injected_transaction(&self, tx: SignedInjectedTransaction);

    fn set_promise(&self, promise: &Promise);

    fn set_compact_promise(&self, promise: &SignedCompactPromise);
}

// +_+_+ rename to CompactMB
/// MB static identity. Keyed by the Blake2b envelope hash; existence implies
/// the matching `Transactions` blob is in CAS at `transactions_hash`.
#[derive(Debug, Clone, Default, Encode, Decode, TypeInfo, PartialEq, Eq, Hash)]
pub struct CompactBlock {
    pub parent: H256,
    pub height: u64,
    pub transactions_hash: H256,
}

/// MB dynamic state. `last_advanced_block` is propagated forward at save time
/// (resets on `AdvanceTillEthereumBlock`); `synced` requires this MB and every
/// ancestor to be persisted.
#[derive(Debug, Clone, Default, Encode, Decode, TypeInfo, PartialEq, Eq, Hash)]
pub struct MbMeta {
    pub computed: bool,
    pub synced: bool,
    // +_+_+ rename to `last_advanced_eb`
    pub last_advanced_block: H256,
}

#[auto_impl::auto_impl(&, Box)]
pub trait MbStorageRO {
    /// Static identity (parent + height + `transactions_hash`).
    /// Existence implies the matching [`Transactions`] blob is in the
    /// CAS at `transactions_hash`.
    fn mb_compact_block(&self, mb_hash: H256) -> Option<CompactBlock>;
    /// Read the [`Transactions`] blob from CAS by its content hash.
    fn transactions(&self, transactions_hash: H256) -> Option<Transactions>;
    fn mb_program_states(&self, mb_hash: H256) -> Option<ProgramStates>;
    fn mb_outcome(&self, mb_hash: H256) -> Option<Vec<StateTransition>>;
    fn mb_schedule(&self, mb_hash: H256) -> Option<Schedule>;
    fn mb_meta(&self, mb_hash: H256) -> MbMeta;
}

#[auto_impl::auto_impl(&)]
pub trait MbStorageRW: MbStorageRO {
    fn set_mb_compact_block(&self, mb_hash: H256, compact: CompactBlock);
    /// Write a [`Transactions`] blob into the CAS and return its hash
    /// (the value stored in [`CompactBlock::transactions_hash`]).
    fn set_transactions(&self, transactions: Transactions) -> H256;
    fn set_mb_program_states(&self, mb_hash: H256, program_states: ProgramStates);
    fn set_mb_outcome(&self, mb_hash: H256, outcome: Vec<StateTransition>);
    fn set_mb_schedule(&self, mb_hash: H256, schedule: Schedule);
    fn mutate_mb_meta(&self, mb_hash: H256, f: impl FnOnce(&mut MbMeta));
}

pub struct PreparedBlockData {
    pub header: BlockHeader,
    pub events: Vec<BlockEvent>,
    pub latest_era_with_committed_validators: u64,
    pub codes_queue: VecDeque<CodeId>,
    pub last_committed_batch: Digest,
    /// `H256::zero()` for genesis (no MB committed on-chain yet).
    pub last_committed_mb: H256,
    // +_+_+ rename to `last_committed_eb`
    /// `H256::zero()` for genesis (no chain commitment has advanced an
    /// Eth block yet).
    pub last_committed_advanced_eth_block: H256,
}

#[derive(Debug, Clone, Encode, Decode, TypeInfo, PartialEq, Eq)]
pub struct DBConfig {
    pub version: u32,
    pub chain_id: u64,
    pub router_address: Address,
    pub timelines: ProtocolTimelines,
    pub genesis_block_hash: H256,
    pub max_validators: u16,
}

#[derive(Debug, Clone, Encode, Decode, TypeInfo, PartialEq, Eq)]
pub struct DBGlobals {
    pub start_block_hash: H256,

    // +_+_+ rename to `latest_synced_eb`
    pub latest_synced_block: SimpleBlockData,

    // +_+_+ rename to `latest_prepared_eb_hash`
    pub latest_prepared_block_hash: H256,

    /// Hash of the most recent Malachite sequencer block this node
    /// has seen finalized. `H256::zero()` means no MB has ever been
    /// finalized. Updated on every `MalachiteEvent::BlockFinalized`.
    pub latest_finalized_mb_hash: H256,
}

#[cfg(feature = "std")]
mod std_interfaces {
    use super::{DBConfig, DBGlobals};
    use std::sync::RwLockReadGuard;

    #[auto_impl::auto_impl(&, Box)]
    pub trait GlobalsStorageRO {
        fn globals(&self) -> RwLockReadGuard<'_, DBGlobals>;
    }

    #[auto_impl::auto_impl(&, Box)]
    pub trait GlobalsStorageRW: GlobalsStorageRO {
        fn globals_mutate<R>(&self, f: impl FnMut(&mut DBGlobals) -> R) -> R;
    }

    #[auto_impl::auto_impl(&, Box)]
    pub trait ConfigStorageRO {
        fn config(&self) -> RwLockReadGuard<'_, DBConfig>;
    }
}

#[cfg(feature = "std")]
pub use std_interfaces::{ConfigStorageRO, GlobalsStorageRO, GlobalsStorageRW};

#[cfg(feature = "mock")]
mod mock_interfaces {
    use super::{DBConfig, DBGlobals};

    #[auto_impl::auto_impl(&, Box)]
    pub trait SetGlobals {
        fn set_globals(&self, globals: DBGlobals);
    }

    #[auto_impl::auto_impl(&, Box)]
    pub trait SetConfig {
        fn set_config(&self, config: DBConfig);
    }
}

#[cfg(feature = "mock")]
pub use mock_interfaces::{SetConfig, SetGlobals};

#[cfg(test)]
mod tests {
    use super::*;
    use indoc::formatdoc;
    use scale_info::{PortableRegistry, Registry, meta_type};
    use sha3::{Digest, Sha3_256};

    #[test]
    fn ensure_types_unchanged() {
        const EXPECTED_TYPE_INFO_HASH: &str =
            "db6bc4839e6492298211d675ad7e98295343bbb3e617dca097d4bbd928fc4a9a";

        let types = [
            meta_type::<BlockMeta>(),
            meta_type::<InstrumentedCode>(),
            meta_type::<CodeMetadata>(),
            meta_type::<BlockHeader>(),
            meta_type::<BlockEvent>(),
            meta_type::<CodeBlobInfo>(),
            meta_type::<ValidatorsVec>(),
            meta_type::<ProtocolTimelines>(),
            meta_type::<HashOf<InjectedTransaction>>(),
            meta_type::<SignedInjectedTransaction>(),
            meta_type::<ProgramStates>(),
            meta_type::<StateTransition>(),
            meta_type::<Schedule>(),
            meta_type::<MbMeta>(),
            meta_type::<CompactBlock>(),
            meta_type::<crate::mb::Transactions>(),
            meta_type::<DBConfig>(),
            meta_type::<DBGlobals>(),
        ];

        let mut registry = Registry::new();
        registry.register_types(types);

        let portable_registry = PortableRegistry::from(registry);
        let encoded_registry = portable_registry.encode();
        let type_info_hash = hex::encode(Sha3_256::digest(encoded_registry));

        if type_info_hash != EXPECTED_TYPE_INFO_HASH {
            panic!(
                "{}",
                formatdoc!(
                    "
                    Some of database types has been changed.

                    It can break existing databases, so be very careful and think at least
                    twice before committing such changes. Ensure that SCALE representations
                    of all changed database types are still the same.

                    If you know what exactly has been changed and sure about it,
                    please update `EXPECTED_TYPE_INFO_HASH` constant in this test
                    to the new value to fix the assertion.

                    Expected hash: {EXPECTED_TYPE_INFO_HASH}
                    Found hash:    {type_info_hash}
                    "
                )
            );
        }
    }
}
