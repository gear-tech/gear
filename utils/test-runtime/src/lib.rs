// This file is part of Gear.

// Copyright (C) 2021-2022 Gear Technologies Inc.
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

//! The Substrate runtime. This can be compiled with `#[no_std]`, ready for Wasm.

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "std")]
pub mod genesismap;
pub mod inner;

use codec::{Decode, Encode, Error, Input, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_std::{marker::PhantomData, prelude::*};

use sp_application_crypto::{ecdsa, ed25519, sr25519, RuntimeAppPublic};
use sp_core::{offchain::KeyTypeId, OpaqueMetadata, RuntimeDebug};
use sp_trie::{
    trie_types::{TrieDBBuilder, TrieDBMutBuilderV1},
    PrefixedMemoryDB, StorageProof,
};
use trie_db::{Trie, TrieMut};

use cfg_if::cfg_if;
use frame_support::{
    dispatch::RawOrigin,
    parameter_types,
    traits::{CallerTrait, ConstU32, ConstU64, CrateVersion, KeyOwnerProofSystem},
    weights::{RuntimeDbWeight, Weight},
};
use frame_system::limits::{BlockLength, BlockWeights};
use sp_api::{decl_runtime_apis, impl_runtime_apis};
pub use sp_core::hash::H256;
use sp_inherents::{CheckInherentsResult, InherentData};
#[cfg(feature = "std")]
use sp_runtime::traits::NumberFor;
use sp_runtime::{
    create_runtime_str, impl_opaque_keys,
    traits::{
        BlakeTwo256, BlindCheckable, Block as BlockT, Extrinsic as ExtrinsicT, GetNodeBlockType,
        GetRuntimeBlockType, IdentityLookup, Verify,
    },
    transaction_validity::{
        InvalidTransaction, TransactionSource, TransactionValidity, TransactionValidityError,
        ValidTransaction,
    },
    ApplyExtrinsicResult, Perbill,
};
#[cfg(any(feature = "std", test))]
use sp_version::NativeVersion;
use sp_version::RuntimeVersion;

pub use sp_consensus_babe::{AllowedSlots, AuthorityId, Slot};

// Include the WASM binary
#[cfg(feature = "std")]
include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));

#[cfg(feature = "std")]
pub mod wasm_binary_logging_disabled {
    include!(concat!(env!("OUT_DIR"), "/wasm_binary_logging_disabled.rs"));
}

// Wasm binary unwrapped. If built with `SKIP_WASM_BUILD`, the function panics.
#[cfg(feature = "std")]
pub fn wasm_binary_unwrap() -> &'static [u8] {
    WASM_BINARY.expect(
        "Development wasm binary is not available. Testing is only supported with the flag \
		 disabled.",
    )
}

// Wasm binary unwrapped. If built with `SKIP_WASM_BUILD`, the function panics.
#[cfg(feature = "std")]
pub fn wasm_binary_logging_disabled_unwrap() -> &'static [u8] {
    wasm_binary_logging_disabled::WASM_BINARY.expect(
        "Development wasm binary is not available. Testing is only supported with the flag \
		 disabled.",
    )
}

// Test runtime version.
#[sp_version::runtime_version]
pub const VERSION: RuntimeVersion = RuntimeVersion {
    spec_name: create_runtime_str!("test"),
    impl_name: create_runtime_str!("gear-test-runtime"),
    authoring_version: 1,
    spec_version: 2,
    impl_version: 2,
    apis: RUNTIME_API_VERSIONS,
    transaction_version: 1,
    state_version: 1,
};

fn version() -> RuntimeVersion {
    VERSION
}

// Native version.
#[cfg(any(feature = "std", test))]
pub fn native_version() -> NativeVersion {
    NativeVersion {
        runtime_version: VERSION,
        can_author_with: Default::default(),
    }
}

// Calls in transactions
#[derive(Clone, PartialEq, Eq, Encode, Decode, RuntimeDebug, TypeInfo)]
pub struct Message {
    pub from: AccountId,
    pub item: u64,
    pub nonce: u64,
}

impl Message {
    // Convert into a signed extrinsic.
    #[cfg(feature = "std")]
    pub fn into_signed_tx(self) -> Extrinsic {
        let signature = sp_keyring::AccountKeyring::from_public(&self.from)
            .expect("Creates keyring from public key.")
            .sign(&self.encode());
        Extrinsic::Submit {
            message: self,
            signature,
        }
    }
}

// Extrinsic for test-runtime
#[derive(Clone, PartialEq, Eq, Encode, Decode, RuntimeDebug, TypeInfo)]
pub enum Extrinsic {
    Submit {
        message: Message,
        signature: AccountSignature,
    },
    Process,
    StorageChange(Vec<u8>, Option<Vec<u8>>),
}

parity_util_mem::malloc_size_of_is_0!(Extrinsic); // non-opaque extrinsic does not need this

#[cfg(feature = "std")]
impl serde::Serialize for Extrinsic {
    fn serialize<S>(&self, seq: S) -> Result<S::Ok, S::Error>
    where
        S: ::serde::Serializer,
    {
        self.using_encoded(|bytes| seq.serialize_bytes(bytes))
    }
}

// rustc can't deduce this trait bound https://github.com/rust-lang/rust/issues/48214
#[cfg(feature = "std")]
impl<'a> serde::Deserialize<'a> for Extrinsic {
    fn deserialize<D>(de: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'a>,
    {
        let r = sp_core::bytes::deserialize(de)?;
        Decode::decode(&mut &r[..])
            .map_err(|e| serde::de::Error::custom(format!("Decode error: {e}")))
    }
}

impl BlindCheckable for Extrinsic {
    type Checked = Self;

    fn check(self) -> Result<Self, TransactionValidityError> {
        match self {
            Extrinsic::Submit { message, signature } => {
                if sp_runtime::verify_encoded_lazy(&signature, &message, &message.from) {
                    Ok(Extrinsic::Submit { message, signature })
                } else {
                    Err(InvalidTransaction::BadProof.into())
                }
            }
            Extrinsic::Process => Ok(Extrinsic::Process),
            Extrinsic::StorageChange(key, value) => Ok(Extrinsic::StorageChange(key, value)),
        }
    }
}

impl ExtrinsicT for Extrinsic {
    type Call = Extrinsic;
    type SignaturePayload = ();

    fn is_signed(&self) -> Option<bool> {
        if let Extrinsic::Process = *self {
            Some(false)
        } else {
            Some(true)
        }
    }

    fn new(call: Self::Call, _signature_payload: Option<Self::SignaturePayload>) -> Option<Self> {
        Some(call)
    }
}

impl sp_runtime::traits::Dispatchable for Extrinsic {
    type RuntimeOrigin = RuntimeOrigin;
    type Config = ();
    type Info = ();
    type PostInfo = ();
    fn dispatch(
        self,
        _origin: Self::RuntimeOrigin,
    ) -> sp_runtime::DispatchResultWithInfo<Self::PostInfo> {
        panic!("This implementation should not be used for actual dispatch.");
    }
}

impl Extrinsic {
    // Convertd `&self` into `&Message`.
    // Panics if the extrinsic holds the wrong variant
    pub fn message(&self) -> &Message {
        self.try_message().expect("cannot convert to transfer ref")
    }

    // Tries to convert `&self` into `&Message`.
    // Returns `None` if the extrinsic holds the wrong variant
    pub fn try_message(&self) -> Option<&Message> {
        match self {
            Extrinsic::Submit { ref message, .. } => Some(message),
            _ => None,
        }
    }
}

pub type AccountSignature = sr25519::Signature;
pub type AccountId = <AccountSignature as Verify>::Signer;
pub type Hash = H256;
pub type Hashing = BlakeTwo256;
pub type BlockNumber = u64;
pub type Index = u64;
pub type DigestItem = sp_runtime::generic::DigestItem;
pub type Digest = sp_runtime::generic::Digest;
pub type Block = sp_runtime::generic::Block<Header, Extrinsic>;
pub type Header = sp_runtime::generic::Header<BlockNumber, Hashing>;

// A type that can not be decoded.
#[derive(PartialEq, Eq)]
pub struct DecodeFails<B: BlockT> {
    _phantom: PhantomData<B>,
}

impl<B: BlockT> Encode for DecodeFails<B> {
    fn encode(&self) -> Vec<u8> {
        Vec::new()
    }
}

impl<B: BlockT> codec::EncodeLike for DecodeFails<B> {}

impl<B: BlockT> Default for DecodeFails<B> {
    // Create a default instance.
    fn default() -> DecodeFails<B> {
        DecodeFails {
            _phantom: Default::default(),
        }
    }
}

impl<B: BlockT> Decode for DecodeFails<B> {
    fn decode<I: Input>(_: &mut I) -> Result<Self, Error> {
        Err("DecodeFails always fails".into())
    }
}

decl_runtime_apis! {
    pub trait TestRuntimeAPI {
        // The balance of a given account
        fn balance_of(id: AccountId) -> u64;
        // Internal storage queue
        fn get_queue() -> Vec<u64>;

        // A function that always fails to convert a parameter between runtime and node.
        fn fail_convert_parameter(param: DecodeFails<Block>);
        // A function that always fails to convert its return value between runtime and node.
        fn fail_convert_return_value() -> DecodeFails<Block>;
        fn fail_on_native() -> u64;
        fn fail_on_wasm() -> u64;
        // trie no_std testing
        fn use_trie() -> u64;
        fn vec_with_capacity(size: u32) -> Vec<u8>;
        // The initialized block number
        fn get_block_number() -> u64;
        // Takes and returns the initialized block number
        fn take_block_number() -> Option<u64>;

        // Test that `ed25519` crypto works in the runtime.
        fn test_ed25519_crypto() -> (ed25519::AppSignature, ed25519::AppPublic);
        // Test that `sr25519` crypto works in the runtime.
        fn test_sr25519_crypto() -> (sr25519::AppSignature, sr25519::AppPublic);
        // Test that `ecdsa` crypto works in the runtime.
        fn test_ecdsa_crypto() -> (ecdsa::AppSignature, ecdsa::AppPublic);
        // Run various tests against storage.
        fn test_storage();
        // Check a witness.
        fn test_witness(proof: StorageProof, root: crate::Hash);
        // Test that ensures that we can call a function that takes multiple arguments
        fn test_multiple_arguments(data: Vec<u8>, other: Vec<u8>, num: u32);
        // Traces log "Hey I'm runtime."
        fn do_trace_log();
    }
}

#[derive(Clone, Eq, PartialEq, TypeInfo)]
pub struct Runtime;

impl GetNodeBlockType for Runtime {
    type NodeBlock = Block;
}

impl GetRuntimeBlockType for Runtime {
    type RuntimeBlock = Block;
}

#[derive(Clone, RuntimeDebug, Encode, Decode, PartialEq, Eq, TypeInfo, MaxEncodedLen)]
pub struct RuntimeOrigin;

impl From<frame_system::Origin<Runtime>> for RuntimeOrigin {
    fn from(_o: frame_system::Origin<Runtime>) -> Self {
        unimplemented!("Not required in tests!")
    }
}
impl From<RuntimeOrigin> for Result<frame_system::Origin<Runtime>, RuntimeOrigin> {
    fn from(_origin: RuntimeOrigin) -> Result<frame_system::Origin<Runtime>, RuntimeOrigin> {
        unimplemented!("Not required in tests!")
    }
}

impl CallerTrait<<Runtime as frame_system::Config>::AccountId> for RuntimeOrigin {
    fn into_system(self) -> Option<RawOrigin<<Runtime as frame_system::Config>::AccountId>> {
        unimplemented!("Not required in tests!")
    }

    fn as_system_ref(&self) -> Option<&RawOrigin<<Runtime as frame_system::Config>::AccountId>> {
        unimplemented!("Not required in tests!")
    }
}

impl frame_support::traits::OriginTrait for RuntimeOrigin {
    type Call = <Runtime as frame_system::Config>::RuntimeCall;
    type PalletsOrigin = RuntimeOrigin;
    type AccountId = <Runtime as frame_system::Config>::AccountId;

    fn add_filter(&mut self, _filter: impl Fn(&Self::Call) -> bool + 'static) {
        unimplemented!("Not required in tests!")
    }

    fn reset_filter(&mut self) {
        unimplemented!("Not required in tests!")
    }

    fn set_caller_from(&mut self, _other: impl Into<Self>) {
        unimplemented!("Not required in tests!")
    }

    fn filter_call(&self, _call: &Self::Call) -> bool {
        unimplemented!("Not required in tests!")
    }

    fn caller(&self) -> &Self::PalletsOrigin {
        unimplemented!("Not required in tests!")
    }

    fn into_caller(self) -> Self::PalletsOrigin {
        unimplemented!("Not required in tests!")
    }

    fn try_with_caller<R>(
        self,
        _f: impl FnOnce(Self::PalletsOrigin) -> Result<R, Self::PalletsOrigin>,
    ) -> Result<R, Self> {
        unimplemented!("Not required in tests!")
    }

    fn none() -> Self {
        unimplemented!("Not required in tests!")
    }
    fn root() -> Self {
        unimplemented!("Not required in tests!")
    }
    fn signed(_by: Self::AccountId) -> Self {
        unimplemented!("Not required in tests!")
    }
    fn as_signed(self) -> Option<Self::AccountId> {
        unimplemented!("Not required in tests!")
    }
}

#[derive(Clone, Encode, Decode, Eq, PartialEq, RuntimeDebug, TypeInfo)]
pub struct RuntimeEvent;

impl From<frame_system::Event<Runtime>> for RuntimeEvent {
    fn from(_evt: frame_system::Event<Runtime>) -> Self {
        unimplemented!("Not required in tests!")
    }
}

impl frame_support::traits::PalletInfo for Runtime {
    fn index<P: 'static>() -> Option<usize> {
        let type_id = sp_std::any::TypeId::of::<P>();
        if type_id == sp_std::any::TypeId::of::<inner::Pallet<Runtime>>() {
            return Some(0);
        }
        if type_id == sp_std::any::TypeId::of::<pallet_timestamp::Pallet<Runtime>>() {
            return Some(1);
        }
        if type_id == sp_std::any::TypeId::of::<pallet_babe::Pallet<Runtime>>() {
            return Some(2);
        }

        None
    }
    fn name<P: 'static>() -> Option<&'static str> {
        let type_id = sp_std::any::TypeId::of::<P>();
        if type_id == sp_std::any::TypeId::of::<inner::Pallet<Runtime>>() {
            return Some("Inner");
        }
        if type_id == sp_std::any::TypeId::of::<pallet_timestamp::Pallet<Runtime>>() {
            return Some("Timestamp");
        }
        if type_id == sp_std::any::TypeId::of::<pallet_babe::Pallet<Runtime>>() {
            return Some("Babe");
        }

        None
    }
    fn module_name<P: 'static>() -> Option<&'static str> {
        let type_id = sp_std::any::TypeId::of::<P>();
        if type_id == sp_std::any::TypeId::of::<inner::Pallet<Runtime>>() {
            return Some("system");
        }
        if type_id == sp_std::any::TypeId::of::<pallet_timestamp::Pallet<Runtime>>() {
            return Some("pallet_timestamp");
        }
        if type_id == sp_std::any::TypeId::of::<pallet_babe::Pallet<Runtime>>() {
            return Some("pallet_babe");
        }

        None
    }
    fn crate_version<P: 'static>() -> Option<CrateVersion> {
        use frame_support::traits::PalletInfoAccess as _;
        let type_id = sp_std::any::TypeId::of::<P>();
        if type_id == sp_std::any::TypeId::of::<inner::Pallet<Runtime>>() {
            return Some(inner::Pallet::<Runtime>::crate_version());
        }
        if type_id == sp_std::any::TypeId::of::<pallet_timestamp::Pallet<Runtime>>() {
            return Some(pallet_timestamp::Pallet::<Runtime>::crate_version());
        }
        if type_id == sp_std::any::TypeId::of::<pallet_babe::Pallet<Runtime>>() {
            return Some(pallet_babe::Pallet::<Runtime>::crate_version());
        }

        None
    }
}

parameter_types! {
    pub const DbWeight: RuntimeDbWeight = RuntimeDbWeight {
        read: 100,
        write: 1000,
    };
    pub RuntimeBlockLength: BlockLength =
        BlockLength::max(4 * 1024 * 1024);
    pub RuntimeBlockWeights: BlockWeights =
        BlockWeights::with_sensible_defaults(Weight::from_ref_time(4 * 1024 * 1024), Perbill::from_percent(75));
}

impl From<frame_system::Call<Runtime>> for Extrinsic {
    fn from(_: frame_system::Call<Runtime>) -> Self {
        unimplemented!("Not required in tests!")
    }
}

impl frame_system::Config for Runtime {
    type BaseCallFilter = frame_support::traits::Everything;
    type BlockWeights = RuntimeBlockWeights;
    type BlockLength = RuntimeBlockLength;
    type RuntimeOrigin = RuntimeOrigin;
    type RuntimeCall = Extrinsic;
    type Index = u64;
    type BlockNumber = u64;
    type Hash = H256;
    type Hashing = Hashing;
    type AccountId = u64;
    type Lookup = IdentityLookup<Self::AccountId>;
    type Header = Header;
    type RuntimeEvent = RuntimeEvent;
    type BlockHashCount = ConstU64<2400>;
    type DbWeight = ();
    type Version = ();
    type PalletInfo = Self;
    type AccountData = ();
    type OnNewAccount = ();
    type OnKilledAccount = ();
    type SystemWeightInfo = ();
    type SS58Prefix = ();
    type OnSetCode = ();
    type MaxConsumers = ConstU32<16>;
}

impl pallet_timestamp::Config for Runtime {
    // A timestamp: milliseconds since the unix epoch.
    type Moment = u64;
    type OnTimestampSet = ();
    type MinimumPeriod = ConstU64<5>;
    type WeightInfo = ();
}

parameter_types! {
    pub const EpochDuration: u64 = 6;
}

impl pallet_babe::Config for Runtime {
    type EpochDuration = EpochDuration;
    type ExpectedBlockTime = ConstU64<10_000>;
    // there is no actual runtime in this test-runtime, so testing crates
    // are manually adding the digests. normally in this situation you'd use
    // pallet_babe::SameAuthoritiesForever.
    type EpochChangeTrigger = pallet_babe::ExternalTrigger;
    type DisabledValidators = ();

    type KeyOwnerProofSystem = ();

    type KeyOwnerProof =
        <Self::KeyOwnerProofSystem as KeyOwnerProofSystem<(KeyTypeId, AuthorityId)>>::Proof;

    type KeyOwnerIdentification = <Self::KeyOwnerProofSystem as KeyOwnerProofSystem<(
        KeyTypeId,
        AuthorityId,
    )>>::IdentificationTuple;

    type HandleEquivocation = ();
    type WeightInfo = ();

    type MaxAuthorities = ConstU32<10>;
}

impl inner::Config for Runtime {
    type PanicThreshold = ConstU32<3>;
}

fn code_using_trie() -> u64 {
    let pairs = [
        (b"0103000000000000000464".to_vec(), b"0400000000".to_vec()),
        (b"0103000000000000000469".to_vec(), b"0401000000".to_vec()),
    ]
    .to_vec();

    let mut mdb = PrefixedMemoryDB::default();
    let mut root = sp_std::default::Default::default();
    {
        let mut t = TrieDBMutBuilderV1::<Hashing>::new(&mut mdb, &mut root).build();
        for (key, value) in &pairs {
            if t.insert(key, value).is_err() {
                return 101;
            }
        }
    }

    let trie = TrieDBBuilder::<Hashing>::new(&mdb, &root).build();
    let res = if let Ok(iter) = trie.iter() {
        iter.flatten().count() as u64
    } else {
        102
    };

    res
}

impl_opaque_keys! {
    pub struct SessionKeys {
        pub ed25519: ed25519::AppPublic,
        pub sr25519: sr25519::AppPublic,
        pub ecdsa: ecdsa::AppPublic,
    }
}

cfg_if! {
    if #[cfg(feature = "std")] {
        impl_runtime_apis! {
            impl sp_api::Core<Block> for Runtime {
                fn version() -> RuntimeVersion {
                    version()
                }

                fn execute_block(block: Block) {
                    inner::execute_block(block);
                }

                fn initialize_block(header: &<Block as BlockT>::Header) {
                    inner::initialize_block(header)
                }
            }

            impl sp_api::Metadata<Block> for Runtime {
                fn metadata() -> OpaqueMetadata {
                    unimplemented!()
                }
            }

            impl sp_transaction_pool::runtime_api::TaggedTransactionQueue<Block> for Runtime {
                fn validate_transaction(
                    _source: TransactionSource,
                    utx: <Block as BlockT>::Extrinsic,
                    _: <Block as BlockT>::Hash,
                ) -> TransactionValidity {
                    // Not validating signature for unsigned extrinsic
                    if let Extrinsic::Process = utx {
                        return Ok(ValidTransaction {
                            priority: u64::MAX,
                            requires: vec![],
                            provides: vec![],
                            longevity: 1,
                            propagate: false,
                        });
                    }

                    inner::validate_transaction(utx)
                }
            }

            impl sp_block_builder::BlockBuilder<Block> for Runtime {
                fn apply_extrinsic(extrinsic: <Block as BlockT>::Extrinsic) -> ApplyExtrinsicResult {
                    inner::execute_transaction(extrinsic)
                }

                fn finalize_block() -> <Block as BlockT>::Header {
                    inner::finalize_block()
                }

                fn inherent_extrinsics(_data: InherentData) -> Vec<<Block as BlockT>::Extrinsic> {
                    vec![]
                }

                fn check_inherents(_block: Block, _data: InherentData) -> CheckInherentsResult {
                    CheckInherentsResult::new()
                }
            }

            impl self::TestRuntimeAPI<Block> for Runtime {
                fn balance_of(id: AccountId) -> u64 {
                    inner::balance_of(id)
                }

                fn get_queue() -> Vec<u64> {
                    inner::queue()
                }

                fn fail_convert_parameter(_: DecodeFails<Block>) {}

                fn fail_convert_return_value() -> DecodeFails<Block> {
                    DecodeFails::default()
                }

                fn fail_on_native() -> u64 {
                    panic!("Failing because we are on native")
                }
                fn fail_on_wasm() -> u64 {
                    1
                }

                fn use_trie() -> u64 {
                    code_using_trie()
                }

                fn vec_with_capacity(_size: u32) -> Vec<u8> {
                    unimplemented!("is not expected to be invoked from non-wasm builds");
                }

                fn get_block_number() -> u64 {
                    inner::get_block_number().expect("Block number is initialized")
                }

                fn take_block_number() -> Option<u64> {
                    inner::take_block_number()
                }

                fn test_ed25519_crypto() -> (ed25519::AppSignature, ed25519::AppPublic) {
                    test_ed25519_crypto()
                }

                fn test_sr25519_crypto() -> (sr25519::AppSignature, sr25519::AppPublic) {
                    test_sr25519_crypto()
                }

                fn test_ecdsa_crypto() -> (ecdsa::AppSignature, ecdsa::AppPublic) {
                    test_ecdsa_crypto()
                }

                fn test_storage() {
                    test_read_storage();
                    test_read_child_storage();
                }

                fn test_witness(proof: StorageProof, root: crate::Hash) {
                    test_witness(proof, root);
                }

                fn test_multiple_arguments(data: Vec<u8>, other: Vec<u8>, num: u32) {
                    assert_eq!(&data[..], &other[..]);
                    assert_eq!(data.len(), num as usize);
                }

                fn do_trace_log() {
                    log::trace!("Hey I'm runtime");
                }
            }

            impl sp_consensus_babe::BabeApi<Block> for Runtime {
                fn configuration() -> sp_consensus_babe::BabeConfiguration {
                    sp_consensus_babe::BabeConfiguration {
                        slot_duration: 1000,
                        epoch_length: EpochDuration::get(),
                        c: (3, 10),
                        authorities: inner::authorities()
                            .into_iter().map(|x|(x, 1)).collect(),
                        randomness: <pallet_babe::Pallet<Runtime>>::randomness(),
                        allowed_slots: AllowedSlots::PrimaryAndSecondaryPlainSlots,
                    }
                }

                fn current_epoch_start() -> Slot {
                    <pallet_babe::Pallet<Runtime>>::current_epoch_start()
                }

                fn current_epoch() -> sp_consensus_babe::Epoch {
                    <pallet_babe::Pallet<Runtime>>::current_epoch()
                }

                fn next_epoch() -> sp_consensus_babe::Epoch {
                    <pallet_babe::Pallet<Runtime>>::next_epoch()
                }

                fn submit_report_equivocation_unsigned_extrinsic(
                    _equivocation_proof: sp_consensus_babe::EquivocationProof<
                        <Block as BlockT>::Header,
                    >,
                    _key_owner_proof: sp_consensus_babe::OpaqueKeyOwnershipProof,
                ) -> Option<()> {
                    None
                }

                fn generate_key_ownership_proof(
                    _slot: sp_consensus_babe::Slot,
                    _authority_id: sp_consensus_babe::AuthorityId,
                ) -> Option<sp_consensus_babe::OpaqueKeyOwnershipProof> {
                    None
                }
            }

            impl sp_offchain::OffchainWorkerApi<Block> for Runtime {
                fn offchain_worker(_header: &<Block as BlockT>::Header) {}
            }

            impl sp_session::SessionKeys<Block> for Runtime {
                fn generate_session_keys(_: Option<Vec<u8>>) -> Vec<u8> {
                    SessionKeys::generate(None)
                }

                fn decode_session_keys(
                    encoded: Vec<u8>,
                ) -> Option<Vec<(Vec<u8>, sp_core::crypto::KeyTypeId)>> {
                    SessionKeys::decode_into_raw_public_keys(&encoded)
                }
            }

            impl sp_finality_grandpa::GrandpaApi<Block> for Runtime {
                fn grandpa_authorities() -> sp_finality_grandpa::AuthorityList {
                    Vec::new()
                }

                fn current_set_id() -> sp_finality_grandpa::SetId {
                    0
                }

                fn submit_report_equivocation_unsigned_extrinsic(
                    _equivocation_proof: sp_finality_grandpa::EquivocationProof<
                        <Block as BlockT>::Hash,
                        NumberFor<Block>,
                    >,
                    _key_owner_proof: sp_finality_grandpa::OpaqueKeyOwnershipProof,
                ) -> Option<()> {
                    None
                }

                fn generate_key_ownership_proof(
                    _set_id: sp_finality_grandpa::SetId,
                    _authority_id: sp_finality_grandpa::AuthorityId,
                ) -> Option<sp_finality_grandpa::OpaqueKeyOwnershipProof> {
                    None
                }
            }

            impl frame_system_rpc_runtime_api::AccountNonceApi<Block, AccountId, Index> for Runtime {
                fn account_nonce(_account: AccountId) -> Index {
                    0
                }
            }
        }
    } else {
        impl_runtime_apis! {
            impl sp_api::Core<Block> for Runtime {
                fn version() -> RuntimeVersion {
                    version()
                }

                fn execute_block(block: Block) {
                    inner::execute_block(block);
                }

                fn initialize_block(header: &<Block as BlockT>::Header) {
                    inner::initialize_block(header)
                }
            }

            impl sp_api::Metadata<Block> for Runtime {
                fn metadata() -> OpaqueMetadata {
                    unimplemented!()
                }
            }

            impl sp_transaction_pool::runtime_api::TaggedTransactionQueue<Block> for Runtime {
                fn validate_transaction(
                    _source: TransactionSource,
                    utx: <Block as BlockT>::Extrinsic,
                    _: <Block as BlockT>::Hash,
                ) -> TransactionValidity {
                    // Not validating signature for unsigned extrinsic
                    if let Extrinsic::Process = utx {
                        return Ok(ValidTransaction{
                            priority: u64::MAX,
                            requires: vec![],
                            provides: vec![],
                            longevity: 1,
                            propagate: false,
                        });
                    }

                    inner::validate_transaction(utx)
                }
            }

            impl sp_block_builder::BlockBuilder<Block> for Runtime {
                fn apply_extrinsic(extrinsic: <Block as BlockT>::Extrinsic) -> ApplyExtrinsicResult {
                    inner::execute_transaction(extrinsic)
                }

                fn finalize_block() -> <Block as BlockT>::Header {
                    inner::finalize_block()
                }

                fn inherent_extrinsics(_data: InherentData) -> Vec<<Block as BlockT>::Extrinsic> {
                    vec![]
                }

                fn check_inherents(_block: Block, _data: InherentData) -> CheckInherentsResult {
                    CheckInherentsResult::new()
                }
            }

            impl self::TestRuntimeAPI<Block> for Runtime {
                fn balance_of(id: AccountId) -> u64 {
                    inner::balance_of(id)
                }

                fn get_queue() -> Vec<u64> {
                    inner::queue()
                }

                fn fail_convert_parameter(_: DecodeFails<Block>) {}

                fn fail_convert_return_value() -> DecodeFails<Block> {
                    DecodeFails::default()
                }

                fn fail_on_native() -> u64 {
                    1
                }

                fn fail_on_wasm() -> u64 {
                    panic!("Failing because we are on wasm")
                }

                fn use_trie() -> u64 {
                    code_using_trie()
                }

                fn vec_with_capacity(size: u32) -> Vec<u8> {
                    Vec::with_capacity(size as usize)
                }

                fn get_block_number() -> u64 {
                    inner::get_block_number().expect("Block number is initialized")
                }

                fn take_block_number() -> Option<u64> {
                    inner::take_block_number()
                }

                fn test_ed25519_crypto() -> (ed25519::AppSignature, ed25519::AppPublic) {
                    test_ed25519_crypto()
                }

                fn test_sr25519_crypto() -> (sr25519::AppSignature, sr25519::AppPublic) {
                    test_sr25519_crypto()
                }

                fn test_ecdsa_crypto() -> (ecdsa::AppSignature, ecdsa::AppPublic) {
                    test_ecdsa_crypto()
                }

                fn test_storage() {
                    test_read_storage();
                    test_read_child_storage();
                }

                fn test_witness(proof: StorageProof, root: crate::Hash) {
                    test_witness(proof, root);
                }

                fn test_multiple_arguments(data: Vec<u8>, other: Vec<u8>, num: u32) {
                    assert_eq!(&data[..], &other[..]);
                    assert_eq!(data.len(), num as usize);
                }

                fn do_trace_log() {
                    log::trace!("Hey I'm runtime: {}", log::STATIC_MAX_LEVEL);
                }
            }

            impl sp_consensus_babe::BabeApi<Block> for Runtime {
                fn configuration() -> sp_consensus_babe::BabeConfiguration {
                    sp_consensus_babe::BabeConfiguration {
                        slot_duration: 1000,
                        epoch_length: EpochDuration::get(),
                        c: (3, 10),
                        authorities: inner::authorities()
                            .into_iter().map(|x|(x, 1)).collect(),
                        randomness: <pallet_babe::Pallet<Runtime>>::randomness(),
                        allowed_slots: AllowedSlots::PrimaryAndSecondaryPlainSlots,
                    }
                }

                fn current_epoch_start() -> Slot {
                    <pallet_babe::Pallet<Runtime>>::current_epoch_start()
                }

                fn current_epoch() -> sp_consensus_babe::Epoch {
                    <pallet_babe::Pallet<Runtime>>::current_epoch()
                }

                fn next_epoch() -> sp_consensus_babe::Epoch {
                    <pallet_babe::Pallet<Runtime>>::next_epoch()
                }

                fn submit_report_equivocation_unsigned_extrinsic(
                    _equivocation_proof: sp_consensus_babe::EquivocationProof<
                        <Block as BlockT>::Header,
                    >,
                    _key_owner_proof: sp_consensus_babe::OpaqueKeyOwnershipProof,
                ) -> Option<()> {
                    None
                }

                fn generate_key_ownership_proof(
                    _slot: sp_consensus_babe::Slot,
                    _authority_id: sp_consensus_babe::AuthorityId,
                ) -> Option<sp_consensus_babe::OpaqueKeyOwnershipProof> {
                    None
                }
            }

            impl sp_offchain::OffchainWorkerApi<Block> for Runtime {
                fn offchain_worker(_header: &<Block as BlockT>::Header) {}
            }

            impl sp_session::SessionKeys<Block> for Runtime {
                fn generate_session_keys(_: Option<Vec<u8>>) -> Vec<u8> {
                    SessionKeys::generate(None)
                }

                fn decode_session_keys(
                    encoded: Vec<u8>,
                ) -> Option<Vec<(Vec<u8>, sp_core::crypto::KeyTypeId)>> {
                    SessionKeys::decode_into_raw_public_keys(&encoded)
                }
            }

            impl frame_system_rpc_runtime_api::AccountNonceApi<Block, AccountId, Index> for Runtime {
                fn account_nonce(_account: AccountId) -> Index {
                    0
                }
            }
        }
    }
}

impl common::TerminalExtrinsicProvider<Extrinsic> for Runtime {
    fn extrinsic() -> Option<Extrinsic> {
        Extrinsic::new(Extrinsic::Process, None)
    }
}

fn test_ed25519_crypto() -> (ed25519::AppSignature, ed25519::AppPublic) {
    let public0 = ed25519::AppPublic::generate_pair(None);
    let public1 = ed25519::AppPublic::generate_pair(None);
    let public2 = ed25519::AppPublic::generate_pair(None);

    let all = ed25519::AppPublic::all();
    assert!(all.contains(&public0));
    assert!(all.contains(&public1));
    assert!(all.contains(&public2));

    let signature = public0
        .sign(&"ed25519")
        .expect("Generates a valid `ed25519` signature.");
    assert!(public0.verify(&"ed25519", &signature));
    (signature, public0)
}

fn test_sr25519_crypto() -> (sr25519::AppSignature, sr25519::AppPublic) {
    let public0 = sr25519::AppPublic::generate_pair(None);
    let public1 = sr25519::AppPublic::generate_pair(None);
    let public2 = sr25519::AppPublic::generate_pair(None);

    let all = sr25519::AppPublic::all();
    assert!(all.contains(&public0));
    assert!(all.contains(&public1));
    assert!(all.contains(&public2));

    let signature = public0
        .sign(&"sr25519")
        .expect("Generates a valid `sr25519` signature.");
    assert!(public0.verify(&"sr25519", &signature));
    (signature, public0)
}

fn test_ecdsa_crypto() -> (ecdsa::AppSignature, ecdsa::AppPublic) {
    let public0 = ecdsa::AppPublic::generate_pair(None);
    let public1 = ecdsa::AppPublic::generate_pair(None);
    let public2 = ecdsa::AppPublic::generate_pair(None);

    let all = ecdsa::AppPublic::all();
    assert!(all.contains(&public0));
    assert!(all.contains(&public1));
    assert!(all.contains(&public2));

    let signature = public0
        .sign(&"ecdsa")
        .expect("Generates a valid `ecdsa` signature.");

    assert!(public0.verify(&"ecdsa", &signature));
    (signature, public0)
}

fn test_read_storage() {
    const KEY: &[u8] = b":read_storage";
    sp_io::storage::set(KEY, b"test");

    let mut v = [0u8; 4];
    let r = sp_io::storage::read(KEY, &mut v, 0);
    assert_eq!(r, Some(4));
    assert_eq!(&v, b"test");

    let mut v = [0u8; 4];
    let r = sp_io::storage::read(KEY, &mut v, 4);
    assert_eq!(r, Some(0));
    assert_eq!(&v, &[0, 0, 0, 0]);
}

fn test_read_child_storage() {
    const STORAGE_KEY: &[u8] = b"unique_id_1";
    const KEY: &[u8] = b":read_child_storage";
    sp_io::default_child_storage::set(STORAGE_KEY, KEY, b"test");

    let mut v = [0u8; 4];
    let r = sp_io::default_child_storage::read(STORAGE_KEY, KEY, &mut v, 0);
    assert_eq!(r, Some(4));
    assert_eq!(&v, b"test");

    let mut v = [0u8; 4];
    let r = sp_io::default_child_storage::read(STORAGE_KEY, KEY, &mut v, 8);
    assert_eq!(r, Some(0));
    assert_eq!(&v, &[0, 0, 0, 0]);
}

fn test_witness(proof: StorageProof, root: crate::Hash) {
    use sp_externalities::Externalities;
    let db: sp_trie::MemoryDB<crate::Hashing> = proof.into_memory_db();
    let backend = sp_state_machine::TrieBackendBuilder::<_, crate::Hashing>::new(db, root).build();
    let mut overlay = sp_state_machine::OverlayedChanges::default();
    let mut cache = sp_state_machine::StorageTransactionCache::<_, _>::default();
    let mut ext = sp_state_machine::Ext::new(
        &mut overlay,
        &mut cache,
        &backend,
        #[cfg(feature = "std")]
        None,
    );
    assert!(ext.storage(b"value3").is_some());
    assert!(ext.storage_root(Default::default()).as_slice() == &root[..]);
    ext.place_storage(vec![0], Some(vec![1]));
    assert!(ext.storage_root(Default::default()).as_slice() != &root[..]);
}

#[cfg(test)]
mod tests {
    use crate::Extrinsic;
    use codec::Encode;
    use pallet_gear_rpc_runtime_api::GearApi;
    use sc_block_builder::BlockBuilderProvider;
    use sp_api::ProvideRuntimeApi;
    use sp_consensus::BlockOrigin;
    use sp_core::storage::well_known_keys::HEAP_PAGES;
    use sp_runtime::{generic::BlockId, traits::Extrinsic as ExtrinsicT};
    use sp_state_machine::ExecutionStrategy;
    use test_client::{
        prelude::*, runtime::TestRuntimeAPI, DefaultTestClientBuilderExt, TestClientBuilder,
    };

    #[test]
    fn heap_pages_is_respected() {
        // This tests that the on-chain HEAP_PAGES parameter is respected.

        // Create a client devoting only 8 pages of wasm memory. This gives us ~512k of heap memory.
        let mut client = TestClientBuilder::new()
            .set_execution_strategy(ExecutionStrategy::AlwaysWasm)
            .set_heap_pages(8)
            .build();
        let block_id = BlockId::Number(client.chain_info().best_number);

        // Try to allocate 1024k of memory on heap. This is going to fail since it is twice larger
        // than the heap.
        let ret = client.runtime_api().vec_with_capacity(&block_id, 1048576);
        assert!(ret.is_err());

        // Create a block that sets the `:heap_pages` to 32 pages of memory which corresponds to
        // ~2048k of heap memory.
        let (new_block_id, block) = {
            let mut builder = client.new_block(Default::default()).unwrap();
            builder
                .push_storage_change(HEAP_PAGES.to_vec(), Some(32u64.encode()))
                .unwrap();
            let block = builder.build().unwrap().block;
            let hash = block.header.hash();
            (BlockId::Hash(hash), block)
        };

        futures::executor::block_on(client.import(BlockOrigin::Own, block)).unwrap();

        // Allocation of 1024k while having ~2048k should succeed.
        let ret = client
            .runtime_api()
            .vec_with_capacity(&new_block_id, 1048576);
        assert!(ret.is_ok());
    }

    #[test]
    fn test_storage() {
        let client = TestClientBuilder::new()
            .set_execution_strategy(ExecutionStrategy::Both)
            .build();
        let runtime_api = client.runtime_api();
        let block_id = BlockId::Number(client.chain_info().best_number);

        runtime_api.test_storage(&block_id).unwrap();
    }

    fn witness_backend() -> (sp_trie::MemoryDB<crate::Hashing>, crate::Hash) {
        use sp_trie::TrieMut;
        let mut root = crate::Hash::default();
        let mut mdb = sp_trie::MemoryDB::<crate::Hashing>::default();
        {
            let mut trie =
                sp_trie::trie_types::TrieDBMutBuilderV1::new(&mut mdb, &mut root).build();
            trie.insert(b"value3", &[142]).expect("insert failed");
            trie.insert(b"value4", &[124]).expect("insert failed");
        };
        (mdb, root)
    }

    #[test]
    fn witness_backend_works() {
        let (db, root) = witness_backend();
        let backend =
            sp_state_machine::TrieBackendBuilder::<_, crate::Hashing>::new(db, root).build();
        let proof = sp_state_machine::prove_read(backend, vec![b"value3"]).unwrap();
        let client = TestClientBuilder::new()
            .set_execution_strategy(ExecutionStrategy::Both)
            .build();
        let runtime_api = client.runtime_api();
        let block_id = BlockId::Number(client.chain_info().best_number);

        runtime_api.test_witness(&block_id, proof, root).unwrap();
    }

    #[test]
    fn test_gear_runtime_api() {
        let client = TestClientBuilder::new()
            .set_execution_strategy(ExecutionStrategy::Both)
            .build();
        let runtime_api = client.runtime_api();
        let block_id = BlockId::Number(client.chain_info().best_number);

        assert_eq!(
            runtime_api.gear_run_extrinsic(&block_id).unwrap().encode(),
            Extrinsic::new(Extrinsic::Process, None).unwrap().encode()
        );
    }
}
