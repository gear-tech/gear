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

//! System manager: Handles all of the top-level stuff; executing block/transaction, setting code
//! and depositing logs.

use crate::{
    AccountId, AuthorityId, Block, BlockNumber, Digest, Extrinsic, Header, Message, Runtime,
    H256 as Hash,
};
use codec::{Decode, Encode, KeyedVec};
use frame_support::{storage, traits::Get};
use sp_core::storage::well_known_keys;
use sp_io::{hashing::blake2_256, storage::root as storage_root, trie};
use sp_runtime::{
    generic,
    traits::Header as _,
    transaction_validity::{
        InvalidTransaction, TransactionValidity, TransactionValidityError, ValidTransaction,
    },
    ApplyExtrinsicResult,
};
use sp_std::prelude::*;

const NONCE_OF: &[u8] = b"nonce:";
const BALANCE_OF: &[u8] = b"balance:";

pub use self::pallet::*;

#[frame_support::pallet]
mod pallet {
    use super::*;
    use frame_support::{pallet_prelude::*, traits::Get};

    #[pallet::pallet]
    #[pallet::generate_store(pub(super) trait Store)]
    #[pallet::without_storage_info]
    pub struct Pallet<T>(PhantomData<T>);

    #[pallet::config]
    pub trait Config: frame_system::Config {
        #[pallet::constant]
        type PanicThreshold: Get<u32>;
    }

    #[pallet::storage]
    pub type ExtrinsicData<T> = StorageMap<_, Blake2_128Concat, u32, Vec<u8>, ValueQuery>;

    // The current block number being processed. Set by `execute_block`.
    #[pallet::storage]
    pub type Number<T> = StorageValue<_, BlockNumber, OptionQuery>;

    #[pallet::storage]
    pub type ParentHash<T> = StorageValue<_, Hash, ValueQuery>;

    #[pallet::storage]
    pub type NewAuthorities<T> = StorageValue<_, Vec<AuthorityId>, OptionQuery>;

    #[pallet::storage]
    pub type StorageDigest<T> = StorageValue<_, Digest, OptionQuery>;

    #[pallet::storage]
    pub type Authorities<T> = StorageValue<_, Vec<AuthorityId>, ValueQuery>;

    #[pallet::storage]
    pub type Queue<T> = StorageValue<_, Vec<u64>, ValueQuery>;

    #[pallet::genesis_config]
    #[cfg_attr(feature = "std", derive(Default))]
    pub struct GenesisConfig {
        pub authorities: Vec<AuthorityId>,
    }

    #[pallet::genesis_build]
    impl<T: Config> GenesisBuild<T> for GenesisConfig {
        fn build(&self) {
            <Authorities<T>>::put(self.authorities.clone());
        }
    }
}

pub fn balance_of_key(who: AccountId) -> Vec<u8> {
    who.to_keyed_vec(BALANCE_OF)
}

pub fn balance_of(who: AccountId) -> u64 {
    storage::hashed::get_or(&blake2_256, &balance_of_key(who), 0)
}

pub fn nonce_of(who: AccountId) -> u64 {
    storage::hashed::get_or(&blake2_256, &who.to_keyed_vec(NONCE_OF), 0)
}

pub fn initialize_block(header: &Header) {
    // populate environment.
    <Number<Runtime>>::put(header.number);
    <ParentHash<Runtime>>::put(header.parent_hash);
    <StorageDigest<Runtime>>::put(header.digest());
    storage::unhashed::put(well_known_keys::EXTRINSIC_INDEX, &0u32);

    // try to read something that depends on current header digest
    // so that it'll be included in execution proof
    if let Some(generic::DigestItem::Other(v)) = header.digest().logs().iter().next() {
        let _: Option<u32> = storage::unhashed::get(v);
    }
}

pub fn authorities() -> Vec<AuthorityId> {
    <Authorities<Runtime>>::get()
}

pub fn get_block_number() -> Option<BlockNumber> {
    <Number<Runtime>>::get()
}

pub fn take_block_number() -> Option<BlockNumber> {
    <Number<Runtime>>::take()
}

pub fn queue() -> Vec<u64> {
    <Queue<Runtime>>::get()
}

#[derive(Copy, Clone)]
enum Mode {
    Verify,
    Overwrite,
}

// Actually execute all transitioning for `block`.
pub fn polish_block(block: &mut Block) {
    execute_block_with_state_root_handler(block, Mode::Overwrite);
}

pub fn execute_block(mut block: Block) -> Header {
    execute_block_with_state_root_handler(&mut block, Mode::Verify)
}

fn execute_block_with_state_root_handler(block: &mut Block, mode: Mode) -> Header {
    let header = &mut block.header;

    initialize_block(header);

    // execute transactions
    block.extrinsics.iter().for_each(|e| {
        let _ = execute_transaction(e.clone()).unwrap_or_else(|_| panic!("Invalid transaction"));
    });

    let new_header = finalize_block();

    if let Mode::Overwrite = mode {
        header.state_root = new_header.state_root;
    } else {
        info_expect_equal_hash(&new_header.state_root, &header.state_root);
        assert_eq!(
            new_header.state_root, header.state_root,
            "Storage root must match that calculated.",
        );
    }

    if let Mode::Overwrite = mode {
        header.extrinsics_root = new_header.extrinsics_root;
    } else {
        info_expect_equal_hash(&new_header.extrinsics_root, &header.extrinsics_root);
        assert_eq!(
            new_header.extrinsics_root, header.extrinsics_root,
            "Transaction trie root must be valid.",
        );
    }

    new_header
}

// The block executor
pub struct BlockExecutor;

impl frame_support::traits::ExecuteBlock<Block> for BlockExecutor {
    fn execute_block(block: Block) {
        execute_block(block);
    }
}

// Validate a transaction outside of the block execution function.
// This doesn't attempt to validate anything regarding the block
pub fn validate_transaction(utx: Extrinsic) -> TransactionValidity {
    if check_signature(&utx).is_err() {
        return InvalidTransaction::BadProof.into();
    }

    // The only transaction being validated is the `Extrinsic::Submit`
    let tx = utx.message();
    let nonce_key = tx.from.to_keyed_vec(NONCE_OF);
    let expected_nonce: u64 = storage::hashed::get_or(&blake2_256, &nonce_key, 0);
    if tx.nonce < expected_nonce {
        return InvalidTransaction::Stale.into();
    }
    if tx.nonce > expected_nonce + 64 {
        return InvalidTransaction::Future.into();
    }

    let encode = |from: &AccountId, nonce: u64| (from, nonce).encode();
    let requires = if tx.nonce != expected_nonce && tx.nonce > 0 {
        vec![encode(&tx.from, tx.nonce - 1)]
    } else {
        vec![]
    };

    let provides = vec![encode(&tx.from, tx.nonce)];

    Ok(ValidTransaction {
        priority: u64::MAX - tx.item,
        requires,
        provides,
        longevity: 64,
        propagate: true,
    })
}

// Execute a transaction outside of the block execution function.
// This doesn't attempt to validate anything regarding the block.
pub fn execute_transaction(utx: Extrinsic) -> ApplyExtrinsicResult {
    let extrinsic_index: u32 =
        storage::unhashed::get(well_known_keys::EXTRINSIC_INDEX).unwrap_or_default();
    let result = execute_transaction_backend(&utx, extrinsic_index);
    <ExtrinsicData<Runtime>>::insert(extrinsic_index, utx.encode());
    storage::unhashed::put(well_known_keys::EXTRINSIC_INDEX, &(extrinsic_index + 1));
    result
}

// Finalize the block.
pub fn finalize_block() -> Header {
    use sp_core::storage::StateVersion;
    let extrinsic_index: u32 = storage::unhashed::take(well_known_keys::EXTRINSIC_INDEX).unwrap();
    let txs: Vec<_> = (0..extrinsic_index)
        .map(<ExtrinsicData<Runtime>>::take)
        .collect();
    let extrinsics_root = trie::blake2_256_ordered_root(txs, StateVersion::V0);
    let number = <Number<Runtime>>::take().expect("Number is set by `initialize_block`");
    let parent_hash = <ParentHash<Runtime>>::take();
    let mut digest =
        <StorageDigest<Runtime>>::take().expect("StorageDigest is set by `initialize_block`");

    let o_new_authorities = <NewAuthorities<Runtime>>::take();

    // This MUST come after all changes to storage are done. Otherwise we will fail the
    // Storage root does not match that calculated assertion.
    let storage_root = Hash::decode(&mut &storage_root(StateVersion::V1)[..])
        .expect("`storage_root` is a valid hash");

    if let Some(new_authorities) = o_new_authorities {
        digest.push(generic::DigestItem::Consensus(
            *b"babe",
            new_authorities.encode(),
        ));
    }

    Header {
        number,
        extrinsics_root,
        state_root: storage_root,
        parent_hash,
        digest,
    }
}

#[inline(always)]
fn check_signature(utx: &Extrinsic) -> Result<(), TransactionValidityError> {
    use sp_runtime::traits::BlindCheckable;
    utx.clone()
        .check()
        .map_err(|_| InvalidTransaction::BadProof.into())
        .map(|_| ())
}

fn execute_transaction_backend(utx: &Extrinsic, _extrinsic_index: u32) -> ApplyExtrinsicResult {
    check_signature(utx)?;
    match utx {
        Extrinsic::Submit { ref message, .. } => execute_submit_backend(message),
        Extrinsic::Process => execute_process_backend(),
        Extrinsic::StorageChange(key, value) => {
            execute_storage_change(key, value.as_ref().map(|v| &**v))
        }
    }
}

fn execute_submit_backend(tx: &Message) -> ApplyExtrinsicResult {
    // check nonce
    let nonce_key = tx.from.to_keyed_vec(NONCE_OF);
    let expected_nonce: u64 = storage::hashed::get_or(&blake2_256, &nonce_key, 0);
    if tx.nonce != expected_nonce {
        return Err(InvalidTransaction::Stale.into());
    }

    // increment nonce in storage
    storage::hashed::put(&blake2_256, &nonce_key, &(expected_nonce + 1));

    // <Queue<Runtime>>::append(tx.item);
    let mut queue = <Queue<Runtime>>::get();
    queue.push(tx.item);
    <Queue<Runtime>>::put(queue);

    Ok(Ok(()))
}

fn execute_process_backend() -> ApplyExtrinsicResult {
    // Emulating message queue processing
    // Depending on the queue size this extrinsic might panic

    // Iterating throuhg the queue
    loop {
        let queue = <Queue<Runtime>>::get();
        if queue.is_empty() {
            return Ok(Ok(()));
        }
        if queue.len() == <<Runtime as Config>::PanicThreshold as Get<u32>>::get() as usize {
            panic!("Panic occurred while processing queue");
        }

        // Reducing the queue to its tail
        <Queue<Runtime>>::mutate(|v| v.remove(0));
    }
}

fn execute_storage_change(key: &[u8], value: Option<&[u8]>) -> ApplyExtrinsicResult {
    match value {
        Some(value) => storage::unhashed::put_raw(key, value),
        None => storage::unhashed::kill(key),
    }
    Ok(Ok(()))
}

#[cfg(feature = "std")]
fn info_expect_equal_hash(given: &Hash, expected: &Hash) {
    use sp_core::hexdisplay::HexDisplay;
    if given != expected {
        println!(
            "Hash: given={}, expected={}",
            HexDisplay::from(given.as_fixed_bytes()),
            HexDisplay::from(expected.as_fixed_bytes()),
        );
    }
}

#[cfg(not(feature = "std"))]
fn info_expect_equal_hash(given: &Hash, expected: &Hash) {
    if given != expected {
        sp_runtime::print("Hash not equal");
        sp_runtime::print(given.as_bytes());
        sp_runtime::print(expected.as_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::{wasm_binary_unwrap, Header, Message};
    use sc_executor::{NativeElseWasmExecutor, WasmExecutionMethod};
    use sp_core::{
        map,
        traits::{CodeExecutor, RuntimeCode},
    };
    use sp_io::{hashing::twox_128, TestExternalities};
    use test_client::{AccountKeyring, Sr25519Keyring};

    // Declare an instance of the native executor dispatch for the test runtime.
    pub struct NativeDispatch;

    impl sc_executor::NativeExecutionDispatch for NativeDispatch {
        type ExtendHostFunctions = ();

        fn dispatch(method: &str, data: &[u8]) -> Option<Vec<u8>> {
            crate::api::dispatch(method, data)
        }

        fn native_version() -> sc_executor::NativeVersion {
            crate::native_version()
        }
    }

    fn executor() -> NativeElseWasmExecutor<NativeDispatch> {
        NativeElseWasmExecutor::new(WasmExecutionMethod::Interpreted, None, 8, 2)
    }

    fn new_test_ext() -> TestExternalities {
        let authorities = vec![
            Sr25519Keyring::Alice.to_raw_public(),
            Sr25519Keyring::Bob.to_raw_public(),
            Sr25519Keyring::Charlie.to_raw_public(),
        ];

        TestExternalities::new_with_code(
            wasm_binary_unwrap(),
            sp_core::storage::Storage {
                top: map![
                    twox_128(b"latest").to_vec() => vec![69u8; 32],
                    twox_128(b"sys:auth").to_vec() => authorities.encode(),
                    blake2_256(&AccountKeyring::Alice.to_raw_public().to_keyed_vec(b"balance:")).to_vec() => {
                        vec![111u8, 0, 0, 0, 0, 0, 0, 0]
                    },
                ],
                children_default: map![],
            },
        )
    }

    fn block_import_works<F>(block_executor: F)
    where
        F: Fn(Block, &mut TestExternalities),
    {
        let h = Header {
            parent_hash: [69u8; 32].into(),
            number: 1,
            state_root: Default::default(),
            extrinsics_root: Default::default(),
            digest: Default::default(),
        };
        let mut b = Block {
            header: h,
            extrinsics: vec![],
        };

        new_test_ext().execute_with(|| polish_block(&mut b));

        block_executor(b, &mut new_test_ext());
    }

    #[test]
    fn block_import_works_native() {
        block_import_works(|b, ext| {
            ext.execute_with(|| {
                execute_block(b);
            })
        });
    }

    #[test]
    fn block_import_works_wasm() {
        block_import_works(|b, ext| {
            let mut ext = ext.ext();
            let runtime_code = RuntimeCode {
                code_fetcher: &sp_core::traits::WrappedRuntimeCode(wasm_binary_unwrap().into()),
                hash: Vec::new(),
                heap_pages: None,
            };

            executor()
                .call(
                    &mut ext,
                    &runtime_code,
                    "Core_execute_block",
                    &b.encode(),
                    false,
                )
                .0
                .unwrap();
        })
    }

    fn block_import_with_transaction_works<F>(block_executor: F)
    where
        F: Fn(Block, &mut TestExternalities),
    {
        let mut b1 = Block {
            header: Header {
                parent_hash: [69u8; 32].into(),
                number: 1,
                state_root: Default::default(),
                extrinsics_root: Default::default(),
                digest: Default::default(),
            },
            extrinsics: vec![Message {
                from: AccountKeyring::Alice.into(),
                item: 69,
                nonce: 0,
            }
            .into_signed_tx()],
        };

        let mut dummy_ext = new_test_ext();
        dummy_ext.execute_with(|| polish_block(&mut b1));

        let mut b2 = Block {
            header: Header {
                parent_hash: b1.header.hash(),
                number: 2,
                state_root: Default::default(),
                extrinsics_root: Default::default(),
                digest: Default::default(),
            },
            extrinsics: vec![
                Message {
                    from: AccountKeyring::Alice.into(),
                    item: 27,
                    nonce: 1,
                }
                .into_signed_tx(),
                Message {
                    from: AccountKeyring::Alice.into(),
                    item: 69,
                    nonce: 2,
                }
                .into_signed_tx(),
            ],
        };

        dummy_ext.execute_with(|| polish_block(&mut b2));
        drop(dummy_ext);

        let mut t = new_test_ext();

        t.execute_with(|| {
            assert_eq!(queue().len(), 0);
        });

        block_executor(b1, &mut t);

        t.execute_with(|| {
            assert_eq!(queue().len(), 1);
        });

        block_executor(b2, &mut t);

        t.execute_with(|| {
            assert_eq!(queue().len(), 3);
        });
    }

    #[test]
    fn block_import_with_transaction_works_native() {
        block_import_with_transaction_works(|b, ext| {
            ext.execute_with(|| {
                execute_block(b);
            })
        });
    }

    #[test]
    fn block_import_with_transaction_works_wasm() {
        block_import_with_transaction_works(|b, ext| {
            let mut ext = ext.ext();
            let runtime_code = RuntimeCode {
                code_fetcher: &sp_core::traits::WrappedRuntimeCode(wasm_binary_unwrap().into()),
                hash: Vec::new(),
                heap_pages: None,
            };

            executor()
                .call(
                    &mut ext,
                    &runtime_code,
                    "Core_execute_block",
                    &b.encode(),
                    false,
                )
                .0
                .unwrap();
        })
    }
}
