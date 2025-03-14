// This file is part of Gear.

// Copyright (C) 2021-2025 Gear Technologies Inc.
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

#[macro_export]
macro_rules! impl_runtime_apis_plus_common {
	{$($custom:tt)*} => {
		impl_runtime_apis! {
			$($custom)*

			impl sp_api::Core<Block> for Runtime {
				fn version() -> RuntimeVersion {
					VERSION
				}

				fn execute_block(block: Block) {
					Executive::execute_block(block);
				}

				fn initialize_block(header: &<Block as BlockT>::Header) -> sp_runtime::ExtrinsicInclusionMode {
					Executive::initialize_block(header)
				}
			}

			impl sp_api::Metadata<Block> for Runtime {
				fn metadata() -> OpaqueMetadata {
					OpaqueMetadata::new(Runtime::metadata().into())
				}

				fn metadata_at_version(version: u32) -> Option<OpaqueMetadata> {
					Runtime::metadata_at_version(version)
				}

				fn metadata_versions() -> sp_std::vec::Vec<u32> {
					Runtime::metadata_versions()
				}
			}

			impl sp_block_builder::BlockBuilder<Block> for Runtime {
				fn apply_extrinsic(extrinsic: <Block as BlockT>::Extrinsic) -> ApplyExtrinsicResult {
					Executive::apply_extrinsic(extrinsic)
				}

				fn finalize_block() -> <Block as BlockT>::Header {
					Executive::finalize_block()
				}

				fn inherent_extrinsics(data: sp_inherents::InherentData) -> Vec<<Block as BlockT>::Extrinsic> {
					data.create_extrinsics()
				}

				fn check_inherents(
					block: Block,
					data: sp_inherents::InherentData,
				) -> sp_inherents::CheckInherentsResult {
					data.check_extrinsics(&block)
				}
			}

			impl sp_transaction_pool::runtime_api::TaggedTransactionQueue<Block> for Runtime {
				fn validate_transaction(
					source: TransactionSource,
					tx: <Block as BlockT>::Extrinsic,
					block_hash: <Block as BlockT>::Hash,
				) -> TransactionValidity {
					Executive::validate_transaction(source, tx, block_hash)
				}
			}

			impl sp_offchain::OffchainWorkerApi<Block> for Runtime {
				fn offchain_worker(header: &<Block as BlockT>::Header) {
					Executive::offchain_worker(header)
				}
			}

			impl sp_session::SessionKeys<Block> for Runtime {
				fn generate_session_keys(seed: Option<Vec<u8>>) -> Vec<u8> {
					SessionKeys::generate(seed)
				}

				fn decode_session_keys(
					encoded: Vec<u8>,
				) -> Option<Vec<(Vec<u8>, KeyTypeId)>> {
					SessionKeys::decode_into_raw_public_keys(&encoded)
				}
			}

			impl fg_primitives::GrandpaApi<Block> for Runtime {
				fn grandpa_authorities() -> GrandpaAuthorityList {
					Grandpa::grandpa_authorities()
				}

				fn current_set_id() -> fg_primitives::SetId {
					Grandpa::current_set_id()
				}

				fn submit_report_equivocation_unsigned_extrinsic(
					_equivocation_proof: fg_primitives::EquivocationProof<
						<Block as BlockT>::Hash,
						NumberFor<Block>,
					>,
					_key_owner_proof: fg_primitives::OpaqueKeyOwnershipProof,
				) -> Option<()> {
					None
				}

				fn generate_key_ownership_proof(
					_set_id: fg_primitives::SetId,
					_authority_id: GrandpaId,
				) -> Option<fg_primitives::OpaqueKeyOwnershipProof> {
					// NOTE: this is the only implementation possible since we've
					// defined our key owner proof type as a bottom type (i.e. a type
					// with no values).
					None
				}
			}

			impl frame_system_rpc_runtime_api::AccountNonceApi<Block, AccountId, Nonce> for Runtime {
				fn account_nonce(account: AccountId) -> Nonce {
					System::account_nonce(account)
				}
			}

			impl pallet_transaction_payment_rpc_runtime_api::TransactionPaymentApi<Block, Balance> for Runtime {
				fn query_info(
					uxt: <Block as BlockT>::Extrinsic,
					len: u32,
				) -> pallet_transaction_payment_rpc_runtime_api::RuntimeDispatchInfo<Balance> {
					GearPayment::query_info(uxt, len)
				}
				fn query_fee_details(
					uxt: <Block as BlockT>::Extrinsic,
					len: u32,
				) -> pallet_transaction_payment::FeeDetails<Balance> {
					GearPayment::query_fee_details(uxt, len)
				}
				fn query_weight_to_fee(weight: Weight) -> Balance {
					TransactionPayment::weight_to_fee(weight)
				}
				fn query_length_to_fee(length: u32) -> Balance {
					TransactionPayment::length_to_fee(length)
				}
			}

			// Here we implement our custom runtime API.
			impl pallet_gear_rpc_runtime_api::GearApi<Block> for Runtime {
				fn calculate_reply_for_handle(
					origin: H256,
					destination: H256,
					payload: Vec<u8>,
					gas_limit: u64,
					value: u128,
					allowance_multiplier: u64,
				) -> Result<pallet_gear::ReplyInfo, Vec<u8>> {
					Gear::calculate_reply_for_handle(origin, destination, payload, gas_limit, value, allowance_multiplier)
				}

				fn calculate_gas_info(
					account_id: H256,
					kind: HandleKind,
					payload: Vec<u8>,
					value: u128,
					allow_other_panics: bool,
					initial_gas: Option<u64>,
					gas_allowance: Option<u64>,
				) -> Result<pallet_gear::GasInfo, Vec<u8>> {
					Gear::calculate_gas_info(account_id, kind, payload, value, allow_other_panics, initial_gas, gas_allowance)
				}

				fn gear_run_extrinsic(max_gas: Option<u64>) -> <Block as BlockT>::Extrinsic {
					UncheckedExtrinsic::new_unsigned(
						pallet_gear::Call::run { max_gas }.into()
					).into()
				}

				fn read_state(program_id: H256, payload: Vec<u8>, gas_allowance: Option<u64>,) -> Result<Vec<u8>, Vec<u8>> {
					Gear::read_state(program_id, payload, gas_allowance)
				}

				fn read_state_using_wasm(
					program_id: H256,
					payload: Vec<u8>,
					fn_name: Vec<u8>,
					wasm: Vec<u8>,
					argument: Option<Vec<u8>>,
					gas_allowance: Option<u64>,
				) -> Result<Vec<u8>, Vec<u8>> {
					Gear::read_state_using_wasm(program_id, payload, fn_name, wasm, argument, gas_allowance)
				}

				fn read_metahash(program_id: H256, gas_allowance: Option<u64>,) -> Result<H256, Vec<u8>> {
					Gear::read_metahash(program_id, gas_allowance)
				}
			}

			#[cfg(feature = "runtime-benchmarks")]
			impl frame_benchmarking::Benchmark<Block> for Runtime {
				fn benchmark_metadata(extra: bool) -> (
					Vec<frame_benchmarking::BenchmarkList>,
					Vec<frame_support::traits::StorageInfo>,
				) {
					use frame_benchmarking::{baseline, Benchmarking, BenchmarkList};
					use frame_support::traits::StorageInfoTrait;
					use frame_system_benchmarking::Pallet as SystemBench;
					use baseline::Pallet as BaselineBench;

					let mut list = Vec::<BenchmarkList>::new();
					list_benchmarks!(list, extra);

					let storage_info = AllPalletsWithSystem::storage_info();

					(list, storage_info)
				}

				fn dispatch_benchmark(
					config: frame_benchmarking::BenchmarkConfig
				) -> Result<Vec<frame_benchmarking::BenchmarkBatch>, sp_runtime::RuntimeString> {
					use frame_benchmarking::{baseline, Benchmarking, BenchmarkBatch};
					use sp_storage::TrackedStorageKey;
					use frame_system_benchmarking::Pallet as SystemBench;
					use baseline::Pallet as BaselineBench;

					impl frame_system_benchmarking::Config for Runtime {}
					impl baseline::Config for Runtime {}

					use frame_support::traits::WhitelistedStorageKeys;
					let whitelist: Vec<TrackedStorageKey> = AllPalletsWithSystem::whitelisted_storage_keys();

					let mut batches = Vec::<BenchmarkBatch>::new();
					let params = (&config, &whitelist);
					add_benchmarks!(params, batches);

					Ok(batches)
				}
			}
		}
	}
}
