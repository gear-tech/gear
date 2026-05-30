// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Common db types and traits.

use crate::{
    Address, BlockHeader, CodeBlobInfo, Digest, HashOf, ProgramStates, ProtocolTimelines, Schedule,
    SimpleBlockData, ValidatorsVec,
    events::BlockEvent,
    gear::StateTransition,
    injected::{InjectedTransaction, Promise, SignedInjectedTransaction, SignedTxReceipt},
    malachite::Transactions,
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
    /// Block has been prepared, meaning:
    /// all metadata is ready, all predecessors till start block are prepared too.
    pub prepared: bool,
    /// Queue of code ids waiting for validation status commitment on-chain.
    pub codes_queue: Option<VecDeque<CodeId>>,
    /// Last committed on-chain batch hash (digest).
    pub last_committed_batch: Option<Digest>,
    /// Last committed MB hash.
    pub last_committed_mb: Option<H256>,
    /// Last committed EB hash.
    pub last_committed_eb: Option<H256>,
    /// Latest era with committed validators.
    pub latest_era_validators_committed: Option<u64>,
}

/// Read-only access to a content-addressed byte store keyed by [`H256`] hash.
#[auto_impl::auto_impl(&, Box)]
pub trait HashStorageRO {
    /// Retrieves the raw byte blob stored under `hash`, or `None` if absent.
    fn read_by_hash(&self, hash: H256) -> Option<Vec<u8>>;
}

/// Read-only access to per-block [`BlockMeta`] records.
#[auto_impl::auto_impl(&, Box)]
pub trait BlockMetaStorageRO {
    /// NOTE: if `BlockMeta` doesn't exist in the database, it will return the default value.
    fn block_meta(&self, block_hash: H256) -> BlockMeta;
}

/// Read-write access to per-block [`BlockMeta`] records.
#[auto_impl::auto_impl(&)]
pub trait BlockMetaStorageRW: BlockMetaStorageRO {
    /// NOTE: if `BlockMeta` doesn't exist in the database,
    /// it will be created with default values and then will be mutated.
    fn mutate_block_meta(&self, block_hash: H256, f: impl FnOnce(&mut BlockMeta));
}

/// Read-only access to Gear program code storage (original blobs, instrumented variants, and metadata).
#[auto_impl::auto_impl(&, Box)]
pub trait CodesStorageRO {
    /// Returns `true` if the original WASM blob for `code_id` is present.
    fn original_code_exists(&self, code_id: CodeId) -> bool;
    /// Returns the raw WASM blob stored under `code_id`, if present.
    fn original_code(&self, code_id: CodeId) -> Option<Vec<u8>>;
    /// Returns the [`CodeId`] of the code associated with a deployed program.
    fn program_code_id(&self, program_id: ActorId) -> Option<CodeId>;
    /// Returns `true` if an instrumented variant of `code_id` for `runtime_id` exists.
    fn instrumented_code_exists(&self, runtime_id: u32, code_id: CodeId) -> bool;
    /// Returns the instrumented code for the given runtime version and code ID.
    fn instrumented_code(&self, runtime_id: u32, code_id: CodeId) -> Option<InstrumentedCode>;
    /// Returns the metadata record associated with a code ID.
    fn code_metadata(&self, code_id: CodeId) -> Option<CodeMetadata>;
    /// Returns the on-chain validation result for a code ID: `Some(true)` if valid,
    /// `Some(false)` if invalid, `None` if not yet determined.
    fn code_valid(&self, code_id: CodeId) -> Option<bool>;
    /// Returns the set of all code IDs that have been validated as valid.
    fn valid_codes(&self) -> BTreeSet<CodeId>;
}

/// Read-write access to Gear program code storage.
#[auto_impl::auto_impl(&)]
pub trait CodesStorageRW: CodesStorageRO {
    /// Stores the raw WASM blob and returns its computed [`CodeId`].
    fn set_original_code(&self, code: &[u8]) -> CodeId;
    /// Associates a deployed program with its [`CodeId`].
    fn set_program_code_id(&self, program_id: ActorId, code_id: CodeId);
    /// Stores an instrumented code variant for the given runtime version.
    fn set_instrumented_code(&self, runtime_id: u32, code_id: CodeId, code: InstrumentedCode);
    /// Stores metadata for a code ID.
    fn set_code_metadata(&self, code_id: CodeId, code_metadata: CodeMetadata);
    /// Records the on-chain validation result for a code ID.
    fn set_code_valid(&self, code_id: CodeId, valid: bool);
}

/// Read-only access to Ethereum on-chain block and validator data as observed by the ethexe node.
#[auto_impl::auto_impl(&, Box)]
pub trait OnChainStorageRO {
    /// Returns the [`BlockHeader`] for the given Ethereum block hash.
    fn block_header(&self, block_hash: H256) -> Option<BlockHeader>;
    /// Returns the decoded [`BlockEvent`] list emitted in the given Ethereum block.
    fn block_events(&self, block_hash: H256) -> Option<Vec<BlockEvent>>;
    /// Returns blob location metadata for a code uploaded via EIP-4844.
    fn code_blob_info(&self, code_id: CodeId) -> Option<CodeBlobInfo>;
    /// Returns `true` if the block has been fully synced (header + events persisted).
    fn block_synced(&self, block_hash: H256) -> bool;
    /// Returns the validator set committed for the given era index.
    fn validators(&self, era_index: u64) -> Option<ValidatorsVec>;

    /// Convenience accessor that pairs the block hash with its header into [`SimpleBlockData`].
    fn block_simple_data(&self, block_hash: H256) -> Option<SimpleBlockData> {
        self.block_header(block_hash).map(|header| SimpleBlockData {
            hash: block_hash,
            header,
        })
    }
}

/// Read-write access to Ethereum on-chain block and validator data.
#[auto_impl::auto_impl(&)]
pub trait OnChainStorageRW: OnChainStorageRO {
    /// Persists the [`BlockHeader`] for the given block hash.
    fn set_block_header(&self, block_hash: H256, header: BlockHeader);
    /// Persists the decoded [`BlockEvent`] list for the given block hash.
    fn set_block_events(&self, block_hash: H256, events: &[BlockEvent]);
    /// Persists EIP-4844 blob location metadata for the given code ID.
    fn set_code_blob_info(&self, code_id: CodeId, code_info: CodeBlobInfo);
    /// Records the validator set for the given era index.
    fn set_validators(&self, era_index: u64, validator_set: ValidatorsVec);
    /// Marks the block as fully synced (header + events persisted).
    fn set_block_synced(&self, block_hash: H256);
}

/// Read-only access to injected transaction and promise storage.
#[auto_impl::auto_impl(&)]
pub trait InjectedStorageRO {
    /// Returns the transactions by its hash.
    fn injected_transaction(
        &self,
        hash: HashOf<InjectedTransaction>,
    ) -> Option<SignedInjectedTransaction>;

    /// Returns the promise by its transaction hash.
    fn promise(&self, hash: HashOf<InjectedTransaction>) -> Option<Promise>;

    /// Returns the receipt by its transaction hash.
    fn receipt(&self, hash: HashOf<InjectedTransaction>) -> Option<SignedTxReceipt>;
}

/// Read-write access to injected transaction and promise storage.
#[auto_impl::auto_impl(&)]
pub trait InjectedStorageRW: InjectedStorageRO {
    /// Persists a signed injected transaction, keyed by its hash.
    fn set_injected_transaction(&self, tx: SignedInjectedTransaction);

    /// Persists a promise associated with the transaction identified by its hash.
    fn set_promise(&self, promise: &Promise);

    /// Persists a signed receipt for the transaction identified by its hash.
    fn set_receipt(&self, receipt: &SignedTxReceipt);
}

/// MB static identity. Keyed by the Blake2b envelope hash; existence implies
/// the matching `Transactions` blob is in CAS at `transactions_hash`.
#[derive(
    Debug, Clone, Copy, Default, Encode, Decode, TypeInfo, PartialEq, Eq, Hash, derive_more::Display,
)]
#[display("MB(height {height}, parent {parent}, transactions_hash {transactions_hash})")]
pub struct CompactMb {
    /// Hash of the parent MB in the MB chain.
    pub parent: H256,
    /// Monotonically increasing MB sequence number.
    pub height: u64,
    /// CAS key under which the [`Transactions`] blob for this MB is stored.
    pub transactions_hash: H256,
}

/// MB dynamic state. `last_advanced_eb` is propagated forward at save time
/// (resets on `AdvanceTillEthereumBlock`); `synced` requires this MB and every
/// ancestor to be persisted.
#[derive(Debug, Clone, Default, Encode, Decode, TypeInfo, PartialEq, Eq, Hash)]
pub struct MbMeta {
    /// Set to `true` once the compute pipeline has processed this MB and written
    /// its per-row state (`mb_program_states`, `mb_outcome`, `mb_schedule`).
    pub computed: bool,
    /// Hash of the most recent Ethereum block that was advanced up to for this MB.
    pub last_advanced_eb: H256,
}

/// Read-only access to MB (micro-block) chain storage.
#[auto_impl::auto_impl(&, Box)]
pub trait MbStorageRO {
    /// Static identity (parent + height + `transactions_hash`).
    /// Existence implies the matching [`Transactions`] blob is in the
    /// CAS at `transactions_hash`.
    fn mb_compact_block(&self, mb_hash: H256) -> Option<CompactMb>;
    /// Read the [`Transactions`] blob from CAS by its content hash.
    fn transactions(&self, transactions_hash: H256) -> Option<Transactions>;
    /// Returns the program state map produced after executing this MB.
    fn mb_program_states(&self, mb_hash: H256) -> Option<ProgramStates>;
    /// Returns the list of state transitions that resulted from executing this MB.
    fn mb_outcome(&self, mb_hash: H256) -> Option<Vec<StateTransition>>;
    /// Returns the task schedule produced by executing this MB.
    fn mb_schedule(&self, mb_hash: H256) -> Option<Schedule>;
    /// Returns the dynamic metadata for an MB; returns the default value if absent.
    fn mb_meta(&self, mb_hash: H256) -> MbMeta;
}

/// Read-write access to MB (micro-block) chain storage.
#[auto_impl::auto_impl(&)]
pub trait MbStorageRW: MbStorageRO {
    /// Persists the static identity record for an MB.
    fn set_mb_compact_block(&self, mb_hash: H256, compact: CompactMb);
    /// Write a [`Transactions`] blob into the CAS and return its hash
    /// (the value stored in [`CompactMb::transactions_hash`]).
    fn set_transactions(&self, transactions: Transactions) -> H256;
    /// Persists the program state map produced after executing an MB.
    fn set_mb_program_states(&self, mb_hash: H256, program_states: ProgramStates);
    /// Persists the state transition outcome of an MB.
    fn set_mb_outcome(&self, mb_hash: H256, outcome: Vec<StateTransition>);
    /// Persists the task schedule produced by executing an MB.
    fn set_mb_schedule(&self, mb_hash: H256, schedule: Schedule);
    /// Mutates the dynamic metadata for an MB in place; creates a default record if absent.
    fn mutate_mb_meta(&self, mb_hash: H256, f: impl FnOnce(&mut MbMeta));
}

/// Aggregated data for a block that has passed the preparation phase.
///
/// Collects all information needed by the compute pipeline: the block header, its
/// decoded events, the inherited chain-wide bookkeeping (validator era, codes queue,
/// last committed hashes), so that compute can proceed without further storage reads.
pub struct PreparedBlockData {
    /// Header of the prepared Ethereum block.
    pub header: BlockHeader,
    /// Decoded events emitted in this block.
    pub events: Vec<BlockEvent>,
    /// Highest era index for which a validator set has been committed on-chain.
    pub latest_era_with_committed_validators: u64,
    /// Queue of code IDs waiting for on-chain validation commitment.
    pub codes_queue: VecDeque<CodeId>,
    /// Hash of the last batch commitment submitted to the Router contract.
    pub last_committed_batch: Digest,
    /// Hash of the last committed MB.
    pub last_committed_mb: H256,
    /// Hash of the last committed Ethereum block.
    pub last_committed_eb: H256,
}

/// Static configuration stored in the database at initialization time.
///
/// Written once when the node first starts and verified on subsequent starts to
/// detect database/configuration mismatches.
#[derive(Debug, Clone, Encode, Decode, TypeInfo, PartialEq, Eq)]
pub struct DBConfig {
    /// Database schema version; bumped on breaking schema changes.
    pub version: u32,
    /// Ethereum chain ID the node is connected to.
    pub chain_id: u64,
    /// Address of the Router contract on Ethereum.
    pub router_address: Address,
    /// Protocol timeline configuration (era lengths, transition heights, etc.).
    pub timelines: ProtocolTimelines,
    /// Hash of the Ethereum block at which the node began syncing.
    pub genesis_block_hash: H256,
    /// Maximum number of validators allowed per era.
    pub max_validators: u16,
}

/// Mutable global state persisted in the database and updated as the node advances.
///
/// Updated frequently (every block / MB) and read under a `RwLock` via [`GlobalsStorageRO`].
#[derive(Debug, Clone, Encode, Decode, TypeInfo, PartialEq, Eq)]
pub struct DBGlobals {
    /// Hash of the first Ethereum block the node started syncing from.
    pub start_block_hash: H256,
    /// The latest Ethereum block for which syncing has completed.
    pub latest_synced_eb: SimpleBlockData,
    /// Hash of the latest Ethereum block that has been fully prepared.
    pub latest_prepared_eb_hash: H256,
    /// Latest MB BFT-finalized by Malachite. Rows
    /// (`mb_program_states`/`mb_outcome`/`mb_schedule`) may not yet
    /// be persisted — use [`Self::latest_computed_mb_hash`] for any
    /// read that depends on those rows existing.
    pub latest_finalized_mb_hash: H256,
    /// Latest MB whose per-row state has been written by the compute
    /// pipeline. Trails `latest_finalized_mb_hash` until compute
    /// catches up.
    pub latest_computed_mb_hash: H256,
}

#[cfg(feature = "std")]
mod std_interfaces {
    use super::{DBConfig, DBGlobals};
    use std::sync::RwLockReadGuard;

    /// Read-only access to the node's mutable global state under an `RwLock`.
    #[auto_impl::auto_impl(&, Box)]
    pub trait GlobalsStorageRO {
        /// Returns a shared read guard over the current [`DBGlobals`].
        fn globals(&self) -> RwLockReadGuard<'_, DBGlobals>;
    }

    /// Read-write access to the node's mutable global state under an `RwLock`.
    #[auto_impl::auto_impl(&, Box)]
    pub trait GlobalsStorageRW: GlobalsStorageRO {
        /// Applies `f` to the [`DBGlobals`] under an exclusive write lock and returns its result.
        fn globals_mutate<R>(&self, f: impl FnMut(&mut DBGlobals) -> R) -> R;
    }

    /// Read-only access to the static [`DBConfig`] stored in the database.
    #[auto_impl::auto_impl(&, Box)]
    pub trait ConfigStorageRO {
        /// Returns a shared read guard over the [`DBConfig`].
        fn config(&self) -> RwLockReadGuard<'_, DBConfig>;
    }
}

#[cfg(feature = "std")]
pub use std_interfaces::{ConfigStorageRO, GlobalsStorageRO, GlobalsStorageRW};

#[cfg(feature = "mock")]
mod mock_interfaces {
    use super::{DBConfig, DBGlobals};

    /// Test helper for directly overwriting the stored [`DBGlobals`].
    #[auto_impl::auto_impl(&, Box)]
    pub trait SetGlobals {
        /// Replaces the stored globals with `globals`.
        fn set_globals(&self, globals: DBGlobals);
    }

    /// Test helper for directly overwriting the stored [`DBConfig`].
    #[auto_impl::auto_impl(&, Box)]
    pub trait SetConfig {
        /// Replaces the stored config with `config`.
        fn set_config(&self, config: DBConfig);
    }
}

#[cfg(feature = "mock")]
pub use mock_interfaces::{SetConfig, SetGlobals};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::malachite::Transactions;
    use indoc::formatdoc;
    use scale_info::{PortableRegistry, Registry, meta_type};
    use sha3::{Digest, Sha3_256};

    #[test]
    fn ensure_types_unchanged() {
        const EXPECTED_TYPE_INFO_HASH: &str =
            "d43d8ab319fb6d934231dba55950c9825e28c6ecf603e8076a90e0cab3855671";

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
            meta_type::<CompactMb>(),
            meta_type::<Transactions>(),
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
