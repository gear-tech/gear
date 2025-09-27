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

#[allow(dead_code, unused_imports, non_camel_case_types)]
#[allow(clippy::all)]
#[allow(rustdoc::broken_intra_doc_links)]
pub mod runtime_types {
    #[allow(unused_imports)]
    mod root_mod {
        pub use super::*;
    }
    pub mod runtime_types {
        use super::runtime_types;
        pub mod bounded_collections {
            use super::runtime_types;
            pub mod bounded_btree_map {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct BoundedBTreeMap<_0, _1>(
                    pub ::subxt::ext::subxt_core::utils::KeyedVec<_0, _1>,
                );
            }
            pub mod bounded_vec {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct BoundedVec<_0>(pub ::subxt::ext::subxt_core::alloc::vec::Vec<_0>);
            }
            pub mod weak_bounded_vec {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct WeakBoundedVec<_0>(pub ::subxt::ext::subxt_core::alloc::vec::Vec<_0>);
            }
        }
        pub mod finality_grandpa {
            use super::runtime_types;
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub struct Equivocation<_0, _1, _2> {
                pub round_number: ::core::primitive::u64,
                pub identity: _0,
                pub first: (_1, _2),
                pub second: (_1, _2),
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub struct Precommit<_0, _1> {
                pub target_hash: _0,
                pub target_number: _1,
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub struct Prevote<_0, _1> {
                pub target_hash: _0,
                pub target_number: _1,
            }
        }
        pub mod frame_metadata_hash_extension {
            use super::runtime_types;
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub struct CheckMetadataHash {
                pub mode: runtime_types::frame_metadata_hash_extension::Mode,
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub enum Mode {
                #[codec(index = 0)]
                Disabled,
                #[codec(index = 1)]
                Enabled,
            }
        }
        pub mod frame_support {
            use super::runtime_types;
            pub mod dispatch {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum DispatchClass {
                    #[codec(index = 0)]
                    Normal,
                    #[codec(index = 1)]
                    Operational,
                    #[codec(index = 2)]
                    Mandatory,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct DispatchInfo {
                    pub weight: runtime_types::sp_weights::weight_v2::Weight,
                    pub class: runtime_types::frame_support::dispatch::DispatchClass,
                    pub pays_fee: runtime_types::frame_support::dispatch::Pays,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum Pays {
                    #[codec(index = 0)]
                    Yes,
                    #[codec(index = 1)]
                    No,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct PerDispatchClass<_0> {
                    pub normal: _0,
                    pub operational: _0,
                    pub mandatory: _0,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct PostDispatchInfo {
                    pub actual_weight:
                        ::core::option::Option<runtime_types::sp_weights::weight_v2::Weight>,
                    pub pays_fee: runtime_types::frame_support::dispatch::Pays,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum RawOrigin<_0> {
                    #[codec(index = 0)]
                    Root,
                    #[codec(index = 1)]
                    Signed(_0),
                    #[codec(index = 2)]
                    None,
                }
            }
            pub mod traits {
                use super::runtime_types;
                pub mod preimages {
                    use super::runtime_types;
                    #[derive(
                        Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode,
                    )]
                    pub enum Bounded<_0, _1> {
                        #[codec(index = 0)]
                        Legacy {
                            hash: ::subxt::ext::subxt_core::utils::H256,
                        },
                        #[codec(index = 1)]
                        Inline(
                            runtime_types::bounded_collections::bounded_vec::BoundedVec<
                                ::core::primitive::u8,
                            >,
                        ),
                        #[codec(index = 2)]
                        Lookup {
                            hash: ::subxt::ext::subxt_core::utils::H256,
                            len: ::core::primitive::u32,
                        },
                        __Ignore(::core::marker::PhantomData<(_0, _1)>),
                    }
                }
                pub mod schedule {
                    use super::runtime_types;
                    #[derive(
                        Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode,
                    )]
                    pub enum DispatchTime<_0> {
                        #[codec(index = 0)]
                        At(_0),
                        #[codec(index = 1)]
                        After(_0),
                    }
                }
                pub mod tokens {
                    use super::runtime_types;
                    pub mod fungible {
                        use super::runtime_types;
                        #[derive(
                            ::subxt::ext ::subxt_core::ext::codec::CompactAs,
                            Debug,
                            crate::gp::Decode,
                            crate::gp::DecodeAsType,
                            crate::gp::Encode,
                        )]
                        pub struct HoldConsideration(pub ::core::primitive::u128);
                    }
                    pub mod misc {
                        use super::runtime_types;
                        #[derive(
                            Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode,
                        )]
                        pub enum BalanceStatus {
                            #[codec(index = 0)]
                            Free,
                            #[codec(index = 1)]
                            Reserved,
                        }
                        #[derive(
                            Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode,
                        )]
                        pub struct IdAmount<_0, _1> {
                            pub id: _0,
                            pub amount: _1,
                        }
                    }
                }
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub struct PalletId(pub [::core::primitive::u8; 8usize]);
        }
        pub mod frame_system {
            use super::runtime_types;
            pub mod extensions {
                use super::runtime_types;
                pub mod check_genesis {
                    use super::runtime_types;
                    #[derive(
                        Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode,
                    )]
                    pub struct CheckGenesis;
                }
                pub mod check_mortality {
                    use super::runtime_types;
                    #[derive(
                        Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode,
                    )]
                    pub struct CheckMortality(pub runtime_types::sp_runtime::generic::era::Era);
                }
                pub mod check_non_zero_sender {
                    use super::runtime_types;
                    #[derive(
                        Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode,
                    )]
                    pub struct CheckNonZeroSender;
                }
                pub mod check_spec_version {
                    use super::runtime_types;
                    #[derive(
                        Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode,
                    )]
                    pub struct CheckSpecVersion;
                }
                pub mod check_tx_version {
                    use super::runtime_types;
                    #[derive(
                        Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode,
                    )]
                    pub struct CheckTxVersion;
                }
                pub mod check_weight {
                    use super::runtime_types;
                    #[derive(
                        Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode,
                    )]
                    pub struct CheckWeight;
                }
            }
            pub mod limits {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct BlockLength {
                    pub max: runtime_types::frame_support::dispatch::PerDispatchClass<
                        ::core::primitive::u32,
                    >,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct BlockWeights {
                    pub base_block: runtime_types::sp_weights::weight_v2::Weight,
                    pub max_block: runtime_types::sp_weights::weight_v2::Weight,
                    pub per_class: runtime_types::frame_support::dispatch::PerDispatchClass<
                        runtime_types::frame_system::limits::WeightsPerClass,
                    >,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct WeightsPerClass {
                    pub base_extrinsic: runtime_types::sp_weights::weight_v2::Weight,
                    pub max_extrinsic:
                        ::core::option::Option<runtime_types::sp_weights::weight_v2::Weight>,
                    pub max_total:
                        ::core::option::Option<runtime_types::sp_weights::weight_v2::Weight>,
                    pub reserved:
                        ::core::option::Option<runtime_types::sp_weights::weight_v2::Weight>,
                }
            }
            pub mod pallet {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "Contains a variant per dispatchable extrinsic that this pallet has."]
                pub enum Call {
                    #[codec(index = 0)]
                    #[doc = "Make some on-chain remark."]
                    #[doc = ""]
                    #[doc = "Can be executed by every `origin`."]
                    remark {
                        remark: ::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
                    },
                    #[codec(index = 1)]
                    #[doc = "Set the number of pages in the WebAssembly environment's heap."]
                    set_heap_pages { pages: ::core::primitive::u64 },
                    #[codec(index = 2)]
                    #[doc = "Set the new runtime code."]
                    set_code {
                        code: ::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
                    },
                    #[codec(index = 3)]
                    #[doc = "Set the new runtime code without doing any checks of the given `code`."]
                    #[doc = ""]
                    #[doc = "Note that runtime upgrades will not run if this is called with a not-increasing spec"]
                    #[doc = "version!"]
                    set_code_without_checks {
                        code: ::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
                    },
                    #[codec(index = 4)]
                    #[doc = "Set some items of storage."]
                    set_storage {
                        items: ::subxt::ext::subxt_core::alloc::vec::Vec<(
                            ::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
                            ::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
                        )>,
                    },
                    #[codec(index = 5)]
                    #[doc = "Kill some items from storage."]
                    kill_storage {
                        keys: ::subxt::ext::subxt_core::alloc::vec::Vec<
                            ::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
                        >,
                    },
                    #[codec(index = 6)]
                    #[doc = "Kill all storage items with a key that starts with the given prefix."]
                    #[doc = ""]
                    #[doc = "**NOTE:** We rely on the Root origin to provide us the number of subkeys under"]
                    #[doc = "the prefix we are removing to accurately calculate the weight of this function."]
                    kill_prefix {
                        prefix: ::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
                        subkeys: ::core::primitive::u32,
                    },
                    #[codec(index = 7)]
                    #[doc = "Make some on-chain remark and emit event."]
                    remark_with_event {
                        remark: ::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
                    },
                    #[codec(index = 9)]
                    #[doc = "Authorize an upgrade to a given `code_hash` for the runtime. The runtime can be supplied"]
                    #[doc = "later."]
                    #[doc = ""]
                    #[doc = "This call requires Root origin."]
                    authorize_upgrade {
                        code_hash: ::subxt::ext::subxt_core::utils::H256,
                    },
                    #[codec(index = 10)]
                    #[doc = "Authorize an upgrade to a given `code_hash` for the runtime. The runtime can be supplied"]
                    #[doc = "later."]
                    #[doc = ""]
                    #[doc = "WARNING: This authorizes an upgrade that will take place without any safety checks, for"]
                    #[doc = "example that the spec name remains the same and that the version number increases. Not"]
                    #[doc = "recommended for normal use. Use `authorize_upgrade` instead."]
                    #[doc = ""]
                    #[doc = "This call requires Root origin."]
                    authorize_upgrade_without_checks {
                        code_hash: ::subxt::ext::subxt_core::utils::H256,
                    },
                    #[codec(index = 11)]
                    #[doc = "Provide the preimage (runtime binary) `code` for an upgrade that has been authorized."]
                    #[doc = ""]
                    #[doc = "If the authorization required a version check, this call will ensure the spec name"]
                    #[doc = "remains unchanged and that the spec version has increased."]
                    #[doc = ""]
                    #[doc = "Depending on the runtime's `OnSetCode` configuration, this function may directly apply"]
                    #[doc = "the new `code` in the same block or attempt to schedule the upgrade."]
                    #[doc = ""]
                    #[doc = "All origins are allowed."]
                    apply_authorized_upgrade {
                        code: ::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
                    },
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "Error for the System pallet"]
                pub enum Error {
                    #[codec(index = 0)]
                    #[doc = "The name of specification does not match between the current runtime"]
                    #[doc = "and the new runtime."]
                    InvalidSpecName,
                    #[codec(index = 1)]
                    #[doc = "The specification version is not allowed to decrease between the current runtime"]
                    #[doc = "and the new runtime."]
                    SpecVersionNeedsToIncrease,
                    #[codec(index = 2)]
                    #[doc = "Failed to extract the runtime version from the new runtime."]
                    #[doc = ""]
                    #[doc = "Either calling `Core_version` or decoding `RuntimeVersion` failed."]
                    FailedToExtractRuntimeVersion,
                    #[codec(index = 3)]
                    #[doc = "Suicide called when the account has non-default composite data."]
                    NonDefaultComposite,
                    #[codec(index = 4)]
                    #[doc = "There is a non-zero reference count preventing the account from being purged."]
                    NonZeroRefCount,
                    #[codec(index = 5)]
                    #[doc = "The origin filter prevent the call to be dispatched."]
                    CallFiltered,
                    #[codec(index = 6)]
                    #[doc = "A multi-block migration is ongoing and prevents the current code from being replaced."]
                    MultiBlockMigrationsOngoing,
                    #[codec(index = 7)]
                    #[doc = "No upgrade authorized."]
                    NothingAuthorized,
                    #[codec(index = 8)]
                    #[doc = "The submitted code is not authorized."]
                    Unauthorized,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "Event for the System pallet."]
                pub enum Event {
                    #[codec(index = 0)]
                    #[doc = "An extrinsic completed successfully."]
                    ExtrinsicSuccess {
                        dispatch_info: runtime_types::frame_support::dispatch::DispatchInfo,
                    },
                    #[codec(index = 1)]
                    #[doc = "An extrinsic failed."]
                    ExtrinsicFailed {
                        dispatch_error: runtime_types::sp_runtime::DispatchError,
                        dispatch_info: runtime_types::frame_support::dispatch::DispatchInfo,
                    },
                    #[codec(index = 2)]
                    #[doc = "`:code` was updated."]
                    CodeUpdated,
                    #[codec(index = 3)]
                    #[doc = "A new account was created."]
                    NewAccount {
                        account: ::subxt::ext::subxt_core::utils::AccountId32,
                    },
                    #[codec(index = 4)]
                    #[doc = "An account was reaped."]
                    KilledAccount {
                        account: ::subxt::ext::subxt_core::utils::AccountId32,
                    },
                    #[codec(index = 5)]
                    #[doc = "On on-chain remark happened."]
                    Remarked {
                        sender: ::subxt::ext::subxt_core::utils::AccountId32,
                        hash: ::subxt::ext::subxt_core::utils::H256,
                    },
                    #[codec(index = 6)]
                    #[doc = "An upgrade was authorized."]
                    UpgradeAuthorized {
                        code_hash: ::subxt::ext::subxt_core::utils::H256,
                        check_version: ::core::primitive::bool,
                    },
                }
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub struct AccountInfo<_0, _1> {
                pub nonce: _0,
                pub consumers: ::core::primitive::u32,
                pub providers: ::core::primitive::u32,
                pub sufficients: ::core::primitive::u32,
                pub data: _1,
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub struct CodeUpgradeAuthorization {
                pub code_hash: ::subxt::ext::subxt_core::utils::H256,
                pub check_version: ::core::primitive::bool,
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub struct EventRecord<_0, _1> {
                pub phase: runtime_types::frame_system::Phase,
                pub event: _0,
                pub topics: ::subxt::ext::subxt_core::alloc::vec::Vec<_1>,
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub struct LastRuntimeUpgradeInfo {
                #[codec(compact)]
                pub spec_version: ::core::primitive::u32,
                pub spec_name: ::subxt::ext::subxt_core::alloc::string::String,
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub enum Phase {
                #[codec(index = 0)]
                ApplyExtrinsic(::core::primitive::u32),
                #[codec(index = 1)]
                Finalization,
                #[codec(index = 2)]
                Initialization,
            }
        }
        pub mod gear_common {
            use super::runtime_types;
            pub mod event {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum CodeChangeKind<_0> {
                    #[codec(index = 0)]
                    Active {
                        expiration: ::core::option::Option<_0>,
                    },
                    #[codec(index = 1)]
                    Inactive,
                    #[codec(index = 2)]
                    Reinstrumented,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum DispatchStatus {
                    #[codec(index = 0)]
                    Success,
                    #[codec(index = 1)]
                    Failed,
                    #[codec(index = 2)]
                    NotExecuted,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum MessageEntry {
                    #[codec(index = 0)]
                    Init,
                    #[codec(index = 1)]
                    Handle,
                    #[codec(index = 2)]
                    Reply(runtime_types::gprimitives::MessageId),
                    #[codec(index = 3)]
                    Signal,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum MessageWaitedRuntimeReason {
                    #[codec(index = 0)]
                    WaitCalled,
                    #[codec(index = 1)]
                    WaitForCalled,
                    #[codec(index = 2)]
                    WaitUpToCalled,
                    #[codec(index = 3)]
                    WaitUpToCalledFull,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum MessageWaitedSystemReason {}
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum MessageWokenRuntimeReason {
                    #[codec(index = 0)]
                    WakeCalled,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum MessageWokenSystemReason {
                    #[codec(index = 0)]
                    ProgramGotInitialized,
                    #[codec(index = 1)]
                    TimeoutHasCome,
                    #[codec(index = 2)]
                    OutOfRent,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum ProgramChangeKind<_0> {
                    #[codec(index = 0)]
                    Active { expiration: _0 },
                    #[codec(index = 1)]
                    Inactive,
                    #[codec(index = 2)]
                    Paused,
                    #[codec(index = 3)]
                    Terminated,
                    #[codec(index = 4)]
                    ExpirationChanged { expiration: _0 },
                    #[codec(index = 5)]
                    ProgramSet { expiration: _0 },
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum Reason<_0, _1> {
                    #[codec(index = 0)]
                    Runtime(_0),
                    #[codec(index = 1)]
                    System(_1),
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum UserMessageReadRuntimeReason {
                    #[codec(index = 0)]
                    MessageReplied,
                    #[codec(index = 1)]
                    MessageClaimed,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum UserMessageReadSystemReason {
                    #[codec(index = 0)]
                    OutOfRent,
                }
            }
            pub mod gas_provider {
                use super::runtime_types;
                pub mod node {
                    use super::runtime_types;
                    #[derive(
                        Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode,
                    )]
                    pub struct ChildrenRefs {
                        pub spec_refs: ::core::primitive::u32,
                        pub unspec_refs: ::core::primitive::u32,
                    }
                    #[derive(
                        Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode,
                    )]
                    pub enum GasNode<_0, _1, _2, _3> {
                        #[codec(index = 0)]
                        External {
                            id: _0,
                            multiplier: runtime_types::gear_common::GasMultiplier<_3, _2>,
                            value: _2,
                            lock: runtime_types::gear_common::gas_provider::node::NodeLock<_2>,
                            system_reserve: _2,
                            refs: runtime_types::gear_common::gas_provider::node::ChildrenRefs,
                            consumed: ::core::primitive::bool,
                            deposit: ::core::primitive::bool,
                        },
                        #[codec(index = 1)]
                        Cut {
                            id: _0,
                            multiplier: runtime_types::gear_common::GasMultiplier<_3, _2>,
                            value: _2,
                            lock: runtime_types::gear_common::gas_provider::node::NodeLock<_2>,
                        },
                        #[codec(index = 2)]
                        Reserved {
                            id: _0,
                            multiplier: runtime_types::gear_common::GasMultiplier<_3, _2>,
                            value: _2,
                            lock: runtime_types::gear_common::gas_provider::node::NodeLock<_2>,
                            refs: runtime_types::gear_common::gas_provider::node::ChildrenRefs,
                            consumed: ::core::primitive::bool,
                        },
                        #[codec(index = 3)]
                        SpecifiedLocal {
                            parent: _1,
                            root: _1,
                            value: _2,
                            lock: runtime_types::gear_common::gas_provider::node::NodeLock<_2>,
                            system_reserve: _2,
                            refs: runtime_types::gear_common::gas_provider::node::ChildrenRefs,
                            consumed: ::core::primitive::bool,
                        },
                        #[codec(index = 4)]
                        UnspecifiedLocal {
                            parent: _1,
                            root: _1,
                            lock: runtime_types::gear_common::gas_provider::node::NodeLock<_2>,
                            system_reserve: _2,
                        },
                    }
                    #[derive(
                        Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode,
                    )]
                    pub enum GasNodeId<_0, _1> {
                        #[codec(index = 0)]
                        Node(_0),
                        #[codec(index = 1)]
                        Reservation(_1),
                    }
                    #[derive(
                        Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode,
                    )]
                    pub struct NodeLock<_0>(pub [_0; 4usize]);
                }
            }
            pub mod storage {
                use super::runtime_types;
                pub mod complicated {
                    use super::runtime_types;
                    pub mod dequeue {
                        use super::runtime_types;
                        #[derive(
                            Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode,
                        )]
                        pub struct LinkedNode<_0, _1> {
                            pub next: ::core::option::Option<_0>,
                            pub value: _1,
                        }
                    }
                }
                pub mod primitives {
                    use super::runtime_types;
                    #[derive(
                        Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode,
                    )]
                    pub struct Interval<_0> {
                        pub start: _0,
                        pub finish: _0,
                    }
                }
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub enum GasMultiplier<_0, _1> {
                #[codec(index = 0)]
                ValuePerGas(_0),
                #[codec(index = 1)]
                GasPerValue(_1),
            }
        }
        pub mod gear_core {
            use super::runtime_types;
            pub mod code {
                use super::runtime_types;
                pub mod instrumented {
                    use super::runtime_types;
                    #[derive(
                        Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode,
                    )]
                    pub struct InstantiatedSectionSizes {
                        pub code_section: ::core::primitive::u32,
                        pub data_section: ::core::primitive::u32,
                        pub global_section: ::core::primitive::u32,
                        pub table_section: ::core::primitive::u32,
                        pub element_section: ::core::primitive::u32,
                        pub type_section: ::core::primitive::u32,
                    }
                    #[derive(
                        Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode,
                    )]
                    pub struct InstrumentedCode {
                        pub bytes: ::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
                        pub instantiated_section_sizes:
                            runtime_types::gear_core::code::instrumented::InstantiatedSectionSizes,
                    }
                }
                pub mod metadata {
                    use super::runtime_types;
                    #[derive(
                        Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode,
                    )]
                    pub struct CodeMetadata {
                        pub original_code_len: ::core::primitive::u32,
                        pub exports: ::subxt::ext::subxt_core::alloc::vec::Vec<
                            runtime_types::gear_core::message::DispatchKind,
                        >,
                        pub static_pages: runtime_types::gear_core::pages::PagesAmount,
                        pub stack_end:
                            ::core::option::Option<runtime_types::gear_core::pages::Page>,
                        pub instrumentation_status:
                            runtime_types::gear_core::code::metadata::InstrumentationStatus,
                    }
                    #[derive(
                        Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode,
                    )]
                    pub enum InstrumentationStatus {
                        #[codec(index = 0)]
                        NotInstrumented,
                        #[codec(index = 1)]
                        Instrumented {
                            version: ::core::primitive::u32,
                            code_len: ::core::primitive::u32,
                        },
                        #[codec(index = 2)]
                        InstrumentationFailed { version: ::core::primitive::u32 },
                    }
                }
            }
            pub mod limited {
                use super::runtime_types;
                pub mod vec {
                    use super::runtime_types;
                    #[derive(
                        Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode,
                    )]
                    pub struct LimitedVec<_0>(pub ::subxt::ext::subxt_core::alloc::vec::Vec<_0>);
                }
            }
            pub mod memory {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct PageBuf(
                    pub runtime_types::gear_core::limited::vec::LimitedVec<::core::primitive::u8>,
                );
            }
            pub mod message {
                use super::runtime_types;
                pub mod common {
                    use super::runtime_types;
                    #[derive(
                        Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode,
                    )]
                    pub enum MessageDetails {
                        #[codec(index = 0)]
                        Reply(runtime_types::gear_core::message::common::ReplyDetails),
                        #[codec(index = 1)]
                        Signal(runtime_types::gear_core::message::common::SignalDetails),
                    }
                    #[derive(
                        Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode,
                    )]
                    pub struct ReplyDetails {
                        pub to: runtime_types::gprimitives::MessageId,
                        pub code: runtime_types::gear_core_errors::simple::ReplyCode,
                    }
                    #[derive(
                        Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode,
                    )]
                    pub struct SignalDetails {
                        pub to: runtime_types::gprimitives::MessageId,
                        pub code: runtime_types::gear_core_errors::simple::SignalCode,
                    }
                }
                pub mod context {
                    use super::runtime_types;
                    #[derive(
                        Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode,
                    )]
                    pub struct ContextStore {
                        pub initialized: ::subxt::ext::subxt_core::alloc::vec::Vec<
                            runtime_types::gprimitives::ActorId,
                        >,
                        pub reservation_nonce:
                            runtime_types::gear_core::reservation::ReservationNonce,
                        pub system_reservation: ::core::option::Option<::core::primitive::u64>,
                        pub local_nonce: ::core::primitive::u32,
                    }
                }
                pub mod stored {
                    use super::runtime_types;
                    #[derive(
                        Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode,
                    )]
                    pub struct StoredDelayedDispatch {
                        pub kind: runtime_types::gear_core::message::DispatchKind,
                        pub message: runtime_types::gear_core::message::stored::StoredMessage,
                    }
                    #[derive(
                        Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode,
                    )]
                    pub struct StoredDispatch {
                        pub kind: runtime_types::gear_core::message::DispatchKind,
                        pub message: runtime_types::gear_core::message::stored::StoredMessage,
                        pub context: ::core::option::Option<
                            runtime_types::gear_core::message::context::ContextStore,
                        >,
                    }
                    #[derive(
                        Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode,
                    )]
                    pub struct StoredMessage {
                        pub id: runtime_types::gprimitives::MessageId,
                        pub source: runtime_types::gprimitives::ActorId,
                        pub destination: runtime_types::gprimitives::ActorId,
                        pub payload: runtime_types::gear_core::limited::vec::LimitedVec<
                            ::core::primitive::u8,
                        >,
                        #[codec(compact)]
                        pub value: ::core::primitive::u128,
                        pub details: ::core::option::Option<
                            runtime_types::gear_core::message::common::MessageDetails,
                        >,
                    }
                }
                pub mod user {
                    use super::runtime_types;
                    #[derive(
                        Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode,
                    )]
                    pub struct UserMessage {
                        pub id: runtime_types::gprimitives::MessageId,
                        pub source: runtime_types::gprimitives::ActorId,
                        pub destination: runtime_types::gprimitives::ActorId,
                        pub payload: runtime_types::gear_core::limited::vec::LimitedVec<
                            ::core::primitive::u8,
                        >,
                        #[codec(compact)]
                        pub value: ::core::primitive::u128,
                        pub details: ::core::option::Option<
                            runtime_types::gear_core::message::common::ReplyDetails,
                        >,
                    }
                    #[derive(
                        Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode,
                    )]
                    pub struct UserStoredMessage {
                        pub id: runtime_types::gprimitives::MessageId,
                        pub source: runtime_types::gprimitives::ActorId,
                        pub destination: runtime_types::gprimitives::ActorId,
                        pub payload: runtime_types::gear_core::limited::vec::LimitedVec<
                            ::core::primitive::u8,
                        >,
                        #[codec(compact)]
                        pub value: ::core::primitive::u128,
                    }
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum DispatchKind {
                    #[codec(index = 0)]
                    Init,
                    #[codec(index = 1)]
                    Handle,
                    #[codec(index = 2)]
                    Reply,
                    #[codec(index = 3)]
                    Signal,
                }
            }
            pub mod pages {
                use super::runtime_types;
                #[derive(
                    ::subxt::ext ::subxt_core::ext::codec::CompactAs,
                    Debug,
                    crate::gp::Decode,
                    crate::gp::DecodeAsType,
                    crate::gp::Encode,
                )]
                pub struct Page(pub ::core::primitive::u32);
                #[derive(
                    ::subxt::ext ::subxt_core::ext::codec::CompactAs,
                    Debug,
                    crate::gp::Decode,
                    crate::gp::DecodeAsType,
                    crate::gp::Encode,
                )]
                pub struct PagesAmount(pub ::core::primitive::u32);
            }
            pub mod percent {
                use super::runtime_types;
                #[derive(
                    ::subxt::ext ::subxt_core::ext::codec::CompactAs,
                    Debug,
                    crate::gp::Decode,
                    crate::gp::DecodeAsType,
                    crate::gp::Encode,
                )]
                pub struct Percent(pub ::core::primitive::u32);
            }
            pub mod program {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct ActiveProgram<_0> {
                    pub allocations_tree_len: ::core::primitive::u32,
                    pub memory_infix: runtime_types::gear_core::program::MemoryInfix,
                    pub gas_reservation_map: ::subxt::ext::subxt_core::utils::KeyedVec<
                        runtime_types::gprimitives::ReservationId,
                        runtime_types::gear_core::reservation::GasReservationSlot,
                    >,
                    pub code_id: runtime_types::gprimitives::CodeId,
                    pub state: runtime_types::gear_core::program::ProgramState,
                    pub expiration_block: _0,
                }
                #[derive(
                    ::subxt::ext ::subxt_core::ext::codec::CompactAs,
                    Debug,
                    crate::gp::Decode,
                    crate::gp::DecodeAsType,
                    crate::gp::Encode,
                )]
                pub struct MemoryInfix(pub ::core::primitive::u32);
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum Program<_0> {
                    #[codec(index = 0)]
                    Active(runtime_types::gear_core::program::ActiveProgram<_0>),
                    #[codec(index = 1)]
                    Exited(runtime_types::gprimitives::ActorId),
                    #[codec(index = 2)]
                    Terminated(runtime_types::gprimitives::ActorId),
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum ProgramState {
                    #[codec(index = 0)]
                    Uninitialized {
                        message_id: runtime_types::gprimitives::MessageId,
                    },
                    #[codec(index = 1)]
                    Initialized,
                }
            }
            pub mod reservation {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct GasReservationSlot {
                    pub amount: ::core::primitive::u64,
                    pub start: ::core::primitive::u32,
                    pub finish: ::core::primitive::u32,
                }
                #[derive(
                    ::subxt::ext ::subxt_core::ext::codec::CompactAs,
                    Debug,
                    crate::gp::Decode,
                    crate::gp::DecodeAsType,
                    crate::gp::Encode,
                )]
                pub struct ReservationNonce(pub ::core::primitive::u64);
            }
            pub mod tasks {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum ScheduledTask<_0, _1, _2> {
                    #[codec(index = 0)]
                    PauseProgram(runtime_types::gprimitives::ActorId),
                    #[codec(index = 1)]
                    RemoveCode(runtime_types::gprimitives::CodeId),
                    #[codec(index = 2)]
                    RemoveFromMailbox(_0, runtime_types::gprimitives::MessageId),
                    #[codec(index = 3)]
                    RemoveFromWaitlist(
                        runtime_types::gprimitives::ActorId,
                        runtime_types::gprimitives::MessageId,
                    ),
                    #[codec(index = 4)]
                    RemovePausedProgram(runtime_types::gprimitives::ActorId),
                    #[codec(index = 5)]
                    WakeMessage(
                        runtime_types::gprimitives::ActorId,
                        runtime_types::gprimitives::MessageId,
                    ),
                    #[codec(index = 6)]
                    SendDispatch(_1),
                    #[codec(index = 7)]
                    SendUserMessage {
                        message_id: runtime_types::gprimitives::MessageId,
                        to_mailbox: _2,
                    },
                    #[codec(index = 8)]
                    RemoveGasReservation(
                        runtime_types::gprimitives::ActorId,
                        runtime_types::gprimitives::ReservationId,
                    ),
                    #[codec(index = 9)]
                    RemoveResumeSession(::core::primitive::u32),
                }
            }
        }
        pub mod gear_core_errors {
            use super::runtime_types;
            pub mod simple {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum ErrorReplyReason {
                    #[codec(index = 0)]
                    Execution(runtime_types::gear_core_errors::simple::SimpleExecutionError),
                    #[codec(index = 2)]
                    UnavailableActor(
                        runtime_types::gear_core_errors::simple::SimpleUnavailableActorError,
                    ),
                    #[codec(index = 3)]
                    RemovedFromWaitlist,
                    #[codec(index = 255)]
                    Unsupported,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum ReplyCode {
                    #[codec(index = 0)]
                    Success(runtime_types::gear_core_errors::simple::SuccessReplyReason),
                    #[codec(index = 1)]
                    Error(runtime_types::gear_core_errors::simple::ErrorReplyReason),
                    #[codec(index = 255)]
                    Unsupported,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum SignalCode {
                    #[codec(index = 0)]
                    Execution(runtime_types::gear_core_errors::simple::SimpleExecutionError),
                    #[codec(index = 1)]
                    RemovedFromWaitlist,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum SimpleExecutionError {
                    #[codec(index = 0)]
                    RanOutOfGas,
                    #[codec(index = 1)]
                    MemoryOverflow,
                    #[codec(index = 2)]
                    BackendError,
                    #[codec(index = 3)]
                    UserspacePanic,
                    #[codec(index = 4)]
                    UnreachableInstruction,
                    #[codec(index = 5)]
                    StackLimitExceeded,
                    #[codec(index = 255)]
                    Unsupported,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum SimpleUnavailableActorError {
                    #[codec(index = 0)]
                    ProgramExited,
                    #[codec(index = 1)]
                    InitializationFailure,
                    #[codec(index = 2)]
                    Uninitialized,
                    #[codec(index = 3)]
                    ProgramNotCreated,
                    #[codec(index = 4)]
                    ReinstrumentationFailure,
                    #[codec(index = 255)]
                    Unsupported,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum SuccessReplyReason {
                    #[codec(index = 0)]
                    Auto,
                    #[codec(index = 1)]
                    Manual,
                    #[codec(index = 255)]
                    Unsupported,
                }
            }
        }
        pub mod gprimitives {
            use super::runtime_types;
            #[derive(
                Copy, Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode,
            )]
            pub struct ActorId(pub [::core::primitive::u8; 32usize]);
            #[derive(
                Copy, Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode,
            )]
            pub struct CodeId(pub [::core::primitive::u8; 32usize]);
            #[derive(
                Copy, Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode,
            )]
            pub struct MessageId(pub [::core::primitive::u8; 32usize]);
            #[derive(
                Copy, Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode,
            )]
            pub struct ReservationId(pub [::core::primitive::u8; 32usize]);
        }
        pub mod numerated {
            use super::runtime_types;
            pub mod tree {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct IntervalsTree<_0> {
                    pub inner: ::subxt::ext::subxt_core::utils::KeyedVec<_0, _0>,
                }
            }
        }
        pub mod pallet_babe {
            use super::runtime_types;
            pub mod pallet {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "Contains a variant per dispatchable extrinsic that this pallet has."]
                pub enum Call {
                    #[codec(index = 0)]
                    #[doc = "Report authority equivocation/misbehavior. This method will verify"]
                    #[doc = "the equivocation proof and validate the given key ownership proof"]
                    #[doc = "against the extracted offender. If both are valid, the offence will"]
                    #[doc = "be reported."]
                    report_equivocation {
                        equivocation_proof: ::subxt::ext::subxt_core::alloc::boxed::Box<
                            runtime_types::sp_consensus_slots::EquivocationProof<
                                runtime_types::sp_runtime::generic::header::Header<
                                    ::core::primitive::u32,
                                >,
                                runtime_types::sp_consensus_babe::app::Public,
                            >,
                        >,
                        key_owner_proof: runtime_types::sp_session::MembershipProof,
                    },
                    #[codec(index = 1)]
                    #[doc = "Report authority equivocation/misbehavior. This method will verify"]
                    #[doc = "the equivocation proof and validate the given key ownership proof"]
                    #[doc = "against the extracted offender. If both are valid, the offence will"]
                    #[doc = "be reported."]
                    #[doc = "This extrinsic must be called unsigned and it is expected that only"]
                    #[doc = "block authors will call it (validated in `ValidateUnsigned`), as such"]
                    #[doc = "if the block author is defined it will be defined as the equivocation"]
                    #[doc = "reporter."]
                    report_equivocation_unsigned {
                        equivocation_proof: ::subxt::ext::subxt_core::alloc::boxed::Box<
                            runtime_types::sp_consensus_slots::EquivocationProof<
                                runtime_types::sp_runtime::generic::header::Header<
                                    ::core::primitive::u32,
                                >,
                                runtime_types::sp_consensus_babe::app::Public,
                            >,
                        >,
                        key_owner_proof: runtime_types::sp_session::MembershipProof,
                    },
                    #[codec(index = 2)]
                    #[doc = "Plan an epoch config change. The epoch config change is recorded and will be enacted on"]
                    #[doc = "the next call to `enact_epoch_change`. The config will be activated one epoch after."]
                    #[doc = "Multiple calls to this method will replace any existing planned config change that had"]
                    #[doc = "not been enacted yet."]
                    plan_config_change {
                        config: runtime_types::sp_consensus_babe::digests::NextConfigDescriptor,
                    },
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "The `Error` enum of this pallet."]
                pub enum Error {
                    #[codec(index = 0)]
                    #[doc = "An equivocation proof provided as part of an equivocation report is invalid."]
                    InvalidEquivocationProof,
                    #[codec(index = 1)]
                    #[doc = "A key ownership proof provided as part of an equivocation report is invalid."]
                    InvalidKeyOwnershipProof,
                    #[codec(index = 2)]
                    #[doc = "A given equivocation report is valid but already previously reported."]
                    DuplicateOffenceReport,
                    #[codec(index = 3)]
                    #[doc = "Submitted configuration is invalid."]
                    InvalidConfiguration,
                }
            }
        }
        pub mod pallet_bags_list {
            use super::runtime_types;
            pub mod list {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct Bag {
                    pub head: ::core::option::Option<::subxt::ext::subxt_core::utils::AccountId32>,
                    pub tail: ::core::option::Option<::subxt::ext::subxt_core::utils::AccountId32>,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum ListError {
                    #[codec(index = 0)]
                    Duplicate,
                    #[codec(index = 1)]
                    NotHeavier,
                    #[codec(index = 2)]
                    NotInSameBag,
                    #[codec(index = 3)]
                    NodeNotFound,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct Node {
                    pub id: ::subxt::ext::subxt_core::utils::AccountId32,
                    pub prev: ::core::option::Option<::subxt::ext::subxt_core::utils::AccountId32>,
                    pub next: ::core::option::Option<::subxt::ext::subxt_core::utils::AccountId32>,
                    pub bag_upper: ::core::primitive::u64,
                    pub score: ::core::primitive::u64,
                }
            }
            pub mod pallet {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "Contains a variant per dispatchable extrinsic that this pallet has."]
                pub enum Call {
                    #[codec(index = 0)]
                    #[doc = "Declare that some `dislocated` account has, through rewards or penalties, sufficiently"]
                    #[doc = "changed its score that it should properly fall into a different bag than its current"]
                    #[doc = "one."]
                    #[doc = ""]
                    #[doc = "Anyone can call this function about any potentially dislocated account."]
                    #[doc = ""]
                    #[doc = "Will always update the stored score of `dislocated` to the correct score, based on"]
                    #[doc = "`ScoreProvider`."]
                    #[doc = ""]
                    #[doc = "If `dislocated` does not exists, it returns an error."]
                    rebag {
                        dislocated: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                    },
                    #[codec(index = 1)]
                    #[doc = "Move the caller's Id directly in front of `lighter`."]
                    #[doc = ""]
                    #[doc = "The dispatch origin for this call must be _Signed_ and can only be called by the Id of"]
                    #[doc = "the account going in front of `lighter`. Fee is payed by the origin under all"]
                    #[doc = "circumstances."]
                    #[doc = ""]
                    #[doc = "Only works if:"]
                    #[doc = ""]
                    #[doc = "- both nodes are within the same bag,"]
                    #[doc = "- and `origin` has a greater `Score` than `lighter`."]
                    put_in_front_of {
                        lighter: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                    },
                    #[codec(index = 2)]
                    #[doc = "Same as [`Pallet::put_in_front_of`], but it can be called by anyone."]
                    #[doc = ""]
                    #[doc = "Fee is paid by the origin under all circumstances."]
                    put_in_front_of_other {
                        heavier: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                        lighter: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                    },
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "The `Error` enum of this pallet."]
                pub enum Error {
                    #[codec(index = 0)]
                    #[doc = "A error in the list interface implementation."]
                    List(runtime_types::pallet_bags_list::list::ListError),
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "The `Event` enum of this pallet"]
                pub enum Event {
                    #[codec(index = 0)]
                    #[doc = "Moved an account from one bag to another."]
                    Rebagged {
                        who: ::subxt::ext::subxt_core::utils::AccountId32,
                        from: ::core::primitive::u64,
                        to: ::core::primitive::u64,
                    },
                    #[codec(index = 1)]
                    #[doc = "Updated the score of some account to the given amount."]
                    ScoreUpdated {
                        who: ::subxt::ext::subxt_core::utils::AccountId32,
                        new_score: ::core::primitive::u64,
                    },
                }
            }
        }
        pub mod pallet_balances {
            use super::runtime_types;
            pub mod pallet {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "Contains a variant per dispatchable extrinsic that this pallet has."]
                pub enum Call {
                    #[codec(index = 0)]
                    #[doc = "Transfer some liquid free balance to another account."]
                    #[doc = ""]
                    #[doc = "`transfer_allow_death` will set the `FreeBalance` of the sender and receiver."]
                    #[doc = "If the sender's account is below the existential deposit as a result"]
                    #[doc = "of the transfer, the account will be reaped."]
                    #[doc = ""]
                    #[doc = "The dispatch origin for this call must be `Signed` by the transactor."]
                    transfer_allow_death {
                        dest: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                        #[codec(compact)]
                        value: ::core::primitive::u128,
                    },
                    #[codec(index = 2)]
                    #[doc = "Exactly as `transfer_allow_death`, except the origin must be root and the source account"]
                    #[doc = "may be specified."]
                    force_transfer {
                        source: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                        dest: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                        #[codec(compact)]
                        value: ::core::primitive::u128,
                    },
                    #[codec(index = 3)]
                    #[doc = "Same as the [`transfer_allow_death`] call, but with a check that the transfer will not"]
                    #[doc = "kill the origin account."]
                    #[doc = ""]
                    #[doc = "99% of the time you want [`transfer_allow_death`] instead."]
                    #[doc = ""]
                    #[doc = "[`transfer_allow_death`]: struct.Pallet.html#method.transfer"]
                    transfer_keep_alive {
                        dest: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                        #[codec(compact)]
                        value: ::core::primitive::u128,
                    },
                    #[codec(index = 4)]
                    #[doc = "Transfer the entire transferable balance from the caller account."]
                    #[doc = ""]
                    #[doc = "NOTE: This function only attempts to transfer _transferable_ balances. This means that"]
                    #[doc = "any locked, reserved, or existential deposits (when `keep_alive` is `true`), will not be"]
                    #[doc = "transferred by this function. To ensure that this function results in a killed account,"]
                    #[doc = "you might need to prepare the account by removing any reference counters, storage"]
                    #[doc = "deposits, etc..."]
                    #[doc = ""]
                    #[doc = "The dispatch origin of this call must be Signed."]
                    #[doc = ""]
                    #[doc = "- `dest`: The recipient of the transfer."]
                    #[doc = "- `keep_alive`: A boolean to determine if the `transfer_all` operation should send all"]
                    #[doc = "  of the funds the account has, causing the sender account to be killed (false), or"]
                    #[doc = "  transfer everything except at least the existential deposit, which will guarantee to"]
                    #[doc = "  keep the sender account alive (true)."]
                    transfer_all {
                        dest: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                        keep_alive: ::core::primitive::bool,
                    },
                    #[codec(index = 5)]
                    #[doc = "Unreserve some balance from a user by force."]
                    #[doc = ""]
                    #[doc = "Can only be called by ROOT."]
                    force_unreserve {
                        who: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                        amount: ::core::primitive::u128,
                    },
                    #[codec(index = 6)]
                    #[doc = "Upgrade a specified account."]
                    #[doc = ""]
                    #[doc = "- `origin`: Must be `Signed`."]
                    #[doc = "- `who`: The account to be upgraded."]
                    #[doc = ""]
                    #[doc = "This will waive the transaction fee if at least all but 10% of the accounts needed to"]
                    #[doc = "be upgraded. (We let some not have to be upgraded just in order to allow for the"]
                    #[doc = "possibility of churn)."]
                    upgrade_accounts {
                        who: ::subxt::ext::subxt_core::alloc::vec::Vec<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                        >,
                    },
                    #[codec(index = 8)]
                    #[doc = "Set the regular balance of a given account."]
                    #[doc = ""]
                    #[doc = "The dispatch origin for this call is `root`."]
                    force_set_balance {
                        who: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                        #[codec(compact)]
                        new_free: ::core::primitive::u128,
                    },
                    #[codec(index = 9)]
                    #[doc = "Adjust the total issuance in a saturating way."]
                    #[doc = ""]
                    #[doc = "Can only be called by root and always needs a positive `delta`."]
                    #[doc = ""]
                    #[doc = "# Example"]
                    force_adjust_total_issuance {
                        direction: runtime_types::pallet_balances::types::AdjustmentDirection,
                        #[codec(compact)]
                        delta: ::core::primitive::u128,
                    },
                    #[codec(index = 10)]
                    #[doc = "Burn the specified liquid free balance from the origin account."]
                    #[doc = ""]
                    #[doc = "If the origin's account ends up below the existential deposit as a result"]
                    #[doc = "of the burn and `keep_alive` is false, the account will be reaped."]
                    #[doc = ""]
                    #[doc = "Unlike sending funds to a _burn_ address, which merely makes the funds inaccessible,"]
                    #[doc = "this `burn` operation will reduce total issuance by the amount _burned_."]
                    burn {
                        #[codec(compact)]
                        value: ::core::primitive::u128,
                        keep_alive: ::core::primitive::bool,
                    },
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "The `Error` enum of this pallet."]
                pub enum Error {
                    #[codec(index = 0)]
                    #[doc = "Vesting balance too high to send value."]
                    VestingBalance,
                    #[codec(index = 1)]
                    #[doc = "Account liquidity restrictions prevent withdrawal."]
                    LiquidityRestrictions,
                    #[codec(index = 2)]
                    #[doc = "Balance too low to send value."]
                    InsufficientBalance,
                    #[codec(index = 3)]
                    #[doc = "Value too low to create account due to existential deposit."]
                    ExistentialDeposit,
                    #[codec(index = 4)]
                    #[doc = "Transfer/payment would kill account."]
                    Expendability,
                    #[codec(index = 5)]
                    #[doc = "A vesting schedule already exists for this account."]
                    ExistingVestingSchedule,
                    #[codec(index = 6)]
                    #[doc = "Beneficiary account must pre-exist."]
                    DeadAccount,
                    #[codec(index = 7)]
                    #[doc = "Number of named reserves exceed `MaxReserves`."]
                    TooManyReserves,
                    #[codec(index = 8)]
                    #[doc = "Number of holds exceed `VariantCountOf<T::RuntimeHoldReason>`."]
                    TooManyHolds,
                    #[codec(index = 9)]
                    #[doc = "Number of freezes exceed `MaxFreezes`."]
                    TooManyFreezes,
                    #[codec(index = 10)]
                    #[doc = "The issuance cannot be modified since it is already deactivated."]
                    IssuanceDeactivated,
                    #[codec(index = 11)]
                    #[doc = "The delta cannot be zero."]
                    DeltaZero,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "The `Event` enum of this pallet"]
                pub enum Event {
                    #[codec(index = 0)]
                    #[doc = "An account was created with some free balance."]
                    Endowed {
                        account: ::subxt::ext::subxt_core::utils::AccountId32,
                        free_balance: ::core::primitive::u128,
                    },
                    #[codec(index = 1)]
                    #[doc = "An account was removed whose balance was non-zero but below ExistentialDeposit,"]
                    #[doc = "resulting in an outright loss."]
                    DustLost {
                        account: ::subxt::ext::subxt_core::utils::AccountId32,
                        amount: ::core::primitive::u128,
                    },
                    #[codec(index = 2)]
                    #[doc = "Transfer succeeded."]
                    Transfer {
                        from: ::subxt::ext::subxt_core::utils::AccountId32,
                        to: ::subxt::ext::subxt_core::utils::AccountId32,
                        amount: ::core::primitive::u128,
                    },
                    #[codec(index = 3)]
                    #[doc = "A balance was set by root."]
                    BalanceSet {
                        who: ::subxt::ext::subxt_core::utils::AccountId32,
                        free: ::core::primitive::u128,
                    },
                    #[codec(index = 4)]
                    #[doc = "Some balance was reserved (moved from free to reserved)."]
                    Reserved {
                        who: ::subxt::ext::subxt_core::utils::AccountId32,
                        amount: ::core::primitive::u128,
                    },
                    #[codec(index = 5)]
                    #[doc = "Some balance was unreserved (moved from reserved to free)."]
                    Unreserved {
                        who: ::subxt::ext::subxt_core::utils::AccountId32,
                        amount: ::core::primitive::u128,
                    },
                    #[codec(index = 6)]
                    #[doc = "Some balance was moved from the reserve of the first account to the second account."]
                    #[doc = "Final argument indicates the destination balance type."]
                    ReserveRepatriated {
                        from: ::subxt::ext::subxt_core::utils::AccountId32,
                        to: ::subxt::ext::subxt_core::utils::AccountId32,
                        amount: ::core::primitive::u128,
                        destination_status:
                            runtime_types::frame_support::traits::tokens::misc::BalanceStatus,
                    },
                    #[codec(index = 7)]
                    #[doc = "Some amount was deposited (e.g. for transaction fees)."]
                    Deposit {
                        who: ::subxt::ext::subxt_core::utils::AccountId32,
                        amount: ::core::primitive::u128,
                    },
                    #[codec(index = 8)]
                    #[doc = "Some amount was withdrawn from the account (e.g. for transaction fees)."]
                    Withdraw {
                        who: ::subxt::ext::subxt_core::utils::AccountId32,
                        amount: ::core::primitive::u128,
                    },
                    #[codec(index = 9)]
                    #[doc = "Some amount was removed from the account (e.g. for misbehavior)."]
                    Slashed {
                        who: ::subxt::ext::subxt_core::utils::AccountId32,
                        amount: ::core::primitive::u128,
                    },
                    #[codec(index = 10)]
                    #[doc = "Some amount was minted into an account."]
                    Minted {
                        who: ::subxt::ext::subxt_core::utils::AccountId32,
                        amount: ::core::primitive::u128,
                    },
                    #[codec(index = 11)]
                    #[doc = "Some amount was burned from an account."]
                    Burned {
                        who: ::subxt::ext::subxt_core::utils::AccountId32,
                        amount: ::core::primitive::u128,
                    },
                    #[codec(index = 12)]
                    #[doc = "Some amount was suspended from an account (it can be restored later)."]
                    Suspended {
                        who: ::subxt::ext::subxt_core::utils::AccountId32,
                        amount: ::core::primitive::u128,
                    },
                    #[codec(index = 13)]
                    #[doc = "Some amount was restored into an account."]
                    Restored {
                        who: ::subxt::ext::subxt_core::utils::AccountId32,
                        amount: ::core::primitive::u128,
                    },
                    #[codec(index = 14)]
                    #[doc = "An account was upgraded."]
                    Upgraded {
                        who: ::subxt::ext::subxt_core::utils::AccountId32,
                    },
                    #[codec(index = 15)]
                    #[doc = "Total issuance was increased by `amount`, creating a credit to be balanced."]
                    Issued { amount: ::core::primitive::u128 },
                    #[codec(index = 16)]
                    #[doc = "Total issuance was decreased by `amount`, creating a debt to be balanced."]
                    Rescinded { amount: ::core::primitive::u128 },
                    #[codec(index = 17)]
                    #[doc = "Some balance was locked."]
                    Locked {
                        who: ::subxt::ext::subxt_core::utils::AccountId32,
                        amount: ::core::primitive::u128,
                    },
                    #[codec(index = 18)]
                    #[doc = "Some balance was unlocked."]
                    Unlocked {
                        who: ::subxt::ext::subxt_core::utils::AccountId32,
                        amount: ::core::primitive::u128,
                    },
                    #[codec(index = 19)]
                    #[doc = "Some balance was frozen."]
                    Frozen {
                        who: ::subxt::ext::subxt_core::utils::AccountId32,
                        amount: ::core::primitive::u128,
                    },
                    #[codec(index = 20)]
                    #[doc = "Some balance was thawed."]
                    Thawed {
                        who: ::subxt::ext::subxt_core::utils::AccountId32,
                        amount: ::core::primitive::u128,
                    },
                    #[codec(index = 21)]
                    #[doc = "The `TotalIssuance` was forcefully changed."]
                    TotalIssuanceForced {
                        old: ::core::primitive::u128,
                        new: ::core::primitive::u128,
                    },
                }
            }
            pub mod types {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct AccountData<_0> {
                    pub free: _0,
                    pub reserved: _0,
                    pub frozen: _0,
                    pub flags: runtime_types::pallet_balances::types::ExtraFlags,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum AdjustmentDirection {
                    #[codec(index = 0)]
                    Increase,
                    #[codec(index = 1)]
                    Decrease,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct BalanceLock<_0> {
                    pub id: [::core::primitive::u8; 8usize],
                    pub amount: _0,
                    pub reasons: runtime_types::pallet_balances::types::Reasons,
                }
                #[derive(
                    ::subxt::ext ::subxt_core::ext::codec::CompactAs,
                    Debug,
                    crate::gp::Decode,
                    crate::gp::DecodeAsType,
                    crate::gp::Encode,
                )]
                pub struct ExtraFlags(pub ::core::primitive::u128);
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum Reasons {
                    #[codec(index = 0)]
                    Fee,
                    #[codec(index = 1)]
                    Misc,
                    #[codec(index = 2)]
                    All,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct ReserveData<_0, _1> {
                    pub id: _0,
                    pub amount: _1,
                }
            }
        }
        pub mod pallet_bounties {
            use super::runtime_types;
            pub mod pallet {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "Contains a variant per dispatchable extrinsic that this pallet has."]
                pub enum Call {
                    #[codec(index = 0)]
                    #[doc = "Propose a new bounty."]
                    #[doc = ""]
                    #[doc = "The dispatch origin for this call must be _Signed_."]
                    #[doc = ""]
                    #[doc = "Payment: `TipReportDepositBase` will be reserved from the origin account, as well as"]
                    #[doc = "`DataDepositPerByte` for each byte in `reason`. It will be unreserved upon approval,"]
                    #[doc = "or slashed when rejected."]
                    #[doc = ""]
                    #[doc = "- `curator`: The curator account whom will manage this bounty."]
                    #[doc = "- `fee`: The curator fee."]
                    #[doc = "- `value`: The total payment amount of this bounty, curator fee included."]
                    #[doc = "- `description`: The description of this bounty."]
                    propose_bounty {
                        #[codec(compact)]
                        value: ::core::primitive::u128,
                        description:
                            ::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
                    },
                    #[codec(index = 1)]
                    #[doc = "Approve a bounty proposal. At a later time, the bounty will be funded and become active"]
                    #[doc = "and the original deposit will be returned."]
                    #[doc = ""]
                    #[doc = "May only be called from `T::SpendOrigin`."]
                    #[doc = ""]
                    #[doc = "## Complexity"]
                    #[doc = "- O(1)."]
                    approve_bounty {
                        #[codec(compact)]
                        bounty_id: ::core::primitive::u32,
                    },
                    #[codec(index = 2)]
                    #[doc = "Propose a curator to a funded bounty."]
                    #[doc = ""]
                    #[doc = "May only be called from `T::SpendOrigin`."]
                    #[doc = ""]
                    #[doc = "## Complexity"]
                    #[doc = "- O(1)."]
                    propose_curator {
                        #[codec(compact)]
                        bounty_id: ::core::primitive::u32,
                        curator: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                        #[codec(compact)]
                        fee: ::core::primitive::u128,
                    },
                    #[codec(index = 3)]
                    #[doc = "Unassign curator from a bounty."]
                    #[doc = ""]
                    #[doc = "This function can only be called by the `RejectOrigin` a signed origin."]
                    #[doc = ""]
                    #[doc = "If this function is called by the `RejectOrigin`, we assume that the curator is"]
                    #[doc = "malicious or inactive. As a result, we will slash the curator when possible."]
                    #[doc = ""]
                    #[doc = "If the origin is the curator, we take this as a sign they are unable to do their job and"]
                    #[doc = "they willingly give up. We could slash them, but for now we allow them to recover their"]
                    #[doc = "deposit and exit without issue. (We may want to change this if it is abused.)"]
                    #[doc = ""]
                    #[doc = "Finally, the origin can be anyone if and only if the curator is \"inactive\". This allows"]
                    #[doc = "anyone in the community to call out that a curator is not doing their due diligence, and"]
                    #[doc = "we should pick a new curator. In this case the curator should also be slashed."]
                    #[doc = ""]
                    #[doc = "## Complexity"]
                    #[doc = "- O(1)."]
                    unassign_curator {
                        #[codec(compact)]
                        bounty_id: ::core::primitive::u32,
                    },
                    #[codec(index = 4)]
                    #[doc = "Accept the curator role for a bounty."]
                    #[doc = "A deposit will be reserved from curator and refund upon successful payout."]
                    #[doc = ""]
                    #[doc = "May only be called from the curator."]
                    #[doc = ""]
                    #[doc = "## Complexity"]
                    #[doc = "- O(1)."]
                    accept_curator {
                        #[codec(compact)]
                        bounty_id: ::core::primitive::u32,
                    },
                    #[codec(index = 5)]
                    #[doc = "Award bounty to a beneficiary account. The beneficiary will be able to claim the funds"]
                    #[doc = "after a delay."]
                    #[doc = ""]
                    #[doc = "The dispatch origin for this call must be the curator of this bounty."]
                    #[doc = ""]
                    #[doc = "- `bounty_id`: Bounty ID to award."]
                    #[doc = "- `beneficiary`: The beneficiary account whom will receive the payout."]
                    #[doc = ""]
                    #[doc = "## Complexity"]
                    #[doc = "- O(1)."]
                    award_bounty {
                        #[codec(compact)]
                        bounty_id: ::core::primitive::u32,
                        beneficiary: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                    },
                    #[codec(index = 6)]
                    #[doc = "Claim the payout from an awarded bounty after payout delay."]
                    #[doc = ""]
                    #[doc = "The dispatch origin for this call must be the beneficiary of this bounty."]
                    #[doc = ""]
                    #[doc = "- `bounty_id`: Bounty ID to claim."]
                    #[doc = ""]
                    #[doc = "## Complexity"]
                    #[doc = "- O(1)."]
                    claim_bounty {
                        #[codec(compact)]
                        bounty_id: ::core::primitive::u32,
                    },
                    #[codec(index = 7)]
                    #[doc = "Cancel a proposed or active bounty. All the funds will be sent to treasury and"]
                    #[doc = "the curator deposit will be unreserved if possible."]
                    #[doc = ""]
                    #[doc = "Only `T::RejectOrigin` is able to cancel a bounty."]
                    #[doc = ""]
                    #[doc = "- `bounty_id`: Bounty ID to cancel."]
                    #[doc = ""]
                    #[doc = "## Complexity"]
                    #[doc = "- O(1)."]
                    close_bounty {
                        #[codec(compact)]
                        bounty_id: ::core::primitive::u32,
                    },
                    #[codec(index = 8)]
                    #[doc = "Extend the expiry time of an active bounty."]
                    #[doc = ""]
                    #[doc = "The dispatch origin for this call must be the curator of this bounty."]
                    #[doc = ""]
                    #[doc = "- `bounty_id`: Bounty ID to extend."]
                    #[doc = "- `remark`: additional information."]
                    #[doc = ""]
                    #[doc = "## Complexity"]
                    #[doc = "- O(1)."]
                    extend_bounty_expiry {
                        #[codec(compact)]
                        bounty_id: ::core::primitive::u32,
                        remark: ::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
                    },
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "The `Error` enum of this pallet."]
                pub enum Error {
                    #[codec(index = 0)]
                    #[doc = "Proposer's balance is too low."]
                    InsufficientProposersBalance,
                    #[codec(index = 1)]
                    #[doc = "No proposal or bounty at that index."]
                    InvalidIndex,
                    #[codec(index = 2)]
                    #[doc = "The reason given is just too big."]
                    ReasonTooBig,
                    #[codec(index = 3)]
                    #[doc = "The bounty status is unexpected."]
                    UnexpectedStatus,
                    #[codec(index = 4)]
                    #[doc = "Require bounty curator."]
                    RequireCurator,
                    #[codec(index = 5)]
                    #[doc = "Invalid bounty value."]
                    InvalidValue,
                    #[codec(index = 6)]
                    #[doc = "Invalid bounty fee."]
                    InvalidFee,
                    #[codec(index = 7)]
                    #[doc = "A bounty payout is pending."]
                    #[doc = "To cancel the bounty, you must unassign and slash the curator."]
                    PendingPayout,
                    #[codec(index = 8)]
                    #[doc = "The bounties cannot be claimed/closed because it's still in the countdown period."]
                    Premature,
                    #[codec(index = 9)]
                    #[doc = "The bounty cannot be closed because it has active child bounties."]
                    HasActiveChildBounty,
                    #[codec(index = 10)]
                    #[doc = "Too many approvals are already queued."]
                    TooManyQueued,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "The `Event` enum of this pallet"]
                pub enum Event {
                    #[codec(index = 0)]
                    #[doc = "New bounty proposal."]
                    BountyProposed { index: ::core::primitive::u32 },
                    #[codec(index = 1)]
                    #[doc = "A bounty proposal was rejected; funds were slashed."]
                    BountyRejected {
                        index: ::core::primitive::u32,
                        bond: ::core::primitive::u128,
                    },
                    #[codec(index = 2)]
                    #[doc = "A bounty proposal is funded and became active."]
                    BountyBecameActive { index: ::core::primitive::u32 },
                    #[codec(index = 3)]
                    #[doc = "A bounty is awarded to a beneficiary."]
                    BountyAwarded {
                        index: ::core::primitive::u32,
                        beneficiary: ::subxt::ext::subxt_core::utils::AccountId32,
                    },
                    #[codec(index = 4)]
                    #[doc = "A bounty is claimed by beneficiary."]
                    BountyClaimed {
                        index: ::core::primitive::u32,
                        payout: ::core::primitive::u128,
                        beneficiary: ::subxt::ext::subxt_core::utils::AccountId32,
                    },
                    #[codec(index = 5)]
                    #[doc = "A bounty is cancelled."]
                    BountyCanceled { index: ::core::primitive::u32 },
                    #[codec(index = 6)]
                    #[doc = "A bounty expiry is extended."]
                    BountyExtended { index: ::core::primitive::u32 },
                    #[codec(index = 7)]
                    #[doc = "A bounty is approved."]
                    BountyApproved { index: ::core::primitive::u32 },
                    #[codec(index = 8)]
                    #[doc = "A bounty curator is proposed."]
                    CuratorProposed {
                        bounty_id: ::core::primitive::u32,
                        curator: ::subxt::ext::subxt_core::utils::AccountId32,
                    },
                    #[codec(index = 9)]
                    #[doc = "A bounty curator is unassigned."]
                    CuratorUnassigned { bounty_id: ::core::primitive::u32 },
                    #[codec(index = 10)]
                    #[doc = "A bounty curator is accepted."]
                    CuratorAccepted {
                        bounty_id: ::core::primitive::u32,
                        curator: ::subxt::ext::subxt_core::utils::AccountId32,
                    },
                }
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub struct Bounty<_0, _1, _2> {
                pub proposer: _0,
                pub value: _1,
                pub fee: _1,
                pub curator_deposit: _1,
                pub bond: _1,
                pub status: runtime_types::pallet_bounties::BountyStatus<_0, _2>,
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub enum BountyStatus<_0, _1> {
                #[codec(index = 0)]
                Proposed,
                #[codec(index = 1)]
                Approved,
                #[codec(index = 2)]
                Funded,
                #[codec(index = 3)]
                CuratorProposed { curator: _0 },
                #[codec(index = 4)]
                Active { curator: _0, update_due: _1 },
                #[codec(index = 5)]
                PendingPayout {
                    curator: _0,
                    beneficiary: _0,
                    unlock_at: _1,
                },
            }
        }
        pub mod pallet_child_bounties {
            use super::runtime_types;
            pub mod pallet {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "Contains a variant per dispatchable extrinsic that this pallet has."]
                pub enum Call {
                    #[codec(index = 0)]
                    #[doc = "Add a new child-bounty."]
                    #[doc = ""]
                    #[doc = "The dispatch origin for this call must be the curator of parent"]
                    #[doc = "bounty and the parent bounty must be in \"active\" state."]
                    #[doc = ""]
                    #[doc = "Child-bounty gets added successfully & fund gets transferred from"]
                    #[doc = "parent bounty to child-bounty account, if parent bounty has enough"]
                    #[doc = "funds, else the call fails."]
                    #[doc = ""]
                    #[doc = "Upper bound to maximum number of active  child bounties that can be"]
                    #[doc = "added are managed via runtime trait config"]
                    #[doc = "[`Config::MaxActiveChildBountyCount`]."]
                    #[doc = ""]
                    #[doc = "If the call is success, the status of child-bounty is updated to"]
                    #[doc = "\"Added\"."]
                    #[doc = ""]
                    #[doc = "- `parent_bounty_id`: Index of parent bounty for which child-bounty is being added."]
                    #[doc = "- `value`: Value for executing the proposal."]
                    #[doc = "- `description`: Text description for the child-bounty."]
                    add_child_bounty {
                        #[codec(compact)]
                        parent_bounty_id: ::core::primitive::u32,
                        #[codec(compact)]
                        value: ::core::primitive::u128,
                        description:
                            ::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
                    },
                    #[codec(index = 1)]
                    #[doc = "Propose curator for funded child-bounty."]
                    #[doc = ""]
                    #[doc = "The dispatch origin for this call must be curator of parent bounty."]
                    #[doc = ""]
                    #[doc = "Parent bounty must be in active state, for this child-bounty call to"]
                    #[doc = "work."]
                    #[doc = ""]
                    #[doc = "Child-bounty must be in \"Added\" state, for processing the call. And"]
                    #[doc = "state of child-bounty is moved to \"CuratorProposed\" on successful"]
                    #[doc = "call completion."]
                    #[doc = ""]
                    #[doc = "- `parent_bounty_id`: Index of parent bounty."]
                    #[doc = "- `child_bounty_id`: Index of child bounty."]
                    #[doc = "- `curator`: Address of child-bounty curator."]
                    #[doc = "- `fee`: payment fee to child-bounty curator for execution."]
                    propose_curator {
                        #[codec(compact)]
                        parent_bounty_id: ::core::primitive::u32,
                        #[codec(compact)]
                        child_bounty_id: ::core::primitive::u32,
                        curator: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                        #[codec(compact)]
                        fee: ::core::primitive::u128,
                    },
                    #[codec(index = 2)]
                    #[doc = "Accept the curator role for the child-bounty."]
                    #[doc = ""]
                    #[doc = "The dispatch origin for this call must be the curator of this"]
                    #[doc = "child-bounty."]
                    #[doc = ""]
                    #[doc = "A deposit will be reserved from the curator and refund upon"]
                    #[doc = "successful payout or cancellation."]
                    #[doc = ""]
                    #[doc = "Fee for curator is deducted from curator fee of parent bounty."]
                    #[doc = ""]
                    #[doc = "Parent bounty must be in active state, for this child-bounty call to"]
                    #[doc = "work."]
                    #[doc = ""]
                    #[doc = "Child-bounty must be in \"CuratorProposed\" state, for processing the"]
                    #[doc = "call. And state of child-bounty is moved to \"Active\" on successful"]
                    #[doc = "call completion."]
                    #[doc = ""]
                    #[doc = "- `parent_bounty_id`: Index of parent bounty."]
                    #[doc = "- `child_bounty_id`: Index of child bounty."]
                    accept_curator {
                        #[codec(compact)]
                        parent_bounty_id: ::core::primitive::u32,
                        #[codec(compact)]
                        child_bounty_id: ::core::primitive::u32,
                    },
                    #[codec(index = 3)]
                    #[doc = "Unassign curator from a child-bounty."]
                    #[doc = ""]
                    #[doc = "The dispatch origin for this call can be either `RejectOrigin`, or"]
                    #[doc = "the curator of the parent bounty, or any signed origin."]
                    #[doc = ""]
                    #[doc = "For the origin other than T::RejectOrigin and the child-bounty"]
                    #[doc = "curator, parent bounty must be in active state, for this call to"]
                    #[doc = "work. We allow child-bounty curator and T::RejectOrigin to execute"]
                    #[doc = "this call irrespective of the parent bounty state."]
                    #[doc = ""]
                    #[doc = "If this function is called by the `RejectOrigin` or the"]
                    #[doc = "parent bounty curator, we assume that the child-bounty curator is"]
                    #[doc = "malicious or inactive. As a result, child-bounty curator deposit is"]
                    #[doc = "slashed."]
                    #[doc = ""]
                    #[doc = "If the origin is the child-bounty curator, we take this as a sign"]
                    #[doc = "that they are unable to do their job, and are willingly giving up."]
                    #[doc = "We could slash the deposit, but for now we allow them to unreserve"]
                    #[doc = "their deposit and exit without issue. (We may want to change this if"]
                    #[doc = "it is abused.)"]
                    #[doc = ""]
                    #[doc = "Finally, the origin can be anyone iff the child-bounty curator is"]
                    #[doc = "\"inactive\". Expiry update due of parent bounty is used to estimate"]
                    #[doc = "inactive state of child-bounty curator."]
                    #[doc = ""]
                    #[doc = "This allows anyone in the community to call out that a child-bounty"]
                    #[doc = "curator is not doing their due diligence, and we should pick a new"]
                    #[doc = "one. In this case the child-bounty curator deposit is slashed."]
                    #[doc = ""]
                    #[doc = "State of child-bounty is moved to Added state on successful call"]
                    #[doc = "completion."]
                    #[doc = ""]
                    #[doc = "- `parent_bounty_id`: Index of parent bounty."]
                    #[doc = "- `child_bounty_id`: Index of child bounty."]
                    unassign_curator {
                        #[codec(compact)]
                        parent_bounty_id: ::core::primitive::u32,
                        #[codec(compact)]
                        child_bounty_id: ::core::primitive::u32,
                    },
                    #[codec(index = 4)]
                    #[doc = "Award child-bounty to a beneficiary."]
                    #[doc = ""]
                    #[doc = "The beneficiary will be able to claim the funds after a delay."]
                    #[doc = ""]
                    #[doc = "The dispatch origin for this call must be the parent curator or"]
                    #[doc = "curator of this child-bounty."]
                    #[doc = ""]
                    #[doc = "Parent bounty must be in active state, for this child-bounty call to"]
                    #[doc = "work."]
                    #[doc = ""]
                    #[doc = "Child-bounty must be in active state, for processing the call. And"]
                    #[doc = "state of child-bounty is moved to \"PendingPayout\" on successful call"]
                    #[doc = "completion."]
                    #[doc = ""]
                    #[doc = "- `parent_bounty_id`: Index of parent bounty."]
                    #[doc = "- `child_bounty_id`: Index of child bounty."]
                    #[doc = "- `beneficiary`: Beneficiary account."]
                    award_child_bounty {
                        #[codec(compact)]
                        parent_bounty_id: ::core::primitive::u32,
                        #[codec(compact)]
                        child_bounty_id: ::core::primitive::u32,
                        beneficiary: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                    },
                    #[codec(index = 5)]
                    #[doc = "Claim the payout from an awarded child-bounty after payout delay."]
                    #[doc = ""]
                    #[doc = "The dispatch origin for this call may be any signed origin."]
                    #[doc = ""]
                    #[doc = "Call works independent of parent bounty state, No need for parent"]
                    #[doc = "bounty to be in active state."]
                    #[doc = ""]
                    #[doc = "The Beneficiary is paid out with agreed bounty value. Curator fee is"]
                    #[doc = "paid & curator deposit is unreserved."]
                    #[doc = ""]
                    #[doc = "Child-bounty must be in \"PendingPayout\" state, for processing the"]
                    #[doc = "call. And instance of child-bounty is removed from the state on"]
                    #[doc = "successful call completion."]
                    #[doc = ""]
                    #[doc = "- `parent_bounty_id`: Index of parent bounty."]
                    #[doc = "- `child_bounty_id`: Index of child bounty."]
                    claim_child_bounty {
                        #[codec(compact)]
                        parent_bounty_id: ::core::primitive::u32,
                        #[codec(compact)]
                        child_bounty_id: ::core::primitive::u32,
                    },
                    #[codec(index = 6)]
                    #[doc = "Cancel a proposed or active child-bounty. Child-bounty account funds"]
                    #[doc = "are transferred to parent bounty account. The child-bounty curator"]
                    #[doc = "deposit may be unreserved if possible."]
                    #[doc = ""]
                    #[doc = "The dispatch origin for this call must be either parent curator or"]
                    #[doc = "`T::RejectOrigin`."]
                    #[doc = ""]
                    #[doc = "If the state of child-bounty is `Active`, curator deposit is"]
                    #[doc = "unreserved."]
                    #[doc = ""]
                    #[doc = "If the state of child-bounty is `PendingPayout`, call fails &"]
                    #[doc = "returns `PendingPayout` error."]
                    #[doc = ""]
                    #[doc = "For the origin other than T::RejectOrigin, parent bounty must be in"]
                    #[doc = "active state, for this child-bounty call to work. For origin"]
                    #[doc = "T::RejectOrigin execution is forced."]
                    #[doc = ""]
                    #[doc = "Instance of child-bounty is removed from the state on successful"]
                    #[doc = "call completion."]
                    #[doc = ""]
                    #[doc = "- `parent_bounty_id`: Index of parent bounty."]
                    #[doc = "- `child_bounty_id`: Index of child bounty."]
                    close_child_bounty {
                        #[codec(compact)]
                        parent_bounty_id: ::core::primitive::u32,
                        #[codec(compact)]
                        child_bounty_id: ::core::primitive::u32,
                    },
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "The `Error` enum of this pallet."]
                pub enum Error {
                    #[codec(index = 0)]
                    #[doc = "The parent bounty is not in active state."]
                    ParentBountyNotActive,
                    #[codec(index = 1)]
                    #[doc = "The bounty balance is not enough to add new child-bounty."]
                    InsufficientBountyBalance,
                    #[codec(index = 2)]
                    #[doc = "Number of child bounties exceeds limit `MaxActiveChildBountyCount`."]
                    TooManyChildBounties,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "The `Event` enum of this pallet"]
                pub enum Event {
                    #[codec(index = 0)]
                    #[doc = "A child-bounty is added."]
                    Added {
                        index: ::core::primitive::u32,
                        child_index: ::core::primitive::u32,
                    },
                    #[codec(index = 1)]
                    #[doc = "A child-bounty is awarded to a beneficiary."]
                    Awarded {
                        index: ::core::primitive::u32,
                        child_index: ::core::primitive::u32,
                        beneficiary: ::subxt::ext::subxt_core::utils::AccountId32,
                    },
                    #[codec(index = 2)]
                    #[doc = "A child-bounty is claimed by beneficiary."]
                    Claimed {
                        index: ::core::primitive::u32,
                        child_index: ::core::primitive::u32,
                        payout: ::core::primitive::u128,
                        beneficiary: ::subxt::ext::subxt_core::utils::AccountId32,
                    },
                    #[codec(index = 3)]
                    #[doc = "A child-bounty is cancelled."]
                    Canceled {
                        index: ::core::primitive::u32,
                        child_index: ::core::primitive::u32,
                    },
                }
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub struct ChildBounty<_0, _1, _2> {
                pub parent_bounty: ::core::primitive::u32,
                pub value: _1,
                pub fee: _1,
                pub curator_deposit: _1,
                pub status: runtime_types::pallet_child_bounties::ChildBountyStatus<_0, _2>,
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub enum ChildBountyStatus<_0, _1> {
                #[codec(index = 0)]
                Added,
                #[codec(index = 1)]
                CuratorProposed { curator: _0 },
                #[codec(index = 2)]
                Active { curator: _0 },
                #[codec(index = 3)]
                PendingPayout {
                    curator: _0,
                    beneficiary: _0,
                    unlock_at: _1,
                },
            }
        }
        pub mod pallet_conviction_voting {
            use super::runtime_types;
            pub mod conviction {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum Conviction {
                    #[codec(index = 0)]
                    None,
                    #[codec(index = 1)]
                    Locked1x,
                    #[codec(index = 2)]
                    Locked2x,
                    #[codec(index = 3)]
                    Locked3x,
                    #[codec(index = 4)]
                    Locked4x,
                    #[codec(index = 5)]
                    Locked5x,
                    #[codec(index = 6)]
                    Locked6x,
                }
            }
            pub mod pallet {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "Contains a variant per dispatchable extrinsic that this pallet has."]
                pub enum Call {
                    #[codec(index = 0)]
                    #[doc = "Vote in a poll. If `vote.is_aye()`, the vote is to enact the proposal;"]
                    #[doc = "otherwise it is a vote to keep the status quo."]
                    #[doc = ""]
                    #[doc = "The dispatch origin of this call must be _Signed_."]
                    #[doc = ""]
                    #[doc = "- `poll_index`: The index of the poll to vote for."]
                    #[doc = "- `vote`: The vote configuration."]
                    #[doc = ""]
                    #[doc = "Weight: `O(R)` where R is the number of polls the voter has voted on."]
                    vote {
                        #[codec(compact)]
                        poll_index: ::core::primitive::u32,
                        vote: runtime_types::pallet_conviction_voting::vote::AccountVote<
                            ::core::primitive::u128,
                        >,
                    },
                    #[codec(index = 1)]
                    #[doc = "Delegate the voting power (with some given conviction) of the sending account for a"]
                    #[doc = "particular class of polls."]
                    #[doc = ""]
                    #[doc = "The balance delegated is locked for as long as it's delegated, and thereafter for the"]
                    #[doc = "time appropriate for the conviction's lock period."]
                    #[doc = ""]
                    #[doc = "The dispatch origin of this call must be _Signed_, and the signing account must either:"]
                    #[doc = "  - be delegating already; or"]
                    #[doc = "  - have no voting activity (if there is, then it will need to be removed through"]
                    #[doc = "    `remove_vote`)."]
                    #[doc = ""]
                    #[doc = "- `to`: The account whose voting the `target` account's voting power will follow."]
                    #[doc = "- `class`: The class of polls to delegate. To delegate multiple classes, multiple calls"]
                    #[doc = "  to this function are required."]
                    #[doc = "- `conviction`: The conviction that will be attached to the delegated votes. When the"]
                    #[doc = "  account is undelegated, the funds will be locked for the corresponding period."]
                    #[doc = "- `balance`: The amount of the account's balance to be used in delegating. This must not"]
                    #[doc = "  be more than the account's current balance."]
                    #[doc = ""]
                    #[doc = "Emits `Delegated`."]
                    #[doc = ""]
                    #[doc = "Weight: `O(R)` where R is the number of polls the voter delegating to has"]
                    #[doc = "  voted on. Weight is initially charged as if maximum votes, but is refunded later."]
                    delegate {
                        class: ::core::primitive::u16,
                        to: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                        conviction: runtime_types::pallet_conviction_voting::conviction::Conviction,
                        balance: ::core::primitive::u128,
                    },
                    #[codec(index = 2)]
                    #[doc = "Undelegate the voting power of the sending account for a particular class of polls."]
                    #[doc = ""]
                    #[doc = "Tokens may be unlocked following once an amount of time consistent with the lock period"]
                    #[doc = "of the conviction with which the delegation was issued has passed."]
                    #[doc = ""]
                    #[doc = "The dispatch origin of this call must be _Signed_ and the signing account must be"]
                    #[doc = "currently delegating."]
                    #[doc = ""]
                    #[doc = "- `class`: The class of polls to remove the delegation from."]
                    #[doc = ""]
                    #[doc = "Emits `Undelegated`."]
                    #[doc = ""]
                    #[doc = "Weight: `O(R)` where R is the number of polls the voter delegating to has"]
                    #[doc = "  voted on. Weight is initially charged as if maximum votes, but is refunded later."]
                    undelegate { class: ::core::primitive::u16 },
                    #[codec(index = 3)]
                    #[doc = "Remove the lock caused by prior voting/delegating which has expired within a particular"]
                    #[doc = "class."]
                    #[doc = ""]
                    #[doc = "The dispatch origin of this call must be _Signed_."]
                    #[doc = ""]
                    #[doc = "- `class`: The class of polls to unlock."]
                    #[doc = "- `target`: The account to remove the lock on."]
                    #[doc = ""]
                    #[doc = "Weight: `O(R)` with R number of vote of target."]
                    unlock {
                        class: ::core::primitive::u16,
                        target: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                    },
                    #[codec(index = 4)]
                    #[doc = "Remove a vote for a poll."]
                    #[doc = ""]
                    #[doc = "If:"]
                    #[doc = "- the poll was cancelled, or"]
                    #[doc = "- the poll is ongoing, or"]
                    #[doc = "- the poll has ended such that"]
                    #[doc = "  - the vote of the account was in opposition to the result; or"]
                    #[doc = "  - there was no conviction to the account's vote; or"]
                    #[doc = "  - the account made a split vote"]
                    #[doc = "...then the vote is removed cleanly and a following call to `unlock` may result in more"]
                    #[doc = "funds being available."]
                    #[doc = ""]
                    #[doc = "If, however, the poll has ended and:"]
                    #[doc = "- it finished corresponding to the vote of the account, and"]
                    #[doc = "- the account made a standard vote with conviction, and"]
                    #[doc = "- the lock period of the conviction is not over"]
                    #[doc = "...then the lock will be aggregated into the overall account's lock, which may involve"]
                    #[doc = "*overlocking* (where the two locks are combined into a single lock that is the maximum"]
                    #[doc = "of both the amount locked and the time is it locked for)."]
                    #[doc = ""]
                    #[doc = "The dispatch origin of this call must be _Signed_, and the signer must have a vote"]
                    #[doc = "registered for poll `index`."]
                    #[doc = ""]
                    #[doc = "- `index`: The index of poll of the vote to be removed."]
                    #[doc = "- `class`: Optional parameter, if given it indicates the class of the poll. For polls"]
                    #[doc = "  which have finished or are cancelled, this must be `Some`."]
                    #[doc = ""]
                    #[doc = "Weight: `O(R + log R)` where R is the number of polls that `target` has voted on."]
                    #[doc = "  Weight is calculated for the maximum number of vote."]
                    remove_vote {
                        class: ::core::option::Option<::core::primitive::u16>,
                        index: ::core::primitive::u32,
                    },
                    #[codec(index = 5)]
                    #[doc = "Remove a vote for a poll."]
                    #[doc = ""]
                    #[doc = "If the `target` is equal to the signer, then this function is exactly equivalent to"]
                    #[doc = "`remove_vote`. If not equal to the signer, then the vote must have expired,"]
                    #[doc = "either because the poll was cancelled, because the voter lost the poll or"]
                    #[doc = "because the conviction period is over."]
                    #[doc = ""]
                    #[doc = "The dispatch origin of this call must be _Signed_."]
                    #[doc = ""]
                    #[doc = "- `target`: The account of the vote to be removed; this account must have voted for poll"]
                    #[doc = "  `index`."]
                    #[doc = "- `index`: The index of poll of the vote to be removed."]
                    #[doc = "- `class`: The class of the poll."]
                    #[doc = ""]
                    #[doc = "Weight: `O(R + log R)` where R is the number of polls that `target` has voted on."]
                    #[doc = "  Weight is calculated for the maximum number of vote."]
                    remove_other_vote {
                        target: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                        class: ::core::primitive::u16,
                        index: ::core::primitive::u32,
                    },
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "The `Error` enum of this pallet."]
                pub enum Error {
                    #[codec(index = 0)]
                    #[doc = "Poll is not ongoing."]
                    NotOngoing,
                    #[codec(index = 1)]
                    #[doc = "The given account did not vote on the poll."]
                    NotVoter,
                    #[codec(index = 2)]
                    #[doc = "The actor has no permission to conduct the action."]
                    NoPermission,
                    #[codec(index = 3)]
                    #[doc = "The actor has no permission to conduct the action right now but will do in the future."]
                    NoPermissionYet,
                    #[codec(index = 4)]
                    #[doc = "The account is already delegating."]
                    AlreadyDelegating,
                    #[codec(index = 5)]
                    #[doc = "The account currently has votes attached to it and the operation cannot succeed until"]
                    #[doc = "these are removed through `remove_vote`."]
                    AlreadyVoting,
                    #[codec(index = 6)]
                    #[doc = "Too high a balance was provided that the account cannot afford."]
                    InsufficientFunds,
                    #[codec(index = 7)]
                    #[doc = "The account is not currently delegating."]
                    NotDelegating,
                    #[codec(index = 8)]
                    #[doc = "Delegation to oneself makes no sense."]
                    Nonsense,
                    #[codec(index = 9)]
                    #[doc = "Maximum number of votes reached."]
                    MaxVotesReached,
                    #[codec(index = 10)]
                    #[doc = "The class must be supplied since it is not easily determinable from the state."]
                    ClassNeeded,
                    #[codec(index = 11)]
                    #[doc = "The class ID supplied is invalid."]
                    BadClass,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "The `Event` enum of this pallet"]
                pub enum Event {
                    #[codec(index = 0)]
                    #[doc = "An account has delegated their vote to another account. \\[who, target\\]"]
                    Delegated(
                        ::subxt::ext::subxt_core::utils::AccountId32,
                        ::subxt::ext::subxt_core::utils::AccountId32,
                    ),
                    #[codec(index = 1)]
                    #[doc = "An \\[account\\] has cancelled a previous delegation operation."]
                    Undelegated(::subxt::ext::subxt_core::utils::AccountId32),
                    #[codec(index = 2)]
                    #[doc = "An account that has voted"]
                    Voted {
                        who: ::subxt::ext::subxt_core::utils::AccountId32,
                        vote: runtime_types::pallet_conviction_voting::vote::AccountVote<
                            ::core::primitive::u128,
                        >,
                    },
                    #[codec(index = 3)]
                    #[doc = "A vote that been removed"]
                    VoteRemoved {
                        who: ::subxt::ext::subxt_core::utils::AccountId32,
                        vote: runtime_types::pallet_conviction_voting::vote::AccountVote<
                            ::core::primitive::u128,
                        >,
                    },
                }
            }
            pub mod types {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct Delegations<_0> {
                    pub votes: _0,
                    pub capital: _0,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct Tally<_0> {
                    pub ayes: _0,
                    pub nays: _0,
                    pub support: _0,
                }
            }
            pub mod vote {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum AccountVote<_0> {
                    #[codec(index = 0)]
                    Standard {
                        vote: runtime_types::pallet_conviction_voting::vote::Vote,
                        balance: _0,
                    },
                    #[codec(index = 1)]
                    Split { aye: _0, nay: _0 },
                    #[codec(index = 2)]
                    SplitAbstain { aye: _0, nay: _0, abstain: _0 },
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct Casting<_0, _1, _2> {
                    pub votes: runtime_types::bounded_collections::bounded_vec::BoundedVec<(
                        _1,
                        runtime_types::pallet_conviction_voting::vote::AccountVote<_0>,
                    )>,
                    pub delegations:
                        runtime_types::pallet_conviction_voting::types::Delegations<_0>,
                    pub prior: runtime_types::pallet_conviction_voting::vote::PriorLock<_1, _0>,
                    #[codec(skip)]
                    pub __ignore: ::core::marker::PhantomData<_2>,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct Delegating<_0, _1, _2> {
                    pub balance: _0,
                    pub target: _1,
                    pub conviction: runtime_types::pallet_conviction_voting::conviction::Conviction,
                    pub delegations:
                        runtime_types::pallet_conviction_voting::types::Delegations<_0>,
                    pub prior: runtime_types::pallet_conviction_voting::vote::PriorLock<_2, _0>,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct PriorLock<_0, _1>(pub _0, pub _1);
                #[derive(
                    ::subxt::ext ::subxt_core::ext::codec::CompactAs,
                    Debug,
                    crate::gp::Decode,
                    crate::gp::DecodeAsType,
                    crate::gp::Encode,
                )]
                pub struct Vote(pub ::core::primitive::u8);
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum Voting<_0, _1, _2, _3> {
                    #[codec(index = 0)]
                    Casting(runtime_types::pallet_conviction_voting::vote::Casting<_0, _2, _2>),
                    #[codec(index = 1)]
                    Delegating(
                        runtime_types::pallet_conviction_voting::vote::Delegating<_0, _1, _2>,
                    ),
                    __Ignore(::core::marker::PhantomData<_3>),
                }
            }
        }
        pub mod pallet_election_provider_multi_phase {
            use super::runtime_types;
            pub mod pallet {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "Contains a variant per dispatchable extrinsic that this pallet has."]
                pub enum Call {
                    # [codec (index = 0)] # [doc = "Submit a solution for the unsigned phase."] # [doc = ""] # [doc = "The dispatch origin fo this call must be __none__."] # [doc = ""] # [doc = "This submission is checked on the fly. Moreover, this unsigned solution is only"] # [doc = "validated when submitted to the pool from the **local** node. Effectively, this means"] # [doc = "that only active validators can submit this transaction when authoring a block (similar"] # [doc = "to an inherent)."] # [doc = ""] # [doc = "To prevent any incorrect solution (and thus wasted time/weight), this transaction will"] # [doc = "panic if the solution submitted by the validator is invalid in any way, effectively"] # [doc = "putting their authoring reward at risk."] # [doc = ""] # [doc = "No deposit or reward is associated with this submission."] submit_unsigned { raw_solution : ::subxt::ext ::subxt_core::alloc::boxed::Box < runtime_types::pallet_election_provider_multi_phase::RawSolution < runtime_types::vara_runtime::NposSolution16 > > , witness : runtime_types::pallet_election_provider_multi_phase::SolutionOrSnapshotSize , } , # [codec (index = 1)] # [doc = "Set a new value for `MinimumUntrustedScore`."] # [doc = ""] # [doc = "Dispatch origin must be aligned with `T::ForceOrigin`."] # [doc = ""] # [doc = "This check can be turned off by setting the value to `None`."] set_minimum_untrusted_score { maybe_next_score: ::core::option::Option < runtime_types::sp_npos_elections::ElectionScore > , } , # [codec (index = 2)] # [doc = "Set a solution in the queue, to be handed out to the client of this pallet in the next"] # [doc = "call to `ElectionProvider::elect`."] # [doc = ""] # [doc = "This can only be set by `T::ForceOrigin`, and only when the phase is `Emergency`."] # [doc = ""] # [doc = "The solution is not checked for any feasibility and is assumed to be trustworthy, as any"] # [doc = "feasibility check itself can in principle cause the election process to fail (due to"] # [doc = "memory/weight constrains)."] set_emergency_election_result { supports : ::subxt::ext ::subxt_core::alloc::vec::Vec < (::subxt::ext ::subxt_core::utils::AccountId32 , runtime_types::sp_npos_elections::Support < ::subxt::ext ::subxt_core::utils::AccountId32 > ,) > , } , # [codec (index = 3)] # [doc = "Submit a solution for the signed phase."] # [doc = ""] # [doc = "The dispatch origin fo this call must be __signed__."] # [doc = ""] # [doc = "The solution is potentially queued, based on the claimed score and processed at the end"] # [doc = "of the signed phase."] # [doc = ""] # [doc = "A deposit is reserved and recorded for the solution. Based on the outcome, the solution"] # [doc = "might be rewarded, slashed, or get all or a part of the deposit back."] submit { raw_solution : ::subxt::ext ::subxt_core::alloc::boxed::Box < runtime_types::pallet_election_provider_multi_phase::RawSolution < runtime_types::vara_runtime::NposSolution16 > > , } , # [codec (index = 4)] # [doc = "Trigger the governance fallback."] # [doc = ""] # [doc = "This can only be called when [`Phase::Emergency`] is enabled, as an alternative to"] # [doc = "calling [`Call::set_emergency_election_result`]."] governance_fallback { maybe_max_voters: ::core::option::Option <::core::primitive::u32 > , maybe_max_targets: ::core::option::Option <::core::primitive::u32 > , } , }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "Error of the pallet that can be returned in response to dispatches."]
                pub enum Error {
                    #[codec(index = 0)]
                    #[doc = "Submission was too early."]
                    PreDispatchEarlySubmission,
                    #[codec(index = 1)]
                    #[doc = "Wrong number of winners presented."]
                    PreDispatchWrongWinnerCount,
                    #[codec(index = 2)]
                    #[doc = "Submission was too weak, score-wise."]
                    PreDispatchWeakSubmission,
                    #[codec(index = 3)]
                    #[doc = "The queue was full, and the solution was not better than any of the existing ones."]
                    SignedQueueFull,
                    #[codec(index = 4)]
                    #[doc = "The origin failed to pay the deposit."]
                    SignedCannotPayDeposit,
                    #[codec(index = 5)]
                    #[doc = "Witness data to dispatchable is invalid."]
                    SignedInvalidWitness,
                    #[codec(index = 6)]
                    #[doc = "The signed submission consumes too much weight"]
                    SignedTooMuchWeight,
                    #[codec(index = 7)]
                    #[doc = "OCW submitted solution for wrong round"]
                    OcwCallWrongEra,
                    #[codec(index = 8)]
                    #[doc = "Snapshot metadata should exist but didn't."]
                    MissingSnapshotMetadata,
                    #[codec(index = 9)]
                    #[doc = "`Self::insert_submission` returned an invalid index."]
                    InvalidSubmissionIndex,
                    #[codec(index = 10)]
                    #[doc = "The call is not allowed at this point."]
                    CallNotAllowed,
                    #[codec(index = 11)]
                    #[doc = "The fallback failed"]
                    FallbackFailed,
                    #[codec(index = 12)]
                    #[doc = "Some bound not met"]
                    BoundNotMet,
                    #[codec(index = 13)]
                    #[doc = "Submitted solution has too many winners"]
                    TooManyWinners,
                    #[codec(index = 14)]
                    #[doc = "Submission was prepared for a different round."]
                    PreDispatchDifferentRound,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "The `Event` enum of this pallet"]
                pub enum Event {
                    #[codec(index = 0)]
                    #[doc = "A solution was stored with the given compute."]
                    #[doc = ""]
                    #[doc = "The `origin` indicates the origin of the solution. If `origin` is `Some(AccountId)`,"]
                    #[doc = "the stored solution was submitted in the signed phase by a miner with the `AccountId`."]
                    #[doc = "Otherwise, the solution was stored either during the unsigned phase or by"]
                    #[doc = "`T::ForceOrigin`. The `bool` is `true` when a previous solution was ejected to make"]
                    #[doc = "room for this one."]
                    SolutionStored {
                        compute:
                            runtime_types::pallet_election_provider_multi_phase::ElectionCompute,
                        origin:
                            ::core::option::Option<::subxt::ext::subxt_core::utils::AccountId32>,
                        prev_ejected: ::core::primitive::bool,
                    },
                    #[codec(index = 1)]
                    #[doc = "The election has been finalized, with the given computation and score."]
                    ElectionFinalized {
                        compute:
                            runtime_types::pallet_election_provider_multi_phase::ElectionCompute,
                        score: runtime_types::sp_npos_elections::ElectionScore,
                    },
                    #[codec(index = 2)]
                    #[doc = "An election failed."]
                    #[doc = ""]
                    #[doc = "Not much can be said about which computes failed in the process."]
                    ElectionFailed,
                    #[codec(index = 3)]
                    #[doc = "An account has been rewarded for their signed submission being finalized."]
                    Rewarded {
                        account: ::subxt::ext::subxt_core::utils::AccountId32,
                        value: ::core::primitive::u128,
                    },
                    #[codec(index = 4)]
                    #[doc = "An account has been slashed for submitting an invalid signed submission."]
                    Slashed {
                        account: ::subxt::ext::subxt_core::utils::AccountId32,
                        value: ::core::primitive::u128,
                    },
                    #[codec(index = 5)]
                    #[doc = "There was a phase transition in a given round."]
                    PhaseTransitioned {
                        from: runtime_types::pallet_election_provider_multi_phase::Phase<
                            ::core::primitive::u32,
                        >,
                        to: runtime_types::pallet_election_provider_multi_phase::Phase<
                            ::core::primitive::u32,
                        >,
                        round: ::core::primitive::u32,
                    },
                }
            }
            pub mod signed {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct SignedSubmission<_0, _1, _2> {
                    pub who: _0,
                    pub deposit: _1,
                    pub raw_solution:
                        runtime_types::pallet_election_provider_multi_phase::RawSolution<_2>,
                    pub call_fee: _1,
                }
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub enum ElectionCompute {
                #[codec(index = 0)]
                OnChain,
                #[codec(index = 1)]
                Signed,
                #[codec(index = 2)]
                Unsigned,
                #[codec(index = 3)]
                Fallback,
                #[codec(index = 4)]
                Emergency,
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub enum Phase<_0> {
                #[codec(index = 0)]
                Off,
                #[codec(index = 1)]
                Signed,
                #[codec(index = 2)]
                Unsigned((::core::primitive::bool, _0)),
                #[codec(index = 3)]
                Emergency,
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub struct RawSolution<_0> {
                pub solution: _0,
                pub score: runtime_types::sp_npos_elections::ElectionScore,
                pub round: ::core::primitive::u32,
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub struct ReadySolution {
                pub supports: runtime_types::bounded_collections::bounded_vec::BoundedVec<(
                    ::subxt::ext::subxt_core::utils::AccountId32,
                    runtime_types::sp_npos_elections::Support<
                        ::subxt::ext::subxt_core::utils::AccountId32,
                    >,
                )>,
                pub score: runtime_types::sp_npos_elections::ElectionScore,
                pub compute: runtime_types::pallet_election_provider_multi_phase::ElectionCompute,
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub struct RoundSnapshot<_0, _1> {
                pub voters: ::subxt::ext::subxt_core::alloc::vec::Vec<_1>,
                pub targets: ::subxt::ext::subxt_core::alloc::vec::Vec<_0>,
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub struct SolutionOrSnapshotSize {
                #[codec(compact)]
                pub voters: ::core::primitive::u32,
                #[codec(compact)]
                pub targets: ::core::primitive::u32,
            }
        }
        pub mod pallet_gear {
            use super::runtime_types;
            pub mod pallet {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "Contains a variant per dispatchable extrinsic that this pallet has."]
                pub enum Call {
                    #[codec(index = 0)]
                    #[doc = "Saves program `code` in storage."]
                    #[doc = ""]
                    #[doc = "The extrinsic was created to provide _deploy program from program_ functionality."]
                    #[doc = "Anyone who wants to define a \"factory\" logic in program should first store the code and metadata for the \"child\""]
                    #[doc = "program in storage. So the code for the child will be initialized by program initialization request only if it exists in storage."]
                    #[doc = ""]
                    #[doc = "More precisely, the code and its metadata are actually saved in the storage under the hash of the `code`. The code hash is computed"]
                    #[doc = "as Blake256 hash. At the time of the call the `code` hash should not be in the storage. If it was stored previously, call will end up"]
                    #[doc = "with an `CodeAlreadyExists` error. In this case user can be sure, that he can actually use the hash of his program's code bytes to define"]
                    #[doc = "\"program factory\" logic in his program."]
                    #[doc = ""]
                    #[doc = "Parameters"]
                    #[doc = "- `code`: wasm code of a program as a byte vector."]
                    #[doc = ""]
                    #[doc = "Emits the following events:"]
                    #[doc = "- `SavedCode(H256)` - when the code is saved in storage."]
                    upload_code {
                        code: ::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
                    },
                    #[codec(index = 1)]
                    #[doc = "Creates program initialization request (message), that is scheduled to be run in the same block."]
                    #[doc = ""]
                    #[doc = "There are no guarantees that initialization message will be run in the same block due to block"]
                    #[doc = "gas limit restrictions. For example, when it will be the message's turn, required gas limit for it"]
                    #[doc = "could be more than remaining block gas limit. Therefore, the message processing will be postponed"]
                    #[doc = "until the next block."]
                    #[doc = ""]
                    #[doc = "`ActorId` is computed as Blake256 hash of concatenated bytes of `code` + `salt`. (todo #512 `code_hash` + `salt`)"]
                    #[doc = "Such `ActorId` must not exist in the Program Storage at the time of this call."]
                    #[doc = ""]
                    #[doc = "There is the same guarantee here as in `upload_code`. That is, future program's"]
                    #[doc = "`code` and metadata are stored before message was added to the queue and processed."]
                    #[doc = ""]
                    #[doc = "The origin must be Signed and the sender must have sufficient funds to pay"]
                    #[doc = "for `gas` and `value` (in case the latter is being transferred)."]
                    #[doc = ""]
                    #[doc = "Gear runtime guarantees that an active program always has an account to store value."]
                    #[doc = "If the underlying account management platform (e.g. Substrate's System pallet) requires"]
                    #[doc = "an existential deposit to keep an account alive, the related overhead is considered an"]
                    #[doc = "extra cost related with a program instantiation and is charged to the program's creator"]
                    #[doc = "and is released back to the creator when the program is removed."]
                    #[doc = "In context of the above, the `value` parameter represents the so-called `reducible` balance"]
                    #[doc = "a program should have at its disposal upon instantiation. It is not used to offset the"]
                    #[doc = "existential deposit required for an account creation."]
                    #[doc = ""]
                    #[doc = "Parameters:"]
                    #[doc = "- `code`: wasm code of a program as a byte vector."]
                    #[doc = "- `salt`: randomness term (a seed) to allow programs with identical code"]
                    #[doc = "  to be created independently."]
                    #[doc = "- `init_payload`: encoded parameters of the wasm module `init` function."]
                    #[doc = "- `gas_limit`: maximum amount of gas the program can spend before it is halted."]
                    #[doc = "- `value`: balance to be transferred to the program once it's been created."]
                    #[doc = ""]
                    #[doc = "Emits the following events:"]
                    #[doc = "- `InitMessageEnqueued(MessageInfo)` when init message is placed in the queue."]
                    #[doc = ""]
                    #[doc = "# Note"]
                    #[doc = "Faulty (uninitialized) programs still have a valid addresses (program ids) that can deterministically be derived on the"]
                    #[doc = "caller's side upfront. It means that if messages are sent to such an address, they might still linger in the queue."]
                    #[doc = ""]
                    #[doc = "In order to mitigate the risk of users' funds being sent to an address,"]
                    #[doc = "where a valid program should have resided, while it's not,"]
                    #[doc = "such \"failed-to-initialize\" programs are not silently deleted from the"]
                    #[doc = "program storage but rather marked as \"ghost\" programs."]
                    #[doc = "Ghost program can be removed by their original author via an explicit call."]
                    #[doc = "The funds stored by a ghost program will be release to the author once the program"]
                    #[doc = "has been removed."]
                    upload_program {
                        code: ::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
                        salt: ::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
                        init_payload:
                            ::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
                        gas_limit: ::core::primitive::u64,
                        value: ::core::primitive::u128,
                        keep_alive: ::core::primitive::bool,
                    },
                    #[codec(index = 2)]
                    #[doc = "Creates program via `code_id` from storage."]
                    #[doc = ""]
                    #[doc = "Parameters:"]
                    #[doc = "- `code_id`: wasm code id in the code storage."]
                    #[doc = "- `salt`: randomness term (a seed) to allow programs with identical code"]
                    #[doc = "  to be created independently."]
                    #[doc = "- `init_payload`: encoded parameters of the wasm module `init` function."]
                    #[doc = "- `gas_limit`: maximum amount of gas the program can spend before it is halted."]
                    #[doc = "- `value`: balance to be transferred to the program once it's been created."]
                    #[doc = ""]
                    #[doc = "Emits the following events:"]
                    #[doc = "- `InitMessageEnqueued(MessageInfo)` when init message is placed in the queue."]
                    #[doc = ""]
                    #[doc = "# NOTE"]
                    #[doc = ""]
                    #[doc = "For the details of this extrinsic, see `upload_code`."]
                    create_program {
                        code_id: runtime_types::gprimitives::CodeId,
                        salt: ::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
                        init_payload:
                            ::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
                        gas_limit: ::core::primitive::u64,
                        value: ::core::primitive::u128,
                        keep_alive: ::core::primitive::bool,
                    },
                    #[codec(index = 3)]
                    #[doc = "Sends a message to a program or to another account."]
                    #[doc = ""]
                    #[doc = "The origin must be Signed and the sender must have sufficient funds to pay"]
                    #[doc = "for `gas` and `value` (in case the latter is being transferred)."]
                    #[doc = ""]
                    #[doc = "To avoid an undefined behavior a check is made that the destination address"]
                    #[doc = "is not a program in uninitialized state. If the opposite holds true,"]
                    #[doc = "the message is not enqueued for processing."]
                    #[doc = ""]
                    #[doc = "Parameters:"]
                    #[doc = "- `destination`: the message destination."]
                    #[doc = "- `payload`: in case of a program destination, parameters of the `handle` function."]
                    #[doc = "- `gas_limit`: maximum amount of gas the program can spend before it is halted."]
                    #[doc = "- `value`: balance to be transferred to the program once it's been created."]
                    #[doc = ""]
                    #[doc = "Emits the following events:"]
                    #[doc = "- `DispatchMessageEnqueued(MessageInfo)` when dispatch message is placed in the queue."]
                    send_message {
                        destination: runtime_types::gprimitives::ActorId,
                        payload: ::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
                        gas_limit: ::core::primitive::u64,
                        value: ::core::primitive::u128,
                        keep_alive: ::core::primitive::bool,
                    },
                    #[codec(index = 4)]
                    #[doc = "Send reply on message in `Mailbox`."]
                    #[doc = ""]
                    #[doc = "Removes message by given `MessageId` from callers `Mailbox`:"]
                    #[doc = "rent funds become free, associated with the message value"]
                    #[doc = "transfers from message sender to extrinsic caller."]
                    #[doc = ""]
                    #[doc = "Generates reply on removed message with given parameters"]
                    #[doc = "and pushes it in `MessageQueue`."]
                    #[doc = ""]
                    #[doc = "NOTE: source of the message in mailbox guaranteed to be a program."]
                    #[doc = ""]
                    #[doc = "NOTE: only user who is destination of the message, can claim value"]
                    #[doc = "or reply on the message from mailbox."]
                    send_reply {
                        reply_to_id: runtime_types::gprimitives::MessageId,
                        payload: ::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
                        gas_limit: ::core::primitive::u64,
                        value: ::core::primitive::u128,
                        keep_alive: ::core::primitive::bool,
                    },
                    #[codec(index = 5)]
                    #[doc = "Claim value from message in `Mailbox`."]
                    #[doc = ""]
                    #[doc = "Removes message by given `MessageId` from callers `Mailbox`:"]
                    #[doc = "rent funds become free, associated with the message value"]
                    #[doc = "transfers from message sender to extrinsic caller."]
                    #[doc = ""]
                    #[doc = "NOTE: only user who is destination of the message, can claim value"]
                    #[doc = "or reply on the message from mailbox."]
                    claim_value {
                        message_id: runtime_types::gprimitives::MessageId,
                    },
                    #[codec(index = 6)]
                    #[doc = "Process message queue"]
                    run {
                        max_gas: ::core::option::Option<::core::primitive::u64>,
                    },
                    #[codec(index = 7)]
                    #[doc = "Sets `ExecuteInherent` flag."]
                    #[doc = ""]
                    #[doc = "Requires root origin (eventually, will only be set via referendum)"]
                    set_execute_inherent { value: ::core::primitive::bool },
                    #[codec(index = 8)]
                    #[doc = "Transfers value from chain of terminated or exited programs to its final inheritor."]
                    #[doc = ""]
                    #[doc = "`depth` parameter is how far to traverse to inheritor."]
                    #[doc = "A value of 10 is sufficient for most cases."]
                    #[doc = ""]
                    #[doc = "# Example of chain"]
                    #[doc = ""]
                    #[doc = "- Program #1 exits (e.g `gr_exit syscall) with argument pointing to user."]
                    #[doc = "Balance of program #1 has been sent to user."]
                    #[doc = "- Program #2 exits with inheritor pointing to program #1."]
                    #[doc = "Balance of program #2 has been sent to exited program #1."]
                    #[doc = "- Program #3 exits with inheritor pointing to program #2"]
                    #[doc = "Balance of program #1 has been sent to exited program #2."]
                    #[doc = ""]
                    #[doc = "So chain of inheritors looks like: Program #3 -> Program #2 -> Program #1 -> User."]
                    #[doc = ""]
                    #[doc = "We have programs #1 and #2 with stuck value on their balances."]
                    #[doc = "The balances should've been transferred to user (final inheritor) according to the chain."]
                    #[doc = "But protocol doesn't traverse the chain automatically, so user have to call this extrinsic."]
                    claim_value_to_inheritor {
                        program_id: runtime_types::gprimitives::ActorId,
                        depth: ::core::num::NonZeroU32,
                    },
                    #[codec(index = 255)]
                    #[doc = "A dummy extrinsic with programmatically set weight."]
                    #[doc = ""]
                    #[doc = "Used in tests to exhaust block resources."]
                    #[doc = ""]
                    #[doc = "Parameters:"]
                    #[doc = "- `fraction`: the fraction of the `max_extrinsic` the extrinsic will use."]
                    exhaust_block_resources {
                        fraction: runtime_types::sp_arithmetic::per_things::Percent,
                    },
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "The `Error` enum of this pallet."]
                pub enum Error {
                    #[codec(index = 0)]
                    #[doc = "Message wasn't found in the mailbox."]
                    MessageNotFound,
                    #[codec(index = 1)]
                    #[doc = "Not enough balance to execute an action."]
                    #[doc = ""]
                    #[doc = "Usually occurs when the gas_limit specified is such that the origin account can't afford the message."]
                    InsufficientBalance,
                    #[codec(index = 2)]
                    #[doc = "Gas limit too high."]
                    #[doc = ""]
                    #[doc = "Occurs when an extrinsic's declared `gas_limit` is greater than a block's maximum gas limit."]
                    GasLimitTooHigh,
                    #[codec(index = 3)]
                    #[doc = "Program already exists."]
                    #[doc = ""]
                    #[doc = "Occurs if a program with some specific program id already exists in program storage."]
                    ProgramAlreadyExists,
                    #[codec(index = 4)]
                    #[doc = "Program is terminated."]
                    #[doc = ""]
                    #[doc = "Program init failed, so such message destination is no longer unavailable."]
                    InactiveProgram,
                    #[codec(index = 5)]
                    #[doc = "Message gas tree is not found."]
                    #[doc = ""]
                    #[doc = "When a message claimed from the mailbox has a corrupted or non-extant gas tree associated."]
                    NoMessageTree,
                    #[codec(index = 6)]
                    #[doc = "Code already exists."]
                    #[doc = ""]
                    #[doc = "Occurs when trying to save to storage a program code that has been saved there."]
                    CodeAlreadyExists,
                    #[codec(index = 7)]
                    #[doc = "Code does not exist."]
                    #[doc = ""]
                    #[doc = "Occurs when trying to get a program code from storage, that doesn't exist."]
                    CodeDoesntExist,
                    #[codec(index = 8)]
                    #[doc = "The code supplied to `upload_code` or `upload_program` exceeds the limit specified in the"]
                    #[doc = "current schedule."]
                    CodeTooLarge,
                    #[codec(index = 9)]
                    #[doc = "Failed to create a program."]
                    ProgramConstructionFailed,
                    #[codec(index = 10)]
                    #[doc = "Message queue processing is disabled."]
                    MessageQueueProcessingDisabled,
                    #[codec(index = 11)]
                    #[doc = "Block count doesn't cover MinimalResumePeriod."]
                    ResumePeriodLessThanMinimal,
                    #[codec(index = 12)]
                    #[doc = "Program with the specified id is not found."]
                    ProgramNotFound,
                    #[codec(index = 13)]
                    #[doc = "Gear::run() already included in current block."]
                    GearRunAlreadyInBlock,
                    #[codec(index = 14)]
                    #[doc = "The program rent logic is disabled."]
                    ProgramRentDisabled,
                    #[codec(index = 15)]
                    #[doc = "Program is active."]
                    ActiveProgram,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "The `Event` enum of this pallet"]
                pub enum Event {
                    #[codec(index = 0)]
                    #[doc = "User sends message to program, which was successfully"]
                    #[doc = "added to the Gear message queue."]
                    MessageQueued {
                        id: runtime_types::gprimitives::MessageId,
                        source: ::subxt::ext::subxt_core::utils::AccountId32,
                        destination: runtime_types::gprimitives::ActorId,
                        entry: runtime_types::gear_common::event::MessageEntry,
                    },
                    #[codec(index = 1)]
                    #[doc = "Somebody sent a message to the user."]
                    UserMessageSent {
                        message: runtime_types::gear_core::message::user::UserMessage,
                        expiration: ::core::option::Option<::core::primitive::u32>,
                    },
                    #[codec(index = 2)]
                    #[doc = "Message marked as \"read\" and removes it from `Mailbox`."]
                    #[doc = "This event only affects messages that were"]
                    #[doc = "already inserted in `Mailbox`."]
                    UserMessageRead {
                        id: runtime_types::gprimitives::MessageId,
                        reason: runtime_types::gear_common::event::Reason<
                            runtime_types::gear_common::event::UserMessageReadRuntimeReason,
                            runtime_types::gear_common::event::UserMessageReadSystemReason,
                        >,
                    },
                    #[codec(index = 3)]
                    #[doc = "The result of processing the messages within the block."]
                    MessagesDispatched {
                        total: ::core::primitive::u32,
                        statuses: ::subxt::ext::subxt_core::utils::KeyedVec<
                            runtime_types::gprimitives::MessageId,
                            runtime_types::gear_common::event::DispatchStatus,
                        >,
                        state_changes: ::subxt::ext::subxt_core::alloc::vec::Vec<
                            runtime_types::gprimitives::ActorId,
                        >,
                    },
                    #[codec(index = 4)]
                    #[doc = "Messages execution delayed (waited) and successfully"]
                    #[doc = "added to gear waitlist."]
                    MessageWaited {
                        id: runtime_types::gprimitives::MessageId,
                        origin: ::core::option::Option<
                            runtime_types::gear_common::gas_provider::node::GasNodeId<
                                runtime_types::gprimitives::MessageId,
                                runtime_types::gprimitives::ReservationId,
                            >,
                        >,
                        reason: runtime_types::gear_common::event::Reason<
                            runtime_types::gear_common::event::MessageWaitedRuntimeReason,
                            runtime_types::gear_common::event::MessageWaitedSystemReason,
                        >,
                        expiration: ::core::primitive::u32,
                    },
                    #[codec(index = 5)]
                    #[doc = "Message is ready to continue its execution"]
                    #[doc = "and was removed from `Waitlist`."]
                    MessageWoken {
                        id: runtime_types::gprimitives::MessageId,
                        reason: runtime_types::gear_common::event::Reason<
                            runtime_types::gear_common::event::MessageWokenRuntimeReason,
                            runtime_types::gear_common::event::MessageWokenSystemReason,
                        >,
                    },
                    #[codec(index = 6)]
                    #[doc = "Any data related to program codes changed."]
                    CodeChanged {
                        id: runtime_types::gprimitives::CodeId,
                        change: runtime_types::gear_common::event::CodeChangeKind<
                            ::core::primitive::u32,
                        >,
                    },
                    #[codec(index = 7)]
                    #[doc = "Any data related to programs changed."]
                    ProgramChanged {
                        id: runtime_types::gprimitives::ActorId,
                        change: runtime_types::gear_common::event::ProgramChangeKind<
                            ::core::primitive::u32,
                        >,
                    },
                    #[codec(index = 8)]
                    #[doc = "The pseudo-inherent extrinsic that runs queue processing rolled back or not executed."]
                    QueueNotProcessed,
                }
            }
            pub mod schedule {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct DbWeights {
                    pub read: runtime_types::sp_weights::weight_v2::Weight,
                    pub read_per_byte: runtime_types::sp_weights::weight_v2::Weight,
                    pub write: runtime_types::sp_weights::weight_v2::Weight,
                    pub write_per_byte: runtime_types::sp_weights::weight_v2::Weight,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct InstantiationWeights {
                    pub code_section_per_byte: runtime_types::sp_weights::weight_v2::Weight,
                    pub data_section_per_byte: runtime_types::sp_weights::weight_v2::Weight,
                    pub global_section_per_byte: runtime_types::sp_weights::weight_v2::Weight,
                    pub table_section_per_byte: runtime_types::sp_weights::weight_v2::Weight,
                    pub element_section_per_byte: runtime_types::sp_weights::weight_v2::Weight,
                    pub type_section_per_byte: runtime_types::sp_weights::weight_v2::Weight,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct InstructionWeights {
                    pub version: ::core::primitive::u32,
                    pub i64const: ::core::primitive::u32,
                    pub i64load: ::core::primitive::u32,
                    pub i32load: ::core::primitive::u32,
                    pub i64store: ::core::primitive::u32,
                    pub i32store: ::core::primitive::u32,
                    pub select: ::core::primitive::u32,
                    pub r#if: ::core::primitive::u32,
                    pub br: ::core::primitive::u32,
                    pub br_if: ::core::primitive::u32,
                    pub br_table: ::core::primitive::u32,
                    pub br_table_per_entry: ::core::primitive::u32,
                    pub call: ::core::primitive::u32,
                    pub call_indirect: ::core::primitive::u32,
                    pub call_indirect_per_param: ::core::primitive::u32,
                    pub call_per_local: ::core::primitive::u32,
                    pub local_get: ::core::primitive::u32,
                    pub local_set: ::core::primitive::u32,
                    pub local_tee: ::core::primitive::u32,
                    pub global_get: ::core::primitive::u32,
                    pub global_set: ::core::primitive::u32,
                    pub memory_current: ::core::primitive::u32,
                    pub i64clz: ::core::primitive::u32,
                    pub i32clz: ::core::primitive::u32,
                    pub i64ctz: ::core::primitive::u32,
                    pub i32ctz: ::core::primitive::u32,
                    pub i64popcnt: ::core::primitive::u32,
                    pub i32popcnt: ::core::primitive::u32,
                    pub i64eqz: ::core::primitive::u32,
                    pub i32eqz: ::core::primitive::u32,
                    pub i32extend8s: ::core::primitive::u32,
                    pub i32extend16s: ::core::primitive::u32,
                    pub i64extend8s: ::core::primitive::u32,
                    pub i64extend16s: ::core::primitive::u32,
                    pub i64extend32s: ::core::primitive::u32,
                    pub i64extendsi32: ::core::primitive::u32,
                    pub i64extendui32: ::core::primitive::u32,
                    pub i32wrapi64: ::core::primitive::u32,
                    pub i64eq: ::core::primitive::u32,
                    pub i32eq: ::core::primitive::u32,
                    pub i64ne: ::core::primitive::u32,
                    pub i32ne: ::core::primitive::u32,
                    pub i64lts: ::core::primitive::u32,
                    pub i32lts: ::core::primitive::u32,
                    pub i64ltu: ::core::primitive::u32,
                    pub i32ltu: ::core::primitive::u32,
                    pub i64gts: ::core::primitive::u32,
                    pub i32gts: ::core::primitive::u32,
                    pub i64gtu: ::core::primitive::u32,
                    pub i32gtu: ::core::primitive::u32,
                    pub i64les: ::core::primitive::u32,
                    pub i32les: ::core::primitive::u32,
                    pub i64leu: ::core::primitive::u32,
                    pub i32leu: ::core::primitive::u32,
                    pub i64ges: ::core::primitive::u32,
                    pub i32ges: ::core::primitive::u32,
                    pub i64geu: ::core::primitive::u32,
                    pub i32geu: ::core::primitive::u32,
                    pub i64add: ::core::primitive::u32,
                    pub i32add: ::core::primitive::u32,
                    pub i64sub: ::core::primitive::u32,
                    pub i32sub: ::core::primitive::u32,
                    pub i64mul: ::core::primitive::u32,
                    pub i32mul: ::core::primitive::u32,
                    pub i64divs: ::core::primitive::u32,
                    pub i32divs: ::core::primitive::u32,
                    pub i64divu: ::core::primitive::u32,
                    pub i32divu: ::core::primitive::u32,
                    pub i64rems: ::core::primitive::u32,
                    pub i32rems: ::core::primitive::u32,
                    pub i64remu: ::core::primitive::u32,
                    pub i32remu: ::core::primitive::u32,
                    pub i64and: ::core::primitive::u32,
                    pub i32and: ::core::primitive::u32,
                    pub i64or: ::core::primitive::u32,
                    pub i32or: ::core::primitive::u32,
                    pub i64xor: ::core::primitive::u32,
                    pub i32xor: ::core::primitive::u32,
                    pub i64shl: ::core::primitive::u32,
                    pub i32shl: ::core::primitive::u32,
                    pub i64shrs: ::core::primitive::u32,
                    pub i32shrs: ::core::primitive::u32,
                    pub i64shru: ::core::primitive::u32,
                    pub i32shru: ::core::primitive::u32,
                    pub i64rotl: ::core::primitive::u32,
                    pub i32rotl: ::core::primitive::u32,
                    pub i64rotr: ::core::primitive::u32,
                    pub i32rotr: ::core::primitive::u32,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct InstrumentationWeights {
                    pub base: runtime_types::sp_weights::weight_v2::Weight,
                    pub per_byte: runtime_types::sp_weights::weight_v2::Weight,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct Limits {
                    pub stack_height: ::core::option::Option<::core::primitive::u32>,
                    pub globals: ::core::primitive::u32,
                    pub locals: ::core::primitive::u32,
                    pub parameters: ::core::primitive::u32,
                    pub memory_pages: ::core::primitive::u16,
                    pub table_size: ::core::primitive::u32,
                    pub br_table_size: ::core::primitive::u32,
                    pub subject_len: ::core::primitive::u32,
                    pub call_depth: ::core::primitive::u32,
                    pub payload_len: ::core::primitive::u32,
                    pub code_len: ::core::primitive::u32,
                    pub data_segments_amount: ::core::primitive::u32,
                    pub type_section_len: ::core::primitive::u32,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct MemoryWeights {
                    pub lazy_pages_signal_read: runtime_types::sp_weights::weight_v2::Weight,
                    pub lazy_pages_signal_write: runtime_types::sp_weights::weight_v2::Weight,
                    pub lazy_pages_signal_write_after_read:
                        runtime_types::sp_weights::weight_v2::Weight,
                    pub lazy_pages_host_func_read: runtime_types::sp_weights::weight_v2::Weight,
                    pub lazy_pages_host_func_write: runtime_types::sp_weights::weight_v2::Weight,
                    pub lazy_pages_host_func_write_after_read:
                        runtime_types::sp_weights::weight_v2::Weight,
                    pub load_page_data: runtime_types::sp_weights::weight_v2::Weight,
                    pub upload_page_data: runtime_types::sp_weights::weight_v2::Weight,
                    pub mem_grow: runtime_types::sp_weights::weight_v2::Weight,
                    pub mem_grow_per_page: runtime_types::sp_weights::weight_v2::Weight,
                    pub parachain_read_heuristic: runtime_types::sp_weights::weight_v2::Weight,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct RentWeights {
                    pub waitlist: runtime_types::sp_weights::weight_v2::Weight,
                    pub dispatch_stash: runtime_types::sp_weights::weight_v2::Weight,
                    pub reservation: runtime_types::sp_weights::weight_v2::Weight,
                    pub mailbox: runtime_types::sp_weights::weight_v2::Weight,
                    pub mailbox_threshold: runtime_types::sp_weights::weight_v2::Weight,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct Schedule {
                    pub limits: runtime_types::pallet_gear::schedule::Limits,
                    pub instruction_weights:
                        runtime_types::pallet_gear::schedule::InstructionWeights,
                    pub syscall_weights: runtime_types::pallet_gear::schedule::SyscallWeights,
                    pub memory_weights: runtime_types::pallet_gear::schedule::MemoryWeights,
                    pub rent_weights: runtime_types::pallet_gear::schedule::RentWeights,
                    pub db_weights: runtime_types::pallet_gear::schedule::DbWeights,
                    pub task_weights: runtime_types::pallet_gear::schedule::TaskWeights,
                    pub instantiation_weights:
                        runtime_types::pallet_gear::schedule::InstantiationWeights,
                    pub instrumentation_weights:
                        runtime_types::pallet_gear::schedule::InstrumentationWeights,
                    pub load_allocations_weight: runtime_types::sp_weights::weight_v2::Weight,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct SyscallWeights {
                    pub alloc: runtime_types::sp_weights::weight_v2::Weight,
                    pub free: runtime_types::sp_weights::weight_v2::Weight,
                    pub free_range: runtime_types::sp_weights::weight_v2::Weight,
                    pub free_range_per_page: runtime_types::sp_weights::weight_v2::Weight,
                    pub gr_reserve_gas: runtime_types::sp_weights::weight_v2::Weight,
                    pub gr_unreserve_gas: runtime_types::sp_weights::weight_v2::Weight,
                    pub gr_system_reserve_gas: runtime_types::sp_weights::weight_v2::Weight,
                    pub gr_gas_available: runtime_types::sp_weights::weight_v2::Weight,
                    pub gr_message_id: runtime_types::sp_weights::weight_v2::Weight,
                    pub gr_program_id: runtime_types::sp_weights::weight_v2::Weight,
                    pub gr_source: runtime_types::sp_weights::weight_v2::Weight,
                    pub gr_value: runtime_types::sp_weights::weight_v2::Weight,
                    pub gr_value_available: runtime_types::sp_weights::weight_v2::Weight,
                    pub gr_size: runtime_types::sp_weights::weight_v2::Weight,
                    pub gr_read: runtime_types::sp_weights::weight_v2::Weight,
                    pub gr_read_per_byte: runtime_types::sp_weights::weight_v2::Weight,
                    pub gr_env_vars: runtime_types::sp_weights::weight_v2::Weight,
                    pub gr_block_height: runtime_types::sp_weights::weight_v2::Weight,
                    pub gr_block_timestamp: runtime_types::sp_weights::weight_v2::Weight,
                    pub gr_random: runtime_types::sp_weights::weight_v2::Weight,
                    pub gr_reply_deposit: runtime_types::sp_weights::weight_v2::Weight,
                    pub gr_send: runtime_types::sp_weights::weight_v2::Weight,
                    pub gr_send_per_byte: runtime_types::sp_weights::weight_v2::Weight,
                    pub gr_send_wgas: runtime_types::sp_weights::weight_v2::Weight,
                    pub gr_send_wgas_per_byte: runtime_types::sp_weights::weight_v2::Weight,
                    pub gr_send_init: runtime_types::sp_weights::weight_v2::Weight,
                    pub gr_send_push: runtime_types::sp_weights::weight_v2::Weight,
                    pub gr_send_push_per_byte: runtime_types::sp_weights::weight_v2::Weight,
                    pub gr_send_commit: runtime_types::sp_weights::weight_v2::Weight,
                    pub gr_send_commit_wgas: runtime_types::sp_weights::weight_v2::Weight,
                    pub gr_reservation_send: runtime_types::sp_weights::weight_v2::Weight,
                    pub gr_reservation_send_per_byte: runtime_types::sp_weights::weight_v2::Weight,
                    pub gr_reservation_send_commit: runtime_types::sp_weights::weight_v2::Weight,
                    pub gr_reply_commit: runtime_types::sp_weights::weight_v2::Weight,
                    pub gr_reply_commit_wgas: runtime_types::sp_weights::weight_v2::Weight,
                    pub gr_reservation_reply: runtime_types::sp_weights::weight_v2::Weight,
                    pub gr_reservation_reply_per_byte: runtime_types::sp_weights::weight_v2::Weight,
                    pub gr_reservation_reply_commit: runtime_types::sp_weights::weight_v2::Weight,
                    pub gr_reply_push: runtime_types::sp_weights::weight_v2::Weight,
                    pub gr_reply: runtime_types::sp_weights::weight_v2::Weight,
                    pub gr_reply_per_byte: runtime_types::sp_weights::weight_v2::Weight,
                    pub gr_reply_wgas: runtime_types::sp_weights::weight_v2::Weight,
                    pub gr_reply_wgas_per_byte: runtime_types::sp_weights::weight_v2::Weight,
                    pub gr_reply_push_per_byte: runtime_types::sp_weights::weight_v2::Weight,
                    pub gr_reply_to: runtime_types::sp_weights::weight_v2::Weight,
                    pub gr_signal_code: runtime_types::sp_weights::weight_v2::Weight,
                    pub gr_signal_from: runtime_types::sp_weights::weight_v2::Weight,
                    pub gr_reply_input: runtime_types::sp_weights::weight_v2::Weight,
                    pub gr_reply_input_wgas: runtime_types::sp_weights::weight_v2::Weight,
                    pub gr_reply_push_input: runtime_types::sp_weights::weight_v2::Weight,
                    pub gr_reply_push_input_per_byte: runtime_types::sp_weights::weight_v2::Weight,
                    pub gr_send_input: runtime_types::sp_weights::weight_v2::Weight,
                    pub gr_send_input_wgas: runtime_types::sp_weights::weight_v2::Weight,
                    pub gr_send_push_input: runtime_types::sp_weights::weight_v2::Weight,
                    pub gr_send_push_input_per_byte: runtime_types::sp_weights::weight_v2::Weight,
                    pub gr_debug: runtime_types::sp_weights::weight_v2::Weight,
                    pub gr_debug_per_byte: runtime_types::sp_weights::weight_v2::Weight,
                    pub gr_reply_code: runtime_types::sp_weights::weight_v2::Weight,
                    pub gr_exit: runtime_types::sp_weights::weight_v2::Weight,
                    pub gr_leave: runtime_types::sp_weights::weight_v2::Weight,
                    pub gr_wait: runtime_types::sp_weights::weight_v2::Weight,
                    pub gr_wait_for: runtime_types::sp_weights::weight_v2::Weight,
                    pub gr_wait_up_to: runtime_types::sp_weights::weight_v2::Weight,
                    pub gr_wake: runtime_types::sp_weights::weight_v2::Weight,
                    pub gr_create_program: runtime_types::sp_weights::weight_v2::Weight,
                    pub gr_create_program_payload_per_byte:
                        runtime_types::sp_weights::weight_v2::Weight,
                    pub gr_create_program_salt_per_byte:
                        runtime_types::sp_weights::weight_v2::Weight,
                    pub gr_create_program_wgas: runtime_types::sp_weights::weight_v2::Weight,
                    pub gr_create_program_wgas_payload_per_byte:
                        runtime_types::sp_weights::weight_v2::Weight,
                    pub gr_create_program_wgas_salt_per_byte:
                        runtime_types::sp_weights::weight_v2::Weight,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct TaskWeights {
                    pub remove_gas_reservation: runtime_types::sp_weights::weight_v2::Weight,
                    pub send_user_message_to_mailbox: runtime_types::sp_weights::weight_v2::Weight,
                    pub send_user_message: runtime_types::sp_weights::weight_v2::Weight,
                    pub send_dispatch: runtime_types::sp_weights::weight_v2::Weight,
                    pub wake_message: runtime_types::sp_weights::weight_v2::Weight,
                    pub wake_message_no_wake: runtime_types::sp_weights::weight_v2::Weight,
                    pub remove_from_waitlist: runtime_types::sp_weights::weight_v2::Weight,
                    pub remove_from_mailbox: runtime_types::sp_weights::weight_v2::Weight,
                }
            }
        }
        pub mod pallet_gear_bank {
            use super::runtime_types;
            pub mod pallet {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct BankAccount<_0> {
                    pub gas: _0,
                    pub value: _0,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "The `Error` enum of this pallet."]
                pub enum Error {
                    #[codec(index = 0)]
                    #[doc = "Insufficient user balance."]
                    InsufficientBalance,
                    #[codec(index = 1)]
                    #[doc = "Insufficient user's bank account gas balance."]
                    InsufficientGasBalance,
                    #[codec(index = 2)]
                    #[doc = "Insufficient user's bank account gas balance."]
                    InsufficientValueBalance,
                    #[codec(index = 3)]
                    #[doc = "Insufficient bank account balance."]
                    #[doc = "**Must be unreachable in Gear main protocol.**"]
                    InsufficientBankBalance,
                    #[codec(index = 4)]
                    #[doc = "Deposit of funds that will not keep bank account alive."]
                    #[doc = "**Must be unreachable in Gear main protocol.**"]
                    InsufficientDeposit,
                    #[codec(index = 5)]
                    #[doc = "Overflow during funds transfer."]
                    #[doc = "**Must be unreachable in Gear main protocol.**"]
                    Overflow,
                }
            }
        }
        pub mod pallet_gear_eth_bridge {
            use super::runtime_types;
            pub mod internal {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum QueueInfo {
                    #[codec(index = 0)]
                    Empty,
                    #[codec(index = 1)]
                    NonEmpty {
                        highest_root: ::subxt::ext::subxt_core::utils::H256,
                        latest_nonce_used: runtime_types::primitive_types::U256,
                    },
                }
            }
            pub mod pallet {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "Contains a variant per dispatchable extrinsic that this pallet has."]
                pub enum Call {
                    #[codec(index = 0)]
                    #[doc = "Root extrinsic that pauses pallet."]
                    #[doc = "When paused, no new messages could be queued."]
                    pause,
                    #[codec(index = 1)]
                    #[doc = "Root extrinsic that unpauses pallet."]
                    #[doc = "When paused, no new messages could be queued."]
                    unpause,
                    #[codec(index = 2)]
                    #[doc = "Extrinsic that inserts message in a bridging queue,"]
                    #[doc = "updating queue merkle root at the end of the block."]
                    send_eth_message {
                        destination: ::subxt::ext::subxt_core::utils::H160,
                        payload: ::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
                    },
                    #[codec(index = 3)]
                    #[doc = "Root extrinsic that sets fee for the transport of messages."]
                    set_fee { fee: ::core::primitive::u128 },
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "Pallet Gear Eth Bridge's error."]
                pub enum Error {
                    #[codec(index = 0)]
                    #[doc = "The error happens when bridge got called before"]
                    #[doc = "proper initialization after deployment."]
                    BridgeIsNotYetInitialized,
                    #[codec(index = 1)]
                    #[doc = "The error happens when bridge got called when paused."]
                    BridgeIsPaused,
                    #[codec(index = 2)]
                    #[doc = "The error happens when bridging message sent with too big payload."]
                    MaxPayloadSizeExceeded,
                    #[codec(index = 3)]
                    #[doc = "The error happens when bridging thorough builtin and message value"]
                    #[doc = "is inapplicable to operation or insufficient."]
                    InsufficientValueApplied,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "Pallet Gear Eth Bridge's event."]
                pub enum Event {
                    #[codec(index = 0)]
                    #[doc = "Grandpa validator's keys set was hashed and set in storage at"]
                    #[doc = "first block of the last session in the era."]
                    AuthoritySetHashChanged(::subxt::ext::subxt_core::utils::H256),
                    #[codec(index = 1)]
                    #[doc = "Authority set hash was reset."]
                    #[doc = ""]
                    #[doc = "Related to bridge clearing on initialization of the second block in a new era."]
                    AuthoritySetReset,
                    #[codec(index = 2)]
                    #[doc = "Optimistically, single-time called event defining that pallet"]
                    #[doc = "got initialized and started processing session changes,"]
                    #[doc = "as well as putting initial zeroed queue merkle root."]
                    BridgeInitialized,
                    #[codec(index = 3)]
                    #[doc = "Bridge was paused and temporary doesn't process any incoming requests."]
                    BridgePaused,
                    #[codec(index = 4)]
                    #[doc = "Bridge was unpaused and from now on processes any incoming requests."]
                    BridgeUnpaused,
                    #[codec(index = 5)]
                    #[doc = "A new message was queued for bridging."]
                    MessageQueued {
                        message: runtime_types::pallet_gear_eth_bridge_primitives::EthMessage,
                        hash: ::subxt::ext::subxt_core::utils::H256,
                    },
                    #[codec(index = 6)]
                    #[doc = "Merkle root of the queue changed: new messages queued within the block."]
                    QueueMerkleRootChanged {
                        queue_id: ::core::primitive::u64,
                        root: ::subxt::ext::subxt_core::utils::H256,
                    },
                    #[codec(index = 7)]
                    #[doc = "Queue was reset."]
                    #[doc = ""]
                    #[doc = "Related to bridge clearing on initialization of the second block in a new era."]
                    QueueReset,
                }
            }
        }
        pub mod pallet_gear_eth_bridge_primitives {
            use super::runtime_types;
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub struct EthMessage {
                pub nonce: runtime_types::primitive_types::U256,
                pub source: ::subxt::ext::subxt_core::utils::H256,
                pub destination: ::subxt::ext::subxt_core::utils::H160,
                pub payload: ::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
            }
        }
        pub mod pallet_gear_gas {
            use super::runtime_types;
            pub mod pallet {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "The `Error` enum of this pallet."]
                pub enum Error {
                    #[codec(index = 0)]
                    Forbidden,
                    #[codec(index = 1)]
                    NodeAlreadyExists,
                    #[codec(index = 2)]
                    InsufficientBalance,
                    #[codec(index = 3)]
                    NodeNotFound,
                    #[codec(index = 4)]
                    NodeWasConsumed,
                    #[codec(index = 5)]
                    #[doc = "Errors stating that gas tree has been invalidated"]
                    ParentIsLost,
                    #[codec(index = 6)]
                    ParentHasNoChildren,
                    #[codec(index = 7)]
                    #[doc = "Output of `Tree::consume` procedure that wasn't expected."]
                    #[doc = ""]
                    #[doc = "Outputs of consumption procedure are determined. The error is returned"]
                    #[doc = "when unexpected one occurred. That signals, that algorithm works wrong"]
                    #[doc = "and expected invariants are not correct."]
                    UnexpectedConsumeOutput,
                    #[codec(index = 8)]
                    #[doc = "Node type that can't occur if algorithm work well"]
                    UnexpectedNodeType,
                    #[codec(index = 9)]
                    #[doc = "Value must have been caught, but was missed or blocked (for more info see `ValueNode::catch_value`)."]
                    ValueIsNotCaught,
                    #[codec(index = 10)]
                    #[doc = "Value must have been caught or moved upstream, but was blocked (for more info see `ValueNode::catch_value`)."]
                    ValueIsBlocked,
                    #[codec(index = 11)]
                    #[doc = "Value must have been blocked, but was either moved or caught (for more info see `ValueNode::catch_value`)."]
                    ValueIsNotBlocked,
                    #[codec(index = 12)]
                    #[doc = "`GasTree::consume` called on node, which has some balance locked."]
                    ConsumedWithLock,
                    #[codec(index = 13)]
                    #[doc = "`GasTree::consume` called on node, which has some system reservation."]
                    ConsumedWithSystemReservation,
                    #[codec(index = 14)]
                    #[doc = "`GasTree::create` called with some value amount leading to"]
                    #[doc = "the total value overflow."]
                    TotalValueIsOverflowed,
                    #[codec(index = 15)]
                    #[doc = "Either `GasTree::consume` or `GasTree::spent` called on a node creating"]
                    #[doc = "negative imbalance which leads to the total value drop below 0."]
                    TotalValueIsUnderflowed,
                }
            }
        }
        pub mod pallet_gear_messenger {
            use super::runtime_types;
            pub mod pallet {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "The `Error` enum of this pallet."]
                pub enum Error {
                    #[codec(index = 0)]
                    #[doc = "Occurs when given key already exists in queue."]
                    QueueDuplicateKey,
                    #[codec(index = 1)]
                    #[doc = "Occurs when queue's element wasn't found in storage."]
                    QueueElementNotFound,
                    #[codec(index = 2)]
                    #[doc = "Occurs when queue's head should contain value,"]
                    #[doc = "but it's empty for some reason."]
                    QueueHeadShouldBeSet,
                    #[codec(index = 3)]
                    #[doc = "Occurs when queue's head should be empty,"]
                    #[doc = "but it contains value for some reason."]
                    QueueHeadShouldNotBeSet,
                    #[codec(index = 4)]
                    #[doc = "Occurs when queue's tail element contains link"]
                    #[doc = "to the next element."]
                    QueueTailHasNextKey,
                    #[codec(index = 5)]
                    #[doc = "Occurs when while searching queue's pre-tail,"]
                    #[doc = "element wasn't found."]
                    QueueTailParentNotFound,
                    #[codec(index = 6)]
                    #[doc = "Occurs when queue's tail should contain value,"]
                    #[doc = "but it's empty for some reason."]
                    QueueTailShouldBeSet,
                    #[codec(index = 7)]
                    #[doc = "Occurs when queue's tail should be empty,"]
                    #[doc = "but it contains value for some reason."]
                    QueueTailShouldNotBeSet,
                    #[codec(index = 8)]
                    #[doc = "Occurs when given value already exists in mailbox."]
                    MailboxDuplicateKey,
                    #[codec(index = 9)]
                    #[doc = "Occurs when mailbox's element wasn't found in storage."]
                    MailboxElementNotFound,
                    #[codec(index = 10)]
                    #[doc = "Occurs when given value already exists in waitlist."]
                    WaitlistDuplicateKey,
                    #[codec(index = 11)]
                    #[doc = "Occurs when waitlist's element wasn't found in storage."]
                    WaitlistElementNotFound,
                }
            }
        }
        pub mod pallet_gear_payment {
            use super::runtime_types;
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub struct CustomChargeTransactionPayment<_0>(
                pub runtime_types::pallet_transaction_payment::ChargeTransactionPayment,
                #[codec(skip)] pub ::core::marker::PhantomData<_0>,
            );
        }
        pub mod pallet_gear_program {
            use super::runtime_types;
            pub mod pallet {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "The `Error` enum of this pallet."]
                pub enum Error {
                    #[codec(index = 0)]
                    DuplicateItem,
                    #[codec(index = 1)]
                    ProgramNotFound,
                    #[codec(index = 2)]
                    NotActiveProgram,
                    #[codec(index = 3)]
                    CannotFindDataForPage,
                    #[codec(index = 4)]
                    ProgramCodeNotFound,
                }
            }
        }
        pub mod pallet_gear_scheduler {
            use super::runtime_types;
            pub mod pallet {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "The `Error` enum of this pallet."]
                pub enum Error {
                    #[codec(index = 0)]
                    #[doc = "Occurs when given task already exists in task pool."]
                    DuplicateTask,
                    #[codec(index = 1)]
                    #[doc = "Occurs when task wasn't found in storage."]
                    TaskNotFound,
                }
            }
        }
        pub mod pallet_gear_staking_rewards {
            use super::runtime_types;
            pub mod extension {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct StakingBlackList;
            }
            pub mod pallet {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "Contains a variant per dispatchable extrinsic that this pallet has."]
                pub enum Call {
                    #[codec(index = 0)]
                    refill { value: ::core::primitive::u128 },
                    #[codec(index = 1)]
                    force_refill {
                        from: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                        value: ::core::primitive::u128,
                    },
                    #[codec(index = 2)]
                    withdraw {
                        to: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                        value: ::core::primitive::u128,
                    },
                    #[codec(index = 3)]
                    set_target_inflation {
                        p: ::core::primitive::u64,
                        n: ::core::primitive::u64,
                    },
                    #[codec(index = 4)]
                    set_ideal_staking_ratio {
                        p: ::core::primitive::u64,
                        n: ::core::primitive::u64,
                    },
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "Error for the staking rewards pallet."]
                pub enum Error {
                    #[codec(index = 0)]
                    #[doc = "Pool not replenished due to error."]
                    FailureToRefillPool,
                    #[codec(index = 1)]
                    #[doc = "Failure to withdraw funds from the rewards pool."]
                    FailureToWithdrawFromPool,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "The `Event` enum of this pallet"]
                pub enum Event {
                    #[codec(index = 0)]
                    #[doc = "Deposited to the pool."]
                    Deposited { amount: ::core::primitive::u128 },
                    #[codec(index = 1)]
                    #[doc = "Transferred from the pool to an external account."]
                    Withdrawn { amount: ::core::primitive::u128 },
                    #[codec(index = 2)]
                    #[doc = "Burned from the pool."]
                    Burned { amount: ::core::primitive::u128 },
                    #[codec(index = 3)]
                    #[doc = "Minted to the pool."]
                    Minted { amount: ::core::primitive::u128 },
                    #[codec(index = 4)]
                    #[doc = "Target inflation changed."]
                    TargetInflationChanged {
                        value: runtime_types::sp_arithmetic::per_things::Perquintill,
                    },
                    #[codec(index = 5)]
                    #[doc = "Ideal staking ratio changed."]
                    IdealStakingRatioChanged {
                        value: runtime_types::sp_arithmetic::per_things::Perquintill,
                    },
                }
            }
        }
        pub mod pallet_gear_voucher {
            use super::runtime_types;
            pub mod internal {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum PrepaidCall<_0> {
                    #[codec(index = 0)]
                    SendMessage {
                        destination: runtime_types::gprimitives::ActorId,
                        payload: ::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
                        gas_limit: ::core::primitive::u64,
                        value: _0,
                        keep_alive: ::core::primitive::bool,
                    },
                    #[codec(index = 1)]
                    SendReply {
                        reply_to_id: runtime_types::gprimitives::MessageId,
                        payload: ::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
                        gas_limit: ::core::primitive::u64,
                        value: _0,
                        keep_alive: ::core::primitive::bool,
                    },
                    #[codec(index = 2)]
                    UploadCode {
                        code: ::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
                    },
                    #[codec(index = 3)]
                    DeclineVoucher,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct VoucherId(pub [::core::primitive::u8; 32usize]);
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct VoucherInfo<_0, _1> {
                    pub owner: _0,
                    pub programs: ::core::option::Option<
                        ::subxt::ext::subxt_core::alloc::vec::Vec<
                            runtime_types::gprimitives::ActorId,
                        >,
                    >,
                    pub code_uploading: ::core::primitive::bool,
                    pub expiry: _1,
                }
            }
            pub mod pallet {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "Contains a variant per dispatchable extrinsic that this pallet has."]
                pub enum Call {
                    #[codec(index = 0)]
                    #[doc = "Issue a new voucher."]
                    #[doc = ""]
                    #[doc = "Deposits event `VoucherIssued`, that contains `VoucherId` to be"]
                    #[doc = "used by spender for balance-less on-chain interactions."]
                    #[doc = ""]
                    #[doc = "Arguments:"]
                    #[doc = "* spender:  user id that is eligible to use the voucher;"]
                    #[doc = "* balance:  voucher balance could be used for transactions"]
                    #[doc = "            fees and gas;"]
                    #[doc = "* programs: pool of programs spender can interact with,"]
                    #[doc = "            if None - means any program,"]
                    #[doc = "            limited by Config param;"]
                    #[doc = "* code_uploading:"]
                    #[doc = "            allow voucher to be used as payer for `upload_code`"]
                    #[doc = "            transactions fee;"]
                    #[doc = "* duration: amount of blocks voucher could be used by spender"]
                    #[doc = "            and couldn't be revoked by owner."]
                    #[doc = "            Must be out in [MinDuration; MaxDuration] constants."]
                    #[doc = "            Expiration block of the voucher calculates as:"]
                    #[doc = "            current bn (extrinsic exec bn) + duration + 1."]
                    issue {
                        spender: ::subxt::ext::subxt_core::utils::AccountId32,
                        balance: ::core::primitive::u128,
                        programs: ::core::option::Option<
                            ::subxt::ext::subxt_core::alloc::vec::Vec<
                                runtime_types::gprimitives::ActorId,
                            >,
                        >,
                        code_uploading: ::core::primitive::bool,
                        duration: ::core::primitive::u32,
                    },
                    #[codec(index = 1)]
                    #[doc = "Execute prepaid call with given voucher id."]
                    #[doc = ""]
                    #[doc = "Arguments:"]
                    #[doc = "* voucher_id: associated with origin existing vouchers id,"]
                    #[doc = "              that should be used to pay for fees and gas"]
                    #[doc = "              within the call;"]
                    #[doc = "* call:       prepaid call that is requested to execute."]
                    call {
                        voucher_id: runtime_types::pallet_gear_voucher::internal::VoucherId,
                        call: runtime_types::pallet_gear_voucher::internal::PrepaidCall<
                            ::core::primitive::u128,
                        >,
                    },
                    #[codec(index = 2)]
                    #[doc = "Revoke existing voucher."]
                    #[doc = ""]
                    #[doc = "This extrinsic revokes existing voucher, if current block is greater"]
                    #[doc = "than expiration block of the voucher (it is no longer valid)."]
                    #[doc = ""]
                    #[doc = "Currently it means sending of all balance from voucher account to"]
                    #[doc = "voucher owner without voucher removal from storage map, but this"]
                    #[doc = "behavior may change in future, as well as the origin validation:"]
                    #[doc = "only owner is able to revoke voucher now."]
                    #[doc = ""]
                    #[doc = "Arguments:"]
                    #[doc = "* spender:    account id of the voucher spender;"]
                    #[doc = "* voucher_id: voucher id to be revoked."]
                    revoke {
                        spender: ::subxt::ext::subxt_core::utils::AccountId32,
                        voucher_id: runtime_types::pallet_gear_voucher::internal::VoucherId,
                    },
                    #[codec(index = 3)]
                    #[doc = "Update existing voucher."]
                    #[doc = ""]
                    #[doc = "This extrinsic updates existing voucher: it can only extend vouchers"]
                    #[doc = "rights in terms of balance, validity or programs to interact pool."]
                    #[doc = ""]
                    #[doc = "Can only be called by the voucher owner."]
                    #[doc = ""]
                    #[doc = "Arguments:"]
                    #[doc = "* spender:          account id of the voucher spender;"]
                    #[doc = "* voucher_id:       voucher id to be updated;"]
                    #[doc = "* move_ownership:   optionally moves ownership to another account;"]
                    #[doc = "* balance_top_up:   optionally top ups balance of the voucher from"]
                    #[doc = "                    origins balance;"]
                    #[doc = "* append_programs:  optionally extends pool of programs by"]
                    #[doc = "                    `Some(programs_set)` passed or allows"]
                    #[doc = "                    it to interact with any program by"]
                    #[doc = "                    `None` passed;"]
                    #[doc = "* code_uploading:   optionally allows voucher to be used to pay"]
                    #[doc = "                    fees for `upload_code` extrinsics;"]
                    #[doc = "* prolong_duration: optionally increases expiry block number."]
                    #[doc = "                    If voucher is expired, prolongs since current bn."]
                    #[doc = "                    Validity prolongation (since current block number"]
                    #[doc = "                    for expired or since storage written expiry)"]
                    #[doc = "                    should be in [MinDuration; MaxDuration], in other"]
                    #[doc = "                    words voucher couldn't have expiry greater than"]
                    #[doc = "                    current block number + MaxDuration."]
                    update {
                        spender: ::subxt::ext::subxt_core::utils::AccountId32,
                        voucher_id: runtime_types::pallet_gear_voucher::internal::VoucherId,
                        move_ownership:
                            ::core::option::Option<::subxt::ext::subxt_core::utils::AccountId32>,
                        balance_top_up: ::core::option::Option<::core::primitive::u128>,
                        append_programs: ::core::option::Option<
                            ::core::option::Option<
                                ::subxt::ext::subxt_core::alloc::vec::Vec<
                                    runtime_types::gprimitives::ActorId,
                                >,
                            >,
                        >,
                        code_uploading: ::core::option::Option<::core::primitive::bool>,
                        prolong_duration: ::core::option::Option<::core::primitive::u32>,
                    },
                    #[codec(index = 4)]
                    #[doc = "Decline existing and not expired voucher."]
                    #[doc = ""]
                    #[doc = "This extrinsic expires voucher of the caller, if it's still active,"]
                    #[doc = "allowing it to be revoked."]
                    #[doc = ""]
                    #[doc = "Arguments:"]
                    #[doc = "* voucher_id:   voucher id to be declined."]
                    decline {
                        voucher_id: runtime_types::pallet_gear_voucher::internal::VoucherId,
                    },
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "The `Error` enum of this pallet."]
                pub enum Error {
                    #[codec(index = 0)]
                    #[doc = "The origin is not eligible to execute call."]
                    BadOrigin,
                    #[codec(index = 1)]
                    #[doc = "Error trying transfer balance to/from voucher account."]
                    BalanceTransfer,
                    #[codec(index = 2)]
                    #[doc = "Destination program is not in whitelisted set for voucher."]
                    InappropriateDestination,
                    #[codec(index = 3)]
                    #[doc = "Voucher with given identifier doesn't exist for given spender id."]
                    InexistentVoucher,
                    #[codec(index = 4)]
                    #[doc = "Voucher still valid and couldn't be revoked."]
                    IrrevocableYet,
                    #[codec(index = 5)]
                    #[doc = "Try to whitelist more programs than allowed."]
                    MaxProgramsLimitExceeded,
                    #[codec(index = 6)]
                    #[doc = "Failed to query destination of the prepaid call."]
                    UnknownDestination,
                    #[codec(index = 7)]
                    #[doc = "Voucher has expired and couldn't be used."]
                    VoucherExpired,
                    #[codec(index = 8)]
                    #[doc = "Voucher issue/prolongation duration out of [min; max] constants."]
                    DurationOutOfBounds,
                    #[codec(index = 9)]
                    #[doc = "Voucher update function tries to cut voucher ability of code upload."]
                    CodeUploadingEnabled,
                    #[codec(index = 10)]
                    #[doc = "Voucher is disabled for code uploading, but requested."]
                    CodeUploadingDisabled,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "Pallet Gear Voucher event."]
                pub enum Event {
                    #[codec(index = 0)]
                    #[doc = "Voucher has been issued."]
                    VoucherIssued {
                        owner: ::subxt::ext::subxt_core::utils::AccountId32,
                        spender: ::subxt::ext::subxt_core::utils::AccountId32,
                        voucher_id: runtime_types::pallet_gear_voucher::internal::VoucherId,
                    },
                    #[codec(index = 1)]
                    #[doc = "Voucher has been revoked by owner."]
                    #[doc = ""]
                    #[doc = "NOTE: currently means only \"refunded\"."]
                    VoucherRevoked {
                        spender: ::subxt::ext::subxt_core::utils::AccountId32,
                        voucher_id: runtime_types::pallet_gear_voucher::internal::VoucherId,
                    },
                    #[codec(index = 2)]
                    #[doc = "Voucher has been updated."]
                    VoucherUpdated {
                        spender: ::subxt::ext::subxt_core::utils::AccountId32,
                        voucher_id: runtime_types::pallet_gear_voucher::internal::VoucherId,
                        new_owner:
                            ::core::option::Option<::subxt::ext::subxt_core::utils::AccountId32>,
                    },
                    #[codec(index = 3)]
                    #[doc = "Voucher has been declined (set to expired state)."]
                    VoucherDeclined {
                        spender: ::subxt::ext::subxt_core::utils::AccountId32,
                        voucher_id: runtime_types::pallet_gear_voucher::internal::VoucherId,
                    },
                }
            }
        }
        pub mod pallet_grandpa {
            use super::runtime_types;
            pub mod pallet {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "Contains a variant per dispatchable extrinsic that this pallet has."]
                pub enum Call {
                    #[codec(index = 0)]
                    #[doc = "Report voter equivocation/misbehavior. This method will verify the"]
                    #[doc = "equivocation proof and validate the given key ownership proof"]
                    #[doc = "against the extracted offender. If both are valid, the offence"]
                    #[doc = "will be reported."]
                    report_equivocation {
                        equivocation_proof: ::subxt::ext::subxt_core::alloc::boxed::Box<
                            runtime_types::sp_consensus_grandpa::EquivocationProof<
                                ::subxt::ext::subxt_core::utils::H256,
                                ::core::primitive::u32,
                            >,
                        >,
                        key_owner_proof: runtime_types::sp_session::MembershipProof,
                    },
                    #[codec(index = 1)]
                    #[doc = "Report voter equivocation/misbehavior. This method will verify the"]
                    #[doc = "equivocation proof and validate the given key ownership proof"]
                    #[doc = "against the extracted offender. If both are valid, the offence"]
                    #[doc = "will be reported."]
                    #[doc = ""]
                    #[doc = "This extrinsic must be called unsigned and it is expected that only"]
                    #[doc = "block authors will call it (validated in `ValidateUnsigned`), as such"]
                    #[doc = "if the block author is defined it will be defined as the equivocation"]
                    #[doc = "reporter."]
                    report_equivocation_unsigned {
                        equivocation_proof: ::subxt::ext::subxt_core::alloc::boxed::Box<
                            runtime_types::sp_consensus_grandpa::EquivocationProof<
                                ::subxt::ext::subxt_core::utils::H256,
                                ::core::primitive::u32,
                            >,
                        >,
                        key_owner_proof: runtime_types::sp_session::MembershipProof,
                    },
                    #[codec(index = 2)]
                    #[doc = "Note that the current authority set of the GRANDPA finality gadget has stalled."]
                    #[doc = ""]
                    #[doc = "This will trigger a forced authority set change at the beginning of the next session, to"]
                    #[doc = "be enacted `delay` blocks after that. The `delay` should be high enough to safely assume"]
                    #[doc = "that the block signalling the forced change will not be re-orged e.g. 1000 blocks."]
                    #[doc = "The block production rate (which may be slowed down because of finality lagging) should"]
                    #[doc = "be taken into account when choosing the `delay`. The GRANDPA voters based on the new"]
                    #[doc = "authority will start voting on top of `best_finalized_block_number` for new finalized"]
                    #[doc = "blocks. `best_finalized_block_number` should be the highest of the latest finalized"]
                    #[doc = "block of all validators of the new authority set."]
                    #[doc = ""]
                    #[doc = "Only callable by root."]
                    note_stalled {
                        delay: ::core::primitive::u32,
                        best_finalized_block_number: ::core::primitive::u32,
                    },
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "The `Error` enum of this pallet."]
                pub enum Error {
                    #[codec(index = 0)]
                    #[doc = "Attempt to signal GRANDPA pause when the authority set isn't live"]
                    #[doc = "(either paused or already pending pause)."]
                    PauseFailed,
                    #[codec(index = 1)]
                    #[doc = "Attempt to signal GRANDPA resume when the authority set isn't paused"]
                    #[doc = "(either live or already pending resume)."]
                    ResumeFailed,
                    #[codec(index = 2)]
                    #[doc = "Attempt to signal GRANDPA change with one already pending."]
                    ChangePending,
                    #[codec(index = 3)]
                    #[doc = "Cannot signal forced change so soon after last."]
                    TooSoon,
                    #[codec(index = 4)]
                    #[doc = "A key ownership proof provided as part of an equivocation report is invalid."]
                    InvalidKeyOwnershipProof,
                    #[codec(index = 5)]
                    #[doc = "An equivocation proof provided as part of an equivocation report is invalid."]
                    InvalidEquivocationProof,
                    #[codec(index = 6)]
                    #[doc = "A given equivocation report is valid but already previously reported."]
                    DuplicateOffenceReport,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "The `Event` enum of this pallet"]
                pub enum Event {
                    #[codec(index = 0)]
                    #[doc = "New authority set has been applied."]
                    NewAuthorities {
                        authority_set: ::subxt::ext::subxt_core::alloc::vec::Vec<(
                            runtime_types::sp_consensus_grandpa::app::Public,
                            ::core::primitive::u64,
                        )>,
                    },
                    #[codec(index = 1)]
                    #[doc = "Current authority set has been paused."]
                    Paused,
                    #[codec(index = 2)]
                    #[doc = "Current authority set has been resumed."]
                    Resumed,
                }
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub struct StoredPendingChange<_0> {
                pub scheduled_at: _0,
                pub delay: _0,
                pub next_authorities:
                    runtime_types::bounded_collections::weak_bounded_vec::WeakBoundedVec<(
                        runtime_types::sp_consensus_grandpa::app::Public,
                        ::core::primitive::u64,
                    )>,
                pub forced: ::core::option::Option<_0>,
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub enum StoredState<_0> {
                #[codec(index = 0)]
                Live,
                #[codec(index = 1)]
                PendingPause { scheduled_at: _0, delay: _0 },
                #[codec(index = 2)]
                Paused,
                #[codec(index = 3)]
                PendingResume { scheduled_at: _0, delay: _0 },
            }
        }
        pub mod pallet_identity {
            use super::runtime_types;
            pub mod legacy {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct IdentityInfo {
                    pub additional: runtime_types::bounded_collections::bounded_vec::BoundedVec<(
                        runtime_types::pallet_identity::types::Data,
                        runtime_types::pallet_identity::types::Data,
                    )>,
                    pub display: runtime_types::pallet_identity::types::Data,
                    pub legal: runtime_types::pallet_identity::types::Data,
                    pub web: runtime_types::pallet_identity::types::Data,
                    pub riot: runtime_types::pallet_identity::types::Data,
                    pub email: runtime_types::pallet_identity::types::Data,
                    pub pgp_fingerprint: ::core::option::Option<[::core::primitive::u8; 20usize]>,
                    pub image: runtime_types::pallet_identity::types::Data,
                    pub twitter: runtime_types::pallet_identity::types::Data,
                }
            }
            pub mod pallet {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "Identity pallet declaration."]
                pub enum Call {
                    #[codec(index = 0)]
                    #[doc = "Add a registrar to the system."]
                    #[doc = ""]
                    #[doc = "The dispatch origin for this call must be `T::RegistrarOrigin`."]
                    #[doc = ""]
                    #[doc = "- `account`: the account of the registrar."]
                    #[doc = ""]
                    #[doc = "Emits `RegistrarAdded` if successful."]
                    add_registrar {
                        account: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                    },
                    #[codec(index = 1)]
                    #[doc = "Set an account's identity information and reserve the appropriate deposit."]
                    #[doc = ""]
                    #[doc = "If the account already has identity information, the deposit is taken as part payment"]
                    #[doc = "for the new deposit."]
                    #[doc = ""]
                    #[doc = "The dispatch origin for this call must be _Signed_."]
                    #[doc = ""]
                    #[doc = "- `info`: The identity information."]
                    #[doc = ""]
                    #[doc = "Emits `IdentitySet` if successful."]
                    set_identity {
                        info: ::subxt::ext::subxt_core::alloc::boxed::Box<
                            runtime_types::pallet_identity::legacy::IdentityInfo,
                        >,
                    },
                    #[codec(index = 2)]
                    #[doc = "Set the sub-accounts of the sender."]
                    #[doc = ""]
                    #[doc = "Payment: Any aggregate balance reserved by previous `set_subs` calls will be returned"]
                    #[doc = "and an amount `SubAccountDeposit` will be reserved for each item in `subs`."]
                    #[doc = ""]
                    #[doc = "The dispatch origin for this call must be _Signed_ and the sender must have a registered"]
                    #[doc = "identity."]
                    #[doc = ""]
                    #[doc = "- `subs`: The identity's (new) sub-accounts."]
                    set_subs {
                        subs: ::subxt::ext::subxt_core::alloc::vec::Vec<(
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            runtime_types::pallet_identity::types::Data,
                        )>,
                    },
                    #[codec(index = 3)]
                    #[doc = "Clear an account's identity info and all sub-accounts and return all deposits."]
                    #[doc = ""]
                    #[doc = "Payment: All reserved balances on the account are returned."]
                    #[doc = ""]
                    #[doc = "The dispatch origin for this call must be _Signed_ and the sender must have a registered"]
                    #[doc = "identity."]
                    #[doc = ""]
                    #[doc = "Emits `IdentityCleared` if successful."]
                    clear_identity,
                    #[codec(index = 4)]
                    #[doc = "Request a judgement from a registrar."]
                    #[doc = ""]
                    #[doc = "Payment: At most `max_fee` will be reserved for payment to the registrar if judgement"]
                    #[doc = "given."]
                    #[doc = ""]
                    #[doc = "The dispatch origin for this call must be _Signed_ and the sender must have a"]
                    #[doc = "registered identity."]
                    #[doc = ""]
                    #[doc = "- `reg_index`: The index of the registrar whose judgement is requested."]
                    #[doc = "- `max_fee`: The maximum fee that may be paid. This should just be auto-populated as:"]
                    #[doc = ""]
                    #[doc = "```nocompile"]
                    #[doc = "Registrars::<T>::get().get(reg_index).unwrap().fee"]
                    #[doc = "```"]
                    #[doc = ""]
                    #[doc = "Emits `JudgementRequested` if successful."]
                    request_judgement {
                        #[codec(compact)]
                        reg_index: ::core::primitive::u32,
                        #[codec(compact)]
                        max_fee: ::core::primitive::u128,
                    },
                    #[codec(index = 5)]
                    #[doc = "Cancel a previous request."]
                    #[doc = ""]
                    #[doc = "Payment: A previously reserved deposit is returned on success."]
                    #[doc = ""]
                    #[doc = "The dispatch origin for this call must be _Signed_ and the sender must have a"]
                    #[doc = "registered identity."]
                    #[doc = ""]
                    #[doc = "- `reg_index`: The index of the registrar whose judgement is no longer requested."]
                    #[doc = ""]
                    #[doc = "Emits `JudgementUnrequested` if successful."]
                    cancel_request { reg_index: ::core::primitive::u32 },
                    #[codec(index = 6)]
                    #[doc = "Set the fee required for a judgement to be requested from a registrar."]
                    #[doc = ""]
                    #[doc = "The dispatch origin for this call must be _Signed_ and the sender must be the account"]
                    #[doc = "of the registrar whose index is `index`."]
                    #[doc = ""]
                    #[doc = "- `index`: the index of the registrar whose fee is to be set."]
                    #[doc = "- `fee`: the new fee."]
                    set_fee {
                        #[codec(compact)]
                        index: ::core::primitive::u32,
                        #[codec(compact)]
                        fee: ::core::primitive::u128,
                    },
                    #[codec(index = 7)]
                    #[doc = "Change the account associated with a registrar."]
                    #[doc = ""]
                    #[doc = "The dispatch origin for this call must be _Signed_ and the sender must be the account"]
                    #[doc = "of the registrar whose index is `index`."]
                    #[doc = ""]
                    #[doc = "- `index`: the index of the registrar whose fee is to be set."]
                    #[doc = "- `new`: the new account ID."]
                    set_account_id {
                        #[codec(compact)]
                        index: ::core::primitive::u32,
                        new: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                    },
                    #[codec(index = 8)]
                    #[doc = "Set the field information for a registrar."]
                    #[doc = ""]
                    #[doc = "The dispatch origin for this call must be _Signed_ and the sender must be the account"]
                    #[doc = "of the registrar whose index is `index`."]
                    #[doc = ""]
                    #[doc = "- `index`: the index of the registrar whose fee is to be set."]
                    #[doc = "- `fields`: the fields that the registrar concerns themselves with."]
                    set_fields {
                        #[codec(compact)]
                        index: ::core::primitive::u32,
                        fields: ::core::primitive::u64,
                    },
                    #[codec(index = 9)]
                    #[doc = "Provide a judgement for an account's identity."]
                    #[doc = ""]
                    #[doc = "The dispatch origin for this call must be _Signed_ and the sender must be the account"]
                    #[doc = "of the registrar whose index is `reg_index`."]
                    #[doc = ""]
                    #[doc = "- `reg_index`: the index of the registrar whose judgement is being made."]
                    #[doc = "- `target`: the account whose identity the judgement is upon. This must be an account"]
                    #[doc = "  with a registered identity."]
                    #[doc = "- `judgement`: the judgement of the registrar of index `reg_index` about `target`."]
                    #[doc = "- `identity`: The hash of the [`IdentityInformationProvider`] for that the judgement is"]
                    #[doc = "  provided."]
                    #[doc = ""]
                    #[doc = "Note: Judgements do not apply to a username."]
                    #[doc = ""]
                    #[doc = "Emits `JudgementGiven` if successful."]
                    provide_judgement {
                        #[codec(compact)]
                        reg_index: ::core::primitive::u32,
                        target: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                        judgement: runtime_types::pallet_identity::types::Judgement<
                            ::core::primitive::u128,
                        >,
                        identity: ::subxt::ext::subxt_core::utils::H256,
                    },
                    #[codec(index = 10)]
                    #[doc = "Remove an account's identity and sub-account information and slash the deposits."]
                    #[doc = ""]
                    #[doc = "Payment: Reserved balances from `set_subs` and `set_identity` are slashed and handled by"]
                    #[doc = "`Slash`. Verification request deposits are not returned; they should be cancelled"]
                    #[doc = "manually using `cancel_request`."]
                    #[doc = ""]
                    #[doc = "The dispatch origin for this call must match `T::ForceOrigin`."]
                    #[doc = ""]
                    #[doc = "- `target`: the account whose identity the judgement is upon. This must be an account"]
                    #[doc = "  with a registered identity."]
                    #[doc = ""]
                    #[doc = "Emits `IdentityKilled` if successful."]
                    kill_identity {
                        target: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                    },
                    #[codec(index = 11)]
                    #[doc = "Add the given account to the sender's subs."]
                    #[doc = ""]
                    #[doc = "Payment: Balance reserved by a previous `set_subs` call for one sub will be repatriated"]
                    #[doc = "to the sender."]
                    #[doc = ""]
                    #[doc = "The dispatch origin for this call must be _Signed_ and the sender must have a registered"]
                    #[doc = "sub identity of `sub`."]
                    add_sub {
                        sub: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                        data: runtime_types::pallet_identity::types::Data,
                    },
                    #[codec(index = 12)]
                    #[doc = "Alter the associated name of the given sub-account."]
                    #[doc = ""]
                    #[doc = "The dispatch origin for this call must be _Signed_ and the sender must have a registered"]
                    #[doc = "sub identity of `sub`."]
                    rename_sub {
                        sub: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                        data: runtime_types::pallet_identity::types::Data,
                    },
                    #[codec(index = 13)]
                    #[doc = "Remove the given account from the sender's subs."]
                    #[doc = ""]
                    #[doc = "Payment: Balance reserved by a previous `set_subs` call for one sub will be repatriated"]
                    #[doc = "to the sender."]
                    #[doc = ""]
                    #[doc = "The dispatch origin for this call must be _Signed_ and the sender must have a registered"]
                    #[doc = "sub identity of `sub`."]
                    remove_sub {
                        sub: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                    },
                    #[codec(index = 14)]
                    #[doc = "Remove the sender as a sub-account."]
                    #[doc = ""]
                    #[doc = "Payment: Balance reserved by a previous `set_subs` call for one sub will be repatriated"]
                    #[doc = "to the sender (*not* the original depositor)."]
                    #[doc = ""]
                    #[doc = "The dispatch origin for this call must be _Signed_ and the sender must have a registered"]
                    #[doc = "super-identity."]
                    #[doc = ""]
                    #[doc = "NOTE: This should not normally be used, but is provided in the case that the non-"]
                    #[doc = "controller of an account is maliciously registered as a sub-account."]
                    quit_sub,
                    #[codec(index = 15)]
                    #[doc = "Add an `AccountId` with permission to grant usernames with a given `suffix` appended."]
                    #[doc = ""]
                    #[doc = "The authority can grant up to `allocation` usernames. To top up their allocation, they"]
                    #[doc = "should just issue (or request via governance) a new `add_username_authority` call."]
                    add_username_authority {
                        authority: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                        suffix: ::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
                        allocation: ::core::primitive::u32,
                    },
                    #[codec(index = 16)]
                    #[doc = "Remove `authority` from the username authorities."]
                    remove_username_authority {
                        authority: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                    },
                    #[codec(index = 17)]
                    #[doc = "Set the username for `who`. Must be called by a username authority."]
                    #[doc = ""]
                    #[doc = "The authority must have an `allocation`. Users can either pre-sign their usernames or"]
                    #[doc = "accept them later."]
                    #[doc = ""]
                    #[doc = "Usernames must:"]
                    #[doc = "  - Only contain lowercase ASCII characters or digits."]
                    #[doc = "  - When combined with the suffix of the issuing authority be _less than_ the"]
                    #[doc = "    `MaxUsernameLength`."]
                    set_username_for {
                        who: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                        username: ::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
                        signature:
                            ::core::option::Option<runtime_types::sp_runtime::MultiSignature>,
                    },
                    #[codec(index = 18)]
                    #[doc = "Accept a given username that an `authority` granted. The call must include the full"]
                    #[doc = "username, as in `username.suffix`."]
                    accept_username {
                        username: runtime_types::bounded_collections::bounded_vec::BoundedVec<
                            ::core::primitive::u8,
                        >,
                    },
                    #[codec(index = 19)]
                    #[doc = "Remove an expired username approval. The username was approved by an authority but never"]
                    #[doc = "accepted by the user and must now be beyond its expiration. The call must include the"]
                    #[doc = "full username, as in `username.suffix`."]
                    remove_expired_approval {
                        username: runtime_types::bounded_collections::bounded_vec::BoundedVec<
                            ::core::primitive::u8,
                        >,
                    },
                    #[codec(index = 20)]
                    #[doc = "Set a given username as the primary. The username should include the suffix."]
                    set_primary_username {
                        username: runtime_types::bounded_collections::bounded_vec::BoundedVec<
                            ::core::primitive::u8,
                        >,
                    },
                    #[codec(index = 21)]
                    #[doc = "Remove a username that corresponds to an account with no identity. Exists when a user"]
                    #[doc = "gets a username but then calls `clear_identity`."]
                    remove_dangling_username {
                        username: runtime_types::bounded_collections::bounded_vec::BoundedVec<
                            ::core::primitive::u8,
                        >,
                    },
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "The `Error` enum of this pallet."]
                pub enum Error {
                    #[codec(index = 0)]
                    #[doc = "Too many subs-accounts."]
                    TooManySubAccounts,
                    #[codec(index = 1)]
                    #[doc = "Account isn't found."]
                    NotFound,
                    #[codec(index = 2)]
                    #[doc = "Account isn't named."]
                    NotNamed,
                    #[codec(index = 3)]
                    #[doc = "Empty index."]
                    EmptyIndex,
                    #[codec(index = 4)]
                    #[doc = "Fee is changed."]
                    FeeChanged,
                    #[codec(index = 5)]
                    #[doc = "No identity found."]
                    NoIdentity,
                    #[codec(index = 6)]
                    #[doc = "Sticky judgement."]
                    StickyJudgement,
                    #[codec(index = 7)]
                    #[doc = "Judgement given."]
                    JudgementGiven,
                    #[codec(index = 8)]
                    #[doc = "Invalid judgement."]
                    InvalidJudgement,
                    #[codec(index = 9)]
                    #[doc = "The index is invalid."]
                    InvalidIndex,
                    #[codec(index = 10)]
                    #[doc = "The target is invalid."]
                    InvalidTarget,
                    #[codec(index = 11)]
                    #[doc = "Maximum amount of registrars reached. Cannot add any more."]
                    TooManyRegistrars,
                    #[codec(index = 12)]
                    #[doc = "Account ID is already named."]
                    AlreadyClaimed,
                    #[codec(index = 13)]
                    #[doc = "Sender is not a sub-account."]
                    NotSub,
                    #[codec(index = 14)]
                    #[doc = "Sub-account isn't owned by sender."]
                    NotOwned,
                    #[codec(index = 15)]
                    #[doc = "The provided judgement was for a different identity."]
                    JudgementForDifferentIdentity,
                    #[codec(index = 16)]
                    #[doc = "Error that occurs when there is an issue paying for judgement."]
                    JudgementPaymentFailed,
                    #[codec(index = 17)]
                    #[doc = "The provided suffix is too long."]
                    InvalidSuffix,
                    #[codec(index = 18)]
                    #[doc = "The sender does not have permission to issue a username."]
                    NotUsernameAuthority,
                    #[codec(index = 19)]
                    #[doc = "The authority cannot allocate any more usernames."]
                    NoAllocation,
                    #[codec(index = 20)]
                    #[doc = "The signature on a username was not valid."]
                    InvalidSignature,
                    #[codec(index = 21)]
                    #[doc = "Setting this username requires a signature, but none was provided."]
                    RequiresSignature,
                    #[codec(index = 22)]
                    #[doc = "The username does not meet the requirements."]
                    InvalidUsername,
                    #[codec(index = 23)]
                    #[doc = "The username is already taken."]
                    UsernameTaken,
                    #[codec(index = 24)]
                    #[doc = "The requested username does not exist."]
                    NoUsername,
                    #[codec(index = 25)]
                    #[doc = "The username cannot be forcefully removed because it can still be accepted."]
                    NotExpired,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "The `Event` enum of this pallet"]
                pub enum Event {
                    #[codec(index = 0)]
                    #[doc = "A name was set or reset (which will remove all judgements)."]
                    IdentitySet {
                        who: ::subxt::ext::subxt_core::utils::AccountId32,
                    },
                    #[codec(index = 1)]
                    #[doc = "A name was cleared, and the given balance returned."]
                    IdentityCleared {
                        who: ::subxt::ext::subxt_core::utils::AccountId32,
                        deposit: ::core::primitive::u128,
                    },
                    #[codec(index = 2)]
                    #[doc = "A name was removed and the given balance slashed."]
                    IdentityKilled {
                        who: ::subxt::ext::subxt_core::utils::AccountId32,
                        deposit: ::core::primitive::u128,
                    },
                    #[codec(index = 3)]
                    #[doc = "A judgement was asked from a registrar."]
                    JudgementRequested {
                        who: ::subxt::ext::subxt_core::utils::AccountId32,
                        registrar_index: ::core::primitive::u32,
                    },
                    #[codec(index = 4)]
                    #[doc = "A judgement request was retracted."]
                    JudgementUnrequested {
                        who: ::subxt::ext::subxt_core::utils::AccountId32,
                        registrar_index: ::core::primitive::u32,
                    },
                    #[codec(index = 5)]
                    #[doc = "A judgement was given by a registrar."]
                    JudgementGiven {
                        target: ::subxt::ext::subxt_core::utils::AccountId32,
                        registrar_index: ::core::primitive::u32,
                    },
                    #[codec(index = 6)]
                    #[doc = "A registrar was added."]
                    RegistrarAdded {
                        registrar_index: ::core::primitive::u32,
                    },
                    #[codec(index = 7)]
                    #[doc = "A sub-identity was added to an identity and the deposit paid."]
                    SubIdentityAdded {
                        sub: ::subxt::ext::subxt_core::utils::AccountId32,
                        main: ::subxt::ext::subxt_core::utils::AccountId32,
                        deposit: ::core::primitive::u128,
                    },
                    #[codec(index = 8)]
                    #[doc = "A sub-identity was removed from an identity and the deposit freed."]
                    SubIdentityRemoved {
                        sub: ::subxt::ext::subxt_core::utils::AccountId32,
                        main: ::subxt::ext::subxt_core::utils::AccountId32,
                        deposit: ::core::primitive::u128,
                    },
                    #[codec(index = 9)]
                    #[doc = "A sub-identity was cleared, and the given deposit repatriated from the"]
                    #[doc = "main identity account to the sub-identity account."]
                    SubIdentityRevoked {
                        sub: ::subxt::ext::subxt_core::utils::AccountId32,
                        main: ::subxt::ext::subxt_core::utils::AccountId32,
                        deposit: ::core::primitive::u128,
                    },
                    #[codec(index = 10)]
                    #[doc = "A username authority was added."]
                    AuthorityAdded {
                        authority: ::subxt::ext::subxt_core::utils::AccountId32,
                    },
                    #[codec(index = 11)]
                    #[doc = "A username authority was removed."]
                    AuthorityRemoved {
                        authority: ::subxt::ext::subxt_core::utils::AccountId32,
                    },
                    #[codec(index = 12)]
                    #[doc = "A username was set for `who`."]
                    UsernameSet {
                        who: ::subxt::ext::subxt_core::utils::AccountId32,
                        username: runtime_types::bounded_collections::bounded_vec::BoundedVec<
                            ::core::primitive::u8,
                        >,
                    },
                    #[codec(index = 13)]
                    #[doc = "A username was queued, but `who` must accept it prior to `expiration`."]
                    UsernameQueued {
                        who: ::subxt::ext::subxt_core::utils::AccountId32,
                        username: runtime_types::bounded_collections::bounded_vec::BoundedVec<
                            ::core::primitive::u8,
                        >,
                        expiration: ::core::primitive::u32,
                    },
                    #[codec(index = 14)]
                    #[doc = "A queued username passed its expiration without being claimed and was removed."]
                    PreapprovalExpired {
                        whose: ::subxt::ext::subxt_core::utils::AccountId32,
                    },
                    #[codec(index = 15)]
                    #[doc = "A username was set as a primary and can be looked up from `who`."]
                    PrimaryUsernameSet {
                        who: ::subxt::ext::subxt_core::utils::AccountId32,
                        username: runtime_types::bounded_collections::bounded_vec::BoundedVec<
                            ::core::primitive::u8,
                        >,
                    },
                    #[codec(index = 16)]
                    #[doc = "A dangling username (as in, a username corresponding to an account that has removed its"]
                    #[doc = "identity) has been removed."]
                    DanglingUsernameRemoved {
                        who: ::subxt::ext::subxt_core::utils::AccountId32,
                        username: runtime_types::bounded_collections::bounded_vec::BoundedVec<
                            ::core::primitive::u8,
                        >,
                    },
                }
            }
            pub mod types {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct AuthorityProperties<_0> {
                    pub suffix: _0,
                    pub allocation: ::core::primitive::u32,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum Data {
                    #[codec(index = 0)]
                    None,
                    #[codec(index = 1)]
                    Raw0([::core::primitive::u8; 0usize]),
                    #[codec(index = 2)]
                    Raw1([::core::primitive::u8; 1usize]),
                    #[codec(index = 3)]
                    Raw2([::core::primitive::u8; 2usize]),
                    #[codec(index = 4)]
                    Raw3([::core::primitive::u8; 3usize]),
                    #[codec(index = 5)]
                    Raw4([::core::primitive::u8; 4usize]),
                    #[codec(index = 6)]
                    Raw5([::core::primitive::u8; 5usize]),
                    #[codec(index = 7)]
                    Raw6([::core::primitive::u8; 6usize]),
                    #[codec(index = 8)]
                    Raw7([::core::primitive::u8; 7usize]),
                    #[codec(index = 9)]
                    Raw8([::core::primitive::u8; 8usize]),
                    #[codec(index = 10)]
                    Raw9([::core::primitive::u8; 9usize]),
                    #[codec(index = 11)]
                    Raw10([::core::primitive::u8; 10usize]),
                    #[codec(index = 12)]
                    Raw11([::core::primitive::u8; 11usize]),
                    #[codec(index = 13)]
                    Raw12([::core::primitive::u8; 12usize]),
                    #[codec(index = 14)]
                    Raw13([::core::primitive::u8; 13usize]),
                    #[codec(index = 15)]
                    Raw14([::core::primitive::u8; 14usize]),
                    #[codec(index = 16)]
                    Raw15([::core::primitive::u8; 15usize]),
                    #[codec(index = 17)]
                    Raw16([::core::primitive::u8; 16usize]),
                    #[codec(index = 18)]
                    Raw17([::core::primitive::u8; 17usize]),
                    #[codec(index = 19)]
                    Raw18([::core::primitive::u8; 18usize]),
                    #[codec(index = 20)]
                    Raw19([::core::primitive::u8; 19usize]),
                    #[codec(index = 21)]
                    Raw20([::core::primitive::u8; 20usize]),
                    #[codec(index = 22)]
                    Raw21([::core::primitive::u8; 21usize]),
                    #[codec(index = 23)]
                    Raw22([::core::primitive::u8; 22usize]),
                    #[codec(index = 24)]
                    Raw23([::core::primitive::u8; 23usize]),
                    #[codec(index = 25)]
                    Raw24([::core::primitive::u8; 24usize]),
                    #[codec(index = 26)]
                    Raw25([::core::primitive::u8; 25usize]),
                    #[codec(index = 27)]
                    Raw26([::core::primitive::u8; 26usize]),
                    #[codec(index = 28)]
                    Raw27([::core::primitive::u8; 27usize]),
                    #[codec(index = 29)]
                    Raw28([::core::primitive::u8; 28usize]),
                    #[codec(index = 30)]
                    Raw29([::core::primitive::u8; 29usize]),
                    #[codec(index = 31)]
                    Raw30([::core::primitive::u8; 30usize]),
                    #[codec(index = 32)]
                    Raw31([::core::primitive::u8; 31usize]),
                    #[codec(index = 33)]
                    Raw32([::core::primitive::u8; 32usize]),
                    #[codec(index = 34)]
                    BlakeTwo256([::core::primitive::u8; 32usize]),
                    #[codec(index = 35)]
                    Sha256([::core::primitive::u8; 32usize]),
                    #[codec(index = 36)]
                    Keccak256([::core::primitive::u8; 32usize]),
                    #[codec(index = 37)]
                    ShaThree256([::core::primitive::u8; 32usize]),
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum Judgement<_0> {
                    #[codec(index = 0)]
                    Unknown,
                    #[codec(index = 1)]
                    FeePaid(_0),
                    #[codec(index = 2)]
                    Reasonable,
                    #[codec(index = 3)]
                    KnownGood,
                    #[codec(index = 4)]
                    OutOfDate,
                    #[codec(index = 5)]
                    LowQuality,
                    #[codec(index = 6)]
                    Erroneous,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct RegistrarInfo<_0, _1, _2> {
                    pub account: _1,
                    pub fee: _0,
                    pub fields: _2,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct Registration<_0, _2> {
                    pub judgements: runtime_types::bounded_collections::bounded_vec::BoundedVec<(
                        ::core::primitive::u32,
                        runtime_types::pallet_identity::types::Judgement<_0>,
                    )>,
                    pub deposit: _0,
                    pub info: _2,
                }
            }
        }
        pub mod pallet_im_online {
            use super::runtime_types;
            pub mod pallet {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "Contains a variant per dispatchable extrinsic that this pallet has."]
                pub enum Call {
                    #[codec(index = 0)]
                    #[doc = "## Complexity:"]
                    #[doc = "- `O(K)` where K is length of `Keys` (heartbeat.validators_len)"]
                    #[doc = "  - `O(K)`: decoding of length `K`"]
                    heartbeat {
                        heartbeat:
                            runtime_types::pallet_im_online::Heartbeat<::core::primitive::u32>,
                        signature: runtime_types::pallet_im_online::sr25519::app_sr25519::Signature,
                    },
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "The `Error` enum of this pallet."]
                pub enum Error {
                    #[codec(index = 0)]
                    #[doc = "Non existent public key."]
                    InvalidKey,
                    #[codec(index = 1)]
                    #[doc = "Duplicated heartbeat."]
                    DuplicatedHeartbeat,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "The `Event` enum of this pallet"]
                pub enum Event {
                    #[codec(index = 0)]
                    #[doc = "A new heartbeat was received from `AuthorityId`."]
                    HeartbeatReceived {
                        authority_id: runtime_types::pallet_im_online::sr25519::app_sr25519::Public,
                    },
                    #[codec(index = 1)]
                    #[doc = "At the end of the session, no offence was committed."]
                    AllGood,
                    #[codec(index = 2)]
                    #[doc = "At the end of the session, at least one validator was found to be offline."]
                    SomeOffline {
                        offline: ::subxt::ext::subxt_core::alloc::vec::Vec<(
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            runtime_types::sp_staking::Exposure<
                                ::subxt::ext::subxt_core::utils::AccountId32,
                                ::core::primitive::u128,
                            >,
                        )>,
                    },
                }
            }
            pub mod sr25519 {
                use super::runtime_types;
                pub mod app_sr25519 {
                    use super::runtime_types;
                    #[derive(
                        Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode,
                    )]
                    pub struct Public(pub [::core::primitive::u8; 32usize]);
                    #[derive(
                        Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode,
                    )]
                    pub struct Signature(pub [::core::primitive::u8; 64usize]);
                }
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub struct Heartbeat<_0> {
                pub block_number: _0,
                pub session_index: ::core::primitive::u32,
                pub authority_index: ::core::primitive::u32,
                pub validators_len: ::core::primitive::u32,
            }
        }
        pub mod pallet_multisig {
            use super::runtime_types;
            pub mod pallet {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "Contains a variant per dispatchable extrinsic that this pallet has."]
                pub enum Call {
                    #[codec(index = 0)]
                    #[doc = "Immediately dispatch a multi-signature call using a single approval from the caller."]
                    #[doc = ""]
                    #[doc = "The dispatch origin for this call must be _Signed_."]
                    #[doc = ""]
                    #[doc = "- `other_signatories`: The accounts (other than the sender) who are part of the"]
                    #[doc = "multi-signature, but do not participate in the approval process."]
                    #[doc = "- `call`: The call to be executed."]
                    #[doc = ""]
                    #[doc = "Result is equivalent to the dispatched result."]
                    #[doc = ""]
                    #[doc = "## Complexity"]
                    #[doc = "O(Z + C) where Z is the length of the call and C its execution weight."]
                    as_multi_threshold_1 {
                        other_signatories: ::subxt::ext::subxt_core::alloc::vec::Vec<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                        >,
                        call: ::subxt::ext::subxt_core::alloc::boxed::Box<
                            runtime_types::vara_runtime::RuntimeCall,
                        >,
                    },
                    #[codec(index = 1)]
                    #[doc = "Register approval for a dispatch to be made from a deterministic composite account if"]
                    #[doc = "approved by a total of `threshold - 1` of `other_signatories`."]
                    #[doc = ""]
                    #[doc = "If there are enough, then dispatch the call."]
                    #[doc = ""]
                    #[doc = "Payment: `DepositBase` will be reserved if this is the first approval, plus"]
                    #[doc = "`threshold` times `DepositFactor`. It is returned once this dispatch happens or"]
                    #[doc = "is cancelled."]
                    #[doc = ""]
                    #[doc = "The dispatch origin for this call must be _Signed_."]
                    #[doc = ""]
                    #[doc = "- `threshold`: The total number of approvals for this dispatch before it is executed."]
                    #[doc = "- `other_signatories`: The accounts (other than the sender) who can approve this"]
                    #[doc = "dispatch. May not be empty."]
                    #[doc = "- `maybe_timepoint`: If this is the first approval, then this must be `None`. If it is"]
                    #[doc = "not the first approval, then it must be `Some`, with the timepoint (block number and"]
                    #[doc = "transaction index) of the first approval transaction."]
                    #[doc = "- `call`: The call to be executed."]
                    #[doc = ""]
                    #[doc = "NOTE: Unless this is the final approval, you will generally want to use"]
                    #[doc = "`approve_as_multi` instead, since it only requires a hash of the call."]
                    #[doc = ""]
                    #[doc = "Result is equivalent to the dispatched result if `threshold` is exactly `1`. Otherwise"]
                    #[doc = "on success, result is `Ok` and the result from the interior call, if it was executed,"]
                    #[doc = "may be found in the deposited `MultisigExecuted` event."]
                    #[doc = ""]
                    #[doc = "## Complexity"]
                    #[doc = "- `O(S + Z + Call)`."]
                    #[doc = "- Up to one balance-reserve or unreserve operation."]
                    #[doc = "- One passthrough operation, one insert, both `O(S)` where `S` is the number of"]
                    #[doc = "  signatories. `S` is capped by `MaxSignatories`, with weight being proportional."]
                    #[doc = "- One call encode & hash, both of complexity `O(Z)` where `Z` is tx-len."]
                    #[doc = "- One encode & hash, both of complexity `O(S)`."]
                    #[doc = "- Up to one binary search and insert (`O(logS + S)`)."]
                    #[doc = "- I/O: 1 read `O(S)`, up to 1 mutate `O(S)`. Up to one remove."]
                    #[doc = "- One event."]
                    #[doc = "- The weight of the `call`."]
                    #[doc = "- Storage: inserts one item, value size bounded by `MaxSignatories`, with a deposit"]
                    #[doc = "  taken for its lifetime of `DepositBase + threshold * DepositFactor`."]
                    as_multi {
                        threshold: ::core::primitive::u16,
                        other_signatories: ::subxt::ext::subxt_core::alloc::vec::Vec<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                        >,
                        maybe_timepoint: ::core::option::Option<
                            runtime_types::pallet_multisig::Timepoint<::core::primitive::u32>,
                        >,
                        call: ::subxt::ext::subxt_core::alloc::boxed::Box<
                            runtime_types::vara_runtime::RuntimeCall,
                        >,
                        max_weight: runtime_types::sp_weights::weight_v2::Weight,
                    },
                    #[codec(index = 2)]
                    #[doc = "Register approval for a dispatch to be made from a deterministic composite account if"]
                    #[doc = "approved by a total of `threshold - 1` of `other_signatories`."]
                    #[doc = ""]
                    #[doc = "Payment: `DepositBase` will be reserved if this is the first approval, plus"]
                    #[doc = "`threshold` times `DepositFactor`. It is returned once this dispatch happens or"]
                    #[doc = "is cancelled."]
                    #[doc = ""]
                    #[doc = "The dispatch origin for this call must be _Signed_."]
                    #[doc = ""]
                    #[doc = "- `threshold`: The total number of approvals for this dispatch before it is executed."]
                    #[doc = "- `other_signatories`: The accounts (other than the sender) who can approve this"]
                    #[doc = "dispatch. May not be empty."]
                    #[doc = "- `maybe_timepoint`: If this is the first approval, then this must be `None`. If it is"]
                    #[doc = "not the first approval, then it must be `Some`, with the timepoint (block number and"]
                    #[doc = "transaction index) of the first approval transaction."]
                    #[doc = "- `call_hash`: The hash of the call to be executed."]
                    #[doc = ""]
                    #[doc = "NOTE: If this is the final approval, you will want to use `as_multi` instead."]
                    #[doc = ""]
                    #[doc = "## Complexity"]
                    #[doc = "- `O(S)`."]
                    #[doc = "- Up to one balance-reserve or unreserve operation."]
                    #[doc = "- One passthrough operation, one insert, both `O(S)` where `S` is the number of"]
                    #[doc = "  signatories. `S` is capped by `MaxSignatories`, with weight being proportional."]
                    #[doc = "- One encode & hash, both of complexity `O(S)`."]
                    #[doc = "- Up to one binary search and insert (`O(logS + S)`)."]
                    #[doc = "- I/O: 1 read `O(S)`, up to 1 mutate `O(S)`. Up to one remove."]
                    #[doc = "- One event."]
                    #[doc = "- Storage: inserts one item, value size bounded by `MaxSignatories`, with a deposit"]
                    #[doc = "  taken for its lifetime of `DepositBase + threshold * DepositFactor`."]
                    approve_as_multi {
                        threshold: ::core::primitive::u16,
                        other_signatories: ::subxt::ext::subxt_core::alloc::vec::Vec<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                        >,
                        maybe_timepoint: ::core::option::Option<
                            runtime_types::pallet_multisig::Timepoint<::core::primitive::u32>,
                        >,
                        call_hash: [::core::primitive::u8; 32usize],
                        max_weight: runtime_types::sp_weights::weight_v2::Weight,
                    },
                    #[codec(index = 3)]
                    #[doc = "Cancel a pre-existing, on-going multisig transaction. Any deposit reserved previously"]
                    #[doc = "for this operation will be unreserved on success."]
                    #[doc = ""]
                    #[doc = "The dispatch origin for this call must be _Signed_."]
                    #[doc = ""]
                    #[doc = "- `threshold`: The total number of approvals for this dispatch before it is executed."]
                    #[doc = "- `other_signatories`: The accounts (other than the sender) who can approve this"]
                    #[doc = "dispatch. May not be empty."]
                    #[doc = "- `timepoint`: The timepoint (block number and transaction index) of the first approval"]
                    #[doc = "transaction for this dispatch."]
                    #[doc = "- `call_hash`: The hash of the call to be executed."]
                    #[doc = ""]
                    #[doc = "## Complexity"]
                    #[doc = "- `O(S)`."]
                    #[doc = "- Up to one balance-reserve or unreserve operation."]
                    #[doc = "- One passthrough operation, one insert, both `O(S)` where `S` is the number of"]
                    #[doc = "  signatories. `S` is capped by `MaxSignatories`, with weight being proportional."]
                    #[doc = "- One encode & hash, both of complexity `O(S)`."]
                    #[doc = "- One event."]
                    #[doc = "- I/O: 1 read `O(S)`, one remove."]
                    #[doc = "- Storage: removes one item."]
                    cancel_as_multi {
                        threshold: ::core::primitive::u16,
                        other_signatories: ::subxt::ext::subxt_core::alloc::vec::Vec<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                        >,
                        timepoint:
                            runtime_types::pallet_multisig::Timepoint<::core::primitive::u32>,
                        call_hash: [::core::primitive::u8; 32usize],
                    },
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "The `Error` enum of this pallet."]
                pub enum Error {
                    #[codec(index = 0)]
                    #[doc = "Threshold must be 2 or greater."]
                    MinimumThreshold,
                    #[codec(index = 1)]
                    #[doc = "Call is already approved by this signatory."]
                    AlreadyApproved,
                    #[codec(index = 2)]
                    #[doc = "Call doesn't need any (more) approvals."]
                    NoApprovalsNeeded,
                    #[codec(index = 3)]
                    #[doc = "There are too few signatories in the list."]
                    TooFewSignatories,
                    #[codec(index = 4)]
                    #[doc = "There are too many signatories in the list."]
                    TooManySignatories,
                    #[codec(index = 5)]
                    #[doc = "The signatories were provided out of order; they should be ordered."]
                    SignatoriesOutOfOrder,
                    #[codec(index = 6)]
                    #[doc = "The sender was contained in the other signatories; it shouldn't be."]
                    SenderInSignatories,
                    #[codec(index = 7)]
                    #[doc = "Multisig operation not found when attempting to cancel."]
                    NotFound,
                    #[codec(index = 8)]
                    #[doc = "Only the account that originally created the multisig is able to cancel it."]
                    NotOwner,
                    #[codec(index = 9)]
                    #[doc = "No timepoint was given, yet the multisig operation is already underway."]
                    NoTimepoint,
                    #[codec(index = 10)]
                    #[doc = "A different timepoint was given to the multisig operation that is underway."]
                    WrongTimepoint,
                    #[codec(index = 11)]
                    #[doc = "A timepoint was given, yet no multisig operation is underway."]
                    UnexpectedTimepoint,
                    #[codec(index = 12)]
                    #[doc = "The maximum weight information provided was too low."]
                    MaxWeightTooLow,
                    #[codec(index = 13)]
                    #[doc = "The data to be stored is already stored."]
                    AlreadyStored,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "The `Event` enum of this pallet"]
                pub enum Event {
                    #[codec(index = 0)]
                    #[doc = "A new multisig operation has begun."]
                    NewMultisig {
                        approving: ::subxt::ext::subxt_core::utils::AccountId32,
                        multisig: ::subxt::ext::subxt_core::utils::AccountId32,
                        call_hash: [::core::primitive::u8; 32usize],
                    },
                    #[codec(index = 1)]
                    #[doc = "A multisig operation has been approved by someone."]
                    MultisigApproval {
                        approving: ::subxt::ext::subxt_core::utils::AccountId32,
                        timepoint:
                            runtime_types::pallet_multisig::Timepoint<::core::primitive::u32>,
                        multisig: ::subxt::ext::subxt_core::utils::AccountId32,
                        call_hash: [::core::primitive::u8; 32usize],
                    },
                    #[codec(index = 2)]
                    #[doc = "A multisig operation has been executed."]
                    MultisigExecuted {
                        approving: ::subxt::ext::subxt_core::utils::AccountId32,
                        timepoint:
                            runtime_types::pallet_multisig::Timepoint<::core::primitive::u32>,
                        multisig: ::subxt::ext::subxt_core::utils::AccountId32,
                        call_hash: [::core::primitive::u8; 32usize],
                        result:
                            ::core::result::Result<(), runtime_types::sp_runtime::DispatchError>,
                    },
                    #[codec(index = 3)]
                    #[doc = "A multisig operation has been cancelled."]
                    MultisigCancelled {
                        cancelling: ::subxt::ext::subxt_core::utils::AccountId32,
                        timepoint:
                            runtime_types::pallet_multisig::Timepoint<::core::primitive::u32>,
                        multisig: ::subxt::ext::subxt_core::utils::AccountId32,
                        call_hash: [::core::primitive::u8; 32usize],
                    },
                }
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub struct Multisig<_0, _1, _2> {
                pub when: runtime_types::pallet_multisig::Timepoint<_0>,
                pub deposit: _1,
                pub depositor: _2,
                pub approvals: runtime_types::bounded_collections::bounded_vec::BoundedVec<_2>,
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub struct Timepoint<_0> {
                pub height: _0,
                pub index: ::core::primitive::u32,
            }
        }
        pub mod pallet_nomination_pools {
            use super::runtime_types;
            pub mod pallet {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "Contains a variant per dispatchable extrinsic that this pallet has."]
                pub enum Call {
                    #[codec(index = 0)]
                    #[doc = "Stake funds with a pool. The amount to bond is transferred from the member to the pool"]
                    #[doc = "account and immediately increases the pools bond."]
                    #[doc = ""]
                    #[doc = "The method of transferring the amount to the pool account is determined by"]
                    #[doc = "[`adapter::StakeStrategyType`]. If the pool is configured to use"]
                    #[doc = "[`adapter::StakeStrategyType::Delegate`], the funds remain in the account of"]
                    #[doc = "the `origin`, while the pool gains the right to use these funds for staking."]
                    #[doc = ""]
                    #[doc = "# Note"]
                    #[doc = ""]
                    #[doc = "* An account can only be a member of a single pool."]
                    #[doc = "* An account cannot join the same pool multiple times."]
                    #[doc = "* This call will *not* dust the member account, so the member must have at least"]
                    #[doc = "  `existential deposit + amount` in their account."]
                    #[doc = "* Only a pool with [`PoolState::Open`] can be joined"]
                    join {
                        #[codec(compact)]
                        amount: ::core::primitive::u128,
                        pool_id: ::core::primitive::u32,
                    },
                    #[codec(index = 1)]
                    #[doc = "Bond `extra` more funds from `origin` into the pool to which they already belong."]
                    #[doc = ""]
                    #[doc = "Additional funds can come from either the free balance of the account, of from the"]
                    #[doc = "accumulated rewards, see [`BondExtra`]."]
                    #[doc = ""]
                    #[doc = "Bonding extra funds implies an automatic payout of all pending rewards as well."]
                    #[doc = "See `bond_extra_other` to bond pending rewards of `other` members."]
                    bond_extra {
                        extra: runtime_types::pallet_nomination_pools::BondExtra<
                            ::core::primitive::u128,
                        >,
                    },
                    #[codec(index = 2)]
                    #[doc = "A bonded member can use this to claim their payout based on the rewards that the pool"]
                    #[doc = "has accumulated since their last claimed payout (OR since joining if this is their first"]
                    #[doc = "time claiming rewards). The payout will be transferred to the member's account."]
                    #[doc = ""]
                    #[doc = "The member will earn rewards pro rata based on the members stake vs the sum of the"]
                    #[doc = "members in the pools stake. Rewards do not \"expire\"."]
                    #[doc = ""]
                    #[doc = "See `claim_payout_other` to claim rewards on behalf of some `other` pool member."]
                    claim_payout,
                    #[codec(index = 3)]
                    #[doc = "Unbond up to `unbonding_points` of the `member_account`'s funds from the pool. It"]
                    #[doc = "implicitly collects the rewards one last time, since not doing so would mean some"]
                    #[doc = "rewards would be forfeited."]
                    #[doc = ""]
                    #[doc = "Under certain conditions, this call can be dispatched permissionlessly (i.e. by any"]
                    #[doc = "account)."]
                    #[doc = ""]
                    #[doc = "# Conditions for a permissionless dispatch."]
                    #[doc = ""]
                    #[doc = "* The pool is blocked and the caller is either the root or bouncer. This is refereed to"]
                    #[doc = "  as a kick."]
                    #[doc = "* The pool is destroying and the member is not the depositor."]
                    #[doc = "* The pool is destroying, the member is the depositor and no other members are in the"]
                    #[doc = "  pool."]
                    #[doc = ""]
                    #[doc = "## Conditions for permissioned dispatch (i.e. the caller is also the"]
                    #[doc = "`member_account`):"]
                    #[doc = ""]
                    #[doc = "* The caller is not the depositor."]
                    #[doc = "* The caller is the depositor, the pool is destroying and no other members are in the"]
                    #[doc = "  pool."]
                    #[doc = ""]
                    #[doc = "# Note"]
                    #[doc = ""]
                    #[doc = "If there are too many unlocking chunks to unbond with the pool account,"]
                    #[doc = "[`Call::pool_withdraw_unbonded`] can be called to try and minimize unlocking chunks."]
                    #[doc = "The [`StakingInterface::unbond`] will implicitly call [`Call::pool_withdraw_unbonded`]"]
                    #[doc = "to try to free chunks if necessary (ie. if unbound was called and no unlocking chunks"]
                    #[doc = "are available). However, it may not be possible to release the current unlocking chunks,"]
                    #[doc = "in which case, the result of this call will likely be the `NoMoreChunks` error from the"]
                    #[doc = "staking system."]
                    unbond {
                        member_account: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                        #[codec(compact)]
                        unbonding_points: ::core::primitive::u128,
                    },
                    #[codec(index = 4)]
                    #[doc = "Call `withdraw_unbonded` for the pools account. This call can be made by any account."]
                    #[doc = ""]
                    #[doc = "This is useful if there are too many unlocking chunks to call `unbond`, and some"]
                    #[doc = "can be cleared by withdrawing. In the case there are too many unlocking chunks, the user"]
                    #[doc = "would probably see an error like `NoMoreChunks` emitted from the staking system when"]
                    #[doc = "they attempt to unbond."]
                    pool_withdraw_unbonded {
                        pool_id: ::core::primitive::u32,
                        num_slashing_spans: ::core::primitive::u32,
                    },
                    #[codec(index = 5)]
                    #[doc = "Withdraw unbonded funds from `member_account`. If no bonded funds can be unbonded, an"]
                    #[doc = "error is returned."]
                    #[doc = ""]
                    #[doc = "Under certain conditions, this call can be dispatched permissionlessly (i.e. by any"]
                    #[doc = "account)."]
                    #[doc = ""]
                    #[doc = "# Conditions for a permissionless dispatch"]
                    #[doc = ""]
                    #[doc = "* The pool is in destroy mode and the target is not the depositor."]
                    #[doc = "* The target is the depositor and they are the only member in the sub pools."]
                    #[doc = "* The pool is blocked and the caller is either the root or bouncer."]
                    #[doc = ""]
                    #[doc = "# Conditions for permissioned dispatch"]
                    #[doc = ""]
                    #[doc = "* The caller is the target and they are not the depositor."]
                    #[doc = ""]
                    #[doc = "# Note"]
                    #[doc = ""]
                    #[doc = "- If the target is the depositor, the pool will be destroyed."]
                    #[doc = "- If the pool has any pending slash, we also try to slash the member before letting them"]
                    #[doc = "withdraw. This calculation adds some weight overhead and is only defensive. In reality,"]
                    #[doc = "pool slashes must have been already applied via permissionless [`Call::apply_slash`]."]
                    withdraw_unbonded {
                        member_account: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                        num_slashing_spans: ::core::primitive::u32,
                    },
                    #[codec(index = 6)]
                    #[doc = "Create a new delegation pool."]
                    #[doc = ""]
                    #[doc = "# Arguments"]
                    #[doc = ""]
                    #[doc = "* `amount` - The amount of funds to delegate to the pool. This also acts of a sort of"]
                    #[doc = "  deposit since the pools creator cannot fully unbond funds until the pool is being"]
                    #[doc = "  destroyed."]
                    #[doc = "* `index` - A disambiguation index for creating the account. Likely only useful when"]
                    #[doc = "  creating multiple pools in the same extrinsic."]
                    #[doc = "* `root` - The account to set as [`PoolRoles::root`]."]
                    #[doc = "* `nominator` - The account to set as the [`PoolRoles::nominator`]."]
                    #[doc = "* `bouncer` - The account to set as the [`PoolRoles::bouncer`]."]
                    #[doc = ""]
                    #[doc = "# Note"]
                    #[doc = ""]
                    #[doc = "In addition to `amount`, the caller will transfer the existential deposit; so the caller"]
                    #[doc = "needs at have at least `amount + existential_deposit` transferable."]
                    create {
                        #[codec(compact)]
                        amount: ::core::primitive::u128,
                        root: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                        nominator: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                        bouncer: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                    },
                    #[codec(index = 7)]
                    #[doc = "Create a new delegation pool with a previously used pool id"]
                    #[doc = ""]
                    #[doc = "# Arguments"]
                    #[doc = ""]
                    #[doc = "same as `create` with the inclusion of"]
                    #[doc = "* `pool_id` - `A valid PoolId."]
                    create_with_pool_id {
                        #[codec(compact)]
                        amount: ::core::primitive::u128,
                        root: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                        nominator: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                        bouncer: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                        pool_id: ::core::primitive::u32,
                    },
                    #[codec(index = 8)]
                    #[doc = "Nominate on behalf of the pool."]
                    #[doc = ""]
                    #[doc = "The dispatch origin of this call must be signed by the pool nominator or the pool"]
                    #[doc = "root role."]
                    #[doc = ""]
                    #[doc = "This directly forward the call to the staking pallet, on behalf of the pool bonded"]
                    #[doc = "account."]
                    #[doc = ""]
                    #[doc = "# Note"]
                    #[doc = ""]
                    #[doc = "In addition to a `root` or `nominator` role of `origin`, pool's depositor needs to have"]
                    #[doc = "at least `depositor_min_bond` in the pool to start nominating."]
                    nominate {
                        pool_id: ::core::primitive::u32,
                        validators: ::subxt::ext::subxt_core::alloc::vec::Vec<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                        >,
                    },
                    #[codec(index = 9)]
                    #[doc = "Set a new state for the pool."]
                    #[doc = ""]
                    #[doc = "If a pool is already in the `Destroying` state, then under no condition can its state"]
                    #[doc = "change again."]
                    #[doc = ""]
                    #[doc = "The dispatch origin of this call must be either:"]
                    #[doc = ""]
                    #[doc = "1. signed by the bouncer, or the root role of the pool,"]
                    #[doc = "2. if the pool conditions to be open are NOT met (as described by `ok_to_be_open`), and"]
                    #[doc = "   then the state of the pool can be permissionlessly changed to `Destroying`."]
                    set_state {
                        pool_id: ::core::primitive::u32,
                        state: runtime_types::pallet_nomination_pools::PoolState,
                    },
                    #[codec(index = 10)]
                    #[doc = "Set a new metadata for the pool."]
                    #[doc = ""]
                    #[doc = "The dispatch origin of this call must be signed by the bouncer, or the root role of the"]
                    #[doc = "pool."]
                    set_metadata {
                        pool_id: ::core::primitive::u32,
                        metadata: ::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
                    },
                    #[codec(index = 11)]
                    #[doc = "Update configurations for the nomination pools. The origin for this call must be"]
                    #[doc = "[`Config::AdminOrigin`]."]
                    #[doc = ""]
                    #[doc = "# Arguments"]
                    #[doc = ""]
                    #[doc = "* `min_join_bond` - Set [`MinJoinBond`]."]
                    #[doc = "* `min_create_bond` - Set [`MinCreateBond`]."]
                    #[doc = "* `max_pools` - Set [`MaxPools`]."]
                    #[doc = "* `max_members` - Set [`MaxPoolMembers`]."]
                    #[doc = "* `max_members_per_pool` - Set [`MaxPoolMembersPerPool`]."]
                    #[doc = "* `global_max_commission` - Set [`GlobalMaxCommission`]."]
                    set_configs {
                        min_join_bond: runtime_types::pallet_nomination_pools::ConfigOp<
                            ::core::primitive::u128,
                        >,
                        min_create_bond: runtime_types::pallet_nomination_pools::ConfigOp<
                            ::core::primitive::u128,
                        >,
                        max_pools: runtime_types::pallet_nomination_pools::ConfigOp<
                            ::core::primitive::u32,
                        >,
                        max_members: runtime_types::pallet_nomination_pools::ConfigOp<
                            ::core::primitive::u32,
                        >,
                        max_members_per_pool: runtime_types::pallet_nomination_pools::ConfigOp<
                            ::core::primitive::u32,
                        >,
                        global_max_commission: runtime_types::pallet_nomination_pools::ConfigOp<
                            runtime_types::sp_arithmetic::per_things::Perbill,
                        >,
                    },
                    #[codec(index = 12)]
                    #[doc = "Update the roles of the pool."]
                    #[doc = ""]
                    #[doc = "The root is the only entity that can change any of the roles, including itself,"]
                    #[doc = "excluding the depositor, who can never change."]
                    #[doc = ""]
                    #[doc = "It emits an event, notifying UIs of the role change. This event is quite relevant to"]
                    #[doc = "most pool members and they should be informed of changes to pool roles."]
                    update_roles {
                        pool_id: ::core::primitive::u32,
                        new_root: runtime_types::pallet_nomination_pools::ConfigOp<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                        >,
                        new_nominator: runtime_types::pallet_nomination_pools::ConfigOp<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                        >,
                        new_bouncer: runtime_types::pallet_nomination_pools::ConfigOp<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                        >,
                    },
                    #[codec(index = 13)]
                    #[doc = "Chill on behalf of the pool."]
                    #[doc = ""]
                    #[doc = "The dispatch origin of this call can be signed by the pool nominator or the pool"]
                    #[doc = "root role, same as [`Pallet::nominate`]."]
                    #[doc = ""]
                    #[doc = "Under certain conditions, this call can be dispatched permissionlessly (i.e. by any"]
                    #[doc = "account)."]
                    #[doc = ""]
                    #[doc = "# Conditions for a permissionless dispatch:"]
                    #[doc = "* When pool depositor has less than `MinNominatorBond` staked, otherwise  pool members"]
                    #[doc = "  are unable to unbond."]
                    #[doc = ""]
                    #[doc = "# Conditions for permissioned dispatch:"]
                    #[doc = "* The caller has a nominator or root role of the pool."]
                    #[doc = "This directly forward the call to the staking pallet, on behalf of the pool bonded"]
                    #[doc = "account."]
                    chill { pool_id: ::core::primitive::u32 },
                    #[codec(index = 14)]
                    #[doc = "`origin` bonds funds from `extra` for some pool member `member` into their respective"]
                    #[doc = "pools."]
                    #[doc = ""]
                    #[doc = "`origin` can bond extra funds from free balance or pending rewards when `origin =="]
                    #[doc = "other`."]
                    #[doc = ""]
                    #[doc = "In the case of `origin != other`, `origin` can only bond extra pending rewards of"]
                    #[doc = "`other` members assuming set_claim_permission for the given member is"]
                    #[doc = "`PermissionlessCompound` or `PermissionlessAll`."]
                    bond_extra_other {
                        member: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                        extra: runtime_types::pallet_nomination_pools::BondExtra<
                            ::core::primitive::u128,
                        >,
                    },
                    #[codec(index = 15)]
                    #[doc = "Allows a pool member to set a claim permission to allow or disallow permissionless"]
                    #[doc = "bonding and withdrawing."]
                    #[doc = ""]
                    #[doc = "# Arguments"]
                    #[doc = ""]
                    #[doc = "* `origin` - Member of a pool."]
                    #[doc = "* `permission` - The permission to be applied."]
                    set_claim_permission {
                        permission: runtime_types::pallet_nomination_pools::ClaimPermission,
                    },
                    #[codec(index = 16)]
                    #[doc = "`origin` can claim payouts on some pool member `other`'s behalf."]
                    #[doc = ""]
                    #[doc = "Pool member `other` must have a `PermissionlessWithdraw` or `PermissionlessAll` claim"]
                    #[doc = "permission for this call to be successful."]
                    claim_payout_other {
                        other: ::subxt::ext::subxt_core::utils::AccountId32,
                    },
                    #[codec(index = 17)]
                    #[doc = "Set the commission of a pool."]
                    #[doc = "Both a commission percentage and a commission payee must be provided in the `current`"]
                    #[doc = "tuple. Where a `current` of `None` is provided, any current commission will be removed."]
                    #[doc = ""]
                    #[doc = "- If a `None` is supplied to `new_commission`, existing commission will be removed."]
                    set_commission {
                        pool_id: ::core::primitive::u32,
                        new_commission: ::core::option::Option<(
                            runtime_types::sp_arithmetic::per_things::Perbill,
                            ::subxt::ext::subxt_core::utils::AccountId32,
                        )>,
                    },
                    #[codec(index = 18)]
                    #[doc = "Set the maximum commission of a pool."]
                    #[doc = ""]
                    #[doc = "- Initial max can be set to any `Perbill`, and only smaller values thereafter."]
                    #[doc = "- Current commission will be lowered in the event it is higher than a new max"]
                    #[doc = "  commission."]
                    set_commission_max {
                        pool_id: ::core::primitive::u32,
                        max_commission: runtime_types::sp_arithmetic::per_things::Perbill,
                    },
                    #[codec(index = 19)]
                    #[doc = "Set the commission change rate for a pool."]
                    #[doc = ""]
                    #[doc = "Initial change rate is not bounded, whereas subsequent updates can only be more"]
                    #[doc = "restrictive than the current."]
                    set_commission_change_rate {
                        pool_id: ::core::primitive::u32,
                        change_rate: runtime_types::pallet_nomination_pools::CommissionChangeRate<
                            ::core::primitive::u32,
                        >,
                    },
                    #[codec(index = 20)]
                    #[doc = "Claim pending commission."]
                    #[doc = ""]
                    #[doc = "The dispatch origin of this call must be signed by the `root` role of the pool. Pending"]
                    #[doc = "commission is paid out and added to total claimed commission`. Total pending commission"]
                    #[doc = "is reset to zero. the current."]
                    claim_commission { pool_id: ::core::primitive::u32 },
                    #[codec(index = 21)]
                    #[doc = "Top up the deficit or withdraw the excess ED from the pool."]
                    #[doc = ""]
                    #[doc = "When a pool is created, the pool depositor transfers ED to the reward account of the"]
                    #[doc = "pool. ED is subject to change and over time, the deposit in the reward account may be"]
                    #[doc = "insufficient to cover the ED deficit of the pool or vice-versa where there is excess"]
                    #[doc = "deposit to the pool. This call allows anyone to adjust the ED deposit of the"]
                    #[doc = "pool by either topping up the deficit or claiming the excess."]
                    adjust_pool_deposit { pool_id: ::core::primitive::u32 },
                    #[codec(index = 22)]
                    #[doc = "Set or remove a pool's commission claim permission."]
                    #[doc = ""]
                    #[doc = "Determines who can claim the pool's pending commission. Only the `Root` role of the pool"]
                    #[doc = "is able to configure commission claim permissions."]
                    set_commission_claim_permission {
                        pool_id: ::core::primitive::u32,
                        permission: ::core::option::Option<
                            runtime_types::pallet_nomination_pools::CommissionClaimPermission<
                                ::subxt::ext::subxt_core::utils::AccountId32,
                            >,
                        >,
                    },
                    #[codec(index = 23)]
                    #[doc = "Apply a pending slash on a member."]
                    #[doc = ""]
                    #[doc = "Fails unless [`crate::pallet::Config::StakeAdapter`] is of strategy type:"]
                    #[doc = "[`adapter::StakeStrategyType::Delegate`]."]
                    #[doc = ""]
                    #[doc = "This call can be dispatched permissionlessly (i.e. by any account). If the member has"]
                    #[doc = "slash to be applied, caller may be rewarded with the part of the slash."]
                    apply_slash {
                        member_account: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                    },
                    #[codec(index = 24)]
                    #[doc = "Migrates delegated funds from the pool account to the `member_account`."]
                    #[doc = ""]
                    #[doc = "Fails unless [`crate::pallet::Config::StakeAdapter`] is of strategy type:"]
                    #[doc = "[`adapter::StakeStrategyType::Delegate`]."]
                    #[doc = ""]
                    #[doc = "This is a permission-less call and refunds any fee if claim is successful."]
                    #[doc = ""]
                    #[doc = "If the pool has migrated to delegation based staking, the staked tokens of pool members"]
                    #[doc = "can be moved and held in their own account. See [`adapter::DelegateStake`]"]
                    migrate_delegation {
                        member_account: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                    },
                    #[codec(index = 25)]
                    #[doc = "Migrate pool from [`adapter::StakeStrategyType::Transfer`] to"]
                    #[doc = "[`adapter::StakeStrategyType::Delegate`]."]
                    #[doc = ""]
                    #[doc = "Fails unless [`crate::pallet::Config::StakeAdapter`] is of strategy type:"]
                    #[doc = "[`adapter::StakeStrategyType::Delegate`]."]
                    #[doc = ""]
                    #[doc = "This call can be dispatched permissionlessly, and refunds any fee if successful."]
                    #[doc = ""]
                    #[doc = "If the pool has already migrated to delegation based staking, this call will fail."]
                    migrate_pool_to_delegate_stake { pool_id: ::core::primitive::u32 },
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum DefensiveError {
                    #[codec(index = 0)]
                    NotEnoughSpaceInUnbondPool,
                    #[codec(index = 1)]
                    PoolNotFound,
                    #[codec(index = 2)]
                    RewardPoolNotFound,
                    #[codec(index = 3)]
                    SubPoolsNotFound,
                    #[codec(index = 4)]
                    BondedStashKilledPrematurely,
                    #[codec(index = 5)]
                    DelegationUnsupported,
                    #[codec(index = 6)]
                    SlashNotApplied,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "The `Error` enum of this pallet."]
                pub enum Error {
                    #[codec(index = 0)]
                    #[doc = "A (bonded) pool id does not exist."]
                    PoolNotFound,
                    #[codec(index = 1)]
                    #[doc = "An account is not a member."]
                    PoolMemberNotFound,
                    #[codec(index = 2)]
                    #[doc = "A reward pool does not exist. In all cases this is a system logic error."]
                    RewardPoolNotFound,
                    #[codec(index = 3)]
                    #[doc = "A sub pool does not exist."]
                    SubPoolsNotFound,
                    #[codec(index = 4)]
                    #[doc = "An account is already delegating in another pool. An account may only belong to one"]
                    #[doc = "pool at a time."]
                    AccountBelongsToOtherPool,
                    #[codec(index = 5)]
                    #[doc = "The member is fully unbonded (and thus cannot access the bonded and reward pool"]
                    #[doc = "anymore to, for example, collect rewards)."]
                    FullyUnbonding,
                    #[codec(index = 6)]
                    #[doc = "The member cannot unbond further chunks due to reaching the limit."]
                    MaxUnbondingLimit,
                    #[codec(index = 7)]
                    #[doc = "None of the funds can be withdrawn yet because the bonding duration has not passed."]
                    CannotWithdrawAny,
                    #[codec(index = 8)]
                    #[doc = "The amount does not meet the minimum bond to either join or create a pool."]
                    #[doc = ""]
                    #[doc = "The depositor can never unbond to a value less than `Pallet::depositor_min_bond`. The"]
                    #[doc = "caller does not have nominating permissions for the pool. Members can never unbond to a"]
                    #[doc = "value below `MinJoinBond`."]
                    MinimumBondNotMet,
                    #[codec(index = 9)]
                    #[doc = "The transaction could not be executed due to overflow risk for the pool."]
                    OverflowRisk,
                    #[codec(index = 10)]
                    #[doc = "A pool must be in [`PoolState::Destroying`] in order for the depositor to unbond or for"]
                    #[doc = "other members to be permissionlessly unbonded."]
                    NotDestroying,
                    #[codec(index = 11)]
                    #[doc = "The caller does not have nominating permissions for the pool."]
                    NotNominator,
                    #[codec(index = 12)]
                    #[doc = "Either a) the caller cannot make a valid kick or b) the pool is not destroying."]
                    NotKickerOrDestroying,
                    #[codec(index = 13)]
                    #[doc = "The pool is not open to join"]
                    NotOpen,
                    #[codec(index = 14)]
                    #[doc = "The system is maxed out on pools."]
                    MaxPools,
                    #[codec(index = 15)]
                    #[doc = "Too many members in the pool or system."]
                    MaxPoolMembers,
                    #[codec(index = 16)]
                    #[doc = "The pools state cannot be changed."]
                    CanNotChangeState,
                    #[codec(index = 17)]
                    #[doc = "The caller does not have adequate permissions."]
                    DoesNotHavePermission,
                    #[codec(index = 18)]
                    #[doc = "Metadata exceeds [`Config::MaxMetadataLen`]"]
                    MetadataExceedsMaxLen,
                    #[codec(index = 19)]
                    #[doc = "Some error occurred that should never happen. This should be reported to the"]
                    #[doc = "maintainers."]
                    Defensive(runtime_types::pallet_nomination_pools::pallet::DefensiveError),
                    #[codec(index = 20)]
                    #[doc = "Partial unbonding now allowed permissionlessly."]
                    PartialUnbondNotAllowedPermissionlessly,
                    #[codec(index = 21)]
                    #[doc = "The pool's max commission cannot be set higher than the existing value."]
                    MaxCommissionRestricted,
                    #[codec(index = 22)]
                    #[doc = "The supplied commission exceeds the max allowed commission."]
                    CommissionExceedsMaximum,
                    #[codec(index = 23)]
                    #[doc = "The supplied commission exceeds global maximum commission."]
                    CommissionExceedsGlobalMaximum,
                    #[codec(index = 24)]
                    #[doc = "Not enough blocks have surpassed since the last commission update."]
                    CommissionChangeThrottled,
                    #[codec(index = 25)]
                    #[doc = "The submitted changes to commission change rate are not allowed."]
                    CommissionChangeRateNotAllowed,
                    #[codec(index = 26)]
                    #[doc = "There is no pending commission to claim."]
                    NoPendingCommission,
                    #[codec(index = 27)]
                    #[doc = "No commission current has been set."]
                    NoCommissionCurrentSet,
                    #[codec(index = 28)]
                    #[doc = "Pool id currently in use."]
                    PoolIdInUse,
                    #[codec(index = 29)]
                    #[doc = "Pool id provided is not correct/usable."]
                    InvalidPoolId,
                    #[codec(index = 30)]
                    #[doc = "Bonding extra is restricted to the exact pending reward amount."]
                    BondExtraRestricted,
                    #[codec(index = 31)]
                    #[doc = "No imbalance in the ED deposit for the pool."]
                    NothingToAdjust,
                    #[codec(index = 32)]
                    #[doc = "No slash pending that can be applied to the member."]
                    NothingToSlash,
                    #[codec(index = 33)]
                    #[doc = "The pool or member delegation has already migrated to delegate stake."]
                    AlreadyMigrated,
                    #[codec(index = 34)]
                    #[doc = "The pool or member delegation has not migrated yet to delegate stake."]
                    NotMigrated,
                    #[codec(index = 35)]
                    #[doc = "This call is not allowed in the current state of the pallet."]
                    NotSupported,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "Events of this pallet."]
                pub enum Event {
                    #[codec(index = 0)]
                    #[doc = "A pool has been created."]
                    Created {
                        depositor: ::subxt::ext::subxt_core::utils::AccountId32,
                        pool_id: ::core::primitive::u32,
                    },
                    #[codec(index = 1)]
                    #[doc = "A member has became bonded in a pool."]
                    Bonded {
                        member: ::subxt::ext::subxt_core::utils::AccountId32,
                        pool_id: ::core::primitive::u32,
                        bonded: ::core::primitive::u128,
                        joined: ::core::primitive::bool,
                    },
                    #[codec(index = 2)]
                    #[doc = "A payout has been made to a member."]
                    PaidOut {
                        member: ::subxt::ext::subxt_core::utils::AccountId32,
                        pool_id: ::core::primitive::u32,
                        payout: ::core::primitive::u128,
                    },
                    #[codec(index = 3)]
                    #[doc = "A member has unbonded from their pool."]
                    #[doc = ""]
                    #[doc = "- `balance` is the corresponding balance of the number of points that has been"]
                    #[doc = "  requested to be unbonded (the argument of the `unbond` transaction) from the bonded"]
                    #[doc = "  pool."]
                    #[doc = "- `points` is the number of points that are issued as a result of `balance` being"]
                    #[doc = "dissolved into the corresponding unbonding pool."]
                    #[doc = "- `era` is the era in which the balance will be unbonded."]
                    #[doc = "In the absence of slashing, these values will match. In the presence of slashing, the"]
                    #[doc = "number of points that are issued in the unbonding pool will be less than the amount"]
                    #[doc = "requested to be unbonded."]
                    Unbonded {
                        member: ::subxt::ext::subxt_core::utils::AccountId32,
                        pool_id: ::core::primitive::u32,
                        balance: ::core::primitive::u128,
                        points: ::core::primitive::u128,
                        era: ::core::primitive::u32,
                    },
                    #[codec(index = 4)]
                    #[doc = "A member has withdrawn from their pool."]
                    #[doc = ""]
                    #[doc = "The given number of `points` have been dissolved in return of `balance`."]
                    #[doc = ""]
                    #[doc = "Similar to `Unbonded` event, in the absence of slashing, the ratio of point to balance"]
                    #[doc = "will be 1."]
                    Withdrawn {
                        member: ::subxt::ext::subxt_core::utils::AccountId32,
                        pool_id: ::core::primitive::u32,
                        balance: ::core::primitive::u128,
                        points: ::core::primitive::u128,
                    },
                    #[codec(index = 5)]
                    #[doc = "A pool has been destroyed."]
                    Destroyed { pool_id: ::core::primitive::u32 },
                    #[codec(index = 6)]
                    #[doc = "The state of a pool has changed"]
                    StateChanged {
                        pool_id: ::core::primitive::u32,
                        new_state: runtime_types::pallet_nomination_pools::PoolState,
                    },
                    #[codec(index = 7)]
                    #[doc = "A member has been removed from a pool."]
                    #[doc = ""]
                    #[doc = "The removal can be voluntary (withdrawn all unbonded funds) or involuntary (kicked)."]
                    #[doc = "Any funds that are still delegated (i.e. dangling delegation) are released and are"]
                    #[doc = "represented by `released_balance`."]
                    MemberRemoved {
                        pool_id: ::core::primitive::u32,
                        member: ::subxt::ext::subxt_core::utils::AccountId32,
                        released_balance: ::core::primitive::u128,
                    },
                    #[codec(index = 8)]
                    #[doc = "The roles of a pool have been updated to the given new roles. Note that the depositor"]
                    #[doc = "can never change."]
                    RolesUpdated {
                        root: ::core::option::Option<::subxt::ext::subxt_core::utils::AccountId32>,
                        bouncer:
                            ::core::option::Option<::subxt::ext::subxt_core::utils::AccountId32>,
                        nominator:
                            ::core::option::Option<::subxt::ext::subxt_core::utils::AccountId32>,
                    },
                    #[codec(index = 9)]
                    #[doc = "The active balance of pool `pool_id` has been slashed to `balance`."]
                    PoolSlashed {
                        pool_id: ::core::primitive::u32,
                        balance: ::core::primitive::u128,
                    },
                    #[codec(index = 10)]
                    #[doc = "The unbond pool at `era` of pool `pool_id` has been slashed to `balance`."]
                    UnbondingPoolSlashed {
                        pool_id: ::core::primitive::u32,
                        era: ::core::primitive::u32,
                        balance: ::core::primitive::u128,
                    },
                    #[codec(index = 11)]
                    #[doc = "A pool's commission setting has been changed."]
                    PoolCommissionUpdated {
                        pool_id: ::core::primitive::u32,
                        current: ::core::option::Option<(
                            runtime_types::sp_arithmetic::per_things::Perbill,
                            ::subxt::ext::subxt_core::utils::AccountId32,
                        )>,
                    },
                    #[codec(index = 12)]
                    #[doc = "A pool's maximum commission setting has been changed."]
                    PoolMaxCommissionUpdated {
                        pool_id: ::core::primitive::u32,
                        max_commission: runtime_types::sp_arithmetic::per_things::Perbill,
                    },
                    #[codec(index = 13)]
                    #[doc = "A pool's commission `change_rate` has been changed."]
                    PoolCommissionChangeRateUpdated {
                        pool_id: ::core::primitive::u32,
                        change_rate: runtime_types::pallet_nomination_pools::CommissionChangeRate<
                            ::core::primitive::u32,
                        >,
                    },
                    #[codec(index = 14)]
                    #[doc = "Pool commission claim permission has been updated."]
                    PoolCommissionClaimPermissionUpdated {
                        pool_id: ::core::primitive::u32,
                        permission: ::core::option::Option<
                            runtime_types::pallet_nomination_pools::CommissionClaimPermission<
                                ::subxt::ext::subxt_core::utils::AccountId32,
                            >,
                        >,
                    },
                    #[codec(index = 15)]
                    #[doc = "Pool commission has been claimed."]
                    PoolCommissionClaimed {
                        pool_id: ::core::primitive::u32,
                        commission: ::core::primitive::u128,
                    },
                    #[codec(index = 16)]
                    #[doc = "Topped up deficit in frozen ED of the reward pool."]
                    MinBalanceDeficitAdjusted {
                        pool_id: ::core::primitive::u32,
                        amount: ::core::primitive::u128,
                    },
                    #[codec(index = 17)]
                    #[doc = "Claimed excess frozen ED of af the reward pool."]
                    MinBalanceExcessAdjusted {
                        pool_id: ::core::primitive::u32,
                        amount: ::core::primitive::u128,
                    },
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum FreezeReason {
                    #[codec(index = 0)]
                    PoolMinBalance,
                }
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub enum BondExtra<_0> {
                #[codec(index = 0)]
                FreeBalance(_0),
                #[codec(index = 1)]
                Rewards,
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub struct BondedPoolInner {
                pub commission: runtime_types::pallet_nomination_pools::Commission,
                pub member_counter: ::core::primitive::u32,
                pub points: ::core::primitive::u128,
                pub roles: runtime_types::pallet_nomination_pools::PoolRoles<
                    ::subxt::ext::subxt_core::utils::AccountId32,
                >,
                pub state: runtime_types::pallet_nomination_pools::PoolState,
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub enum ClaimPermission {
                #[codec(index = 0)]
                Permissioned,
                #[codec(index = 1)]
                PermissionlessCompound,
                #[codec(index = 2)]
                PermissionlessWithdraw,
                #[codec(index = 3)]
                PermissionlessAll,
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub struct Commission {
                pub current: ::core::option::Option<(
                    runtime_types::sp_arithmetic::per_things::Perbill,
                    ::subxt::ext::subxt_core::utils::AccountId32,
                )>,
                pub max: ::core::option::Option<runtime_types::sp_arithmetic::per_things::Perbill>,
                pub change_rate: ::core::option::Option<
                    runtime_types::pallet_nomination_pools::CommissionChangeRate<
                        ::core::primitive::u32,
                    >,
                >,
                pub throttle_from: ::core::option::Option<::core::primitive::u32>,
                pub claim_permission: ::core::option::Option<
                    runtime_types::pallet_nomination_pools::CommissionClaimPermission<
                        ::subxt::ext::subxt_core::utils::AccountId32,
                    >,
                >,
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub struct CommissionChangeRate<_0> {
                pub max_increase: runtime_types::sp_arithmetic::per_things::Perbill,
                pub min_delay: _0,
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub enum CommissionClaimPermission<_0> {
                #[codec(index = 0)]
                Permissionless,
                #[codec(index = 1)]
                Account(_0),
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub enum ConfigOp<_0> {
                #[codec(index = 0)]
                Noop,
                #[codec(index = 1)]
                Set(_0),
                #[codec(index = 2)]
                Remove,
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub struct PoolMember {
                pub pool_id: ::core::primitive::u32,
                pub points: ::core::primitive::u128,
                pub last_recorded_reward_counter:
                    runtime_types::sp_arithmetic::fixed_point::FixedU128,
                pub unbonding_eras:
                    runtime_types::bounded_collections::bounded_btree_map::BoundedBTreeMap<
                        ::core::primitive::u32,
                        ::core::primitive::u128,
                    >,
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub struct PoolRoles<_0> {
                pub depositor: _0,
                pub root: ::core::option::Option<_0>,
                pub nominator: ::core::option::Option<_0>,
                pub bouncer: ::core::option::Option<_0>,
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub enum PoolState {
                #[codec(index = 0)]
                Open,
                #[codec(index = 1)]
                Blocked,
                #[codec(index = 2)]
                Destroying,
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub struct RewardPool {
                pub last_recorded_reward_counter:
                    runtime_types::sp_arithmetic::fixed_point::FixedU128,
                pub last_recorded_total_payouts: ::core::primitive::u128,
                pub total_rewards_claimed: ::core::primitive::u128,
                pub total_commission_pending: ::core::primitive::u128,
                pub total_commission_claimed: ::core::primitive::u128,
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub struct SubPools {
                pub no_era: runtime_types::pallet_nomination_pools::UnbondPool,
                pub with_era:
                    runtime_types::bounded_collections::bounded_btree_map::BoundedBTreeMap<
                        ::core::primitive::u32,
                        runtime_types::pallet_nomination_pools::UnbondPool,
                    >,
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub struct UnbondPool {
                pub points: ::core::primitive::u128,
                pub balance: ::core::primitive::u128,
            }
        }
        pub mod pallet_offences {
            use super::runtime_types;
            pub mod pallet {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "Events type."]
                pub enum Event {
                    #[codec(index = 0)]
                    #[doc = "There is an offence reported of the given `kind` happened at the `session_index` and"]
                    #[doc = "(kind-specific) time slot. This event is not deposited for duplicate slashes."]
                    #[doc = "\\[kind, timeslot\\]."]
                    Offence {
                        kind: [::core::primitive::u8; 16usize],
                        timeslot: ::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
                    },
                }
            }
        }
        pub mod pallet_preimage {
            use super::runtime_types;
            pub mod pallet {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "Contains a variant per dispatchable extrinsic that this pallet has."]
                pub enum Call {
                    #[codec(index = 0)]
                    #[doc = "Register a preimage on-chain."]
                    #[doc = ""]
                    #[doc = "If the preimage was previously requested, no fees or deposits are taken for providing"]
                    #[doc = "the preimage. Otherwise, a deposit is taken proportional to the size of the preimage."]
                    note_preimage {
                        bytes: ::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
                    },
                    #[codec(index = 1)]
                    #[doc = "Clear an unrequested preimage from the runtime storage."]
                    #[doc = ""]
                    #[doc = "If `len` is provided, then it will be a much cheaper operation."]
                    #[doc = ""]
                    #[doc = "- `hash`: The hash of the preimage to be removed from the store."]
                    #[doc = "- `len`: The length of the preimage of `hash`."]
                    unnote_preimage {
                        hash: ::subxt::ext::subxt_core::utils::H256,
                    },
                    #[codec(index = 2)]
                    #[doc = "Request a preimage be uploaded to the chain without paying any fees or deposits."]
                    #[doc = ""]
                    #[doc = "If the preimage requests has already been provided on-chain, we unreserve any deposit"]
                    #[doc = "a user may have paid, and take the control of the preimage out of their hands."]
                    request_preimage {
                        hash: ::subxt::ext::subxt_core::utils::H256,
                    },
                    #[codec(index = 3)]
                    #[doc = "Clear a previously made request for a preimage."]
                    #[doc = ""]
                    #[doc = "NOTE: THIS MUST NOT BE CALLED ON `hash` MORE TIMES THAN `request_preimage`."]
                    unrequest_preimage {
                        hash: ::subxt::ext::subxt_core::utils::H256,
                    },
                    #[codec(index = 4)]
                    #[doc = "Ensure that the a bulk of pre-images is upgraded."]
                    #[doc = ""]
                    #[doc = "The caller pays no fee if at least 90% of pre-images were successfully updated."]
                    ensure_updated {
                        hashes: ::subxt::ext::subxt_core::alloc::vec::Vec<
                            ::subxt::ext::subxt_core::utils::H256,
                        >,
                    },
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "The `Error` enum of this pallet."]
                pub enum Error {
                    #[codec(index = 0)]
                    #[doc = "Preimage is too large to store on-chain."]
                    TooBig,
                    #[codec(index = 1)]
                    #[doc = "Preimage has already been noted on-chain."]
                    AlreadyNoted,
                    #[codec(index = 2)]
                    #[doc = "The user is not authorized to perform this action."]
                    NotAuthorized,
                    #[codec(index = 3)]
                    #[doc = "The preimage cannot be removed since it has not yet been noted."]
                    NotNoted,
                    #[codec(index = 4)]
                    #[doc = "A preimage may not be removed when there are outstanding requests."]
                    Requested,
                    #[codec(index = 5)]
                    #[doc = "The preimage request cannot be removed since no outstanding requests exist."]
                    NotRequested,
                    #[codec(index = 6)]
                    #[doc = "More than `MAX_HASH_UPGRADE_BULK_COUNT` hashes were requested to be upgraded at once."]
                    TooMany,
                    #[codec(index = 7)]
                    #[doc = "Too few hashes were requested to be upgraded (i.e. zero)."]
                    TooFew,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "The `Event` enum of this pallet"]
                pub enum Event {
                    #[codec(index = 0)]
                    #[doc = "A preimage has been noted."]
                    Noted {
                        hash: ::subxt::ext::subxt_core::utils::H256,
                    },
                    #[codec(index = 1)]
                    #[doc = "A preimage has been requested."]
                    Requested {
                        hash: ::subxt::ext::subxt_core::utils::H256,
                    },
                    #[codec(index = 2)]
                    #[doc = "A preimage has ben cleared."]
                    Cleared {
                        hash: ::subxt::ext::subxt_core::utils::H256,
                    },
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum HoldReason {
                    #[codec(index = 0)]
                    Preimage,
                }
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub enum OldRequestStatus<_0, _1> {
                #[codec(index = 0)]
                Unrequested {
                    deposit: (_0, _1),
                    len: ::core::primitive::u32,
                },
                #[codec(index = 1)]
                Requested {
                    deposit: ::core::option::Option<(_0, _1)>,
                    count: ::core::primitive::u32,
                    len: ::core::option::Option<::core::primitive::u32>,
                },
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub enum RequestStatus<_0, _1> {
                #[codec(index = 0)]
                Unrequested {
                    ticket: (_0, _1),
                    len: ::core::primitive::u32,
                },
                #[codec(index = 1)]
                Requested {
                    maybe_ticket: ::core::option::Option<(_0, _1)>,
                    count: ::core::primitive::u32,
                    maybe_len: ::core::option::Option<::core::primitive::u32>,
                },
            }
        }
        pub mod pallet_proxy {
            use super::runtime_types;
            pub mod pallet {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "Contains a variant per dispatchable extrinsic that this pallet has."]
                pub enum Call {
                    #[codec(index = 0)]
                    #[doc = "Dispatch the given `call` from an account that the sender is authorised for through"]
                    #[doc = "`add_proxy`."]
                    #[doc = ""]
                    #[doc = "The dispatch origin for this call must be _Signed_."]
                    #[doc = ""]
                    #[doc = "Parameters:"]
                    #[doc = "- `real`: The account that the proxy will make a call on behalf of."]
                    #[doc = "- `force_proxy_type`: Specify the exact proxy type to be used and checked for this call."]
                    #[doc = "- `call`: The call to be made by the `real` account."]
                    proxy {
                        real: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                        force_proxy_type:
                            ::core::option::Option<runtime_types::vara_runtime::ProxyType>,
                        call: ::subxt::ext::subxt_core::alloc::boxed::Box<
                            runtime_types::vara_runtime::RuntimeCall,
                        >,
                    },
                    #[codec(index = 1)]
                    #[doc = "Register a proxy account for the sender that is able to make calls on its behalf."]
                    #[doc = ""]
                    #[doc = "The dispatch origin for this call must be _Signed_."]
                    #[doc = ""]
                    #[doc = "Parameters:"]
                    #[doc = "- `proxy`: The account that the `caller` would like to make a proxy."]
                    #[doc = "- `proxy_type`: The permissions allowed for this proxy account."]
                    #[doc = "- `delay`: The announcement period required of the initial proxy. Will generally be"]
                    #[doc = "zero."]
                    add_proxy {
                        delegate: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                        proxy_type: runtime_types::vara_runtime::ProxyType,
                        delay: ::core::primitive::u32,
                    },
                    #[codec(index = 2)]
                    #[doc = "Unregister a proxy account for the sender."]
                    #[doc = ""]
                    #[doc = "The dispatch origin for this call must be _Signed_."]
                    #[doc = ""]
                    #[doc = "Parameters:"]
                    #[doc = "- `proxy`: The account that the `caller` would like to remove as a proxy."]
                    #[doc = "- `proxy_type`: The permissions currently enabled for the removed proxy account."]
                    remove_proxy {
                        delegate: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                        proxy_type: runtime_types::vara_runtime::ProxyType,
                        delay: ::core::primitive::u32,
                    },
                    #[codec(index = 3)]
                    #[doc = "Unregister all proxy accounts for the sender."]
                    #[doc = ""]
                    #[doc = "The dispatch origin for this call must be _Signed_."]
                    #[doc = ""]
                    #[doc = "WARNING: This may be called on accounts created by `pure`, however if done, then"]
                    #[doc = "the unreserved fees will be inaccessible. **All access to this account will be lost.**"]
                    remove_proxies,
                    #[codec(index = 4)]
                    #[doc = "Spawn a fresh new account that is guaranteed to be otherwise inaccessible, and"]
                    #[doc = "initialize it with a proxy of `proxy_type` for `origin` sender."]
                    #[doc = ""]
                    #[doc = "Requires a `Signed` origin."]
                    #[doc = ""]
                    #[doc = "- `proxy_type`: The type of the proxy that the sender will be registered as over the"]
                    #[doc = "new account. This will almost always be the most permissive `ProxyType` possible to"]
                    #[doc = "allow for maximum flexibility."]
                    #[doc = "- `index`: A disambiguation index, in case this is called multiple times in the same"]
                    #[doc = "transaction (e.g. with `utility::batch`). Unless you're using `batch` you probably just"]
                    #[doc = "want to use `0`."]
                    #[doc = "- `delay`: The announcement period required of the initial proxy. Will generally be"]
                    #[doc = "zero."]
                    #[doc = ""]
                    #[doc = "Fails with `Duplicate` if this has already been called in this transaction, from the"]
                    #[doc = "same sender, with the same parameters."]
                    #[doc = ""]
                    #[doc = "Fails if there are insufficient funds to pay for deposit."]
                    create_pure {
                        proxy_type: runtime_types::vara_runtime::ProxyType,
                        delay: ::core::primitive::u32,
                        index: ::core::primitive::u16,
                    },
                    #[codec(index = 5)]
                    #[doc = "Removes a previously spawned pure proxy."]
                    #[doc = ""]
                    #[doc = "WARNING: **All access to this account will be lost.** Any funds held in it will be"]
                    #[doc = "inaccessible."]
                    #[doc = ""]
                    #[doc = "Requires a `Signed` origin, and the sender account must have been created by a call to"]
                    #[doc = "`pure` with corresponding parameters."]
                    #[doc = ""]
                    #[doc = "- `spawner`: The account that originally called `pure` to create this account."]
                    #[doc = "- `index`: The disambiguation index originally passed to `pure`. Probably `0`."]
                    #[doc = "- `proxy_type`: The proxy type originally passed to `pure`."]
                    #[doc = "- `height`: The height of the chain when the call to `pure` was processed."]
                    #[doc = "- `ext_index`: The extrinsic index in which the call to `pure` was processed."]
                    #[doc = ""]
                    #[doc = "Fails with `NoPermission` in case the caller is not a previously created pure"]
                    #[doc = "account whose `pure` call has corresponding parameters."]
                    kill_pure {
                        spawner: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                        proxy_type: runtime_types::vara_runtime::ProxyType,
                        index: ::core::primitive::u16,
                        #[codec(compact)]
                        height: ::core::primitive::u32,
                        #[codec(compact)]
                        ext_index: ::core::primitive::u32,
                    },
                    #[codec(index = 6)]
                    #[doc = "Publish the hash of a proxy-call that will be made in the future."]
                    #[doc = ""]
                    #[doc = "This must be called some number of blocks before the corresponding `proxy` is attempted"]
                    #[doc = "if the delay associated with the proxy relationship is greater than zero."]
                    #[doc = ""]
                    #[doc = "No more than `MaxPending` announcements may be made at any one time."]
                    #[doc = ""]
                    #[doc = "This will take a deposit of `AnnouncementDepositFactor` as well as"]
                    #[doc = "`AnnouncementDepositBase` if there are no other pending announcements."]
                    #[doc = ""]
                    #[doc = "The dispatch origin for this call must be _Signed_ and a proxy of `real`."]
                    #[doc = ""]
                    #[doc = "Parameters:"]
                    #[doc = "- `real`: The account that the proxy will make a call on behalf of."]
                    #[doc = "- `call_hash`: The hash of the call to be made by the `real` account."]
                    announce {
                        real: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                        call_hash: ::subxt::ext::subxt_core::utils::H256,
                    },
                    #[codec(index = 7)]
                    #[doc = "Remove a given announcement."]
                    #[doc = ""]
                    #[doc = "May be called by a proxy account to remove a call they previously announced and return"]
                    #[doc = "the deposit."]
                    #[doc = ""]
                    #[doc = "The dispatch origin for this call must be _Signed_."]
                    #[doc = ""]
                    #[doc = "Parameters:"]
                    #[doc = "- `real`: The account that the proxy will make a call on behalf of."]
                    #[doc = "- `call_hash`: The hash of the call to be made by the `real` account."]
                    remove_announcement {
                        real: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                        call_hash: ::subxt::ext::subxt_core::utils::H256,
                    },
                    #[codec(index = 8)]
                    #[doc = "Remove the given announcement of a delegate."]
                    #[doc = ""]
                    #[doc = "May be called by a target (proxied) account to remove a call that one of their delegates"]
                    #[doc = "(`delegate`) has announced they want to execute. The deposit is returned."]
                    #[doc = ""]
                    #[doc = "The dispatch origin for this call must be _Signed_."]
                    #[doc = ""]
                    #[doc = "Parameters:"]
                    #[doc = "- `delegate`: The account that previously announced the call."]
                    #[doc = "- `call_hash`: The hash of the call to be made."]
                    reject_announcement {
                        delegate: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                        call_hash: ::subxt::ext::subxt_core::utils::H256,
                    },
                    #[codec(index = 9)]
                    #[doc = "Dispatch the given `call` from an account that the sender is authorized for through"]
                    #[doc = "`add_proxy`."]
                    #[doc = ""]
                    #[doc = "Removes any corresponding announcement(s)."]
                    #[doc = ""]
                    #[doc = "The dispatch origin for this call must be _Signed_."]
                    #[doc = ""]
                    #[doc = "Parameters:"]
                    #[doc = "- `real`: The account that the proxy will make a call on behalf of."]
                    #[doc = "- `force_proxy_type`: Specify the exact proxy type to be used and checked for this call."]
                    #[doc = "- `call`: The call to be made by the `real` account."]
                    proxy_announced {
                        delegate: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                        real: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                        force_proxy_type:
                            ::core::option::Option<runtime_types::vara_runtime::ProxyType>,
                        call: ::subxt::ext::subxt_core::alloc::boxed::Box<
                            runtime_types::vara_runtime::RuntimeCall,
                        >,
                    },
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "The `Error` enum of this pallet."]
                pub enum Error {
                    #[codec(index = 0)]
                    #[doc = "There are too many proxies registered or too many announcements pending."]
                    TooMany,
                    #[codec(index = 1)]
                    #[doc = "Proxy registration not found."]
                    NotFound,
                    #[codec(index = 2)]
                    #[doc = "Sender is not a proxy of the account to be proxied."]
                    NotProxy,
                    #[codec(index = 3)]
                    #[doc = "A call which is incompatible with the proxy type's filter was attempted."]
                    Unproxyable,
                    #[codec(index = 4)]
                    #[doc = "Account is already a proxy."]
                    Duplicate,
                    #[codec(index = 5)]
                    #[doc = "Call may not be made by proxy because it may escalate its privileges."]
                    NoPermission,
                    #[codec(index = 6)]
                    #[doc = "Announcement, if made at all, was made too recently."]
                    Unannounced,
                    #[codec(index = 7)]
                    #[doc = "Cannot add self as proxy."]
                    NoSelfProxy,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "The `Event` enum of this pallet"]
                pub enum Event {
                    #[codec(index = 0)]
                    #[doc = "A proxy was executed correctly, with the given."]
                    ProxyExecuted {
                        result:
                            ::core::result::Result<(), runtime_types::sp_runtime::DispatchError>,
                    },
                    #[codec(index = 1)]
                    #[doc = "A pure account has been created by new proxy with given"]
                    #[doc = "disambiguation index and proxy type."]
                    PureCreated {
                        pure: ::subxt::ext::subxt_core::utils::AccountId32,
                        who: ::subxt::ext::subxt_core::utils::AccountId32,
                        proxy_type: runtime_types::vara_runtime::ProxyType,
                        disambiguation_index: ::core::primitive::u16,
                    },
                    #[codec(index = 2)]
                    #[doc = "An announcement was placed to make a call in the future."]
                    Announced {
                        real: ::subxt::ext::subxt_core::utils::AccountId32,
                        proxy: ::subxt::ext::subxt_core::utils::AccountId32,
                        call_hash: ::subxt::ext::subxt_core::utils::H256,
                    },
                    #[codec(index = 3)]
                    #[doc = "A proxy was added."]
                    ProxyAdded {
                        delegator: ::subxt::ext::subxt_core::utils::AccountId32,
                        delegatee: ::subxt::ext::subxt_core::utils::AccountId32,
                        proxy_type: runtime_types::vara_runtime::ProxyType,
                        delay: ::core::primitive::u32,
                    },
                    #[codec(index = 4)]
                    #[doc = "A proxy was removed."]
                    ProxyRemoved {
                        delegator: ::subxt::ext::subxt_core::utils::AccountId32,
                        delegatee: ::subxt::ext::subxt_core::utils::AccountId32,
                        proxy_type: runtime_types::vara_runtime::ProxyType,
                        delay: ::core::primitive::u32,
                    },
                }
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub struct Announcement<_0, _1, _2> {
                pub real: _0,
                pub call_hash: _1,
                pub height: _2,
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub struct ProxyDefinition<_0, _1, _2> {
                pub delegate: _0,
                pub proxy_type: _1,
                pub delay: _2,
            }
        }
        pub mod pallet_ranked_collective {
            use super::runtime_types;
            pub mod pallet {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "Contains a variant per dispatchable extrinsic that this pallet has."]
                pub enum Call {
                    #[codec(index = 0)]
                    #[doc = "Introduce a new member."]
                    #[doc = ""]
                    #[doc = "- `origin`: Must be the `AddOrigin`."]
                    #[doc = "- `who`: Account of non-member which will become a member."]
                    #[doc = ""]
                    #[doc = "Weight: `O(1)`"]
                    add_member {
                        who: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                    },
                    #[codec(index = 1)]
                    #[doc = "Increment the rank of an existing member by one."]
                    #[doc = ""]
                    #[doc = "- `origin`: Must be the `PromoteOrigin`."]
                    #[doc = "- `who`: Account of existing member."]
                    #[doc = ""]
                    #[doc = "Weight: `O(1)`"]
                    promote_member {
                        who: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                    },
                    #[codec(index = 2)]
                    #[doc = "Decrement the rank of an existing member by one. If the member is already at rank zero,"]
                    #[doc = "then they are removed entirely."]
                    #[doc = ""]
                    #[doc = "- `origin`: Must be the `DemoteOrigin`."]
                    #[doc = "- `who`: Account of existing member of rank greater than zero."]
                    #[doc = ""]
                    #[doc = "Weight: `O(1)`, less if the member's index is highest in its rank."]
                    demote_member {
                        who: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                    },
                    #[codec(index = 3)]
                    #[doc = "Remove the member entirely."]
                    #[doc = ""]
                    #[doc = "- `origin`: Must be the `RemoveOrigin`."]
                    #[doc = "- `who`: Account of existing member of rank greater than zero."]
                    #[doc = "- `min_rank`: The rank of the member or greater."]
                    #[doc = ""]
                    #[doc = "Weight: `O(min_rank)`."]
                    remove_member {
                        who: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                        min_rank: ::core::primitive::u16,
                    },
                    #[codec(index = 4)]
                    #[doc = "Add an aye or nay vote for the sender to the given proposal."]
                    #[doc = ""]
                    #[doc = "- `origin`: Must be `Signed` by a member account."]
                    #[doc = "- `poll`: Index of a poll which is ongoing."]
                    #[doc = "- `aye`: `true` if the vote is to approve the proposal, `false` otherwise."]
                    #[doc = ""]
                    #[doc = "Transaction fees are be waived if the member is voting on any particular proposal"]
                    #[doc = "for the first time and the call is successful. Subsequent vote changes will charge a"]
                    #[doc = "fee."]
                    #[doc = ""]
                    #[doc = "Weight: `O(1)`, less if there was no previous vote on the poll by the member."]
                    vote {
                        poll: ::core::primitive::u32,
                        aye: ::core::primitive::bool,
                    },
                    #[codec(index = 5)]
                    #[doc = "Remove votes from the given poll. It must have ended."]
                    #[doc = ""]
                    #[doc = "- `origin`: Must be `Signed` by any account."]
                    #[doc = "- `poll_index`: Index of a poll which is completed and for which votes continue to"]
                    #[doc = "  exist."]
                    #[doc = "- `max`: Maximum number of vote items from remove in this call."]
                    #[doc = ""]
                    #[doc = "Transaction fees are waived if the operation is successful."]
                    #[doc = ""]
                    #[doc = "Weight `O(max)` (less if there are fewer items to remove than `max`)."]
                    cleanup_poll {
                        poll_index: ::core::primitive::u32,
                        max: ::core::primitive::u32,
                    },
                    #[codec(index = 6)]
                    #[doc = "Exchanges a member with a new account and the same existing rank."]
                    #[doc = ""]
                    #[doc = "- `origin`: Must be the `ExchangeOrigin`."]
                    #[doc = "- `who`: Account of existing member of rank greater than zero to be exchanged."]
                    #[doc = "- `new_who`: New Account of existing member of rank greater than zero to exchanged to."]
                    exchange_member {
                        who: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                        new_who: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                    },
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "The `Error` enum of this pallet."]
                pub enum Error {
                    #[codec(index = 0)]
                    #[doc = "Account is already a member."]
                    AlreadyMember,
                    #[codec(index = 1)]
                    #[doc = "Account is not a member."]
                    NotMember,
                    #[codec(index = 2)]
                    #[doc = "The given poll index is unknown or has closed."]
                    NotPolling,
                    #[codec(index = 3)]
                    #[doc = "The given poll is still ongoing."]
                    Ongoing,
                    #[codec(index = 4)]
                    #[doc = "There are no further records to be removed."]
                    NoneRemaining,
                    #[codec(index = 5)]
                    #[doc = "Unexpected error in state."]
                    Corruption,
                    #[codec(index = 6)]
                    #[doc = "The member's rank is too low to vote."]
                    RankTooLow,
                    #[codec(index = 7)]
                    #[doc = "The information provided is incorrect."]
                    InvalidWitness,
                    #[codec(index = 8)]
                    #[doc = "The origin is not sufficiently privileged to do the operation."]
                    NoPermission,
                    #[codec(index = 9)]
                    #[doc = "The new member to exchange is the same as the old member"]
                    SameMember,
                    #[codec(index = 10)]
                    #[doc = "The max member count for the rank has been reached."]
                    TooManyMembers,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "The `Event` enum of this pallet"]
                pub enum Event {
                    #[codec(index = 0)]
                    #[doc = "A member `who` has been added."]
                    MemberAdded {
                        who: ::subxt::ext::subxt_core::utils::AccountId32,
                    },
                    #[codec(index = 1)]
                    #[doc = "The member `who`se rank has been changed to the given `rank`."]
                    RankChanged {
                        who: ::subxt::ext::subxt_core::utils::AccountId32,
                        rank: ::core::primitive::u16,
                    },
                    #[codec(index = 2)]
                    #[doc = "The member `who` of given `rank` has been removed from the collective."]
                    MemberRemoved {
                        who: ::subxt::ext::subxt_core::utils::AccountId32,
                        rank: ::core::primitive::u16,
                    },
                    #[codec(index = 3)]
                    #[doc = "The member `who` has voted for the `poll` with the given `vote` leading to an updated"]
                    #[doc = "`tally`."]
                    Voted {
                        who: ::subxt::ext::subxt_core::utils::AccountId32,
                        poll: ::core::primitive::u32,
                        vote: runtime_types::pallet_ranked_collective::VoteRecord,
                        tally: runtime_types::pallet_ranked_collective::Tally,
                    },
                    #[codec(index = 4)]
                    #[doc = "The member `who` had their `AccountId` changed to `new_who`."]
                    MemberExchanged {
                        who: ::subxt::ext::subxt_core::utils::AccountId32,
                        new_who: ::subxt::ext::subxt_core::utils::AccountId32,
                    },
                }
            }
            #[derive(
                ::subxt::ext ::subxt_core::ext::codec::CompactAs,
                Debug,
                crate::gp::Decode,
                crate::gp::DecodeAsType,
                crate::gp::Encode,
            )]
            pub struct MemberRecord {
                pub rank: ::core::primitive::u16,
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub struct Tally {
                pub bare_ayes: ::core::primitive::u32,
                pub ayes: ::core::primitive::u32,
                pub nays: ::core::primitive::u32,
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub enum VoteRecord {
                #[codec(index = 0)]
                Aye(::core::primitive::u32),
                #[codec(index = 1)]
                Nay(::core::primitive::u32),
            }
        }
        pub mod pallet_referenda {
            use super::runtime_types;
            pub mod pallet {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "Contains a variant per dispatchable extrinsic that this pallet has."]
                pub enum Call {
                    #[codec(index = 0)]
                    #[doc = "Propose a referendum on a privileged action."]
                    #[doc = ""]
                    #[doc = "- `origin`: must be `SubmitOrigin` and the account must have `SubmissionDeposit` funds"]
                    #[doc = "  available."]
                    #[doc = "- `proposal_origin`: The origin from which the proposal should be executed."]
                    #[doc = "- `proposal`: The proposal."]
                    #[doc = "- `enactment_moment`: The moment that the proposal should be enacted."]
                    #[doc = ""]
                    #[doc = "Emits `Submitted`."]
                    submit {
                        proposal_origin: ::subxt::ext::subxt_core::alloc::boxed::Box<
                            runtime_types::vara_runtime::OriginCaller,
                        >,
                        proposal: runtime_types::frame_support::traits::preimages::Bounded<
                            runtime_types::vara_runtime::RuntimeCall,
                            runtime_types::sp_runtime::traits::BlakeTwo256,
                        >,
                        enactment_moment:
                            runtime_types::frame_support::traits::schedule::DispatchTime<
                                ::core::primitive::u32,
                            >,
                    },
                    #[codec(index = 1)]
                    #[doc = "Post the Decision Deposit for a referendum."]
                    #[doc = ""]
                    #[doc = "- `origin`: must be `Signed` and the account must have funds available for the"]
                    #[doc = "  referendum's track's Decision Deposit."]
                    #[doc = "- `index`: The index of the submitted referendum whose Decision Deposit is yet to be"]
                    #[doc = "  posted."]
                    #[doc = ""]
                    #[doc = "Emits `DecisionDepositPlaced`."]
                    place_decision_deposit { index: ::core::primitive::u32 },
                    #[codec(index = 2)]
                    #[doc = "Refund the Decision Deposit for a closed referendum back to the depositor."]
                    #[doc = ""]
                    #[doc = "- `origin`: must be `Signed` or `Root`."]
                    #[doc = "- `index`: The index of a closed referendum whose Decision Deposit has not yet been"]
                    #[doc = "  refunded."]
                    #[doc = ""]
                    #[doc = "Emits `DecisionDepositRefunded`."]
                    refund_decision_deposit { index: ::core::primitive::u32 },
                    #[codec(index = 3)]
                    #[doc = "Cancel an ongoing referendum."]
                    #[doc = ""]
                    #[doc = "- `origin`: must be the `CancelOrigin`."]
                    #[doc = "- `index`: The index of the referendum to be cancelled."]
                    #[doc = ""]
                    #[doc = "Emits `Cancelled`."]
                    cancel { index: ::core::primitive::u32 },
                    #[codec(index = 4)]
                    #[doc = "Cancel an ongoing referendum and slash the deposits."]
                    #[doc = ""]
                    #[doc = "- `origin`: must be the `KillOrigin`."]
                    #[doc = "- `index`: The index of the referendum to be cancelled."]
                    #[doc = ""]
                    #[doc = "Emits `Killed` and `DepositSlashed`."]
                    kill { index: ::core::primitive::u32 },
                    #[codec(index = 5)]
                    #[doc = "Advance a referendum onto its next logical state. Only used internally."]
                    #[doc = ""]
                    #[doc = "- `origin`: must be `Root`."]
                    #[doc = "- `index`: the referendum to be advanced."]
                    nudge_referendum { index: ::core::primitive::u32 },
                    #[codec(index = 6)]
                    #[doc = "Advance a track onto its next logical state. Only used internally."]
                    #[doc = ""]
                    #[doc = "- `origin`: must be `Root`."]
                    #[doc = "- `track`: the track to be advanced."]
                    #[doc = ""]
                    #[doc = "Action item for when there is now one fewer referendum in the deciding phase and the"]
                    #[doc = "`DecidingCount` is not yet updated. This means that we should either:"]
                    #[doc = "- begin deciding another referendum (and leave `DecidingCount` alone); or"]
                    #[doc = "- decrement `DecidingCount`."]
                    one_fewer_deciding { track: ::core::primitive::u16 },
                    #[codec(index = 7)]
                    #[doc = "Refund the Submission Deposit for a closed referendum back to the depositor."]
                    #[doc = ""]
                    #[doc = "- `origin`: must be `Signed` or `Root`."]
                    #[doc = "- `index`: The index of a closed referendum whose Submission Deposit has not yet been"]
                    #[doc = "  refunded."]
                    #[doc = ""]
                    #[doc = "Emits `SubmissionDepositRefunded`."]
                    refund_submission_deposit { index: ::core::primitive::u32 },
                    #[codec(index = 8)]
                    #[doc = "Set or clear metadata of a referendum."]
                    #[doc = ""]
                    #[doc = "Parameters:"]
                    #[doc = "- `origin`: Must be `Signed` by a creator of a referendum or by anyone to clear a"]
                    #[doc = "  metadata of a finished referendum."]
                    #[doc = "- `index`:  The index of a referendum to set or clear metadata for."]
                    #[doc = "- `maybe_hash`: The hash of an on-chain stored preimage. `None` to clear a metadata."]
                    set_metadata {
                        index: ::core::primitive::u32,
                        maybe_hash: ::core::option::Option<::subxt::ext::subxt_core::utils::H256>,
                    },
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "The `Error` enum of this pallet."]
                pub enum Error {
                    #[codec(index = 0)]
                    #[doc = "Referendum is not ongoing."]
                    NotOngoing,
                    #[codec(index = 1)]
                    #[doc = "Referendum's decision deposit is already paid."]
                    HasDeposit,
                    #[codec(index = 2)]
                    #[doc = "The track identifier given was invalid."]
                    BadTrack,
                    #[codec(index = 3)]
                    #[doc = "There are already a full complement of referenda in progress for this track."]
                    Full,
                    #[codec(index = 4)]
                    #[doc = "The queue of the track is empty."]
                    QueueEmpty,
                    #[codec(index = 5)]
                    #[doc = "The referendum index provided is invalid in this context."]
                    BadReferendum,
                    #[codec(index = 6)]
                    #[doc = "There was nothing to do in the advancement."]
                    NothingToDo,
                    #[codec(index = 7)]
                    #[doc = "No track exists for the proposal origin."]
                    NoTrack,
                    #[codec(index = 8)]
                    #[doc = "Any deposit cannot be refunded until after the decision is over."]
                    Unfinished,
                    #[codec(index = 9)]
                    #[doc = "The deposit refunder is not the depositor."]
                    NoPermission,
                    #[codec(index = 10)]
                    #[doc = "The deposit cannot be refunded since none was made."]
                    NoDeposit,
                    #[codec(index = 11)]
                    #[doc = "The referendum status is invalid for this operation."]
                    BadStatus,
                    #[codec(index = 12)]
                    #[doc = "The preimage does not exist."]
                    PreimageNotExist,
                    #[codec(index = 13)]
                    #[doc = "The preimage is stored with a different length than the one provided."]
                    PreimageStoredWithDifferentLength,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "The `Event` enum of this pallet"]
                pub enum Event1 {
                    #[codec(index = 0)]
                    #[doc = "A referendum has been submitted."]
                    Submitted {
                        index: ::core::primitive::u32,
                        track: ::core::primitive::u16,
                        proposal: runtime_types::frame_support::traits::preimages::Bounded<
                            runtime_types::vara_runtime::RuntimeCall,
                            runtime_types::sp_runtime::traits::BlakeTwo256,
                        >,
                    },
                    #[codec(index = 1)]
                    #[doc = "The decision deposit has been placed."]
                    DecisionDepositPlaced {
                        index: ::core::primitive::u32,
                        who: ::subxt::ext::subxt_core::utils::AccountId32,
                        amount: ::core::primitive::u128,
                    },
                    #[codec(index = 2)]
                    #[doc = "The decision deposit has been refunded."]
                    DecisionDepositRefunded {
                        index: ::core::primitive::u32,
                        who: ::subxt::ext::subxt_core::utils::AccountId32,
                        amount: ::core::primitive::u128,
                    },
                    #[codec(index = 3)]
                    #[doc = "A deposit has been slashed."]
                    DepositSlashed {
                        who: ::subxt::ext::subxt_core::utils::AccountId32,
                        amount: ::core::primitive::u128,
                    },
                    #[codec(index = 4)]
                    #[doc = "A referendum has moved into the deciding phase."]
                    DecisionStarted {
                        index: ::core::primitive::u32,
                        track: ::core::primitive::u16,
                        proposal: runtime_types::frame_support::traits::preimages::Bounded<
                            runtime_types::vara_runtime::RuntimeCall,
                            runtime_types::sp_runtime::traits::BlakeTwo256,
                        >,
                        tally: runtime_types::pallet_conviction_voting::types::Tally<
                            ::core::primitive::u128,
                        >,
                    },
                    #[codec(index = 5)]
                    ConfirmStarted { index: ::core::primitive::u32 },
                    #[codec(index = 6)]
                    ConfirmAborted { index: ::core::primitive::u32 },
                    #[codec(index = 7)]
                    #[doc = "A referendum has ended its confirmation phase and is ready for approval."]
                    Confirmed {
                        index: ::core::primitive::u32,
                        tally: runtime_types::pallet_conviction_voting::types::Tally<
                            ::core::primitive::u128,
                        >,
                    },
                    #[codec(index = 8)]
                    #[doc = "A referendum has been approved and its proposal has been scheduled."]
                    Approved { index: ::core::primitive::u32 },
                    #[codec(index = 9)]
                    #[doc = "A proposal has been rejected by referendum."]
                    Rejected {
                        index: ::core::primitive::u32,
                        tally: runtime_types::pallet_conviction_voting::types::Tally<
                            ::core::primitive::u128,
                        >,
                    },
                    #[codec(index = 10)]
                    #[doc = "A referendum has been timed out without being decided."]
                    TimedOut {
                        index: ::core::primitive::u32,
                        tally: runtime_types::pallet_conviction_voting::types::Tally<
                            ::core::primitive::u128,
                        >,
                    },
                    #[codec(index = 11)]
                    #[doc = "A referendum has been cancelled."]
                    Cancelled {
                        index: ::core::primitive::u32,
                        tally: runtime_types::pallet_conviction_voting::types::Tally<
                            ::core::primitive::u128,
                        >,
                    },
                    #[codec(index = 12)]
                    #[doc = "A referendum has been killed."]
                    Killed {
                        index: ::core::primitive::u32,
                        tally: runtime_types::pallet_conviction_voting::types::Tally<
                            ::core::primitive::u128,
                        >,
                    },
                    #[codec(index = 13)]
                    #[doc = "The submission deposit has been refunded."]
                    SubmissionDepositRefunded {
                        index: ::core::primitive::u32,
                        who: ::subxt::ext::subxt_core::utils::AccountId32,
                        amount: ::core::primitive::u128,
                    },
                    #[codec(index = 14)]
                    #[doc = "Metadata for a referendum has been set."]
                    MetadataSet {
                        index: ::core::primitive::u32,
                        hash: ::subxt::ext::subxt_core::utils::H256,
                    },
                    #[codec(index = 15)]
                    #[doc = "Metadata for a referendum has been cleared."]
                    MetadataCleared {
                        index: ::core::primitive::u32,
                        hash: ::subxt::ext::subxt_core::utils::H256,
                    },
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "The `Event` enum of this pallet"]
                pub enum Event2 {
                    #[codec(index = 0)]
                    #[doc = "A referendum has been submitted."]
                    Submitted {
                        index: ::core::primitive::u32,
                        track: ::core::primitive::u16,
                        proposal: runtime_types::frame_support::traits::preimages::Bounded<
                            runtime_types::vara_runtime::RuntimeCall,
                            runtime_types::sp_runtime::traits::BlakeTwo256,
                        >,
                    },
                    #[codec(index = 1)]
                    #[doc = "The decision deposit has been placed."]
                    DecisionDepositPlaced {
                        index: ::core::primitive::u32,
                        who: ::subxt::ext::subxt_core::utils::AccountId32,
                        amount: ::core::primitive::u128,
                    },
                    #[codec(index = 2)]
                    #[doc = "The decision deposit has been refunded."]
                    DecisionDepositRefunded {
                        index: ::core::primitive::u32,
                        who: ::subxt::ext::subxt_core::utils::AccountId32,
                        amount: ::core::primitive::u128,
                    },
                    #[codec(index = 3)]
                    #[doc = "A deposit has been slashed."]
                    DepositSlashed {
                        who: ::subxt::ext::subxt_core::utils::AccountId32,
                        amount: ::core::primitive::u128,
                    },
                    #[codec(index = 4)]
                    #[doc = "A referendum has moved into the deciding phase."]
                    DecisionStarted {
                        index: ::core::primitive::u32,
                        track: ::core::primitive::u16,
                        proposal: runtime_types::frame_support::traits::preimages::Bounded<
                            runtime_types::vara_runtime::RuntimeCall,
                            runtime_types::sp_runtime::traits::BlakeTwo256,
                        >,
                        tally: runtime_types::pallet_ranked_collective::Tally,
                    },
                    #[codec(index = 5)]
                    ConfirmStarted { index: ::core::primitive::u32 },
                    #[codec(index = 6)]
                    ConfirmAborted { index: ::core::primitive::u32 },
                    #[codec(index = 7)]
                    #[doc = "A referendum has ended its confirmation phase and is ready for approval."]
                    Confirmed {
                        index: ::core::primitive::u32,
                        tally: runtime_types::pallet_ranked_collective::Tally,
                    },
                    #[codec(index = 8)]
                    #[doc = "A referendum has been approved and its proposal has been scheduled."]
                    Approved { index: ::core::primitive::u32 },
                    #[codec(index = 9)]
                    #[doc = "A proposal has been rejected by referendum."]
                    Rejected {
                        index: ::core::primitive::u32,
                        tally: runtime_types::pallet_ranked_collective::Tally,
                    },
                    #[codec(index = 10)]
                    #[doc = "A referendum has been timed out without being decided."]
                    TimedOut {
                        index: ::core::primitive::u32,
                        tally: runtime_types::pallet_ranked_collective::Tally,
                    },
                    #[codec(index = 11)]
                    #[doc = "A referendum has been cancelled."]
                    Cancelled {
                        index: ::core::primitive::u32,
                        tally: runtime_types::pallet_ranked_collective::Tally,
                    },
                    #[codec(index = 12)]
                    #[doc = "A referendum has been killed."]
                    Killed {
                        index: ::core::primitive::u32,
                        tally: runtime_types::pallet_ranked_collective::Tally,
                    },
                    #[codec(index = 13)]
                    #[doc = "The submission deposit has been refunded."]
                    SubmissionDepositRefunded {
                        index: ::core::primitive::u32,
                        who: ::subxt::ext::subxt_core::utils::AccountId32,
                        amount: ::core::primitive::u128,
                    },
                    #[codec(index = 14)]
                    #[doc = "Metadata for a referendum has been set."]
                    MetadataSet {
                        index: ::core::primitive::u32,
                        hash: ::subxt::ext::subxt_core::utils::H256,
                    },
                    #[codec(index = 15)]
                    #[doc = "Metadata for a referendum has been cleared."]
                    MetadataCleared {
                        index: ::core::primitive::u32,
                        hash: ::subxt::ext::subxt_core::utils::H256,
                    },
                }
            }
            pub mod types {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum Curve {
                    #[codec(index = 0)]
                    LinearDecreasing {
                        length: runtime_types::sp_arithmetic::per_things::Perbill,
                        floor: runtime_types::sp_arithmetic::per_things::Perbill,
                        ceil: runtime_types::sp_arithmetic::per_things::Perbill,
                    },
                    #[codec(index = 1)]
                    SteppedDecreasing {
                        begin: runtime_types::sp_arithmetic::per_things::Perbill,
                        end: runtime_types::sp_arithmetic::per_things::Perbill,
                        step: runtime_types::sp_arithmetic::per_things::Perbill,
                        period: runtime_types::sp_arithmetic::per_things::Perbill,
                    },
                    #[codec(index = 2)]
                    Reciprocal {
                        factor: runtime_types::sp_arithmetic::fixed_point::FixedI64,
                        x_offset: runtime_types::sp_arithmetic::fixed_point::FixedI64,
                        y_offset: runtime_types::sp_arithmetic::fixed_point::FixedI64,
                    },
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct DecidingStatus<_0> {
                    pub since: _0,
                    pub confirming: ::core::option::Option<_0>,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct Deposit<_0, _1> {
                    pub who: _0,
                    pub amount: _1,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum ReferendumInfo<_0, _1, _2, _3, _4, _5, _6, _7> {
                    #[codec(index = 0)]
                    Ongoing(
                        runtime_types::pallet_referenda::types::ReferendumStatus<
                            _0,
                            _1,
                            _2,
                            _3,
                            _4,
                            _5,
                            _6,
                            _7,
                        >,
                    ),
                    #[codec(index = 1)]
                    Approved(
                        _2,
                        ::core::option::Option<
                            runtime_types::pallet_referenda::types::Deposit<_6, _4>,
                        >,
                        ::core::option::Option<
                            runtime_types::pallet_referenda::types::Deposit<_6, _4>,
                        >,
                    ),
                    #[codec(index = 2)]
                    Rejected(
                        _2,
                        ::core::option::Option<
                            runtime_types::pallet_referenda::types::Deposit<_6, _4>,
                        >,
                        ::core::option::Option<
                            runtime_types::pallet_referenda::types::Deposit<_6, _4>,
                        >,
                    ),
                    #[codec(index = 3)]
                    Cancelled(
                        _2,
                        ::core::option::Option<
                            runtime_types::pallet_referenda::types::Deposit<_6, _4>,
                        >,
                        ::core::option::Option<
                            runtime_types::pallet_referenda::types::Deposit<_6, _4>,
                        >,
                    ),
                    #[codec(index = 4)]
                    TimedOut(
                        _2,
                        ::core::option::Option<
                            runtime_types::pallet_referenda::types::Deposit<_6, _4>,
                        >,
                        ::core::option::Option<
                            runtime_types::pallet_referenda::types::Deposit<_6, _4>,
                        >,
                    ),
                    #[codec(index = 5)]
                    Killed(_2),
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct ReferendumStatus<_0, _1, _2, _3, _4, _5, _6, _7> {
                    pub track: _0,
                    pub origin: _1,
                    pub proposal: _3,
                    pub enactment: runtime_types::frame_support::traits::schedule::DispatchTime<_2>,
                    pub submitted: _2,
                    pub submission_deposit: runtime_types::pallet_referenda::types::Deposit<_6, _4>,
                    pub decision_deposit: ::core::option::Option<
                        runtime_types::pallet_referenda::types::Deposit<_6, _4>,
                    >,
                    pub deciding: ::core::option::Option<
                        runtime_types::pallet_referenda::types::DecidingStatus<_2>,
                    >,
                    pub tally: _5,
                    pub in_queue: ::core::primitive::bool,
                    pub alarm: ::core::option::Option<(_2, _7)>,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct TrackInfo<_0, _1> {
                    pub name: ::subxt::ext::subxt_core::alloc::string::String,
                    pub max_deciding: ::core::primitive::u32,
                    pub decision_deposit: _0,
                    pub prepare_period: _1,
                    pub decision_period: _1,
                    pub confirm_period: _1,
                    pub min_enactment_period: _1,
                    pub min_approval: runtime_types::pallet_referenda::types::Curve,
                    pub min_support: runtime_types::pallet_referenda::types::Curve,
                }
            }
        }
        pub mod pallet_scheduler {
            use super::runtime_types;
            pub mod pallet {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "Contains a variant per dispatchable extrinsic that this pallet has."]
                pub enum Call {
                    #[codec(index = 0)]
                    #[doc = "Anonymously schedule a task."]
                    schedule {
                        when: ::core::primitive::u32,
                        maybe_periodic: ::core::option::Option<(
                            ::core::primitive::u32,
                            ::core::primitive::u32,
                        )>,
                        priority: ::core::primitive::u8,
                        call: ::subxt::ext::subxt_core::alloc::boxed::Box<
                            runtime_types::vara_runtime::RuntimeCall,
                        >,
                    },
                    #[codec(index = 1)]
                    #[doc = "Cancel an anonymously scheduled task."]
                    cancel {
                        when: ::core::primitive::u32,
                        index: ::core::primitive::u32,
                    },
                    #[codec(index = 2)]
                    #[doc = "Schedule a named task."]
                    schedule_named {
                        id: [::core::primitive::u8; 32usize],
                        when: ::core::primitive::u32,
                        maybe_periodic: ::core::option::Option<(
                            ::core::primitive::u32,
                            ::core::primitive::u32,
                        )>,
                        priority: ::core::primitive::u8,
                        call: ::subxt::ext::subxt_core::alloc::boxed::Box<
                            runtime_types::vara_runtime::RuntimeCall,
                        >,
                    },
                    #[codec(index = 3)]
                    #[doc = "Cancel a named scheduled task."]
                    cancel_named {
                        id: [::core::primitive::u8; 32usize],
                    },
                    #[codec(index = 4)]
                    #[doc = "Anonymously schedule a task after a delay."]
                    schedule_after {
                        after: ::core::primitive::u32,
                        maybe_periodic: ::core::option::Option<(
                            ::core::primitive::u32,
                            ::core::primitive::u32,
                        )>,
                        priority: ::core::primitive::u8,
                        call: ::subxt::ext::subxt_core::alloc::boxed::Box<
                            runtime_types::vara_runtime::RuntimeCall,
                        >,
                    },
                    #[codec(index = 5)]
                    #[doc = "Schedule a named task after a delay."]
                    schedule_named_after {
                        id: [::core::primitive::u8; 32usize],
                        after: ::core::primitive::u32,
                        maybe_periodic: ::core::option::Option<(
                            ::core::primitive::u32,
                            ::core::primitive::u32,
                        )>,
                        priority: ::core::primitive::u8,
                        call: ::subxt::ext::subxt_core::alloc::boxed::Box<
                            runtime_types::vara_runtime::RuntimeCall,
                        >,
                    },
                    #[codec(index = 6)]
                    #[doc = "Set a retry configuration for a task so that, in case its scheduled run fails, it will"]
                    #[doc = "be retried after `period` blocks, for a total amount of `retries` retries or until it"]
                    #[doc = "succeeds."]
                    #[doc = ""]
                    #[doc = "Tasks which need to be scheduled for a retry are still subject to weight metering and"]
                    #[doc = "agenda space, same as a regular task. If a periodic task fails, it will be scheduled"]
                    #[doc = "normally while the task is retrying."]
                    #[doc = ""]
                    #[doc = "Tasks scheduled as a result of a retry for a periodic task are unnamed, non-periodic"]
                    #[doc = "clones of the original task. Their retry configuration will be derived from the"]
                    #[doc = "original task's configuration, but will have a lower value for `remaining` than the"]
                    #[doc = "original `total_retries`."]
                    set_retry {
                        task: (::core::primitive::u32, ::core::primitive::u32),
                        retries: ::core::primitive::u8,
                        period: ::core::primitive::u32,
                    },
                    #[codec(index = 7)]
                    #[doc = "Set a retry configuration for a named task so that, in case its scheduled run fails, it"]
                    #[doc = "will be retried after `period` blocks, for a total amount of `retries` retries or until"]
                    #[doc = "it succeeds."]
                    #[doc = ""]
                    #[doc = "Tasks which need to be scheduled for a retry are still subject to weight metering and"]
                    #[doc = "agenda space, same as a regular task. If a periodic task fails, it will be scheduled"]
                    #[doc = "normally while the task is retrying."]
                    #[doc = ""]
                    #[doc = "Tasks scheduled as a result of a retry for a periodic task are unnamed, non-periodic"]
                    #[doc = "clones of the original task. Their retry configuration will be derived from the"]
                    #[doc = "original task's configuration, but will have a lower value for `remaining` than the"]
                    #[doc = "original `total_retries`."]
                    set_retry_named {
                        id: [::core::primitive::u8; 32usize],
                        retries: ::core::primitive::u8,
                        period: ::core::primitive::u32,
                    },
                    #[codec(index = 8)]
                    #[doc = "Removes the retry configuration of a task."]
                    cancel_retry {
                        task: (::core::primitive::u32, ::core::primitive::u32),
                    },
                    #[codec(index = 9)]
                    #[doc = "Cancel the retry configuration of a named task."]
                    cancel_retry_named {
                        id: [::core::primitive::u8; 32usize],
                    },
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "The `Error` enum of this pallet."]
                pub enum Error {
                    #[codec(index = 0)]
                    #[doc = "Failed to schedule a call"]
                    FailedToSchedule,
                    #[codec(index = 1)]
                    #[doc = "Cannot find the scheduled call."]
                    NotFound,
                    #[codec(index = 2)]
                    #[doc = "Given target block number is in the past."]
                    TargetBlockNumberInPast,
                    #[codec(index = 3)]
                    #[doc = "Reschedule failed because it does not change scheduled time."]
                    RescheduleNoChange,
                    #[codec(index = 4)]
                    #[doc = "Attempt to use a non-named function on a named task."]
                    Named,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "Events type."]
                pub enum Event {
                    #[codec(index = 0)]
                    #[doc = "Scheduled some task."]
                    Scheduled {
                        when: ::core::primitive::u32,
                        index: ::core::primitive::u32,
                    },
                    #[codec(index = 1)]
                    #[doc = "Canceled some task."]
                    Canceled {
                        when: ::core::primitive::u32,
                        index: ::core::primitive::u32,
                    },
                    #[codec(index = 2)]
                    #[doc = "Dispatched some task."]
                    Dispatched {
                        task: (::core::primitive::u32, ::core::primitive::u32),
                        id: ::core::option::Option<[::core::primitive::u8; 32usize]>,
                        result:
                            ::core::result::Result<(), runtime_types::sp_runtime::DispatchError>,
                    },
                    #[codec(index = 3)]
                    #[doc = "Set a retry configuration for some task."]
                    RetrySet {
                        task: (::core::primitive::u32, ::core::primitive::u32),
                        id: ::core::option::Option<[::core::primitive::u8; 32usize]>,
                        period: ::core::primitive::u32,
                        retries: ::core::primitive::u8,
                    },
                    #[codec(index = 4)]
                    #[doc = "Cancel a retry configuration for some task."]
                    RetryCancelled {
                        task: (::core::primitive::u32, ::core::primitive::u32),
                        id: ::core::option::Option<[::core::primitive::u8; 32usize]>,
                    },
                    #[codec(index = 5)]
                    #[doc = "The call for the provided hash was not found so the task has been aborted."]
                    CallUnavailable {
                        task: (::core::primitive::u32, ::core::primitive::u32),
                        id: ::core::option::Option<[::core::primitive::u8; 32usize]>,
                    },
                    #[codec(index = 6)]
                    #[doc = "The given task was unable to be renewed since the agenda is full at that block."]
                    PeriodicFailed {
                        task: (::core::primitive::u32, ::core::primitive::u32),
                        id: ::core::option::Option<[::core::primitive::u8; 32usize]>,
                    },
                    #[codec(index = 7)]
                    #[doc = "The given task was unable to be retried since the agenda is full at that block or there"]
                    #[doc = "was not enough weight to reschedule it."]
                    RetryFailed {
                        task: (::core::primitive::u32, ::core::primitive::u32),
                        id: ::core::option::Option<[::core::primitive::u8; 32usize]>,
                    },
                    #[codec(index = 8)]
                    #[doc = "The given task can never be executed since it is overweight."]
                    PermanentlyOverweight {
                        task: (::core::primitive::u32, ::core::primitive::u32),
                        id: ::core::option::Option<[::core::primitive::u8; 32usize]>,
                    },
                }
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub struct RetryConfig<_0> {
                pub total_retries: ::core::primitive::u8,
                pub remaining: ::core::primitive::u8,
                pub period: _0,
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub struct Scheduled<_0, _1, _2, _3, _4> {
                pub maybe_id: ::core::option::Option<_0>,
                pub priority: ::core::primitive::u8,
                pub call: _1,
                pub maybe_periodic: ::core::option::Option<(_2, _2)>,
                pub origin: _3,
                #[codec(skip)]
                pub __ignore: ::core::marker::PhantomData<_4>,
            }
        }
        pub mod pallet_session {
            use super::runtime_types;
            pub mod pallet {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "Contains a variant per dispatchable extrinsic that this pallet has."]
                pub enum Call {
                    #[codec(index = 0)]
                    #[doc = "Sets the session key(s) of the function caller to `keys`."]
                    #[doc = "Allows an account to set its session key prior to becoming a validator."]
                    #[doc = "This doesn't take effect until the next session."]
                    #[doc = ""]
                    #[doc = "The dispatch origin of this function must be signed."]
                    #[doc = ""]
                    #[doc = "## Complexity"]
                    #[doc = "- `O(1)`. Actual cost depends on the number of length of `T::Keys::key_ids()` which is"]
                    #[doc = "  fixed."]
                    set_keys {
                        keys: runtime_types::vara_runtime::SessionKeys,
                        proof: ::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
                    },
                    #[codec(index = 1)]
                    #[doc = "Removes any session key(s) of the function caller."]
                    #[doc = ""]
                    #[doc = "This doesn't take effect until the next session."]
                    #[doc = ""]
                    #[doc = "The dispatch origin of this function must be Signed and the account must be either be"]
                    #[doc = "convertible to a validator ID using the chain's typical addressing system (this usually"]
                    #[doc = "means being a controller account) or directly convertible into a validator ID (which"]
                    #[doc = "usually means being a stash account)."]
                    #[doc = ""]
                    #[doc = "## Complexity"]
                    #[doc = "- `O(1)` in number of key types. Actual cost depends on the number of length of"]
                    #[doc = "  `T::Keys::key_ids()` which is fixed."]
                    purge_keys,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "Error for the session pallet."]
                pub enum Error {
                    #[codec(index = 0)]
                    #[doc = "Invalid ownership proof."]
                    InvalidProof,
                    #[codec(index = 1)]
                    #[doc = "No associated validator ID for account."]
                    NoAssociatedValidatorId,
                    #[codec(index = 2)]
                    #[doc = "Registered duplicate key."]
                    DuplicatedKey,
                    #[codec(index = 3)]
                    #[doc = "No keys are associated with this account."]
                    NoKeys,
                    #[codec(index = 4)]
                    #[doc = "Key setting account is not live, so it's impossible to associate keys."]
                    NoAccount,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "The `Event` enum of this pallet"]
                pub enum Event {
                    #[codec(index = 0)]
                    #[doc = "New session has happened. Note that the argument is the session index, not the"]
                    #[doc = "block number as the type might suggest."]
                    NewSession {
                        session_index: ::core::primitive::u32,
                    },
                }
            }
        }
        pub mod pallet_staking {
            use super::runtime_types;
            pub mod pallet {
                use super::runtime_types;
                pub mod pallet {
                    use super::runtime_types;
                    #[derive(
                        Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode,
                    )]
                    #[doc = "Contains a variant per dispatchable extrinsic that this pallet has."]
                    pub enum Call {
                        #[codec(index = 0)]
                        #[doc = "Take the origin account as a stash and lock up `value` of its balance. `controller` will"]
                        #[doc = "be the account that controls it."]
                        #[doc = ""]
                        #[doc = "`value` must be more than the `minimum_balance` specified by `T::Currency`."]
                        #[doc = ""]
                        #[doc = "The dispatch origin for this call must be _Signed_ by the stash account."]
                        #[doc = ""]
                        #[doc = "Emits `Bonded`."]
                        #[doc = "## Complexity"]
                        #[doc = "- Independent of the arguments. Moderate complexity."]
                        #[doc = "- O(1)."]
                        #[doc = "- Three extra DB entries."]
                        #[doc = ""]
                        #[doc = "NOTE: Two of the storage writes (`Self::bonded`, `Self::payee`) are _never_ cleaned"]
                        #[doc = "unless the `origin` falls below _existential deposit_ (or equal to 0) and gets removed"]
                        #[doc = "as dust."]
                        bond {
                            #[codec(compact)]
                            value: ::core::primitive::u128,
                            payee: runtime_types::pallet_staking::RewardDestination<
                                ::subxt::ext::subxt_core::utils::AccountId32,
                            >,
                        },
                        #[codec(index = 1)]
                        #[doc = "Add some extra amount that have appeared in the stash `free_balance` into the balance up"]
                        #[doc = "for staking."]
                        #[doc = ""]
                        #[doc = "The dispatch origin for this call must be _Signed_ by the stash, not the controller."]
                        #[doc = ""]
                        #[doc = "Use this if there are additional funds in your stash account that you wish to bond."]
                        #[doc = "Unlike [`bond`](Self::bond) or [`unbond`](Self::unbond) this function does not impose"]
                        #[doc = "any limitation on the amount that can be added."]
                        #[doc = ""]
                        #[doc = "Emits `Bonded`."]
                        #[doc = ""]
                        #[doc = "## Complexity"]
                        #[doc = "- Independent of the arguments. Insignificant complexity."]
                        #[doc = "- O(1)."]
                        bond_extra {
                            #[codec(compact)]
                            max_additional: ::core::primitive::u128,
                        },
                        #[codec(index = 2)]
                        #[doc = "Schedule a portion of the stash to be unlocked ready for transfer out after the bond"]
                        #[doc = "period ends. If this leaves an amount actively bonded less than"]
                        #[doc = "T::Currency::minimum_balance(), then it is increased to the full amount."]
                        #[doc = ""]
                        #[doc = "The dispatch origin for this call must be _Signed_ by the controller, not the stash."]
                        #[doc = ""]
                        #[doc = "Once the unlock period is done, you can call `withdraw_unbonded` to actually move"]
                        #[doc = "the funds out of management ready for transfer."]
                        #[doc = ""]
                        #[doc = "No more than a limited number of unlocking chunks (see `MaxUnlockingChunks`)"]
                        #[doc = "can co-exists at the same time. If there are no unlocking chunks slots available"]
                        #[doc = "[`Call::withdraw_unbonded`] is called to remove some of the chunks (if possible)."]
                        #[doc = ""]
                        #[doc = "If a user encounters the `InsufficientBond` error when calling this extrinsic,"]
                        #[doc = "they should call `chill` first in order to free up their bonded funds."]
                        #[doc = ""]
                        #[doc = "Emits `Unbonded`."]
                        #[doc = ""]
                        #[doc = "See also [`Call::withdraw_unbonded`]."]
                        unbond {
                            #[codec(compact)]
                            value: ::core::primitive::u128,
                        },
                        #[codec(index = 3)]
                        #[doc = "Remove any unlocked chunks from the `unlocking` queue from our management."]
                        #[doc = ""]
                        #[doc = "This essentially frees up that balance to be used by the stash account to do whatever"]
                        #[doc = "it wants."]
                        #[doc = ""]
                        #[doc = "The dispatch origin for this call must be _Signed_ by the controller."]
                        #[doc = ""]
                        #[doc = "Emits `Withdrawn`."]
                        #[doc = ""]
                        #[doc = "See also [`Call::unbond`]."]
                        #[doc = ""]
                        #[doc = "## Parameters"]
                        #[doc = ""]
                        #[doc = "- `num_slashing_spans` indicates the number of metadata slashing spans to clear when"]
                        #[doc = "this call results in a complete removal of all the data related to the stash account."]
                        #[doc = "In this case, the `num_slashing_spans` must be larger or equal to the number of"]
                        #[doc = "slashing spans associated with the stash account in the [`SlashingSpans`] storage type,"]
                        #[doc = "otherwise the call will fail. The call weight is directly proportional to"]
                        #[doc = "`num_slashing_spans`."]
                        #[doc = ""]
                        #[doc = "## Complexity"]
                        #[doc = "O(S) where S is the number of slashing spans to remove"]
                        #[doc = "NOTE: Weight annotation is the kill scenario, we refund otherwise."]
                        withdraw_unbonded {
                            num_slashing_spans: ::core::primitive::u32,
                        },
                        #[codec(index = 4)]
                        #[doc = "Declare the desire to validate for the origin controller."]
                        #[doc = ""]
                        #[doc = "Effects will be felt at the beginning of the next era."]
                        #[doc = ""]
                        #[doc = "The dispatch origin for this call must be _Signed_ by the controller, not the stash."]
                        validate {
                            prefs: runtime_types::pallet_staking::ValidatorPrefs,
                        },
                        #[codec(index = 5)]
                        #[doc = "Declare the desire to nominate `targets` for the origin controller."]
                        #[doc = ""]
                        #[doc = "Effects will be felt at the beginning of the next era."]
                        #[doc = ""]
                        #[doc = "The dispatch origin for this call must be _Signed_ by the controller, not the stash."]
                        #[doc = ""]
                        #[doc = "## Complexity"]
                        #[doc = "- The transaction's complexity is proportional to the size of `targets` (N)"]
                        #[doc = "which is capped at CompactAssignments::LIMIT (T::MaxNominations)."]
                        #[doc = "- Both the reads and writes follow a similar pattern."]
                        nominate {
                            targets: ::subxt::ext::subxt_core::alloc::vec::Vec<
                                ::subxt::ext::subxt_core::utils::MultiAddress<
                                    ::subxt::ext::subxt_core::utils::AccountId32,
                                    (),
                                >,
                            >,
                        },
                        #[codec(index = 6)]
                        #[doc = "Declare no desire to either validate or nominate."]
                        #[doc = ""]
                        #[doc = "Effects will be felt at the beginning of the next era."]
                        #[doc = ""]
                        #[doc = "The dispatch origin for this call must be _Signed_ by the controller, not the stash."]
                        #[doc = ""]
                        #[doc = "## Complexity"]
                        #[doc = "- Independent of the arguments. Insignificant complexity."]
                        #[doc = "- Contains one read."]
                        #[doc = "- Writes are limited to the `origin` account key."]
                        chill,
                        #[codec(index = 7)]
                        #[doc = "(Re-)set the payment target for a controller."]
                        #[doc = ""]
                        #[doc = "Effects will be felt instantly (as soon as this function is completed successfully)."]
                        #[doc = ""]
                        #[doc = "The dispatch origin for this call must be _Signed_ by the controller, not the stash."]
                        #[doc = ""]
                        #[doc = "## Complexity"]
                        #[doc = "- O(1)"]
                        #[doc = "- Independent of the arguments. Insignificant complexity."]
                        #[doc = "- Contains a limited number of reads."]
                        #[doc = "- Writes are limited to the `origin` account key."]
                        #[doc = "---------"]
                        set_payee {
                            payee: runtime_types::pallet_staking::RewardDestination<
                                ::subxt::ext::subxt_core::utils::AccountId32,
                            >,
                        },
                        #[codec(index = 8)]
                        #[doc = "(Re-)sets the controller of a stash to the stash itself. This function previously"]
                        #[doc = "accepted a `controller` argument to set the controller to an account other than the"]
                        #[doc = "stash itself. This functionality has now been removed, now only setting the controller"]
                        #[doc = "to the stash, if it is not already."]
                        #[doc = ""]
                        #[doc = "Effects will be felt instantly (as soon as this function is completed successfully)."]
                        #[doc = ""]
                        #[doc = "The dispatch origin for this call must be _Signed_ by the stash, not the controller."]
                        #[doc = ""]
                        #[doc = "## Complexity"]
                        #[doc = "O(1)"]
                        #[doc = "- Independent of the arguments. Insignificant complexity."]
                        #[doc = "- Contains a limited number of reads."]
                        #[doc = "- Writes are limited to the `origin` account key."]
                        set_controller,
                        #[codec(index = 9)]
                        #[doc = "Sets the ideal number of validators."]
                        #[doc = ""]
                        #[doc = "The dispatch origin must be Root."]
                        #[doc = ""]
                        #[doc = "## Complexity"]
                        #[doc = "O(1)"]
                        set_validator_count {
                            #[codec(compact)]
                            new: ::core::primitive::u32,
                        },
                        #[codec(index = 10)]
                        #[doc = "Increments the ideal number of validators up to maximum of"]
                        #[doc = "`ElectionProviderBase::MaxWinners`."]
                        #[doc = ""]
                        #[doc = "The dispatch origin must be Root."]
                        #[doc = ""]
                        #[doc = "## Complexity"]
                        #[doc = "Same as [`Self::set_validator_count`]."]
                        increase_validator_count {
                            #[codec(compact)]
                            additional: ::core::primitive::u32,
                        },
                        #[codec(index = 11)]
                        #[doc = "Scale up the ideal number of validators by a factor up to maximum of"]
                        #[doc = "`ElectionProviderBase::MaxWinners`."]
                        #[doc = ""]
                        #[doc = "The dispatch origin must be Root."]
                        #[doc = ""]
                        #[doc = "## Complexity"]
                        #[doc = "Same as [`Self::set_validator_count`]."]
                        scale_validator_count {
                            factor: runtime_types::sp_arithmetic::per_things::Percent,
                        },
                        #[codec(index = 12)]
                        #[doc = "Force there to be no new eras indefinitely."]
                        #[doc = ""]
                        #[doc = "The dispatch origin must be Root."]
                        #[doc = ""]
                        #[doc = "# Warning"]
                        #[doc = ""]
                        #[doc = "The election process starts multiple blocks before the end of the era."]
                        #[doc = "Thus the election process may be ongoing when this is called. In this case the"]
                        #[doc = "election will continue until the next era is triggered."]
                        #[doc = ""]
                        #[doc = "## Complexity"]
                        #[doc = "- No arguments."]
                        #[doc = "- Weight: O(1)"]
                        force_no_eras,
                        #[codec(index = 13)]
                        #[doc = "Force there to be a new era at the end of the next session. After this, it will be"]
                        #[doc = "reset to normal (non-forced) behaviour."]
                        #[doc = ""]
                        #[doc = "The dispatch origin must be Root."]
                        #[doc = ""]
                        #[doc = "# Warning"]
                        #[doc = ""]
                        #[doc = "The election process starts multiple blocks before the end of the era."]
                        #[doc = "If this is called just before a new era is triggered, the election process may not"]
                        #[doc = "have enough blocks to get a result."]
                        #[doc = ""]
                        #[doc = "## Complexity"]
                        #[doc = "- No arguments."]
                        #[doc = "- Weight: O(1)"]
                        force_new_era,
                        #[codec(index = 14)]
                        #[doc = "Set the validators who cannot be slashed (if any)."]
                        #[doc = ""]
                        #[doc = "The dispatch origin must be Root."]
                        set_invulnerables {
                            invulnerables: ::subxt::ext::subxt_core::alloc::vec::Vec<
                                ::subxt::ext::subxt_core::utils::AccountId32,
                            >,
                        },
                        #[codec(index = 15)]
                        #[doc = "Force a current staker to become completely unstaked, immediately."]
                        #[doc = ""]
                        #[doc = "The dispatch origin must be Root."]
                        #[doc = ""]
                        #[doc = "## Parameters"]
                        #[doc = ""]
                        #[doc = "- `num_slashing_spans`: Refer to comments on [`Call::withdraw_unbonded`] for more"]
                        #[doc = "details."]
                        force_unstake {
                            stash: ::subxt::ext::subxt_core::utils::AccountId32,
                            num_slashing_spans: ::core::primitive::u32,
                        },
                        #[codec(index = 16)]
                        #[doc = "Force there to be a new era at the end of sessions indefinitely."]
                        #[doc = ""]
                        #[doc = "The dispatch origin must be Root."]
                        #[doc = ""]
                        #[doc = "# Warning"]
                        #[doc = ""]
                        #[doc = "The election process starts multiple blocks before the end of the era."]
                        #[doc = "If this is called just before a new era is triggered, the election process may not"]
                        #[doc = "have enough blocks to get a result."]
                        force_new_era_always,
                        #[codec(index = 17)]
                        #[doc = "Cancel enactment of a deferred slash."]
                        #[doc = ""]
                        #[doc = "Can be called by the `T::AdminOrigin`."]
                        #[doc = ""]
                        #[doc = "Parameters: era and indices of the slashes for that era to kill."]
                        cancel_deferred_slash {
                            era: ::core::primitive::u32,
                            slash_indices:
                                ::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u32>,
                        },
                        #[codec(index = 18)]
                        #[doc = "Pay out next page of the stakers behind a validator for the given era."]
                        #[doc = ""]
                        #[doc = "- `validator_stash` is the stash account of the validator."]
                        #[doc = "- `era` may be any era between `[current_era - history_depth; current_era]`."]
                        #[doc = ""]
                        #[doc = "The origin of this call must be _Signed_. Any account can call this function, even if"]
                        #[doc = "it is not one of the stakers."]
                        #[doc = ""]
                        #[doc = "The reward payout could be paged in case there are too many nominators backing the"]
                        #[doc = "`validator_stash`. This call will payout unpaid pages in an ascending order. To claim a"]
                        #[doc = "specific page, use `payout_stakers_by_page`.`"]
                        #[doc = ""]
                        #[doc = "If all pages are claimed, it returns an error `InvalidPage`."]
                        payout_stakers {
                            validator_stash: ::subxt::ext::subxt_core::utils::AccountId32,
                            era: ::core::primitive::u32,
                        },
                        #[codec(index = 19)]
                        #[doc = "Rebond a portion of the stash scheduled to be unlocked."]
                        #[doc = ""]
                        #[doc = "The dispatch origin must be signed by the controller."]
                        #[doc = ""]
                        #[doc = "## Complexity"]
                        #[doc = "- Time complexity: O(L), where L is unlocking chunks"]
                        #[doc = "- Bounded by `MaxUnlockingChunks`."]
                        rebond {
                            #[codec(compact)]
                            value: ::core::primitive::u128,
                        },
                        #[codec(index = 20)]
                        #[doc = "Remove all data structures concerning a staker/stash once it is at a state where it can"]
                        #[doc = "be considered `dust` in the staking system. The requirements are:"]
                        #[doc = ""]
                        #[doc = "1. the `total_balance` of the stash is below existential deposit."]
                        #[doc = "2. or, the `ledger.total` of the stash is below existential deposit."]
                        #[doc = "3. or, existential deposit is zero and either `total_balance` or `ledger.total` is zero."]
                        #[doc = ""]
                        #[doc = "The former can happen in cases like a slash; the latter when a fully unbonded account"]
                        #[doc = "is still receiving staking rewards in `RewardDestination::Staked`."]
                        #[doc = ""]
                        #[doc = "It can be called by anyone, as long as `stash` meets the above requirements."]
                        #[doc = ""]
                        #[doc = "Refunds the transaction fees upon successful execution."]
                        #[doc = ""]
                        #[doc = "## Parameters"]
                        #[doc = ""]
                        #[doc = "- `num_slashing_spans`: Refer to comments on [`Call::withdraw_unbonded`] for more"]
                        #[doc = "details."]
                        reap_stash {
                            stash: ::subxt::ext::subxt_core::utils::AccountId32,
                            num_slashing_spans: ::core::primitive::u32,
                        },
                        #[codec(index = 21)]
                        #[doc = "Remove the given nominations from the calling validator."]
                        #[doc = ""]
                        #[doc = "Effects will be felt at the beginning of the next era."]
                        #[doc = ""]
                        #[doc = "The dispatch origin for this call must be _Signed_ by the controller, not the stash."]
                        #[doc = ""]
                        #[doc = "- `who`: A list of nominator stash accounts who are nominating this validator which"]
                        #[doc = "  should no longer be nominating this validator."]
                        #[doc = ""]
                        #[doc = "Note: Making this call only makes sense if you first set the validator preferences to"]
                        #[doc = "block any further nominations."]
                        kick {
                            who: ::subxt::ext::subxt_core::alloc::vec::Vec<
                                ::subxt::ext::subxt_core::utils::MultiAddress<
                                    ::subxt::ext::subxt_core::utils::AccountId32,
                                    (),
                                >,
                            >,
                        },
                        #[codec(index = 22)]
                        #[doc = "Update the various staking configurations ."]
                        #[doc = ""]
                        #[doc = "* `min_nominator_bond`: The minimum active bond needed to be a nominator."]
                        #[doc = "* `min_validator_bond`: The minimum active bond needed to be a validator."]
                        #[doc = "* `max_nominator_count`: The max number of users who can be a nominator at once. When"]
                        #[doc = "  set to `None`, no limit is enforced."]
                        #[doc = "* `max_validator_count`: The max number of users who can be a validator at once. When"]
                        #[doc = "  set to `None`, no limit is enforced."]
                        #[doc = "* `chill_threshold`: The ratio of `max_nominator_count` or `max_validator_count` which"]
                        #[doc = "  should be filled in order for the `chill_other` transaction to work."]
                        #[doc = "* `min_commission`: The minimum amount of commission that each validators must maintain."]
                        #[doc = "  This is checked only upon calling `validate`. Existing validators are not affected."]
                        #[doc = ""]
                        #[doc = "RuntimeOrigin must be Root to call this function."]
                        #[doc = ""]
                        #[doc = "NOTE: Existing nominators and validators will not be affected by this update."]
                        #[doc = "to kick people under the new limits, `chill_other` should be called."]
                        set_staking_configs {
                            min_nominator_bond:
                                runtime_types::pallet_staking::pallet::pallet::ConfigOp<
                                    ::core::primitive::u128,
                                >,
                            min_validator_bond:
                                runtime_types::pallet_staking::pallet::pallet::ConfigOp<
                                    ::core::primitive::u128,
                                >,
                            max_nominator_count:
                                runtime_types::pallet_staking::pallet::pallet::ConfigOp<
                                    ::core::primitive::u32,
                                >,
                            max_validator_count:
                                runtime_types::pallet_staking::pallet::pallet::ConfigOp<
                                    ::core::primitive::u32,
                                >,
                            chill_threshold:
                                runtime_types::pallet_staking::pallet::pallet::ConfigOp<
                                    runtime_types::sp_arithmetic::per_things::Percent,
                                >,
                            min_commission: runtime_types::pallet_staking::pallet::pallet::ConfigOp<
                                runtime_types::sp_arithmetic::per_things::Perbill,
                            >,
                            max_staked_rewards:
                                runtime_types::pallet_staking::pallet::pallet::ConfigOp<
                                    runtime_types::sp_arithmetic::per_things::Percent,
                                >,
                        },
                        #[codec(index = 23)]
                        #[doc = "Declare a `controller` to stop participating as either a validator or nominator."]
                        #[doc = ""]
                        #[doc = "Effects will be felt at the beginning of the next era."]
                        #[doc = ""]
                        #[doc = "The dispatch origin for this call must be _Signed_, but can be called by anyone."]
                        #[doc = ""]
                        #[doc = "If the caller is the same as the controller being targeted, then no further checks are"]
                        #[doc = "enforced, and this function behaves just like `chill`."]
                        #[doc = ""]
                        #[doc = "If the caller is different than the controller being targeted, the following conditions"]
                        #[doc = "must be met:"]
                        #[doc = ""]
                        #[doc = "* `controller` must belong to a nominator who has become non-decodable,"]
                        #[doc = ""]
                        #[doc = "Or:"]
                        #[doc = ""]
                        #[doc = "* A `ChillThreshold` must be set and checked which defines how close to the max"]
                        #[doc = "  nominators or validators we must reach before users can start chilling one-another."]
                        #[doc = "* A `MaxNominatorCount` and `MaxValidatorCount` must be set which is used to determine"]
                        #[doc = "  how close we are to the threshold."]
                        #[doc = "* A `MinNominatorBond` and `MinValidatorBond` must be set and checked, which determines"]
                        #[doc = "  if this is a person that should be chilled because they have not met the threshold"]
                        #[doc = "  bond required."]
                        #[doc = ""]
                        #[doc = "This can be helpful if bond requirements are updated, and we need to remove old users"]
                        #[doc = "who do not satisfy these requirements."]
                        chill_other {
                            stash: ::subxt::ext::subxt_core::utils::AccountId32,
                        },
                        #[codec(index = 24)]
                        #[doc = "Force a validator to have at least the minimum commission. This will not affect a"]
                        #[doc = "validator who already has a commission greater than or equal to the minimum. Any account"]
                        #[doc = "can call this."]
                        force_apply_min_commission {
                            validator_stash: ::subxt::ext::subxt_core::utils::AccountId32,
                        },
                        #[codec(index = 25)]
                        #[doc = "Sets the minimum amount of commission that each validators must maintain."]
                        #[doc = ""]
                        #[doc = "This call has lower privilege requirements than `set_staking_config` and can be called"]
                        #[doc = "by the `T::AdminOrigin`. Root can always call this."]
                        set_min_commission {
                            new: runtime_types::sp_arithmetic::per_things::Perbill,
                        },
                        #[codec(index = 26)]
                        #[doc = "Pay out a page of the stakers behind a validator for the given era and page."]
                        #[doc = ""]
                        #[doc = "- `validator_stash` is the stash account of the validator."]
                        #[doc = "- `era` may be any era between `[current_era - history_depth; current_era]`."]
                        #[doc = "- `page` is the page index of nominators to pay out with value between 0 and"]
                        #[doc = "  `num_nominators / T::MaxExposurePageSize`."]
                        #[doc = ""]
                        #[doc = "The origin of this call must be _Signed_. Any account can call this function, even if"]
                        #[doc = "it is not one of the stakers."]
                        #[doc = ""]
                        #[doc = "If a validator has more than [`Config::MaxExposurePageSize`] nominators backing"]
                        #[doc = "them, then the list of nominators is paged, with each page being capped at"]
                        #[doc = "[`Config::MaxExposurePageSize`.] If a validator has more than one page of nominators,"]
                        #[doc = "the call needs to be made for each page separately in order for all the nominators"]
                        #[doc = "backing a validator to receive the reward. The nominators are not sorted across pages"]
                        #[doc = "and so it should not be assumed the highest staker would be on the topmost page and vice"]
                        #[doc = "versa. If rewards are not claimed in [`Config::HistoryDepth`] eras, they are lost."]
                        payout_stakers_by_page {
                            validator_stash: ::subxt::ext::subxt_core::utils::AccountId32,
                            era: ::core::primitive::u32,
                            page: ::core::primitive::u32,
                        },
                        #[codec(index = 27)]
                        #[doc = "Migrates an account's `RewardDestination::Controller` to"]
                        #[doc = "`RewardDestination::Account(controller)`."]
                        #[doc = ""]
                        #[doc = "Effects will be felt instantly (as soon as this function is completed successfully)."]
                        #[doc = ""]
                        #[doc = "This will waive the transaction fee if the `payee` is successfully migrated."]
                        update_payee {
                            controller: ::subxt::ext::subxt_core::utils::AccountId32,
                        },
                        #[codec(index = 28)]
                        #[doc = "Updates a batch of controller accounts to their corresponding stash account if they are"]
                        #[doc = "not the same. Ignores any controller accounts that do not exist, and does not operate if"]
                        #[doc = "the stash and controller are already the same."]
                        #[doc = ""]
                        #[doc = "Effects will be felt instantly (as soon as this function is completed successfully)."]
                        #[doc = ""]
                        #[doc = "The dispatch origin must be `T::AdminOrigin`."]
                        deprecate_controller_batch {
                            controllers:
                                runtime_types::bounded_collections::bounded_vec::BoundedVec<
                                    ::subxt::ext::subxt_core::utils::AccountId32,
                                >,
                        },
                        #[codec(index = 29)]
                        #[doc = "Restores the state of a ledger which is in an inconsistent state."]
                        #[doc = ""]
                        #[doc = "The requirements to restore a ledger are the following:"]
                        #[doc = "* The stash is bonded; or"]
                        #[doc = "* The stash is not bonded but it has a staking lock left behind; or"]
                        #[doc = "* If the stash has an associated ledger and its state is inconsistent; or"]
                        #[doc = "* If the ledger is not corrupted *but* its staking lock is out of sync."]
                        #[doc = ""]
                        #[doc = "The `maybe_*` input parameters will overwrite the corresponding data and metadata of the"]
                        #[doc = "ledger associated with the stash. If the input parameters are not set, the ledger will"]
                        #[doc = "be reset values from on-chain state."]
                        restore_ledger {
                            stash: ::subxt::ext::subxt_core::utils::AccountId32,
                            maybe_controller: ::core::option::Option<
                                ::subxt::ext::subxt_core::utils::AccountId32,
                            >,
                            maybe_total: ::core::option::Option<::core::primitive::u128>,
                            maybe_unlocking: ::core::option::Option<
                                runtime_types::bounded_collections::bounded_vec::BoundedVec<
                                    runtime_types::pallet_staking::UnlockChunk<
                                        ::core::primitive::u128,
                                    >,
                                >,
                            >,
                        },
                    }
                    #[derive(
                        Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode,
                    )]
                    pub enum ConfigOp<_0> {
                        #[codec(index = 0)]
                        Noop,
                        #[codec(index = 1)]
                        Set(_0),
                        #[codec(index = 2)]
                        Remove,
                    }
                    #[derive(
                        Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode,
                    )]
                    #[doc = "The `Error` enum of this pallet."]
                    pub enum Error {
                        #[codec(index = 0)]
                        #[doc = "Not a controller account."]
                        NotController,
                        #[codec(index = 1)]
                        #[doc = "Not a stash account."]
                        NotStash,
                        #[codec(index = 2)]
                        #[doc = "Stash is already bonded."]
                        AlreadyBonded,
                        #[codec(index = 3)]
                        #[doc = "Controller is already paired."]
                        AlreadyPaired,
                        #[codec(index = 4)]
                        #[doc = "Targets cannot be empty."]
                        EmptyTargets,
                        #[codec(index = 5)]
                        #[doc = "Duplicate index."]
                        DuplicateIndex,
                        #[codec(index = 6)]
                        #[doc = "Slash record index out of bounds."]
                        InvalidSlashIndex,
                        #[codec(index = 7)]
                        #[doc = "Cannot have a validator or nominator role, with value less than the minimum defined by"]
                        #[doc = "governance (see `MinValidatorBond` and `MinNominatorBond`). If unbonding is the"]
                        #[doc = "intention, `chill` first to remove one's role as validator/nominator."]
                        InsufficientBond,
                        #[codec(index = 8)]
                        #[doc = "Can not schedule more unlock chunks."]
                        NoMoreChunks,
                        #[codec(index = 9)]
                        #[doc = "Can not rebond without unlocking chunks."]
                        NoUnlockChunk,
                        #[codec(index = 10)]
                        #[doc = "Attempting to target a stash that still has funds."]
                        FundedTarget,
                        #[codec(index = 11)]
                        #[doc = "Invalid era to reward."]
                        InvalidEraToReward,
                        #[codec(index = 12)]
                        #[doc = "Invalid number of nominations."]
                        InvalidNumberOfNominations,
                        #[codec(index = 13)]
                        #[doc = "Items are not sorted and unique."]
                        NotSortedAndUnique,
                        #[codec(index = 14)]
                        #[doc = "Rewards for this era have already been claimed for this validator."]
                        AlreadyClaimed,
                        #[codec(index = 15)]
                        #[doc = "No nominators exist on this page."]
                        InvalidPage,
                        #[codec(index = 16)]
                        #[doc = "Incorrect previous history depth input provided."]
                        IncorrectHistoryDepth,
                        #[codec(index = 17)]
                        #[doc = "Incorrect number of slashing spans provided."]
                        IncorrectSlashingSpans,
                        #[codec(index = 18)]
                        #[doc = "Internal state has become somehow corrupted and the operation cannot continue."]
                        BadState,
                        #[codec(index = 19)]
                        #[doc = "Too many nomination targets supplied."]
                        TooManyTargets,
                        #[codec(index = 20)]
                        #[doc = "A nomination target was supplied that was blocked or otherwise not a validator."]
                        BadTarget,
                        #[codec(index = 21)]
                        #[doc = "The user has enough bond and thus cannot be chilled forcefully by an external person."]
                        CannotChillOther,
                        #[codec(index = 22)]
                        #[doc = "There are too many nominators in the system. Governance needs to adjust the staking"]
                        #[doc = "settings to keep things safe for the runtime."]
                        TooManyNominators,
                        #[codec(index = 23)]
                        #[doc = "There are too many validator candidates in the system. Governance needs to adjust the"]
                        #[doc = "staking settings to keep things safe for the runtime."]
                        TooManyValidators,
                        #[codec(index = 24)]
                        #[doc = "Commission is too low. Must be at least `MinCommission`."]
                        CommissionTooLow,
                        #[codec(index = 25)]
                        #[doc = "Some bound is not met."]
                        BoundNotMet,
                        #[codec(index = 26)]
                        #[doc = "Used when attempting to use deprecated controller account logic."]
                        ControllerDeprecated,
                        #[codec(index = 27)]
                        #[doc = "Cannot reset a ledger."]
                        CannotRestoreLedger,
                        #[codec(index = 28)]
                        #[doc = "Provided reward destination is not allowed."]
                        RewardDestinationRestricted,
                        #[codec(index = 29)]
                        #[doc = "Not enough funds available to withdraw."]
                        NotEnoughFunds,
                        #[codec(index = 30)]
                        #[doc = "Operation not allowed for virtual stakers."]
                        VirtualStakerNotAllowed,
                    }
                    #[derive(
                        Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode,
                    )]
                    #[doc = "The `Event` enum of this pallet"]
                    pub enum Event {
                        #[codec(index = 0)]
                        #[doc = "The era payout has been set; the first balance is the validator-payout; the second is"]
                        #[doc = "the remainder from the maximum amount of reward."]
                        EraPaid {
                            era_index: ::core::primitive::u32,
                            validator_payout: ::core::primitive::u128,
                            remainder: ::core::primitive::u128,
                        },
                        #[codec(index = 1)]
                        #[doc = "The nominator has been rewarded by this amount to this destination."]
                        Rewarded {
                            stash: ::subxt::ext::subxt_core::utils::AccountId32,
                            dest: runtime_types::pallet_staking::RewardDestination<
                                ::subxt::ext::subxt_core::utils::AccountId32,
                            >,
                            amount: ::core::primitive::u128,
                        },
                        #[codec(index = 2)]
                        #[doc = "A staker (validator or nominator) has been slashed by the given amount."]
                        Slashed {
                            staker: ::subxt::ext::subxt_core::utils::AccountId32,
                            amount: ::core::primitive::u128,
                        },
                        #[codec(index = 3)]
                        #[doc = "A slash for the given validator, for the given percentage of their stake, at the given"]
                        #[doc = "era as been reported."]
                        SlashReported {
                            validator: ::subxt::ext::subxt_core::utils::AccountId32,
                            fraction: runtime_types::sp_arithmetic::per_things::Perbill,
                            slash_era: ::core::primitive::u32,
                        },
                        #[codec(index = 4)]
                        #[doc = "An old slashing report from a prior era was discarded because it could"]
                        #[doc = "not be processed."]
                        OldSlashingReportDiscarded {
                            session_index: ::core::primitive::u32,
                        },
                        #[codec(index = 5)]
                        #[doc = "A new set of stakers was elected."]
                        StakersElected,
                        #[codec(index = 6)]
                        #[doc = "An account has bonded this amount. \\[stash, amount\\]"]
                        #[doc = ""]
                        #[doc = "NOTE: This event is only emitted when funds are bonded via a dispatchable. Notably,"]
                        #[doc = "it will not be emitted for staking rewards when they are added to stake."]
                        Bonded {
                            stash: ::subxt::ext::subxt_core::utils::AccountId32,
                            amount: ::core::primitive::u128,
                        },
                        #[codec(index = 7)]
                        #[doc = "An account has unbonded this amount."]
                        Unbonded {
                            stash: ::subxt::ext::subxt_core::utils::AccountId32,
                            amount: ::core::primitive::u128,
                        },
                        #[codec(index = 8)]
                        #[doc = "An account has called `withdraw_unbonded` and removed unbonding chunks worth `Balance`"]
                        #[doc = "from the unlocking queue."]
                        Withdrawn {
                            stash: ::subxt::ext::subxt_core::utils::AccountId32,
                            amount: ::core::primitive::u128,
                        },
                        #[codec(index = 9)]
                        #[doc = "A nominator has been kicked from a validator."]
                        Kicked {
                            nominator: ::subxt::ext::subxt_core::utils::AccountId32,
                            stash: ::subxt::ext::subxt_core::utils::AccountId32,
                        },
                        #[codec(index = 10)]
                        #[doc = "The election failed. No new era is planned."]
                        StakingElectionFailed,
                        #[codec(index = 11)]
                        #[doc = "An account has stopped participating as either a validator or nominator."]
                        Chilled {
                            stash: ::subxt::ext::subxt_core::utils::AccountId32,
                        },
                        #[codec(index = 12)]
                        #[doc = "The stakers' rewards are getting paid."]
                        PayoutStarted {
                            era_index: ::core::primitive::u32,
                            validator_stash: ::subxt::ext::subxt_core::utils::AccountId32,
                        },
                        #[codec(index = 13)]
                        #[doc = "A validator has set their preferences."]
                        ValidatorPrefsSet {
                            stash: ::subxt::ext::subxt_core::utils::AccountId32,
                            prefs: runtime_types::pallet_staking::ValidatorPrefs,
                        },
                        #[codec(index = 14)]
                        #[doc = "Voters size limit reached."]
                        SnapshotVotersSizeExceeded { size: ::core::primitive::u32 },
                        #[codec(index = 15)]
                        #[doc = "Targets size limit reached."]
                        SnapshotTargetsSizeExceeded { size: ::core::primitive::u32 },
                        #[codec(index = 16)]
                        #[doc = "A new force era mode was set."]
                        ForceEra {
                            mode: runtime_types::pallet_staking::Forcing,
                        },
                        #[codec(index = 17)]
                        #[doc = "Report of a controller batch deprecation."]
                        ControllerBatchDeprecated { failures: ::core::primitive::u32 },
                    }
                }
            }
            pub mod slashing {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct SlashingSpans {
                    pub span_index: ::core::primitive::u32,
                    pub last_start: ::core::primitive::u32,
                    pub last_nonzero_slash: ::core::primitive::u32,
                    pub prior: ::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u32>,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct SpanRecord<_0> {
                    pub slashed: _0,
                    pub paid_out: _0,
                }
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub struct ActiveEraInfo {
                pub index: ::core::primitive::u32,
                pub start: ::core::option::Option<::core::primitive::u64>,
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub struct EraRewardPoints<_0> {
                pub total: ::core::primitive::u32,
                pub individual:
                    ::subxt::ext::subxt_core::utils::KeyedVec<_0, ::core::primitive::u32>,
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub enum Forcing {
                #[codec(index = 0)]
                NotForcing,
                #[codec(index = 1)]
                ForceNew,
                #[codec(index = 2)]
                ForceNone,
                #[codec(index = 3)]
                ForceAlways,
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub struct Nominations {
                pub targets: runtime_types::bounded_collections::bounded_vec::BoundedVec<
                    ::subxt::ext::subxt_core::utils::AccountId32,
                >,
                pub submitted_in: ::core::primitive::u32,
                pub suppressed: ::core::primitive::bool,
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub enum RewardDestination<_0> {
                #[codec(index = 0)]
                Staked,
                #[codec(index = 1)]
                Stash,
                #[codec(index = 2)]
                Controller,
                #[codec(index = 3)]
                Account(_0),
                #[codec(index = 4)]
                None,
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub struct StakingLedger {
                pub stash: ::subxt::ext::subxt_core::utils::AccountId32,
                #[codec(compact)]
                pub total: ::core::primitive::u128,
                #[codec(compact)]
                pub active: ::core::primitive::u128,
                pub unlocking: runtime_types::bounded_collections::bounded_vec::BoundedVec<
                    runtime_types::pallet_staking::UnlockChunk<::core::primitive::u128>,
                >,
                pub legacy_claimed_rewards:
                    runtime_types::bounded_collections::bounded_vec::BoundedVec<
                        ::core::primitive::u32,
                    >,
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub struct UnappliedSlash<_0, _1> {
                pub validator: _0,
                pub own: _1,
                pub others: ::subxt::ext::subxt_core::alloc::vec::Vec<(_0, _1)>,
                pub reporters: ::subxt::ext::subxt_core::alloc::vec::Vec<_0>,
                pub payout: _1,
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub struct UnlockChunk<_0> {
                #[codec(compact)]
                pub value: _0,
                #[codec(compact)]
                pub era: ::core::primitive::u32,
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub struct ValidatorPrefs {
                #[codec(compact)]
                pub commission: runtime_types::sp_arithmetic::per_things::Perbill,
                pub blocked: ::core::primitive::bool,
            }
        }
        pub mod pallet_sudo {
            use super::runtime_types;
            pub mod pallet {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "Contains a variant per dispatchable extrinsic that this pallet has."]
                pub enum Call {
                    #[codec(index = 0)]
                    #[doc = "Authenticates the sudo key and dispatches a function call with `Root` origin."]
                    sudo {
                        call: ::subxt::ext::subxt_core::alloc::boxed::Box<
                            runtime_types::vara_runtime::RuntimeCall,
                        >,
                    },
                    #[codec(index = 1)]
                    #[doc = "Authenticates the sudo key and dispatches a function call with `Root` origin."]
                    #[doc = "This function does not check the weight of the call, and instead allows the"]
                    #[doc = "Sudo user to specify the weight of the call."]
                    #[doc = ""]
                    #[doc = "The dispatch origin for this call must be _Signed_."]
                    sudo_unchecked_weight {
                        call: ::subxt::ext::subxt_core::alloc::boxed::Box<
                            runtime_types::vara_runtime::RuntimeCall,
                        >,
                        weight: runtime_types::sp_weights::weight_v2::Weight,
                    },
                    #[codec(index = 2)]
                    #[doc = "Authenticates the current sudo key and sets the given AccountId (`new`) as the new sudo"]
                    #[doc = "key."]
                    set_key {
                        new: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                    },
                    #[codec(index = 3)]
                    #[doc = "Authenticates the sudo key and dispatches a function call with `Signed` origin from"]
                    #[doc = "a given account."]
                    #[doc = ""]
                    #[doc = "The dispatch origin for this call must be _Signed_."]
                    sudo_as {
                        who: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                        call: ::subxt::ext::subxt_core::alloc::boxed::Box<
                            runtime_types::vara_runtime::RuntimeCall,
                        >,
                    },
                    #[codec(index = 4)]
                    #[doc = "Permanently removes the sudo key."]
                    #[doc = ""]
                    #[doc = "**This cannot be un-done.**"]
                    remove_key,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "Error for the Sudo pallet."]
                pub enum Error {
                    #[codec(index = 0)]
                    #[doc = "Sender must be the Sudo account."]
                    RequireSudo,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "The `Event` enum of this pallet"]
                pub enum Event {
                    #[codec(index = 0)]
                    #[doc = "A sudo call just took place."]
                    Sudid {
                        sudo_result:
                            ::core::result::Result<(), runtime_types::sp_runtime::DispatchError>,
                    },
                    #[codec(index = 1)]
                    #[doc = "The sudo key has been updated."]
                    KeyChanged {
                        old: ::core::option::Option<::subxt::ext::subxt_core::utils::AccountId32>,
                        new: ::subxt::ext::subxt_core::utils::AccountId32,
                    },
                    #[codec(index = 2)]
                    #[doc = "The key was permanently removed."]
                    KeyRemoved,
                    #[codec(index = 3)]
                    #[doc = "A [sudo_as](Pallet::sudo_as) call just took place."]
                    SudoAsDone {
                        sudo_result:
                            ::core::result::Result<(), runtime_types::sp_runtime::DispatchError>,
                    },
                }
            }
        }
        pub mod pallet_timestamp {
            use super::runtime_types;
            pub mod pallet {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "Contains a variant per dispatchable extrinsic that this pallet has."]
                pub enum Call {
                    #[codec(index = 0)]
                    #[doc = "Set the current time."]
                    #[doc = ""]
                    #[doc = "This call should be invoked exactly once per block. It will panic at the finalization"]
                    #[doc = "phase, if this call hasn't been invoked by that time."]
                    #[doc = ""]
                    #[doc = "The timestamp should be greater than the previous one by the amount specified by"]
                    #[doc = "[`Config::MinimumPeriod`]."]
                    #[doc = ""]
                    #[doc = "The dispatch origin for this call must be _None_."]
                    #[doc = ""]
                    #[doc = "This dispatch class is _Mandatory_ to ensure it gets executed in the block. Be aware"]
                    #[doc = "that changing the complexity of this call could result exhausting the resources in a"]
                    #[doc = "block to execute any other calls."]
                    #[doc = ""]
                    #[doc = "## Complexity"]
                    #[doc = "- `O(1)` (Note that implementations of `OnTimestampSet` must also be `O(1)`)"]
                    #[doc = "- 1 storage read and 1 storage mutation (codec `O(1)` because of `DidUpdate::take` in"]
                    #[doc = "  `on_finalize`)"]
                    #[doc = "- 1 event handler `on_timestamp_set`. Must be `O(1)`."]
                    set {
                        #[codec(compact)]
                        now: ::core::primitive::u64,
                    },
                }
            }
        }
        pub mod pallet_transaction_payment {
            use super::runtime_types;
            pub mod pallet {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "The `Event` enum of this pallet"]
                pub enum Event {
                    #[codec(index = 0)]
                    #[doc = "A transaction fee `actual_fee`, of which `tip` was added to the minimum inclusion fee,"]
                    #[doc = "has been paid by `who`."]
                    TransactionFeePaid {
                        who: ::subxt::ext::subxt_core::utils::AccountId32,
                        actual_fee: ::core::primitive::u128,
                        tip: ::core::primitive::u128,
                    },
                }
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub struct ChargeTransactionPayment(#[codec(compact)] pub ::core::primitive::u128);
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub enum Releases {
                #[codec(index = 0)]
                V1Ancient,
                #[codec(index = 1)]
                V2,
            }
        }
        pub mod pallet_treasury {
            use super::runtime_types;
            pub mod pallet {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "Contains a variant per dispatchable extrinsic that this pallet has."]
                pub enum Call {
                    #[codec(index = 3)]
                    #[doc = "Propose and approve a spend of treasury funds."]
                    #[doc = ""]
                    #[doc = "## Dispatch Origin"]
                    #[doc = ""]
                    #[doc = "Must be [`Config::SpendOrigin`] with the `Success` value being at least `amount`."]
                    #[doc = ""]
                    #[doc = "### Details"]
                    #[doc = "NOTE: For record-keeping purposes, the proposer is deemed to be equivalent to the"]
                    #[doc = "beneficiary."]
                    #[doc = ""]
                    #[doc = "### Parameters"]
                    #[doc = "- `amount`: The amount to be transferred from the treasury to the `beneficiary`."]
                    #[doc = "- `beneficiary`: The destination account for the transfer."]
                    #[doc = ""]
                    #[doc = "## Events"]
                    #[doc = ""]
                    #[doc = "Emits [`Event::SpendApproved`] if successful."]
                    spend_local {
                        #[codec(compact)]
                        amount: ::core::primitive::u128,
                        beneficiary: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                    },
                    #[codec(index = 4)]
                    #[doc = "Force a previously approved proposal to be removed from the approval queue."]
                    #[doc = ""]
                    #[doc = "## Dispatch Origin"]
                    #[doc = ""]
                    #[doc = "Must be [`Config::RejectOrigin`]."]
                    #[doc = ""]
                    #[doc = "## Details"]
                    #[doc = ""]
                    #[doc = "The original deposit will no longer be returned."]
                    #[doc = ""]
                    #[doc = "### Parameters"]
                    #[doc = "- `proposal_id`: The index of a proposal"]
                    #[doc = ""]
                    #[doc = "### Complexity"]
                    #[doc = "- O(A) where `A` is the number of approvals"]
                    #[doc = ""]
                    #[doc = "### Errors"]
                    #[doc = "- [`Error::ProposalNotApproved`]: The `proposal_id` supplied was not found in the"]
                    #[doc = "  approval queue, i.e., the proposal has not been approved. This could also mean the"]
                    #[doc = "  proposal does not exist altogether, thus there is no way it would have been approved"]
                    #[doc = "  in the first place."]
                    remove_approval {
                        #[codec(compact)]
                        proposal_id: ::core::primitive::u32,
                    },
                    #[codec(index = 5)]
                    #[doc = "Propose and approve a spend of treasury funds."]
                    #[doc = ""]
                    #[doc = "## Dispatch Origin"]
                    #[doc = ""]
                    #[doc = "Must be [`Config::SpendOrigin`] with the `Success` value being at least"]
                    #[doc = "`amount` of `asset_kind` in the native asset. The amount of `asset_kind` is converted"]
                    #[doc = "for assertion using the [`Config::BalanceConverter`]."]
                    #[doc = ""]
                    #[doc = "## Details"]
                    #[doc = ""]
                    #[doc = "Create an approved spend for transferring a specific `amount` of `asset_kind` to a"]
                    #[doc = "designated beneficiary. The spend must be claimed using the `payout` dispatchable within"]
                    #[doc = "the [`Config::PayoutPeriod`]."]
                    #[doc = ""]
                    #[doc = "### Parameters"]
                    #[doc = "- `asset_kind`: An indicator of the specific asset class to be spent."]
                    #[doc = "- `amount`: The amount to be transferred from the treasury to the `beneficiary`."]
                    #[doc = "- `beneficiary`: The beneficiary of the spend."]
                    #[doc = "- `valid_from`: The block number from which the spend can be claimed. It can refer to"]
                    #[doc = "  the past if the resulting spend has not yet expired according to the"]
                    #[doc = "  [`Config::PayoutPeriod`]. If `None`, the spend can be claimed immediately after"]
                    #[doc = "  approval."]
                    #[doc = ""]
                    #[doc = "## Events"]
                    #[doc = ""]
                    #[doc = "Emits [`Event::AssetSpendApproved`] if successful."]
                    spend {
                        asset_kind: ::subxt::ext::subxt_core::alloc::boxed::Box<()>,
                        #[codec(compact)]
                        amount: ::core::primitive::u128,
                        beneficiary: ::subxt::ext::subxt_core::alloc::boxed::Box<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                        >,
                        valid_from: ::core::option::Option<::core::primitive::u32>,
                    },
                    #[codec(index = 6)]
                    #[doc = "Claim a spend."]
                    #[doc = ""]
                    #[doc = "## Dispatch Origin"]
                    #[doc = ""]
                    #[doc = "Must be signed"]
                    #[doc = ""]
                    #[doc = "## Details"]
                    #[doc = ""]
                    #[doc = "Spends must be claimed within some temporal bounds. A spend may be claimed within one"]
                    #[doc = "[`Config::PayoutPeriod`] from the `valid_from` block."]
                    #[doc = "In case of a payout failure, the spend status must be updated with the `check_status`"]
                    #[doc = "dispatchable before retrying with the current function."]
                    #[doc = ""]
                    #[doc = "### Parameters"]
                    #[doc = "- `index`: The spend index."]
                    #[doc = ""]
                    #[doc = "## Events"]
                    #[doc = ""]
                    #[doc = "Emits [`Event::Paid`] if successful."]
                    payout { index: ::core::primitive::u32 },
                    #[codec(index = 7)]
                    #[doc = "Check the status of the spend and remove it from the storage if processed."]
                    #[doc = ""]
                    #[doc = "## Dispatch Origin"]
                    #[doc = ""]
                    #[doc = "Must be signed."]
                    #[doc = ""]
                    #[doc = "## Details"]
                    #[doc = ""]
                    #[doc = "The status check is a prerequisite for retrying a failed payout."]
                    #[doc = "If a spend has either succeeded or expired, it is removed from the storage by this"]
                    #[doc = "function. In such instances, transaction fees are refunded."]
                    #[doc = ""]
                    #[doc = "### Parameters"]
                    #[doc = "- `index`: The spend index."]
                    #[doc = ""]
                    #[doc = "## Events"]
                    #[doc = ""]
                    #[doc = "Emits [`Event::PaymentFailed`] if the spend payout has failed."]
                    #[doc = "Emits [`Event::SpendProcessed`] if the spend payout has succeed."]
                    check_status { index: ::core::primitive::u32 },
                    #[codec(index = 8)]
                    #[doc = "Void previously approved spend."]
                    #[doc = ""]
                    #[doc = "## Dispatch Origin"]
                    #[doc = ""]
                    #[doc = "Must be [`Config::RejectOrigin`]."]
                    #[doc = ""]
                    #[doc = "## Details"]
                    #[doc = ""]
                    #[doc = "A spend void is only possible if the payout has not been attempted yet."]
                    #[doc = ""]
                    #[doc = "### Parameters"]
                    #[doc = "- `index`: The spend index."]
                    #[doc = ""]
                    #[doc = "## Events"]
                    #[doc = ""]
                    #[doc = "Emits [`Event::AssetSpendVoided`] if successful."]
                    void_spend { index: ::core::primitive::u32 },
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "Error for the treasury pallet."]
                pub enum Error {
                    #[codec(index = 0)]
                    #[doc = "No proposal, bounty or spend at that index."]
                    InvalidIndex,
                    #[codec(index = 1)]
                    #[doc = "Too many approvals in the queue."]
                    TooManyApprovals,
                    #[codec(index = 2)]
                    #[doc = "The spend origin is valid but the amount it is allowed to spend is lower than the"]
                    #[doc = "amount to be spent."]
                    InsufficientPermission,
                    #[codec(index = 3)]
                    #[doc = "Proposal has not been approved."]
                    ProposalNotApproved,
                    #[codec(index = 4)]
                    #[doc = "The balance of the asset kind is not convertible to the balance of the native asset."]
                    FailedToConvertBalance,
                    #[codec(index = 5)]
                    #[doc = "The spend has expired and cannot be claimed."]
                    SpendExpired,
                    #[codec(index = 6)]
                    #[doc = "The spend is not yet eligible for payout."]
                    EarlyPayout,
                    #[codec(index = 7)]
                    #[doc = "The payment has already been attempted."]
                    AlreadyAttempted,
                    #[codec(index = 8)]
                    #[doc = "There was some issue with the mechanism of payment."]
                    PayoutError,
                    #[codec(index = 9)]
                    #[doc = "The payout was not yet attempted/claimed."]
                    NotAttempted,
                    #[codec(index = 10)]
                    #[doc = "The payment has neither failed nor succeeded yet."]
                    Inconclusive,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "The `Event` enum of this pallet"]
                pub enum Event {
                    #[codec(index = 0)]
                    #[doc = "We have ended a spend period and will now allocate funds."]
                    Spending {
                        budget_remaining: ::core::primitive::u128,
                    },
                    #[codec(index = 1)]
                    #[doc = "Some funds have been allocated."]
                    Awarded {
                        proposal_index: ::core::primitive::u32,
                        award: ::core::primitive::u128,
                        account: ::subxt::ext::subxt_core::utils::AccountId32,
                    },
                    #[codec(index = 2)]
                    #[doc = "Some of our funds have been burnt."]
                    Burnt {
                        burnt_funds: ::core::primitive::u128,
                    },
                    #[codec(index = 3)]
                    #[doc = "Spending has finished; this is the amount that rolls over until next spend."]
                    Rollover {
                        rollover_balance: ::core::primitive::u128,
                    },
                    #[codec(index = 4)]
                    #[doc = "Some funds have been deposited."]
                    Deposit { value: ::core::primitive::u128 },
                    #[codec(index = 5)]
                    #[doc = "A new spend proposal has been approved."]
                    SpendApproved {
                        proposal_index: ::core::primitive::u32,
                        amount: ::core::primitive::u128,
                        beneficiary: ::subxt::ext::subxt_core::utils::AccountId32,
                    },
                    #[codec(index = 6)]
                    #[doc = "The inactive funds of the pallet have been updated."]
                    UpdatedInactive {
                        reactivated: ::core::primitive::u128,
                        deactivated: ::core::primitive::u128,
                    },
                    #[codec(index = 7)]
                    #[doc = "A new asset spend proposal has been approved."]
                    AssetSpendApproved {
                        index: ::core::primitive::u32,
                        asset_kind: (),
                        amount: ::core::primitive::u128,
                        beneficiary: ::subxt::ext::subxt_core::utils::AccountId32,
                        valid_from: ::core::primitive::u32,
                        expire_at: ::core::primitive::u32,
                    },
                    #[codec(index = 8)]
                    #[doc = "An approved spend was voided."]
                    AssetSpendVoided { index: ::core::primitive::u32 },
                    #[codec(index = 9)]
                    #[doc = "A payment happened."]
                    Paid {
                        index: ::core::primitive::u32,
                        payment_id: (),
                    },
                    #[codec(index = 10)]
                    #[doc = "A payment failed and can be retried."]
                    PaymentFailed {
                        index: ::core::primitive::u32,
                        payment_id: (),
                    },
                    #[codec(index = 11)]
                    #[doc = "A spend was processed and removed from the storage. It might have been successfully"]
                    #[doc = "paid or it may have expired."]
                    SpendProcessed { index: ::core::primitive::u32 },
                }
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub enum PaymentState<_0> {
                #[codec(index = 0)]
                Pending,
                #[codec(index = 1)]
                Attempted { id: _0 },
                #[codec(index = 2)]
                Failed,
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub struct Proposal<_0, _1> {
                pub proposer: _0,
                pub value: _1,
                pub beneficiary: _0,
                pub bond: _1,
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub struct SpendStatus<_0, _1, _2, _3, _4> {
                pub asset_kind: _0,
                pub amount: _1,
                pub beneficiary: _2,
                pub valid_from: _3,
                pub expire_at: _3,
                pub status: runtime_types::pallet_treasury::PaymentState<_0>,
                #[codec(skip)]
                pub __ignore: ::core::marker::PhantomData<_4>,
            }
        }
        pub mod pallet_utility {
            use super::runtime_types;
            pub mod pallet {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "Contains a variant per dispatchable extrinsic that this pallet has."]
                pub enum Call {
                    #[codec(index = 0)]
                    #[doc = "Send a batch of dispatch calls."]
                    #[doc = ""]
                    #[doc = "May be called from any origin except `None`."]
                    #[doc = ""]
                    #[doc = "- `calls`: The calls to be dispatched from the same origin. The number of call must not"]
                    #[doc = "  exceed the constant: `batched_calls_limit` (available in constant metadata)."]
                    #[doc = ""]
                    #[doc = "If origin is root then the calls are dispatched without checking origin filter. (This"]
                    #[doc = "includes bypassing `frame_system::Config::BaseCallFilter`)."]
                    #[doc = ""]
                    #[doc = "## Complexity"]
                    #[doc = "- O(C) where C is the number of calls to be batched."]
                    #[doc = ""]
                    #[doc = "This will return `Ok` in all circumstances. To determine the success of the batch, an"]
                    #[doc = "event is deposited. If a call failed and the batch was interrupted, then the"]
                    #[doc = "`BatchInterrupted` event is deposited, along with the number of successful calls made"]
                    #[doc = "and the error of the failed call. If all were successful, then the `BatchCompleted`"]
                    #[doc = "event is deposited."]
                    batch {
                        calls: ::subxt::ext::subxt_core::alloc::vec::Vec<
                            runtime_types::vara_runtime::RuntimeCall,
                        >,
                    },
                    #[codec(index = 1)]
                    #[doc = "Send a call through an indexed pseudonym of the sender."]
                    #[doc = ""]
                    #[doc = "Filter from origin are passed along. The call will be dispatched with an origin which"]
                    #[doc = "use the same filter as the origin of this call."]
                    #[doc = ""]
                    #[doc = "NOTE: If you need to ensure that any account-based filtering is not honored (i.e."]
                    #[doc = "because you expect `proxy` to have been used prior in the call stack and you do not want"]
                    #[doc = "the call restrictions to apply to any sub-accounts), then use `as_multi_threshold_1`"]
                    #[doc = "in the Multisig pallet instead."]
                    #[doc = ""]
                    #[doc = "NOTE: Prior to version *12, this was called `as_limited_sub`."]
                    #[doc = ""]
                    #[doc = "The dispatch origin for this call must be _Signed_."]
                    as_derivative {
                        index: ::core::primitive::u16,
                        call: ::subxt::ext::subxt_core::alloc::boxed::Box<
                            runtime_types::vara_runtime::RuntimeCall,
                        >,
                    },
                    #[codec(index = 2)]
                    #[doc = "Send a batch of dispatch calls and atomically execute them."]
                    #[doc = "The whole transaction will rollback and fail if any of the calls failed."]
                    #[doc = ""]
                    #[doc = "May be called from any origin except `None`."]
                    #[doc = ""]
                    #[doc = "- `calls`: The calls to be dispatched from the same origin. The number of call must not"]
                    #[doc = "  exceed the constant: `batched_calls_limit` (available in constant metadata)."]
                    #[doc = ""]
                    #[doc = "If origin is root then the calls are dispatched without checking origin filter. (This"]
                    #[doc = "includes bypassing `frame_system::Config::BaseCallFilter`)."]
                    #[doc = ""]
                    #[doc = "## Complexity"]
                    #[doc = "- O(C) where C is the number of calls to be batched."]
                    batch_all {
                        calls: ::subxt::ext::subxt_core::alloc::vec::Vec<
                            runtime_types::vara_runtime::RuntimeCall,
                        >,
                    },
                    #[codec(index = 3)]
                    #[doc = "Dispatches a function call with a provided origin."]
                    #[doc = ""]
                    #[doc = "The dispatch origin for this call must be _Root_."]
                    #[doc = ""]
                    #[doc = "## Complexity"]
                    #[doc = "- O(1)."]
                    dispatch_as {
                        as_origin: ::subxt::ext::subxt_core::alloc::boxed::Box<
                            runtime_types::vara_runtime::OriginCaller,
                        >,
                        call: ::subxt::ext::subxt_core::alloc::boxed::Box<
                            runtime_types::vara_runtime::RuntimeCall,
                        >,
                    },
                    #[codec(index = 4)]
                    #[doc = "Send a batch of dispatch calls."]
                    #[doc = "Unlike `batch`, it allows errors and won't interrupt."]
                    #[doc = ""]
                    #[doc = "May be called from any origin except `None`."]
                    #[doc = ""]
                    #[doc = "- `calls`: The calls to be dispatched from the same origin. The number of call must not"]
                    #[doc = "  exceed the constant: `batched_calls_limit` (available in constant metadata)."]
                    #[doc = ""]
                    #[doc = "If origin is root then the calls are dispatch without checking origin filter. (This"]
                    #[doc = "includes bypassing `frame_system::Config::BaseCallFilter`)."]
                    #[doc = ""]
                    #[doc = "## Complexity"]
                    #[doc = "- O(C) where C is the number of calls to be batched."]
                    force_batch {
                        calls: ::subxt::ext::subxt_core::alloc::vec::Vec<
                            runtime_types::vara_runtime::RuntimeCall,
                        >,
                    },
                    #[codec(index = 5)]
                    #[doc = "Dispatch a function call with a specified weight."]
                    #[doc = ""]
                    #[doc = "This function does not check the weight of the call, and instead allows the"]
                    #[doc = "Root origin to specify the weight of the call."]
                    #[doc = ""]
                    #[doc = "The dispatch origin for this call must be _Root_."]
                    with_weight {
                        call: ::subxt::ext::subxt_core::alloc::boxed::Box<
                            runtime_types::vara_runtime::RuntimeCall,
                        >,
                        weight: runtime_types::sp_weights::weight_v2::Weight,
                    },
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "The `Error` enum of this pallet."]
                pub enum Error {
                    #[codec(index = 0)]
                    #[doc = "Too many calls batched."]
                    TooManyCalls,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "The `Event` enum of this pallet"]
                pub enum Event {
                    #[codec(index = 0)]
                    #[doc = "Batch of dispatches did not complete fully. Index of first failing dispatch given, as"]
                    #[doc = "well as the error."]
                    BatchInterrupted {
                        index: ::core::primitive::u32,
                        error: runtime_types::sp_runtime::DispatchError,
                    },
                    #[codec(index = 1)]
                    #[doc = "Batch of dispatches completed fully with no error."]
                    BatchCompleted,
                    #[codec(index = 2)]
                    #[doc = "Batch of dispatches completed but has errors."]
                    BatchCompletedWithErrors,
                    #[codec(index = 3)]
                    #[doc = "A single item within a Batch of dispatches has completed with no error."]
                    ItemCompleted,
                    #[codec(index = 4)]
                    #[doc = "A single item within a Batch of dispatches has completed with error."]
                    ItemFailed {
                        error: runtime_types::sp_runtime::DispatchError,
                    },
                    #[codec(index = 5)]
                    #[doc = "A call was dispatched."]
                    DispatchedAs {
                        result:
                            ::core::result::Result<(), runtime_types::sp_runtime::DispatchError>,
                    },
                }
            }
        }
        pub mod pallet_vesting {
            use super::runtime_types;
            pub mod pallet {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "Contains a variant per dispatchable extrinsic that this pallet has."]
                pub enum Call {
                    #[codec(index = 0)]
                    #[doc = "Unlock any vested funds of the sender account."]
                    #[doc = ""]
                    #[doc = "The dispatch origin for this call must be _Signed_ and the sender must have funds still"]
                    #[doc = "locked under this pallet."]
                    #[doc = ""]
                    #[doc = "Emits either `VestingCompleted` or `VestingUpdated`."]
                    #[doc = ""]
                    #[doc = "## Complexity"]
                    #[doc = "- `O(1)`."]
                    vest,
                    #[codec(index = 1)]
                    #[doc = "Unlock any vested funds of a `target` account."]
                    #[doc = ""]
                    #[doc = "The dispatch origin for this call must be _Signed_."]
                    #[doc = ""]
                    #[doc = "- `target`: The account whose vested funds should be unlocked. Must have funds still"]
                    #[doc = "locked under this pallet."]
                    #[doc = ""]
                    #[doc = "Emits either `VestingCompleted` or `VestingUpdated`."]
                    #[doc = ""]
                    #[doc = "## Complexity"]
                    #[doc = "- `O(1)`."]
                    vest_other {
                        target: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                    },
                    #[codec(index = 2)]
                    #[doc = "Create a vested transfer."]
                    #[doc = ""]
                    #[doc = "The dispatch origin for this call must be _Signed_."]
                    #[doc = ""]
                    #[doc = "- `target`: The account receiving the vested funds."]
                    #[doc = "- `schedule`: The vesting schedule attached to the transfer."]
                    #[doc = ""]
                    #[doc = "Emits `VestingCreated`."]
                    #[doc = ""]
                    #[doc = "NOTE: This will unlock all schedules through the current block."]
                    #[doc = ""]
                    #[doc = "## Complexity"]
                    #[doc = "- `O(1)`."]
                    vested_transfer {
                        target: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                        schedule: runtime_types::pallet_vesting::vesting_info::VestingInfo<
                            ::core::primitive::u128,
                            ::core::primitive::u32,
                        >,
                    },
                    #[codec(index = 3)]
                    #[doc = "Force a vested transfer."]
                    #[doc = ""]
                    #[doc = "The dispatch origin for this call must be _Root_."]
                    #[doc = ""]
                    #[doc = "- `source`: The account whose funds should be transferred."]
                    #[doc = "- `target`: The account that should be transferred the vested funds."]
                    #[doc = "- `schedule`: The vesting schedule attached to the transfer."]
                    #[doc = ""]
                    #[doc = "Emits `VestingCreated`."]
                    #[doc = ""]
                    #[doc = "NOTE: This will unlock all schedules through the current block."]
                    #[doc = ""]
                    #[doc = "## Complexity"]
                    #[doc = "- `O(1)`."]
                    force_vested_transfer {
                        source: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                        target: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                        schedule: runtime_types::pallet_vesting::vesting_info::VestingInfo<
                            ::core::primitive::u128,
                            ::core::primitive::u32,
                        >,
                    },
                    #[codec(index = 4)]
                    #[doc = "Merge two vesting schedules together, creating a new vesting schedule that unlocks over"]
                    #[doc = "the highest possible start and end blocks. If both schedules have already started the"]
                    #[doc = "current block will be used as the schedule start; with the caveat that if one schedule"]
                    #[doc = "is finished by the current block, the other will be treated as the new merged schedule,"]
                    #[doc = "unmodified."]
                    #[doc = ""]
                    #[doc = "NOTE: If `schedule1_index == schedule2_index` this is a no-op."]
                    #[doc = "NOTE: This will unlock all schedules through the current block prior to merging."]
                    #[doc = "NOTE: If both schedules have ended by the current block, no new schedule will be created"]
                    #[doc = "and both will be removed."]
                    #[doc = ""]
                    #[doc = "Merged schedule attributes:"]
                    #[doc = "- `starting_block`: `MAX(schedule1.starting_block, scheduled2.starting_block,"]
                    #[doc = "  current_block)`."]
                    #[doc = "- `ending_block`: `MAX(schedule1.ending_block, schedule2.ending_block)`."]
                    #[doc = "- `locked`: `schedule1.locked_at(current_block) + schedule2.locked_at(current_block)`."]
                    #[doc = ""]
                    #[doc = "The dispatch origin for this call must be _Signed_."]
                    #[doc = ""]
                    #[doc = "- `schedule1_index`: index of the first schedule to merge."]
                    #[doc = "- `schedule2_index`: index of the second schedule to merge."]
                    merge_schedules {
                        schedule1_index: ::core::primitive::u32,
                        schedule2_index: ::core::primitive::u32,
                    },
                    #[codec(index = 5)]
                    #[doc = "Force remove a vesting schedule"]
                    #[doc = ""]
                    #[doc = "The dispatch origin for this call must be _Root_."]
                    #[doc = ""]
                    #[doc = "- `target`: An account that has a vesting schedule"]
                    #[doc = "- `schedule_index`: The vesting schedule index that should be removed"]
                    force_remove_vesting_schedule {
                        target: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                        schedule_index: ::core::primitive::u32,
                    },
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "Error for the vesting pallet."]
                pub enum Error {
                    #[codec(index = 0)]
                    #[doc = "The account given is not vesting."]
                    NotVesting,
                    #[codec(index = 1)]
                    #[doc = "The account already has `MaxVestingSchedules` count of schedules and thus"]
                    #[doc = "cannot add another one. Consider merging existing schedules in order to add another."]
                    AtMaxVestingSchedules,
                    #[codec(index = 2)]
                    #[doc = "Amount being transferred is too low to create a vesting schedule."]
                    AmountLow,
                    #[codec(index = 3)]
                    #[doc = "An index was out of bounds of the vesting schedules."]
                    ScheduleIndexOutOfBounds,
                    #[codec(index = 4)]
                    #[doc = "Failed to create a new schedule because some parameter was invalid."]
                    InvalidScheduleParams,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "The `Event` enum of this pallet"]
                pub enum Event {
                    #[codec(index = 0)]
                    #[doc = "The amount vested has been updated. This could indicate a change in funds available."]
                    #[doc = "The balance given is the amount which is left unvested (and thus locked)."]
                    VestingUpdated {
                        account: ::subxt::ext::subxt_core::utils::AccountId32,
                        unvested: ::core::primitive::u128,
                    },
                    #[codec(index = 1)]
                    #[doc = "An \\[account\\] has become fully vested."]
                    VestingCompleted {
                        account: ::subxt::ext::subxt_core::utils::AccountId32,
                    },
                }
            }
            pub mod vesting_info {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct VestingInfo<_0, _1> {
                    pub locked: _0,
                    pub per_block: _0,
                    pub starting_block: _1,
                }
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub enum Releases {
                #[codec(index = 0)]
                V0,
                #[codec(index = 1)]
                V1,
            }
        }
        pub mod pallet_whitelist {
            use super::runtime_types;
            pub mod pallet {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "Contains a variant per dispatchable extrinsic that this pallet has."]
                pub enum Call {
                    #[codec(index = 0)]
                    whitelist_call {
                        call_hash: ::subxt::ext::subxt_core::utils::H256,
                    },
                    #[codec(index = 1)]
                    remove_whitelisted_call {
                        call_hash: ::subxt::ext::subxt_core::utils::H256,
                    },
                    #[codec(index = 2)]
                    dispatch_whitelisted_call {
                        call_hash: ::subxt::ext::subxt_core::utils::H256,
                        call_encoded_len: ::core::primitive::u32,
                        call_weight_witness: runtime_types::sp_weights::weight_v2::Weight,
                    },
                    #[codec(index = 3)]
                    dispatch_whitelisted_call_with_preimage {
                        call: ::subxt::ext::subxt_core::alloc::boxed::Box<
                            runtime_types::vara_runtime::RuntimeCall,
                        >,
                    },
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "The `Error` enum of this pallet."]
                pub enum Error {
                    #[codec(index = 0)]
                    #[doc = "The preimage of the call hash could not be loaded."]
                    UnavailablePreImage,
                    #[codec(index = 1)]
                    #[doc = "The call could not be decoded."]
                    UndecodableCall,
                    #[codec(index = 2)]
                    #[doc = "The weight of the decoded call was higher than the witness."]
                    InvalidCallWeightWitness,
                    #[codec(index = 3)]
                    #[doc = "The call was not whitelisted."]
                    CallIsNotWhitelisted,
                    #[codec(index = 4)]
                    #[doc = "The call was already whitelisted; No-Op."]
                    CallAlreadyWhitelisted,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "The `Event` enum of this pallet"]
                pub enum Event {
                    #[codec(index = 0)]
                    CallWhitelisted {
                        call_hash: ::subxt::ext::subxt_core::utils::H256,
                    },
                    #[codec(index = 1)]
                    WhitelistedCallRemoved {
                        call_hash: ::subxt::ext::subxt_core::utils::H256,
                    },
                    #[codec(index = 2)]
                    WhitelistedCallDispatched {
                        call_hash: ::subxt::ext::subxt_core::utils::H256,
                        result: ::core::result::Result<
                            runtime_types::frame_support::dispatch::PostDispatchInfo,
                            runtime_types::sp_runtime::DispatchErrorWithPostInfo<
                                runtime_types::frame_support::dispatch::PostDispatchInfo,
                            >,
                        >,
                    },
                }
            }
        }
        pub mod primitive_types {
            use super::runtime_types;
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub struct U256(pub [::core::primitive::u64; 4usize]);
        }
        pub mod sp_arithmetic {
            use super::runtime_types;
            pub mod fixed_point {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct FixedI64(pub ::core::primitive::i64);
                #[derive(
                    ::subxt::ext ::subxt_core::ext::codec::CompactAs,
                    Debug,
                    crate::gp::Decode,
                    crate::gp::DecodeAsType,
                    crate::gp::Encode,
                )]
                pub struct FixedU128(pub ::core::primitive::u128);
            }
            pub mod per_things {
                use super::runtime_types;
                #[derive(
                    ::subxt::ext ::subxt_core::ext::codec::CompactAs,
                    Debug,
                    crate::gp::Decode,
                    crate::gp::DecodeAsType,
                    crate::gp::Encode,
                )]
                pub struct PerU16(pub ::core::primitive::u16);
                #[derive(
                    ::subxt::ext ::subxt_core::ext::codec::CompactAs,
                    Debug,
                    crate::gp::Decode,
                    crate::gp::DecodeAsType,
                    crate::gp::Encode,
                )]
                pub struct Perbill(pub ::core::primitive::u32);
                #[derive(
                    ::subxt::ext ::subxt_core::ext::codec::CompactAs,
                    Debug,
                    crate::gp::Decode,
                    crate::gp::DecodeAsType,
                    crate::gp::Encode,
                )]
                pub struct Percent(pub ::core::primitive::u8);
                #[derive(
                    ::subxt::ext ::subxt_core::ext::codec::CompactAs,
                    Debug,
                    crate::gp::Decode,
                    crate::gp::DecodeAsType,
                    crate::gp::Encode,
                )]
                pub struct Permill(pub ::core::primitive::u32);
                #[derive(
                    ::subxt::ext ::subxt_core::ext::codec::CompactAs,
                    Debug,
                    crate::gp::Decode,
                    crate::gp::DecodeAsType,
                    crate::gp::Encode,
                )]
                pub struct Perquintill(pub ::core::primitive::u64);
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub enum ArithmeticError {
                #[codec(index = 0)]
                Underflow,
                #[codec(index = 1)]
                Overflow,
                #[codec(index = 2)]
                DivisionByZero,
            }
        }
        pub mod sp_authority_discovery {
            use super::runtime_types;
            pub mod app {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct Public(pub [::core::primitive::u8; 32usize]);
            }
        }
        pub mod sp_consensus_babe {
            use super::runtime_types;
            pub mod app {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct Public(pub [::core::primitive::u8; 32usize]);
            }
            pub mod digests {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum NextConfigDescriptor {
                    #[codec(index = 1)]
                    V1 {
                        c: (::core::primitive::u64, ::core::primitive::u64),
                        allowed_slots: runtime_types::sp_consensus_babe::AllowedSlots,
                    },
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum PreDigest {
                    #[codec(index = 1)]
                    Primary(runtime_types::sp_consensus_babe::digests::PrimaryPreDigest),
                    #[codec(index = 2)]
                    SecondaryPlain(
                        runtime_types::sp_consensus_babe::digests::SecondaryPlainPreDigest,
                    ),
                    #[codec(index = 3)]
                    SecondaryVRF(runtime_types::sp_consensus_babe::digests::SecondaryVRFPreDigest),
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct PrimaryPreDigest {
                    pub authority_index: ::core::primitive::u32,
                    pub slot: runtime_types::sp_consensus_slots::Slot,
                    pub vrf_signature: runtime_types::sp_core::sr25519::vrf::VrfSignature,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct SecondaryPlainPreDigest {
                    pub authority_index: ::core::primitive::u32,
                    pub slot: runtime_types::sp_consensus_slots::Slot,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct SecondaryVRFPreDigest {
                    pub authority_index: ::core::primitive::u32,
                    pub slot: runtime_types::sp_consensus_slots::Slot,
                    pub vrf_signature: runtime_types::sp_core::sr25519::vrf::VrfSignature,
                }
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub enum AllowedSlots {
                #[codec(index = 0)]
                PrimarySlots,
                #[codec(index = 1)]
                PrimaryAndSecondaryPlainSlots,
                #[codec(index = 2)]
                PrimaryAndSecondaryVRFSlots,
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub struct BabeEpochConfiguration {
                pub c: (::core::primitive::u64, ::core::primitive::u64),
                pub allowed_slots: runtime_types::sp_consensus_babe::AllowedSlots,
            }
        }
        pub mod sp_consensus_grandpa {
            use super::runtime_types;
            pub mod app {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct Public(pub [::core::primitive::u8; 32usize]);
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct Signature(pub [::core::primitive::u8; 64usize]);
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub enum Equivocation<_0, _1> {
                #[codec(index = 0)]
                Prevote(
                    runtime_types::finality_grandpa::Equivocation<
                        runtime_types::sp_consensus_grandpa::app::Public,
                        runtime_types::finality_grandpa::Prevote<_0, _1>,
                        runtime_types::sp_consensus_grandpa::app::Signature,
                    >,
                ),
                #[codec(index = 1)]
                Precommit(
                    runtime_types::finality_grandpa::Equivocation<
                        runtime_types::sp_consensus_grandpa::app::Public,
                        runtime_types::finality_grandpa::Precommit<_0, _1>,
                        runtime_types::sp_consensus_grandpa::app::Signature,
                    >,
                ),
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub struct EquivocationProof<_0, _1> {
                pub set_id: ::core::primitive::u64,
                pub equivocation: runtime_types::sp_consensus_grandpa::Equivocation<_0, _1>,
            }
        }
        pub mod sp_consensus_slots {
            use super::runtime_types;
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub struct EquivocationProof<_0, _1> {
                pub offender: _1,
                pub slot: runtime_types::sp_consensus_slots::Slot,
                pub first_header: _0,
                pub second_header: _0,
            }
            #[derive(
                ::subxt::ext ::subxt_core::ext::codec::CompactAs,
                Debug,
                crate::gp::Decode,
                crate::gp::DecodeAsType,
                crate::gp::Encode,
            )]
            pub struct Slot(pub ::core::primitive::u64);
        }
        pub mod sp_core {
            use super::runtime_types;
            pub mod crypto {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct KeyTypeId(pub [::core::primitive::u8; 4usize]);
            }
            pub mod sr25519 {
                use super::runtime_types;
                pub mod vrf {
                    use super::runtime_types;
                    #[derive(
                        Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode,
                    )]
                    pub struct VrfSignature {
                        pub pre_output: [::core::primitive::u8; 32usize],
                        pub proof: [::core::primitive::u8; 64usize],
                    }
                }
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub enum Void {}
        }
        pub mod sp_npos_elections {
            use super::runtime_types;
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub struct ElectionScore {
                pub minimal_stake: ::core::primitive::u128,
                pub sum_stake: ::core::primitive::u128,
                pub sum_stake_squared: ::core::primitive::u128,
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub struct Support<_0> {
                pub total: ::core::primitive::u128,
                pub voters:
                    ::subxt::ext::subxt_core::alloc::vec::Vec<(_0, ::core::primitive::u128)>,
            }
        }
        pub mod sp_runtime {
            use super::runtime_types;
            pub mod generic {
                use super::runtime_types;
                pub mod digest {
                    use super::runtime_types;
                    #[derive(
                        Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode,
                    )]
                    pub struct Digest {
                        pub logs: ::subxt::ext::subxt_core::alloc::vec::Vec<
                            runtime_types::sp_runtime::generic::digest::DigestItem,
                        >,
                    }
                    #[derive(
                        Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode,
                    )]
                    pub enum DigestItem {
                        #[codec(index = 6)]
                        PreRuntime(
                            [::core::primitive::u8; 4usize],
                            ::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
                        ),
                        #[codec(index = 4)]
                        Consensus(
                            [::core::primitive::u8; 4usize],
                            ::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
                        ),
                        #[codec(index = 5)]
                        Seal(
                            [::core::primitive::u8; 4usize],
                            ::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
                        ),
                        #[codec(index = 0)]
                        Other(::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>),
                        #[codec(index = 8)]
                        RuntimeEnvironmentUpdated,
                    }
                }
                pub mod era {
                    use super::runtime_types;
                    #[derive(
                        Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode,
                    )]
                    pub enum Era {
                        #[codec(index = 0)]
                        Immortal,
                        #[codec(index = 1)]
                        Mortal1(::core::primitive::u8),
                        #[codec(index = 2)]
                        Mortal2(::core::primitive::u8),
                        #[codec(index = 3)]
                        Mortal3(::core::primitive::u8),
                        #[codec(index = 4)]
                        Mortal4(::core::primitive::u8),
                        #[codec(index = 5)]
                        Mortal5(::core::primitive::u8),
                        #[codec(index = 6)]
                        Mortal6(::core::primitive::u8),
                        #[codec(index = 7)]
                        Mortal7(::core::primitive::u8),
                        #[codec(index = 8)]
                        Mortal8(::core::primitive::u8),
                        #[codec(index = 9)]
                        Mortal9(::core::primitive::u8),
                        #[codec(index = 10)]
                        Mortal10(::core::primitive::u8),
                        #[codec(index = 11)]
                        Mortal11(::core::primitive::u8),
                        #[codec(index = 12)]
                        Mortal12(::core::primitive::u8),
                        #[codec(index = 13)]
                        Mortal13(::core::primitive::u8),
                        #[codec(index = 14)]
                        Mortal14(::core::primitive::u8),
                        #[codec(index = 15)]
                        Mortal15(::core::primitive::u8),
                        #[codec(index = 16)]
                        Mortal16(::core::primitive::u8),
                        #[codec(index = 17)]
                        Mortal17(::core::primitive::u8),
                        #[codec(index = 18)]
                        Mortal18(::core::primitive::u8),
                        #[codec(index = 19)]
                        Mortal19(::core::primitive::u8),
                        #[codec(index = 20)]
                        Mortal20(::core::primitive::u8),
                        #[codec(index = 21)]
                        Mortal21(::core::primitive::u8),
                        #[codec(index = 22)]
                        Mortal22(::core::primitive::u8),
                        #[codec(index = 23)]
                        Mortal23(::core::primitive::u8),
                        #[codec(index = 24)]
                        Mortal24(::core::primitive::u8),
                        #[codec(index = 25)]
                        Mortal25(::core::primitive::u8),
                        #[codec(index = 26)]
                        Mortal26(::core::primitive::u8),
                        #[codec(index = 27)]
                        Mortal27(::core::primitive::u8),
                        #[codec(index = 28)]
                        Mortal28(::core::primitive::u8),
                        #[codec(index = 29)]
                        Mortal29(::core::primitive::u8),
                        #[codec(index = 30)]
                        Mortal30(::core::primitive::u8),
                        #[codec(index = 31)]
                        Mortal31(::core::primitive::u8),
                        #[codec(index = 32)]
                        Mortal32(::core::primitive::u8),
                        #[codec(index = 33)]
                        Mortal33(::core::primitive::u8),
                        #[codec(index = 34)]
                        Mortal34(::core::primitive::u8),
                        #[codec(index = 35)]
                        Mortal35(::core::primitive::u8),
                        #[codec(index = 36)]
                        Mortal36(::core::primitive::u8),
                        #[codec(index = 37)]
                        Mortal37(::core::primitive::u8),
                        #[codec(index = 38)]
                        Mortal38(::core::primitive::u8),
                        #[codec(index = 39)]
                        Mortal39(::core::primitive::u8),
                        #[codec(index = 40)]
                        Mortal40(::core::primitive::u8),
                        #[codec(index = 41)]
                        Mortal41(::core::primitive::u8),
                        #[codec(index = 42)]
                        Mortal42(::core::primitive::u8),
                        #[codec(index = 43)]
                        Mortal43(::core::primitive::u8),
                        #[codec(index = 44)]
                        Mortal44(::core::primitive::u8),
                        #[codec(index = 45)]
                        Mortal45(::core::primitive::u8),
                        #[codec(index = 46)]
                        Mortal46(::core::primitive::u8),
                        #[codec(index = 47)]
                        Mortal47(::core::primitive::u8),
                        #[codec(index = 48)]
                        Mortal48(::core::primitive::u8),
                        #[codec(index = 49)]
                        Mortal49(::core::primitive::u8),
                        #[codec(index = 50)]
                        Mortal50(::core::primitive::u8),
                        #[codec(index = 51)]
                        Mortal51(::core::primitive::u8),
                        #[codec(index = 52)]
                        Mortal52(::core::primitive::u8),
                        #[codec(index = 53)]
                        Mortal53(::core::primitive::u8),
                        #[codec(index = 54)]
                        Mortal54(::core::primitive::u8),
                        #[codec(index = 55)]
                        Mortal55(::core::primitive::u8),
                        #[codec(index = 56)]
                        Mortal56(::core::primitive::u8),
                        #[codec(index = 57)]
                        Mortal57(::core::primitive::u8),
                        #[codec(index = 58)]
                        Mortal58(::core::primitive::u8),
                        #[codec(index = 59)]
                        Mortal59(::core::primitive::u8),
                        #[codec(index = 60)]
                        Mortal60(::core::primitive::u8),
                        #[codec(index = 61)]
                        Mortal61(::core::primitive::u8),
                        #[codec(index = 62)]
                        Mortal62(::core::primitive::u8),
                        #[codec(index = 63)]
                        Mortal63(::core::primitive::u8),
                        #[codec(index = 64)]
                        Mortal64(::core::primitive::u8),
                        #[codec(index = 65)]
                        Mortal65(::core::primitive::u8),
                        #[codec(index = 66)]
                        Mortal66(::core::primitive::u8),
                        #[codec(index = 67)]
                        Mortal67(::core::primitive::u8),
                        #[codec(index = 68)]
                        Mortal68(::core::primitive::u8),
                        #[codec(index = 69)]
                        Mortal69(::core::primitive::u8),
                        #[codec(index = 70)]
                        Mortal70(::core::primitive::u8),
                        #[codec(index = 71)]
                        Mortal71(::core::primitive::u8),
                        #[codec(index = 72)]
                        Mortal72(::core::primitive::u8),
                        #[codec(index = 73)]
                        Mortal73(::core::primitive::u8),
                        #[codec(index = 74)]
                        Mortal74(::core::primitive::u8),
                        #[codec(index = 75)]
                        Mortal75(::core::primitive::u8),
                        #[codec(index = 76)]
                        Mortal76(::core::primitive::u8),
                        #[codec(index = 77)]
                        Mortal77(::core::primitive::u8),
                        #[codec(index = 78)]
                        Mortal78(::core::primitive::u8),
                        #[codec(index = 79)]
                        Mortal79(::core::primitive::u8),
                        #[codec(index = 80)]
                        Mortal80(::core::primitive::u8),
                        #[codec(index = 81)]
                        Mortal81(::core::primitive::u8),
                        #[codec(index = 82)]
                        Mortal82(::core::primitive::u8),
                        #[codec(index = 83)]
                        Mortal83(::core::primitive::u8),
                        #[codec(index = 84)]
                        Mortal84(::core::primitive::u8),
                        #[codec(index = 85)]
                        Mortal85(::core::primitive::u8),
                        #[codec(index = 86)]
                        Mortal86(::core::primitive::u8),
                        #[codec(index = 87)]
                        Mortal87(::core::primitive::u8),
                        #[codec(index = 88)]
                        Mortal88(::core::primitive::u8),
                        #[codec(index = 89)]
                        Mortal89(::core::primitive::u8),
                        #[codec(index = 90)]
                        Mortal90(::core::primitive::u8),
                        #[codec(index = 91)]
                        Mortal91(::core::primitive::u8),
                        #[codec(index = 92)]
                        Mortal92(::core::primitive::u8),
                        #[codec(index = 93)]
                        Mortal93(::core::primitive::u8),
                        #[codec(index = 94)]
                        Mortal94(::core::primitive::u8),
                        #[codec(index = 95)]
                        Mortal95(::core::primitive::u8),
                        #[codec(index = 96)]
                        Mortal96(::core::primitive::u8),
                        #[codec(index = 97)]
                        Mortal97(::core::primitive::u8),
                        #[codec(index = 98)]
                        Mortal98(::core::primitive::u8),
                        #[codec(index = 99)]
                        Mortal99(::core::primitive::u8),
                        #[codec(index = 100)]
                        Mortal100(::core::primitive::u8),
                        #[codec(index = 101)]
                        Mortal101(::core::primitive::u8),
                        #[codec(index = 102)]
                        Mortal102(::core::primitive::u8),
                        #[codec(index = 103)]
                        Mortal103(::core::primitive::u8),
                        #[codec(index = 104)]
                        Mortal104(::core::primitive::u8),
                        #[codec(index = 105)]
                        Mortal105(::core::primitive::u8),
                        #[codec(index = 106)]
                        Mortal106(::core::primitive::u8),
                        #[codec(index = 107)]
                        Mortal107(::core::primitive::u8),
                        #[codec(index = 108)]
                        Mortal108(::core::primitive::u8),
                        #[codec(index = 109)]
                        Mortal109(::core::primitive::u8),
                        #[codec(index = 110)]
                        Mortal110(::core::primitive::u8),
                        #[codec(index = 111)]
                        Mortal111(::core::primitive::u8),
                        #[codec(index = 112)]
                        Mortal112(::core::primitive::u8),
                        #[codec(index = 113)]
                        Mortal113(::core::primitive::u8),
                        #[codec(index = 114)]
                        Mortal114(::core::primitive::u8),
                        #[codec(index = 115)]
                        Mortal115(::core::primitive::u8),
                        #[codec(index = 116)]
                        Mortal116(::core::primitive::u8),
                        #[codec(index = 117)]
                        Mortal117(::core::primitive::u8),
                        #[codec(index = 118)]
                        Mortal118(::core::primitive::u8),
                        #[codec(index = 119)]
                        Mortal119(::core::primitive::u8),
                        #[codec(index = 120)]
                        Mortal120(::core::primitive::u8),
                        #[codec(index = 121)]
                        Mortal121(::core::primitive::u8),
                        #[codec(index = 122)]
                        Mortal122(::core::primitive::u8),
                        #[codec(index = 123)]
                        Mortal123(::core::primitive::u8),
                        #[codec(index = 124)]
                        Mortal124(::core::primitive::u8),
                        #[codec(index = 125)]
                        Mortal125(::core::primitive::u8),
                        #[codec(index = 126)]
                        Mortal126(::core::primitive::u8),
                        #[codec(index = 127)]
                        Mortal127(::core::primitive::u8),
                        #[codec(index = 128)]
                        Mortal128(::core::primitive::u8),
                        #[codec(index = 129)]
                        Mortal129(::core::primitive::u8),
                        #[codec(index = 130)]
                        Mortal130(::core::primitive::u8),
                        #[codec(index = 131)]
                        Mortal131(::core::primitive::u8),
                        #[codec(index = 132)]
                        Mortal132(::core::primitive::u8),
                        #[codec(index = 133)]
                        Mortal133(::core::primitive::u8),
                        #[codec(index = 134)]
                        Mortal134(::core::primitive::u8),
                        #[codec(index = 135)]
                        Mortal135(::core::primitive::u8),
                        #[codec(index = 136)]
                        Mortal136(::core::primitive::u8),
                        #[codec(index = 137)]
                        Mortal137(::core::primitive::u8),
                        #[codec(index = 138)]
                        Mortal138(::core::primitive::u8),
                        #[codec(index = 139)]
                        Mortal139(::core::primitive::u8),
                        #[codec(index = 140)]
                        Mortal140(::core::primitive::u8),
                        #[codec(index = 141)]
                        Mortal141(::core::primitive::u8),
                        #[codec(index = 142)]
                        Mortal142(::core::primitive::u8),
                        #[codec(index = 143)]
                        Mortal143(::core::primitive::u8),
                        #[codec(index = 144)]
                        Mortal144(::core::primitive::u8),
                        #[codec(index = 145)]
                        Mortal145(::core::primitive::u8),
                        #[codec(index = 146)]
                        Mortal146(::core::primitive::u8),
                        #[codec(index = 147)]
                        Mortal147(::core::primitive::u8),
                        #[codec(index = 148)]
                        Mortal148(::core::primitive::u8),
                        #[codec(index = 149)]
                        Mortal149(::core::primitive::u8),
                        #[codec(index = 150)]
                        Mortal150(::core::primitive::u8),
                        #[codec(index = 151)]
                        Mortal151(::core::primitive::u8),
                        #[codec(index = 152)]
                        Mortal152(::core::primitive::u8),
                        #[codec(index = 153)]
                        Mortal153(::core::primitive::u8),
                        #[codec(index = 154)]
                        Mortal154(::core::primitive::u8),
                        #[codec(index = 155)]
                        Mortal155(::core::primitive::u8),
                        #[codec(index = 156)]
                        Mortal156(::core::primitive::u8),
                        #[codec(index = 157)]
                        Mortal157(::core::primitive::u8),
                        #[codec(index = 158)]
                        Mortal158(::core::primitive::u8),
                        #[codec(index = 159)]
                        Mortal159(::core::primitive::u8),
                        #[codec(index = 160)]
                        Mortal160(::core::primitive::u8),
                        #[codec(index = 161)]
                        Mortal161(::core::primitive::u8),
                        #[codec(index = 162)]
                        Mortal162(::core::primitive::u8),
                        #[codec(index = 163)]
                        Mortal163(::core::primitive::u8),
                        #[codec(index = 164)]
                        Mortal164(::core::primitive::u8),
                        #[codec(index = 165)]
                        Mortal165(::core::primitive::u8),
                        #[codec(index = 166)]
                        Mortal166(::core::primitive::u8),
                        #[codec(index = 167)]
                        Mortal167(::core::primitive::u8),
                        #[codec(index = 168)]
                        Mortal168(::core::primitive::u8),
                        #[codec(index = 169)]
                        Mortal169(::core::primitive::u8),
                        #[codec(index = 170)]
                        Mortal170(::core::primitive::u8),
                        #[codec(index = 171)]
                        Mortal171(::core::primitive::u8),
                        #[codec(index = 172)]
                        Mortal172(::core::primitive::u8),
                        #[codec(index = 173)]
                        Mortal173(::core::primitive::u8),
                        #[codec(index = 174)]
                        Mortal174(::core::primitive::u8),
                        #[codec(index = 175)]
                        Mortal175(::core::primitive::u8),
                        #[codec(index = 176)]
                        Mortal176(::core::primitive::u8),
                        #[codec(index = 177)]
                        Mortal177(::core::primitive::u8),
                        #[codec(index = 178)]
                        Mortal178(::core::primitive::u8),
                        #[codec(index = 179)]
                        Mortal179(::core::primitive::u8),
                        #[codec(index = 180)]
                        Mortal180(::core::primitive::u8),
                        #[codec(index = 181)]
                        Mortal181(::core::primitive::u8),
                        #[codec(index = 182)]
                        Mortal182(::core::primitive::u8),
                        #[codec(index = 183)]
                        Mortal183(::core::primitive::u8),
                        #[codec(index = 184)]
                        Mortal184(::core::primitive::u8),
                        #[codec(index = 185)]
                        Mortal185(::core::primitive::u8),
                        #[codec(index = 186)]
                        Mortal186(::core::primitive::u8),
                        #[codec(index = 187)]
                        Mortal187(::core::primitive::u8),
                        #[codec(index = 188)]
                        Mortal188(::core::primitive::u8),
                        #[codec(index = 189)]
                        Mortal189(::core::primitive::u8),
                        #[codec(index = 190)]
                        Mortal190(::core::primitive::u8),
                        #[codec(index = 191)]
                        Mortal191(::core::primitive::u8),
                        #[codec(index = 192)]
                        Mortal192(::core::primitive::u8),
                        #[codec(index = 193)]
                        Mortal193(::core::primitive::u8),
                        #[codec(index = 194)]
                        Mortal194(::core::primitive::u8),
                        #[codec(index = 195)]
                        Mortal195(::core::primitive::u8),
                        #[codec(index = 196)]
                        Mortal196(::core::primitive::u8),
                        #[codec(index = 197)]
                        Mortal197(::core::primitive::u8),
                        #[codec(index = 198)]
                        Mortal198(::core::primitive::u8),
                        #[codec(index = 199)]
                        Mortal199(::core::primitive::u8),
                        #[codec(index = 200)]
                        Mortal200(::core::primitive::u8),
                        #[codec(index = 201)]
                        Mortal201(::core::primitive::u8),
                        #[codec(index = 202)]
                        Mortal202(::core::primitive::u8),
                        #[codec(index = 203)]
                        Mortal203(::core::primitive::u8),
                        #[codec(index = 204)]
                        Mortal204(::core::primitive::u8),
                        #[codec(index = 205)]
                        Mortal205(::core::primitive::u8),
                        #[codec(index = 206)]
                        Mortal206(::core::primitive::u8),
                        #[codec(index = 207)]
                        Mortal207(::core::primitive::u8),
                        #[codec(index = 208)]
                        Mortal208(::core::primitive::u8),
                        #[codec(index = 209)]
                        Mortal209(::core::primitive::u8),
                        #[codec(index = 210)]
                        Mortal210(::core::primitive::u8),
                        #[codec(index = 211)]
                        Mortal211(::core::primitive::u8),
                        #[codec(index = 212)]
                        Mortal212(::core::primitive::u8),
                        #[codec(index = 213)]
                        Mortal213(::core::primitive::u8),
                        #[codec(index = 214)]
                        Mortal214(::core::primitive::u8),
                        #[codec(index = 215)]
                        Mortal215(::core::primitive::u8),
                        #[codec(index = 216)]
                        Mortal216(::core::primitive::u8),
                        #[codec(index = 217)]
                        Mortal217(::core::primitive::u8),
                        #[codec(index = 218)]
                        Mortal218(::core::primitive::u8),
                        #[codec(index = 219)]
                        Mortal219(::core::primitive::u8),
                        #[codec(index = 220)]
                        Mortal220(::core::primitive::u8),
                        #[codec(index = 221)]
                        Mortal221(::core::primitive::u8),
                        #[codec(index = 222)]
                        Mortal222(::core::primitive::u8),
                        #[codec(index = 223)]
                        Mortal223(::core::primitive::u8),
                        #[codec(index = 224)]
                        Mortal224(::core::primitive::u8),
                        #[codec(index = 225)]
                        Mortal225(::core::primitive::u8),
                        #[codec(index = 226)]
                        Mortal226(::core::primitive::u8),
                        #[codec(index = 227)]
                        Mortal227(::core::primitive::u8),
                        #[codec(index = 228)]
                        Mortal228(::core::primitive::u8),
                        #[codec(index = 229)]
                        Mortal229(::core::primitive::u8),
                        #[codec(index = 230)]
                        Mortal230(::core::primitive::u8),
                        #[codec(index = 231)]
                        Mortal231(::core::primitive::u8),
                        #[codec(index = 232)]
                        Mortal232(::core::primitive::u8),
                        #[codec(index = 233)]
                        Mortal233(::core::primitive::u8),
                        #[codec(index = 234)]
                        Mortal234(::core::primitive::u8),
                        #[codec(index = 235)]
                        Mortal235(::core::primitive::u8),
                        #[codec(index = 236)]
                        Mortal236(::core::primitive::u8),
                        #[codec(index = 237)]
                        Mortal237(::core::primitive::u8),
                        #[codec(index = 238)]
                        Mortal238(::core::primitive::u8),
                        #[codec(index = 239)]
                        Mortal239(::core::primitive::u8),
                        #[codec(index = 240)]
                        Mortal240(::core::primitive::u8),
                        #[codec(index = 241)]
                        Mortal241(::core::primitive::u8),
                        #[codec(index = 242)]
                        Mortal242(::core::primitive::u8),
                        #[codec(index = 243)]
                        Mortal243(::core::primitive::u8),
                        #[codec(index = 244)]
                        Mortal244(::core::primitive::u8),
                        #[codec(index = 245)]
                        Mortal245(::core::primitive::u8),
                        #[codec(index = 246)]
                        Mortal246(::core::primitive::u8),
                        #[codec(index = 247)]
                        Mortal247(::core::primitive::u8),
                        #[codec(index = 248)]
                        Mortal248(::core::primitive::u8),
                        #[codec(index = 249)]
                        Mortal249(::core::primitive::u8),
                        #[codec(index = 250)]
                        Mortal250(::core::primitive::u8),
                        #[codec(index = 251)]
                        Mortal251(::core::primitive::u8),
                        #[codec(index = 252)]
                        Mortal252(::core::primitive::u8),
                        #[codec(index = 253)]
                        Mortal253(::core::primitive::u8),
                        #[codec(index = 254)]
                        Mortal254(::core::primitive::u8),
                        #[codec(index = 255)]
                        Mortal255(::core::primitive::u8),
                    }
                }
                pub mod header {
                    use super::runtime_types;
                    #[derive(
                        Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode,
                    )]
                    pub struct Header<_0> {
                        pub parent_hash: ::subxt::ext::subxt_core::utils::H256,
                        #[codec(compact)]
                        pub number: _0,
                        pub state_root: ::subxt::ext::subxt_core::utils::H256,
                        pub extrinsics_root: ::subxt::ext::subxt_core::utils::H256,
                        pub digest: runtime_types::sp_runtime::generic::digest::Digest,
                    }
                }
            }
            pub mod traits {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct BlakeTwo256;
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub enum DispatchError {
                #[codec(index = 0)]
                Other,
                #[codec(index = 1)]
                CannotLookup,
                #[codec(index = 2)]
                BadOrigin,
                #[codec(index = 3)]
                Module(runtime_types::sp_runtime::ModuleError),
                #[codec(index = 4)]
                ConsumerRemaining,
                #[codec(index = 5)]
                NoProviders,
                #[codec(index = 6)]
                TooManyConsumers,
                #[codec(index = 7)]
                Token(runtime_types::sp_runtime::TokenError),
                #[codec(index = 8)]
                Arithmetic(runtime_types::sp_arithmetic::ArithmeticError),
                #[codec(index = 9)]
                Transactional(runtime_types::sp_runtime::TransactionalError),
                #[codec(index = 10)]
                Exhausted,
                #[codec(index = 11)]
                Corruption,
                #[codec(index = 12)]
                Unavailable,
                #[codec(index = 13)]
                RootNotAllowed,
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub struct DispatchErrorWithPostInfo<_0> {
                pub post_info: _0,
                pub error: runtime_types::sp_runtime::DispatchError,
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub struct ModuleError {
                pub index: ::core::primitive::u8,
                pub error: [::core::primitive::u8; 4usize],
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub enum MultiSignature {
                #[codec(index = 0)]
                Ed25519([::core::primitive::u8; 64usize]),
                #[codec(index = 1)]
                Sr25519([::core::primitive::u8; 64usize]),
                #[codec(index = 2)]
                Ecdsa([::core::primitive::u8; 65usize]),
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub enum TokenError {
                #[codec(index = 0)]
                FundsUnavailable,
                #[codec(index = 1)]
                OnlyProvider,
                #[codec(index = 2)]
                BelowMinimum,
                #[codec(index = 3)]
                CannotCreate,
                #[codec(index = 4)]
                UnknownAsset,
                #[codec(index = 5)]
                Frozen,
                #[codec(index = 6)]
                Unsupported,
                #[codec(index = 7)]
                CannotCreateHold,
                #[codec(index = 8)]
                NotExpendable,
                #[codec(index = 9)]
                Blocked,
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub enum TransactionalError {
                #[codec(index = 0)]
                LimitReached,
                #[codec(index = 1)]
                NoLayer,
            }
        }
        pub mod sp_session {
            use super::runtime_types;
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub struct MembershipProof {
                pub session: ::core::primitive::u32,
                pub trie_nodes: ::subxt::ext::subxt_core::alloc::vec::Vec<
                    ::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
                >,
                pub validator_count: ::core::primitive::u32,
            }
        }
        pub mod sp_staking {
            use super::runtime_types;
            pub mod offence {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct OffenceDetails<_0, _1> {
                    pub offender: _1,
                    pub reporters: ::subxt::ext::subxt_core::alloc::vec::Vec<_0>,
                }
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub struct Exposure<_0, _1> {
                #[codec(compact)]
                pub total: _1,
                #[codec(compact)]
                pub own: _1,
                pub others: ::subxt::ext::subxt_core::alloc::vec::Vec<
                    runtime_types::sp_staking::IndividualExposure<_0, _1>,
                >,
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub struct ExposurePage<_0, _1> {
                #[codec(compact)]
                pub page_total: _1,
                pub others: ::subxt::ext::subxt_core::alloc::vec::Vec<
                    runtime_types::sp_staking::IndividualExposure<_0, _1>,
                >,
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub struct IndividualExposure<_0, _1> {
                pub who: _0,
                #[codec(compact)]
                pub value: _1,
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub struct PagedExposureMetadata<_0> {
                #[codec(compact)]
                pub total: _0,
                #[codec(compact)]
                pub own: _0,
                pub nominator_count: ::core::primitive::u32,
                pub page_count: ::core::primitive::u32,
            }
        }
        pub mod sp_version {
            use super::runtime_types;
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub struct RuntimeVersion {
                pub spec_name: ::subxt::ext::subxt_core::alloc::string::String,
                pub impl_name: ::subxt::ext::subxt_core::alloc::string::String,
                pub authoring_version: ::core::primitive::u32,
                pub spec_version: ::core::primitive::u32,
                pub impl_version: ::core::primitive::u32,
                pub apis: ::subxt::ext::subxt_core::alloc::vec::Vec<(
                    [::core::primitive::u8; 8usize],
                    ::core::primitive::u32,
                )>,
                pub transaction_version: ::core::primitive::u32,
                pub state_version: ::core::primitive::u8,
            }
        }
        pub mod sp_weights {
            use super::runtime_types;
            pub mod weight_v2 {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct Weight {
                    #[codec(compact)]
                    pub ref_time: ::core::primitive::u64,
                    #[codec(compact)]
                    pub proof_size: ::core::primitive::u64,
                }
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub struct RuntimeDbWeight {
                pub read: ::core::primitive::u64,
                pub write: ::core::primitive::u64,
            }
        }
        pub mod vara_runtime {
            use super::runtime_types;
            pub mod governance {
                use super::runtime_types;
                pub mod origins {
                    use super::runtime_types;
                    pub mod pallet_custom_origins {
                        use super::runtime_types;
                        #[derive(
                            Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode,
                        )]
                        pub enum Origin {
                            #[codec(index = 0)]
                            StakingAdmin,
                            #[codec(index = 1)]
                            Treasurer,
                            #[codec(index = 2)]
                            FellowshipAdmin,
                            #[codec(index = 3)]
                            GeneralAdmin,
                            #[codec(index = 4)]
                            ReferendumCanceller,
                            #[codec(index = 5)]
                            ReferendumKiller,
                            #[codec(index = 6)]
                            SmallTipper,
                            #[codec(index = 7)]
                            BigTipper,
                            #[codec(index = 8)]
                            SmallSpender,
                            #[codec(index = 9)]
                            MediumSpender,
                            #[codec(index = 10)]
                            BigSpender,
                            #[codec(index = 11)]
                            WhitelistedCaller,
                            #[codec(index = 12)]
                            FellowshipInitiates,
                            #[codec(index = 13)]
                            Fellows,
                            #[codec(index = 14)]
                            FellowshipExperts,
                            #[codec(index = 15)]
                            FellowshipMasters,
                            #[codec(index = 16)]
                            Fellowship1Dan,
                            #[codec(index = 17)]
                            Fellowship2Dan,
                            #[codec(index = 18)]
                            Fellowship3Dan,
                            #[codec(index = 19)]
                            Fellowship4Dan,
                            #[codec(index = 20)]
                            Fellowship5Dan,
                            #[codec(index = 21)]
                            Fellowship6Dan,
                            #[codec(index = 22)]
                            Fellowship7Dan,
                            #[codec(index = 23)]
                            Fellowship8Dan,
                            #[codec(index = 24)]
                            Fellowship9Dan,
                        }
                    }
                }
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub struct CustomCheckNonce(#[codec(compact)] pub ::core::primitive::u32);
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub struct NposSolution16 {
                pub votes1: ::subxt::ext::subxt_core::alloc::vec::Vec<(
                    ::subxt::ext::subxt_core::ext::codec::Compact<::core::primitive::u32>,
                    ::subxt::ext::subxt_core::ext::codec::Compact<::core::primitive::u16>,
                )>,
                pub votes2: ::subxt::ext::subxt_core::alloc::vec::Vec<(
                    ::subxt::ext::subxt_core::ext::codec::Compact<::core::primitive::u32>,
                    (
                        ::subxt::ext::subxt_core::ext::codec::Compact<::core::primitive::u16>,
                        ::subxt::ext::subxt_core::ext::codec::Compact<
                            runtime_types::sp_arithmetic::per_things::PerU16,
                        >,
                    ),
                    ::subxt::ext::subxt_core::ext::codec::Compact<::core::primitive::u16>,
                )>,
                pub votes3: ::subxt::ext::subxt_core::alloc::vec::Vec<(
                    ::subxt::ext::subxt_core::ext::codec::Compact<::core::primitive::u32>,
                    [(
                        ::subxt::ext::subxt_core::ext::codec::Compact<::core::primitive::u16>,
                        ::subxt::ext::subxt_core::ext::codec::Compact<
                            runtime_types::sp_arithmetic::per_things::PerU16,
                        >,
                    ); 2usize],
                    ::subxt::ext::subxt_core::ext::codec::Compact<::core::primitive::u16>,
                )>,
                pub votes4: ::subxt::ext::subxt_core::alloc::vec::Vec<(
                    ::subxt::ext::subxt_core::ext::codec::Compact<::core::primitive::u32>,
                    [(
                        ::subxt::ext::subxt_core::ext::codec::Compact<::core::primitive::u16>,
                        ::subxt::ext::subxt_core::ext::codec::Compact<
                            runtime_types::sp_arithmetic::per_things::PerU16,
                        >,
                    ); 3usize],
                    ::subxt::ext::subxt_core::ext::codec::Compact<::core::primitive::u16>,
                )>,
                pub votes5: ::subxt::ext::subxt_core::alloc::vec::Vec<(
                    ::subxt::ext::subxt_core::ext::codec::Compact<::core::primitive::u32>,
                    [(
                        ::subxt::ext::subxt_core::ext::codec::Compact<::core::primitive::u16>,
                        ::subxt::ext::subxt_core::ext::codec::Compact<
                            runtime_types::sp_arithmetic::per_things::PerU16,
                        >,
                    ); 4usize],
                    ::subxt::ext::subxt_core::ext::codec::Compact<::core::primitive::u16>,
                )>,
                pub votes6: ::subxt::ext::subxt_core::alloc::vec::Vec<(
                    ::subxt::ext::subxt_core::ext::codec::Compact<::core::primitive::u32>,
                    [(
                        ::subxt::ext::subxt_core::ext::codec::Compact<::core::primitive::u16>,
                        ::subxt::ext::subxt_core::ext::codec::Compact<
                            runtime_types::sp_arithmetic::per_things::PerU16,
                        >,
                    ); 5usize],
                    ::subxt::ext::subxt_core::ext::codec::Compact<::core::primitive::u16>,
                )>,
                pub votes7: ::subxt::ext::subxt_core::alloc::vec::Vec<(
                    ::subxt::ext::subxt_core::ext::codec::Compact<::core::primitive::u32>,
                    [(
                        ::subxt::ext::subxt_core::ext::codec::Compact<::core::primitive::u16>,
                        ::subxt::ext::subxt_core::ext::codec::Compact<
                            runtime_types::sp_arithmetic::per_things::PerU16,
                        >,
                    ); 6usize],
                    ::subxt::ext::subxt_core::ext::codec::Compact<::core::primitive::u16>,
                )>,
                pub votes8: ::subxt::ext::subxt_core::alloc::vec::Vec<(
                    ::subxt::ext::subxt_core::ext::codec::Compact<::core::primitive::u32>,
                    [(
                        ::subxt::ext::subxt_core::ext::codec::Compact<::core::primitive::u16>,
                        ::subxt::ext::subxt_core::ext::codec::Compact<
                            runtime_types::sp_arithmetic::per_things::PerU16,
                        >,
                    ); 7usize],
                    ::subxt::ext::subxt_core::ext::codec::Compact<::core::primitive::u16>,
                )>,
                pub votes9: ::subxt::ext::subxt_core::alloc::vec::Vec<(
                    ::subxt::ext::subxt_core::ext::codec::Compact<::core::primitive::u32>,
                    [(
                        ::subxt::ext::subxt_core::ext::codec::Compact<::core::primitive::u16>,
                        ::subxt::ext::subxt_core::ext::codec::Compact<
                            runtime_types::sp_arithmetic::per_things::PerU16,
                        >,
                    ); 8usize],
                    ::subxt::ext::subxt_core::ext::codec::Compact<::core::primitive::u16>,
                )>,
                pub votes10: ::subxt::ext::subxt_core::alloc::vec::Vec<(
                    ::subxt::ext::subxt_core::ext::codec::Compact<::core::primitive::u32>,
                    [(
                        ::subxt::ext::subxt_core::ext::codec::Compact<::core::primitive::u16>,
                        ::subxt::ext::subxt_core::ext::codec::Compact<
                            runtime_types::sp_arithmetic::per_things::PerU16,
                        >,
                    ); 9usize],
                    ::subxt::ext::subxt_core::ext::codec::Compact<::core::primitive::u16>,
                )>,
                pub votes11: ::subxt::ext::subxt_core::alloc::vec::Vec<(
                    ::subxt::ext::subxt_core::ext::codec::Compact<::core::primitive::u32>,
                    [(
                        ::subxt::ext::subxt_core::ext::codec::Compact<::core::primitive::u16>,
                        ::subxt::ext::subxt_core::ext::codec::Compact<
                            runtime_types::sp_arithmetic::per_things::PerU16,
                        >,
                    ); 10usize],
                    ::subxt::ext::subxt_core::ext::codec::Compact<::core::primitive::u16>,
                )>,
                pub votes12: ::subxt::ext::subxt_core::alloc::vec::Vec<(
                    ::subxt::ext::subxt_core::ext::codec::Compact<::core::primitive::u32>,
                    [(
                        ::subxt::ext::subxt_core::ext::codec::Compact<::core::primitive::u16>,
                        ::subxt::ext::subxt_core::ext::codec::Compact<
                            runtime_types::sp_arithmetic::per_things::PerU16,
                        >,
                    ); 11usize],
                    ::subxt::ext::subxt_core::ext::codec::Compact<::core::primitive::u16>,
                )>,
                pub votes13: ::subxt::ext::subxt_core::alloc::vec::Vec<(
                    ::subxt::ext::subxt_core::ext::codec::Compact<::core::primitive::u32>,
                    [(
                        ::subxt::ext::subxt_core::ext::codec::Compact<::core::primitive::u16>,
                        ::subxt::ext::subxt_core::ext::codec::Compact<
                            runtime_types::sp_arithmetic::per_things::PerU16,
                        >,
                    ); 12usize],
                    ::subxt::ext::subxt_core::ext::codec::Compact<::core::primitive::u16>,
                )>,
                pub votes14: ::subxt::ext::subxt_core::alloc::vec::Vec<(
                    ::subxt::ext::subxt_core::ext::codec::Compact<::core::primitive::u32>,
                    [(
                        ::subxt::ext::subxt_core::ext::codec::Compact<::core::primitive::u16>,
                        ::subxt::ext::subxt_core::ext::codec::Compact<
                            runtime_types::sp_arithmetic::per_things::PerU16,
                        >,
                    ); 13usize],
                    ::subxt::ext::subxt_core::ext::codec::Compact<::core::primitive::u16>,
                )>,
                pub votes15: ::subxt::ext::subxt_core::alloc::vec::Vec<(
                    ::subxt::ext::subxt_core::ext::codec::Compact<::core::primitive::u32>,
                    [(
                        ::subxt::ext::subxt_core::ext::codec::Compact<::core::primitive::u16>,
                        ::subxt::ext::subxt_core::ext::codec::Compact<
                            runtime_types::sp_arithmetic::per_things::PerU16,
                        >,
                    ); 14usize],
                    ::subxt::ext::subxt_core::ext::codec::Compact<::core::primitive::u16>,
                )>,
                pub votes16: ::subxt::ext::subxt_core::alloc::vec::Vec<(
                    ::subxt::ext::subxt_core::ext::codec::Compact<::core::primitive::u32>,
                    [(
                        ::subxt::ext::subxt_core::ext::codec::Compact<::core::primitive::u16>,
                        ::subxt::ext::subxt_core::ext::codec::Compact<
                            runtime_types::sp_arithmetic::per_things::PerU16,
                        >,
                    ); 15usize],
                    ::subxt::ext::subxt_core::ext::codec::Compact<::core::primitive::u16>,
                )>,
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub enum OriginCaller {
                #[codec(index = 0)]
                system(
                    runtime_types::frame_support::dispatch::RawOrigin<
                        ::subxt::ext::subxt_core::utils::AccountId32,
                    >,
                ),
                #[codec(index = 20)]
                Origins(
                    runtime_types::vara_runtime::governance::origins::pallet_custom_origins::Origin,
                ),
                #[codec(index = 2)]
                Void(runtime_types::sp_core::Void),
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub enum ProxyType {
                #[codec(index = 0)]
                Any,
                #[codec(index = 1)]
                NonTransfer,
                #[codec(index = 2)]
                Governance,
                #[codec(index = 3)]
                Staking,
                #[codec(index = 4)]
                IdentityJudgement,
                #[codec(index = 5)]
                CancelProxy,
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub struct Runtime;
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub enum RuntimeCall {
                #[codec(index = 0)]
                System(runtime_types::frame_system::pallet::Call),
                #[codec(index = 1)]
                Timestamp(runtime_types::pallet_timestamp::pallet::Call),
                #[codec(index = 3)]
                Babe(runtime_types::pallet_babe::pallet::Call),
                #[codec(index = 4)]
                Grandpa(runtime_types::pallet_grandpa::pallet::Call),
                #[codec(index = 5)]
                Balances(runtime_types::pallet_balances::pallet::Call),
                #[codec(index = 7)]
                Session(runtime_types::pallet_session::pallet::Call),
                #[codec(index = 8)]
                Utility(runtime_types::pallet_utility::pallet::Call),
                #[codec(index = 10)]
                Vesting(runtime_types::pallet_vesting::pallet::Call),
                #[codec(index = 11)]
                BagsList(runtime_types::pallet_bags_list::pallet::Call),
                #[codec(index = 12)]
                ImOnline(runtime_types::pallet_im_online::pallet::Call),
                #[codec(index = 13)]
                Staking(runtime_types::pallet_staking::pallet::pallet::Call),
                #[codec(index = 14)]
                Treasury(runtime_types::pallet_treasury::pallet::Call),
                #[codec(index = 16)]
                ConvictionVoting(runtime_types::pallet_conviction_voting::pallet::Call),
                #[codec(index = 17)]
                Referenda(runtime_types::pallet_referenda::pallet::Call),
                #[codec(index = 18)]
                FellowshipCollective(runtime_types::pallet_ranked_collective::pallet::Call),
                #[codec(index = 19)]
                FellowshipReferenda(runtime_types::pallet_referenda::pallet::Call),
                #[codec(index = 21)]
                Whitelist(runtime_types::pallet_whitelist::pallet::Call),
                #[codec(index = 22)]
                Scheduler(runtime_types::pallet_scheduler::pallet::Call),
                #[codec(index = 23)]
                Preimage(runtime_types::pallet_preimage::pallet::Call),
                #[codec(index = 24)]
                Identity(runtime_types::pallet_identity::pallet::Call),
                #[codec(index = 25)]
                Proxy(runtime_types::pallet_proxy::pallet::Call),
                #[codec(index = 26)]
                Multisig(runtime_types::pallet_multisig::pallet::Call),
                #[codec(index = 27)]
                ElectionProviderMultiPhase(
                    runtime_types::pallet_election_provider_multi_phase::pallet::Call,
                ),
                #[codec(index = 29)]
                Bounties(runtime_types::pallet_bounties::pallet::Call),
                #[codec(index = 30)]
                ChildBounties(runtime_types::pallet_child_bounties::pallet::Call),
                #[codec(index = 31)]
                NominationPools(runtime_types::pallet_nomination_pools::pallet::Call),
                #[codec(index = 99)]
                Sudo(runtime_types::pallet_sudo::pallet::Call),
                #[codec(index = 104)]
                Gear(runtime_types::pallet_gear::pallet::Call),
                #[codec(index = 106)]
                StakingRewards(runtime_types::pallet_gear_staking_rewards::pallet::Call),
                #[codec(index = 107)]
                GearVoucher(runtime_types::pallet_gear_voucher::pallet::Call),
                #[codec(index = 110)]
                GearEthBridge(runtime_types::pallet_gear_eth_bridge::pallet::Call),
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub enum RuntimeError {
                #[codec(index = 0)]
                System(runtime_types::frame_system::pallet::Error),
                #[codec(index = 3)]
                Babe(runtime_types::pallet_babe::pallet::Error),
                #[codec(index = 4)]
                Grandpa(runtime_types::pallet_grandpa::pallet::Error),
                #[codec(index = 5)]
                Balances(runtime_types::pallet_balances::pallet::Error),
                #[codec(index = 7)]
                Session(runtime_types::pallet_session::pallet::Error),
                #[codec(index = 8)]
                Utility(runtime_types::pallet_utility::pallet::Error),
                #[codec(index = 10)]
                Vesting(runtime_types::pallet_vesting::pallet::Error),
                #[codec(index = 11)]
                BagsList(runtime_types::pallet_bags_list::pallet::Error),
                #[codec(index = 12)]
                ImOnline(runtime_types::pallet_im_online::pallet::Error),
                #[codec(index = 13)]
                Staking(runtime_types::pallet_staking::pallet::pallet::Error),
                #[codec(index = 14)]
                Treasury(runtime_types::pallet_treasury::pallet::Error),
                #[codec(index = 16)]
                ConvictionVoting(runtime_types::pallet_conviction_voting::pallet::Error),
                #[codec(index = 17)]
                Referenda(runtime_types::pallet_referenda::pallet::Error),
                #[codec(index = 18)]
                FellowshipCollective(runtime_types::pallet_ranked_collective::pallet::Error),
                #[codec(index = 19)]
                FellowshipReferenda(runtime_types::pallet_referenda::pallet::Error),
                #[codec(index = 21)]
                Whitelist(runtime_types::pallet_whitelist::pallet::Error),
                #[codec(index = 22)]
                Scheduler(runtime_types::pallet_scheduler::pallet::Error),
                #[codec(index = 23)]
                Preimage(runtime_types::pallet_preimage::pallet::Error),
                #[codec(index = 24)]
                Identity(runtime_types::pallet_identity::pallet::Error),
                #[codec(index = 25)]
                Proxy(runtime_types::pallet_proxy::pallet::Error),
                #[codec(index = 26)]
                Multisig(runtime_types::pallet_multisig::pallet::Error),
                #[codec(index = 27)]
                ElectionProviderMultiPhase(
                    runtime_types::pallet_election_provider_multi_phase::pallet::Error,
                ),
                #[codec(index = 29)]
                Bounties(runtime_types::pallet_bounties::pallet::Error),
                #[codec(index = 30)]
                ChildBounties(runtime_types::pallet_child_bounties::pallet::Error),
                #[codec(index = 31)]
                NominationPools(runtime_types::pallet_nomination_pools::pallet::Error),
                #[codec(index = 99)]
                Sudo(runtime_types::pallet_sudo::pallet::Error),
                #[codec(index = 100)]
                GearProgram(runtime_types::pallet_gear_program::pallet::Error),
                #[codec(index = 101)]
                GearMessenger(runtime_types::pallet_gear_messenger::pallet::Error),
                #[codec(index = 102)]
                GearScheduler(runtime_types::pallet_gear_scheduler::pallet::Error),
                #[codec(index = 103)]
                GearGas(runtime_types::pallet_gear_gas::pallet::Error),
                #[codec(index = 104)]
                Gear(runtime_types::pallet_gear::pallet::Error),
                #[codec(index = 106)]
                StakingRewards(runtime_types::pallet_gear_staking_rewards::pallet::Error),
                #[codec(index = 107)]
                GearVoucher(runtime_types::pallet_gear_voucher::pallet::Error),
                #[codec(index = 108)]
                GearBank(runtime_types::pallet_gear_bank::pallet::Error),
                #[codec(index = 110)]
                GearEthBridge(runtime_types::pallet_gear_eth_bridge::pallet::Error),
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub enum RuntimeEvent {
                #[codec(index = 0)]
                System(runtime_types::frame_system::pallet::Event),
                #[codec(index = 4)]
                Grandpa(runtime_types::pallet_grandpa::pallet::Event),
                #[codec(index = 5)]
                Balances(runtime_types::pallet_balances::pallet::Event),
                #[codec(index = 6)]
                TransactionPayment(runtime_types::pallet_transaction_payment::pallet::Event),
                #[codec(index = 7)]
                Session(runtime_types::pallet_session::pallet::Event),
                #[codec(index = 8)]
                Utility(runtime_types::pallet_utility::pallet::Event),
                #[codec(index = 10)]
                Vesting(runtime_types::pallet_vesting::pallet::Event),
                #[codec(index = 11)]
                BagsList(runtime_types::pallet_bags_list::pallet::Event),
                #[codec(index = 12)]
                ImOnline(runtime_types::pallet_im_online::pallet::Event),
                #[codec(index = 13)]
                Staking(runtime_types::pallet_staking::pallet::pallet::Event),
                #[codec(index = 14)]
                Treasury(runtime_types::pallet_treasury::pallet::Event),
                #[codec(index = 16)]
                ConvictionVoting(runtime_types::pallet_conviction_voting::pallet::Event),
                #[codec(index = 17)]
                Referenda(runtime_types::pallet_referenda::pallet::Event1),
                #[codec(index = 18)]
                FellowshipCollective(runtime_types::pallet_ranked_collective::pallet::Event),
                #[codec(index = 19)]
                FellowshipReferenda(runtime_types::pallet_referenda::pallet::Event2),
                #[codec(index = 21)]
                Whitelist(runtime_types::pallet_whitelist::pallet::Event),
                #[codec(index = 22)]
                Scheduler(runtime_types::pallet_scheduler::pallet::Event),
                #[codec(index = 23)]
                Preimage(runtime_types::pallet_preimage::pallet::Event),
                #[codec(index = 24)]
                Identity(runtime_types::pallet_identity::pallet::Event),
                #[codec(index = 25)]
                Proxy(runtime_types::pallet_proxy::pallet::Event),
                #[codec(index = 26)]
                Multisig(runtime_types::pallet_multisig::pallet::Event),
                #[codec(index = 27)]
                ElectionProviderMultiPhase(
                    runtime_types::pallet_election_provider_multi_phase::pallet::Event,
                ),
                #[codec(index = 28)]
                Offences(runtime_types::pallet_offences::pallet::Event),
                #[codec(index = 29)]
                Bounties(runtime_types::pallet_bounties::pallet::Event),
                #[codec(index = 30)]
                ChildBounties(runtime_types::pallet_child_bounties::pallet::Event),
                #[codec(index = 31)]
                NominationPools(runtime_types::pallet_nomination_pools::pallet::Event),
                #[codec(index = 99)]
                Sudo(runtime_types::pallet_sudo::pallet::Event),
                #[codec(index = 104)]
                Gear(runtime_types::pallet_gear::pallet::Event),
                #[codec(index = 106)]
                StakingRewards(runtime_types::pallet_gear_staking_rewards::pallet::Event),
                #[codec(index = 107)]
                GearVoucher(runtime_types::pallet_gear_voucher::pallet::Event),
                #[codec(index = 110)]
                GearEthBridge(runtime_types::pallet_gear_eth_bridge::pallet::Event),
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub enum RuntimeFreezeReason {
                #[codec(index = 31)]
                NominationPools(runtime_types::pallet_nomination_pools::pallet::FreezeReason),
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub enum RuntimeHoldReason {
                #[codec(index = 23)]
                Preimage(runtime_types::pallet_preimage::pallet::HoldReason),
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub struct SessionKeys {
                pub babe: runtime_types::sp_consensus_babe::app::Public,
                pub grandpa: runtime_types::sp_consensus_grandpa::app::Public,
                pub im_online: runtime_types::pallet_im_online::sr25519::app_sr25519::Public,
                pub authority_discovery: runtime_types::sp_authority_discovery::app::Public,
            }
        }
    }
}
pub mod calls {
    #[doc = r" Show the call info."]
    pub trait CallInfo {
        const PALLET: &'static str;
        #[doc = r" returns call name."]
        fn call_name(&self) -> &'static str;
    }
    #[doc = "Calls of pallet `Babe`."]
    pub enum BabeCall {
        ReportEquivocation,
        ReportEquivocationUnsigned,
        PlanConfigChange,
    }
    impl CallInfo for BabeCall {
        const PALLET: &'static str = "Babe";
        fn call_name(&self) -> &'static str {
            match self {
                Self::ReportEquivocation => "report_equivocation",
                Self::ReportEquivocationUnsigned => "report_equivocation_unsigned",
                Self::PlanConfigChange => "plan_config_change",
            }
        }
    }
    #[doc = "Calls of pallet `BagsList`."]
    pub enum BagsListCall {
        Rebag,
        PutInFrontOf,
        PutInFrontOfOther,
    }
    impl CallInfo for BagsListCall {
        const PALLET: &'static str = "BagsList";
        fn call_name(&self) -> &'static str {
            match self {
                Self::Rebag => "rebag",
                Self::PutInFrontOf => "put_in_front_of",
                Self::PutInFrontOfOther => "put_in_front_of_other",
            }
        }
    }
    #[doc = "Calls of pallet `Balances`."]
    pub enum BalancesCall {
        TransferAllowDeath,
        ForceTransfer,
        TransferKeepAlive,
        TransferAll,
        ForceUnreserve,
        UpgradeAccounts,
        ForceSetBalance,
        ForceAdjustTotalIssuance,
        Burn,
    }
    impl CallInfo for BalancesCall {
        const PALLET: &'static str = "Balances";
        fn call_name(&self) -> &'static str {
            match self {
                Self::TransferAllowDeath => "transfer_allow_death",
                Self::ForceTransfer => "force_transfer",
                Self::TransferKeepAlive => "transfer_keep_alive",
                Self::TransferAll => "transfer_all",
                Self::ForceUnreserve => "force_unreserve",
                Self::UpgradeAccounts => "upgrade_accounts",
                Self::ForceSetBalance => "force_set_balance",
                Self::ForceAdjustTotalIssuance => "force_adjust_total_issuance",
                Self::Burn => "burn",
            }
        }
    }
    #[doc = "Calls of pallet `Bounties`."]
    pub enum BountiesCall {
        ProposeBounty,
        ApproveBounty,
        ProposeCurator,
        UnassignCurator,
        AcceptCurator,
        AwardBounty,
        ClaimBounty,
        CloseBounty,
        ExtendBountyExpiry,
    }
    impl CallInfo for BountiesCall {
        const PALLET: &'static str = "Bounties";
        fn call_name(&self) -> &'static str {
            match self {
                Self::ProposeBounty => "propose_bounty",
                Self::ApproveBounty => "approve_bounty",
                Self::ProposeCurator => "propose_curator",
                Self::UnassignCurator => "unassign_curator",
                Self::AcceptCurator => "accept_curator",
                Self::AwardBounty => "award_bounty",
                Self::ClaimBounty => "claim_bounty",
                Self::CloseBounty => "close_bounty",
                Self::ExtendBountyExpiry => "extend_bounty_expiry",
            }
        }
    }
    #[doc = "Calls of pallet `ChildBounties`."]
    pub enum ChildBountiesCall {
        AddChildBounty,
        ProposeCurator,
        AcceptCurator,
        UnassignCurator,
        AwardChildBounty,
        ClaimChildBounty,
        CloseChildBounty,
    }
    impl CallInfo for ChildBountiesCall {
        const PALLET: &'static str = "ChildBounties";
        fn call_name(&self) -> &'static str {
            match self {
                Self::AddChildBounty => "add_child_bounty",
                Self::ProposeCurator => "propose_curator",
                Self::AcceptCurator => "accept_curator",
                Self::UnassignCurator => "unassign_curator",
                Self::AwardChildBounty => "award_child_bounty",
                Self::ClaimChildBounty => "claim_child_bounty",
                Self::CloseChildBounty => "close_child_bounty",
            }
        }
    }
    #[doc = "Calls of pallet `ConvictionVoting`."]
    pub enum ConvictionVotingCall {
        Vote,
        Delegate,
        Undelegate,
        Unlock,
        RemoveVote,
        RemoveOtherVote,
    }
    impl CallInfo for ConvictionVotingCall {
        const PALLET: &'static str = "ConvictionVoting";
        fn call_name(&self) -> &'static str {
            match self {
                Self::Vote => "vote",
                Self::Delegate => "delegate",
                Self::Undelegate => "undelegate",
                Self::Unlock => "unlock",
                Self::RemoveVote => "remove_vote",
                Self::RemoveOtherVote => "remove_other_vote",
            }
        }
    }
    #[doc = "Calls of pallet `ElectionProviderMultiPhase`."]
    pub enum ElectionProviderMultiPhaseCall {
        SubmitUnsigned,
        SetMinimumUntrustedScore,
        SetEmergencyElectionResult,
        Submit,
        GovernanceFallback,
    }
    impl CallInfo for ElectionProviderMultiPhaseCall {
        const PALLET: &'static str = "ElectionProviderMultiPhase";
        fn call_name(&self) -> &'static str {
            match self {
                Self::SubmitUnsigned => "submit_unsigned",
                Self::SetMinimumUntrustedScore => "set_minimum_untrusted_score",
                Self::SetEmergencyElectionResult => "set_emergency_election_result",
                Self::Submit => "submit",
                Self::GovernanceFallback => "governance_fallback",
            }
        }
    }
    #[doc = "Calls of pallet `FellowshipCollective`."]
    pub enum FellowshipCollectiveCall {
        AddMember,
        PromoteMember,
        DemoteMember,
        RemoveMember,
        Vote,
        CleanupPoll,
        ExchangeMember,
    }
    impl CallInfo for FellowshipCollectiveCall {
        const PALLET: &'static str = "FellowshipCollective";
        fn call_name(&self) -> &'static str {
            match self {
                Self::AddMember => "add_member",
                Self::PromoteMember => "promote_member",
                Self::DemoteMember => "demote_member",
                Self::RemoveMember => "remove_member",
                Self::Vote => "vote",
                Self::CleanupPoll => "cleanup_poll",
                Self::ExchangeMember => "exchange_member",
            }
        }
    }
    #[doc = "Calls of pallet `FellowshipReferenda`."]
    pub enum FellowshipReferendaCall {
        Submit,
        PlaceDecisionDeposit,
        RefundDecisionDeposit,
        Cancel,
        Kill,
        NudgeReferendum,
        OneFewerDeciding,
        RefundSubmissionDeposit,
        SetMetadata,
    }
    impl CallInfo for FellowshipReferendaCall {
        const PALLET: &'static str = "FellowshipReferenda";
        fn call_name(&self) -> &'static str {
            match self {
                Self::Submit => "submit",
                Self::PlaceDecisionDeposit => "place_decision_deposit",
                Self::RefundDecisionDeposit => "refund_decision_deposit",
                Self::Cancel => "cancel",
                Self::Kill => "kill",
                Self::NudgeReferendum => "nudge_referendum",
                Self::OneFewerDeciding => "one_fewer_deciding",
                Self::RefundSubmissionDeposit => "refund_submission_deposit",
                Self::SetMetadata => "set_metadata",
            }
        }
    }
    #[doc = "Calls of pallet `Gear`."]
    pub enum GearCall {
        UploadCode,
        UploadProgram,
        CreateProgram,
        SendMessage,
        SendReply,
        ClaimValue,
        Run,
        SetExecuteInherent,
        ClaimValueToInheritor,
        ExhaustBlockResources,
    }
    impl CallInfo for GearCall {
        const PALLET: &'static str = "Gear";
        fn call_name(&self) -> &'static str {
            match self {
                Self::UploadCode => "upload_code",
                Self::UploadProgram => "upload_program",
                Self::CreateProgram => "create_program",
                Self::SendMessage => "send_message",
                Self::SendReply => "send_reply",
                Self::ClaimValue => "claim_value",
                Self::Run => "run",
                Self::SetExecuteInherent => "set_execute_inherent",
                Self::ClaimValueToInheritor => "claim_value_to_inheritor",
                Self::ExhaustBlockResources => "exhaust_block_resources",
            }
        }
    }
    #[doc = "Calls of pallet `GearEthBridge`."]
    pub enum GearEthBridgeCall {
        Pause,
        Unpause,
        SendEthMessage,
        SetFee,
    }
    impl CallInfo for GearEthBridgeCall {
        const PALLET: &'static str = "GearEthBridge";
        fn call_name(&self) -> &'static str {
            match self {
                Self::Pause => "pause",
                Self::Unpause => "unpause",
                Self::SendEthMessage => "send_eth_message",
                Self::SetFee => "set_fee",
            }
        }
    }
    #[doc = "Calls of pallet `GearVoucher`."]
    pub enum GearVoucherCall {
        Issue,
        Call,
        Revoke,
        Update,
        Decline,
    }
    impl CallInfo for GearVoucherCall {
        const PALLET: &'static str = "GearVoucher";
        fn call_name(&self) -> &'static str {
            match self {
                Self::Issue => "issue",
                Self::Call => "call",
                Self::Revoke => "revoke",
                Self::Update => "update",
                Self::Decline => "decline",
            }
        }
    }
    #[doc = "Calls of pallet `Grandpa`."]
    pub enum GrandpaCall {
        ReportEquivocation,
        ReportEquivocationUnsigned,
        NoteStalled,
    }
    impl CallInfo for GrandpaCall {
        const PALLET: &'static str = "Grandpa";
        fn call_name(&self) -> &'static str {
            match self {
                Self::ReportEquivocation => "report_equivocation",
                Self::ReportEquivocationUnsigned => "report_equivocation_unsigned",
                Self::NoteStalled => "note_stalled",
            }
        }
    }
    #[doc = "Calls of pallet `Identity`."]
    pub enum IdentityCall {
        AddRegistrar,
        SetIdentity,
        SetSubs,
        ClearIdentity,
        RequestJudgement,
        CancelRequest,
        SetFee,
        SetAccountId,
        SetFields,
        ProvideJudgement,
        KillIdentity,
        AddSub,
        RenameSub,
        RemoveSub,
        QuitSub,
        AddUsernameAuthority,
        RemoveUsernameAuthority,
        SetUsernameFor,
        AcceptUsername,
        RemoveExpiredApproval,
        SetPrimaryUsername,
        RemoveDanglingUsername,
    }
    impl CallInfo for IdentityCall {
        const PALLET: &'static str = "Identity";
        fn call_name(&self) -> &'static str {
            match self {
                Self::AddRegistrar => "add_registrar",
                Self::SetIdentity => "set_identity",
                Self::SetSubs => "set_subs",
                Self::ClearIdentity => "clear_identity",
                Self::RequestJudgement => "request_judgement",
                Self::CancelRequest => "cancel_request",
                Self::SetFee => "set_fee",
                Self::SetAccountId => "set_account_id",
                Self::SetFields => "set_fields",
                Self::ProvideJudgement => "provide_judgement",
                Self::KillIdentity => "kill_identity",
                Self::AddSub => "add_sub",
                Self::RenameSub => "rename_sub",
                Self::RemoveSub => "remove_sub",
                Self::QuitSub => "quit_sub",
                Self::AddUsernameAuthority => "add_username_authority",
                Self::RemoveUsernameAuthority => "remove_username_authority",
                Self::SetUsernameFor => "set_username_for",
                Self::AcceptUsername => "accept_username",
                Self::RemoveExpiredApproval => "remove_expired_approval",
                Self::SetPrimaryUsername => "set_primary_username",
                Self::RemoveDanglingUsername => "remove_dangling_username",
            }
        }
    }
    #[doc = "Calls of pallet `ImOnline`."]
    pub enum ImOnlineCall {
        Heartbeat,
    }
    impl CallInfo for ImOnlineCall {
        const PALLET: &'static str = "ImOnline";
        fn call_name(&self) -> &'static str {
            match self {
                Self::Heartbeat => "heartbeat",
            }
        }
    }
    #[doc = "Calls of pallet `Multisig`."]
    pub enum MultisigCall {
        AsMultiThreshold1,
        AsMulti,
        ApproveAsMulti,
        CancelAsMulti,
    }
    impl CallInfo for MultisigCall {
        const PALLET: &'static str = "Multisig";
        fn call_name(&self) -> &'static str {
            match self {
                Self::AsMultiThreshold1 => "as_multi_threshold_1",
                Self::AsMulti => "as_multi",
                Self::ApproveAsMulti => "approve_as_multi",
                Self::CancelAsMulti => "cancel_as_multi",
            }
        }
    }
    #[doc = "Calls of pallet `NominationPools`."]
    pub enum NominationPoolsCall {
        Join,
        BondExtra,
        ClaimPayout,
        Unbond,
        PoolWithdrawUnbonded,
        WithdrawUnbonded,
        Create,
        CreateWithPoolId,
        Nominate,
        SetState,
        SetMetadata,
        SetConfigs,
        UpdateRoles,
        Chill,
        BondExtraOther,
        SetClaimPermission,
        ClaimPayoutOther,
        SetCommission,
        SetCommissionMax,
        SetCommissionChangeRate,
        ClaimCommission,
        AdjustPoolDeposit,
        SetCommissionClaimPermission,
        ApplySlash,
        MigrateDelegation,
        MigratePoolToDelegateStake,
    }
    impl CallInfo for NominationPoolsCall {
        const PALLET: &'static str = "NominationPools";
        fn call_name(&self) -> &'static str {
            match self {
                Self::Join => "join",
                Self::BondExtra => "bond_extra",
                Self::ClaimPayout => "claim_payout",
                Self::Unbond => "unbond",
                Self::PoolWithdrawUnbonded => "pool_withdraw_unbonded",
                Self::WithdrawUnbonded => "withdraw_unbonded",
                Self::Create => "create",
                Self::CreateWithPoolId => "create_with_pool_id",
                Self::Nominate => "nominate",
                Self::SetState => "set_state",
                Self::SetMetadata => "set_metadata",
                Self::SetConfigs => "set_configs",
                Self::UpdateRoles => "update_roles",
                Self::Chill => "chill",
                Self::BondExtraOther => "bond_extra_other",
                Self::SetClaimPermission => "set_claim_permission",
                Self::ClaimPayoutOther => "claim_payout_other",
                Self::SetCommission => "set_commission",
                Self::SetCommissionMax => "set_commission_max",
                Self::SetCommissionChangeRate => "set_commission_change_rate",
                Self::ClaimCommission => "claim_commission",
                Self::AdjustPoolDeposit => "adjust_pool_deposit",
                Self::SetCommissionClaimPermission => "set_commission_claim_permission",
                Self::ApplySlash => "apply_slash",
                Self::MigrateDelegation => "migrate_delegation",
                Self::MigratePoolToDelegateStake => "migrate_pool_to_delegate_stake",
            }
        }
    }
    #[doc = "Calls of pallet `Preimage`."]
    pub enum PreimageCall {
        NotePreimage,
        UnnotePreimage,
        RequestPreimage,
        UnrequestPreimage,
        EnsureUpdated,
    }
    impl CallInfo for PreimageCall {
        const PALLET: &'static str = "Preimage";
        fn call_name(&self) -> &'static str {
            match self {
                Self::NotePreimage => "note_preimage",
                Self::UnnotePreimage => "unnote_preimage",
                Self::RequestPreimage => "request_preimage",
                Self::UnrequestPreimage => "unrequest_preimage",
                Self::EnsureUpdated => "ensure_updated",
            }
        }
    }
    #[doc = "Calls of pallet `Proxy`."]
    pub enum ProxyCall {
        Proxy,
        AddProxy,
        RemoveProxy,
        RemoveProxies,
        CreatePure,
        KillPure,
        Announce,
        RemoveAnnouncement,
        RejectAnnouncement,
        ProxyAnnounced,
    }
    impl CallInfo for ProxyCall {
        const PALLET: &'static str = "Proxy";
        fn call_name(&self) -> &'static str {
            match self {
                Self::Proxy => "proxy",
                Self::AddProxy => "add_proxy",
                Self::RemoveProxy => "remove_proxy",
                Self::RemoveProxies => "remove_proxies",
                Self::CreatePure => "create_pure",
                Self::KillPure => "kill_pure",
                Self::Announce => "announce",
                Self::RemoveAnnouncement => "remove_announcement",
                Self::RejectAnnouncement => "reject_announcement",
                Self::ProxyAnnounced => "proxy_announced",
            }
        }
    }
    #[doc = "Calls of pallet `Referenda`."]
    pub enum ReferendaCall {
        Submit,
        PlaceDecisionDeposit,
        RefundDecisionDeposit,
        Cancel,
        Kill,
        NudgeReferendum,
        OneFewerDeciding,
        RefundSubmissionDeposit,
        SetMetadata,
    }
    impl CallInfo for ReferendaCall {
        const PALLET: &'static str = "Referenda";
        fn call_name(&self) -> &'static str {
            match self {
                Self::Submit => "submit",
                Self::PlaceDecisionDeposit => "place_decision_deposit",
                Self::RefundDecisionDeposit => "refund_decision_deposit",
                Self::Cancel => "cancel",
                Self::Kill => "kill",
                Self::NudgeReferendum => "nudge_referendum",
                Self::OneFewerDeciding => "one_fewer_deciding",
                Self::RefundSubmissionDeposit => "refund_submission_deposit",
                Self::SetMetadata => "set_metadata",
            }
        }
    }
    #[doc = "Calls of pallet `Scheduler`."]
    pub enum SchedulerCall {
        Schedule,
        Cancel,
        ScheduleNamed,
        CancelNamed,
        ScheduleAfter,
        ScheduleNamedAfter,
        SetRetry,
        SetRetryNamed,
        CancelRetry,
        CancelRetryNamed,
    }
    impl CallInfo for SchedulerCall {
        const PALLET: &'static str = "Scheduler";
        fn call_name(&self) -> &'static str {
            match self {
                Self::Schedule => "schedule",
                Self::Cancel => "cancel",
                Self::ScheduleNamed => "schedule_named",
                Self::CancelNamed => "cancel_named",
                Self::ScheduleAfter => "schedule_after",
                Self::ScheduleNamedAfter => "schedule_named_after",
                Self::SetRetry => "set_retry",
                Self::SetRetryNamed => "set_retry_named",
                Self::CancelRetry => "cancel_retry",
                Self::CancelRetryNamed => "cancel_retry_named",
            }
        }
    }
    #[doc = "Calls of pallet `Session`."]
    pub enum SessionCall {
        SetKeys,
        PurgeKeys,
    }
    impl CallInfo for SessionCall {
        const PALLET: &'static str = "Session";
        fn call_name(&self) -> &'static str {
            match self {
                Self::SetKeys => "set_keys",
                Self::PurgeKeys => "purge_keys",
            }
        }
    }
    #[doc = "Calls of pallet `Staking`."]
    pub enum StakingCall {
        Bond,
        BondExtra,
        Unbond,
        WithdrawUnbonded,
        Validate,
        Nominate,
        Chill,
        SetPayee,
        SetController,
        SetValidatorCount,
        IncreaseValidatorCount,
        ScaleValidatorCount,
        ForceNoEras,
        ForceNewEra,
        SetInvulnerables,
        ForceUnstake,
        ForceNewEraAlways,
        CancelDeferredSlash,
        PayoutStakers,
        Rebond,
        ReapStash,
        Kick,
        SetStakingConfigs,
        ChillOther,
        ForceApplyMinCommission,
        SetMinCommission,
        PayoutStakersByPage,
        UpdatePayee,
        DeprecateControllerBatch,
        RestoreLedger,
    }
    impl CallInfo for StakingCall {
        const PALLET: &'static str = "Staking";
        fn call_name(&self) -> &'static str {
            match self {
                Self::Bond => "bond",
                Self::BondExtra => "bond_extra",
                Self::Unbond => "unbond",
                Self::WithdrawUnbonded => "withdraw_unbonded",
                Self::Validate => "validate",
                Self::Nominate => "nominate",
                Self::Chill => "chill",
                Self::SetPayee => "set_payee",
                Self::SetController => "set_controller",
                Self::SetValidatorCount => "set_validator_count",
                Self::IncreaseValidatorCount => "increase_validator_count",
                Self::ScaleValidatorCount => "scale_validator_count",
                Self::ForceNoEras => "force_no_eras",
                Self::ForceNewEra => "force_new_era",
                Self::SetInvulnerables => "set_invulnerables",
                Self::ForceUnstake => "force_unstake",
                Self::ForceNewEraAlways => "force_new_era_always",
                Self::CancelDeferredSlash => "cancel_deferred_slash",
                Self::PayoutStakers => "payout_stakers",
                Self::Rebond => "rebond",
                Self::ReapStash => "reap_stash",
                Self::Kick => "kick",
                Self::SetStakingConfigs => "set_staking_configs",
                Self::ChillOther => "chill_other",
                Self::ForceApplyMinCommission => "force_apply_min_commission",
                Self::SetMinCommission => "set_min_commission",
                Self::PayoutStakersByPage => "payout_stakers_by_page",
                Self::UpdatePayee => "update_payee",
                Self::DeprecateControllerBatch => "deprecate_controller_batch",
                Self::RestoreLedger => "restore_ledger",
            }
        }
    }
    #[doc = "Calls of pallet `StakingRewards`."]
    pub enum StakingRewardsCall {
        Refill,
        ForceRefill,
        Withdraw,
        SetTargetInflation,
        SetIdealStakingRatio,
    }
    impl CallInfo for StakingRewardsCall {
        const PALLET: &'static str = "StakingRewards";
        fn call_name(&self) -> &'static str {
            match self {
                Self::Refill => "refill",
                Self::ForceRefill => "force_refill",
                Self::Withdraw => "withdraw",
                Self::SetTargetInflation => "set_target_inflation",
                Self::SetIdealStakingRatio => "set_ideal_staking_ratio",
            }
        }
    }
    #[doc = "Calls of pallet `Sudo`."]
    pub enum SudoCall {
        Sudo,
        SudoUncheckedWeight,
        SetKey,
        SudoAs,
        RemoveKey,
    }
    impl CallInfo for SudoCall {
        const PALLET: &'static str = "Sudo";
        fn call_name(&self) -> &'static str {
            match self {
                Self::Sudo => "sudo",
                Self::SudoUncheckedWeight => "sudo_unchecked_weight",
                Self::SetKey => "set_key",
                Self::SudoAs => "sudo_as",
                Self::RemoveKey => "remove_key",
            }
        }
    }
    #[doc = "Calls of pallet `System`."]
    pub enum SystemCall {
        Remark,
        SetHeapPages,
        SetCode,
        SetCodeWithoutChecks,
        SetStorage,
        KillStorage,
        KillPrefix,
        RemarkWithEvent,
        AuthorizeUpgrade,
        AuthorizeUpgradeWithoutChecks,
        ApplyAuthorizedUpgrade,
    }
    impl CallInfo for SystemCall {
        const PALLET: &'static str = "System";
        fn call_name(&self) -> &'static str {
            match self {
                Self::Remark => "remark",
                Self::SetHeapPages => "set_heap_pages",
                Self::SetCode => "set_code",
                Self::SetCodeWithoutChecks => "set_code_without_checks",
                Self::SetStorage => "set_storage",
                Self::KillStorage => "kill_storage",
                Self::KillPrefix => "kill_prefix",
                Self::RemarkWithEvent => "remark_with_event",
                Self::AuthorizeUpgrade => "authorize_upgrade",
                Self::AuthorizeUpgradeWithoutChecks => "authorize_upgrade_without_checks",
                Self::ApplyAuthorizedUpgrade => "apply_authorized_upgrade",
            }
        }
    }
    #[doc = "Calls of pallet `Timestamp`."]
    pub enum TimestampCall {
        Set,
    }
    impl CallInfo for TimestampCall {
        const PALLET: &'static str = "Timestamp";
        fn call_name(&self) -> &'static str {
            match self {
                Self::Set => "set",
            }
        }
    }
    #[doc = "Calls of pallet `Treasury`."]
    pub enum TreasuryCall {
        SpendLocal,
        RemoveApproval,
        Spend,
        Payout,
        CheckStatus,
        VoidSpend,
    }
    impl CallInfo for TreasuryCall {
        const PALLET: &'static str = "Treasury";
        fn call_name(&self) -> &'static str {
            match self {
                Self::SpendLocal => "spend_local",
                Self::RemoveApproval => "remove_approval",
                Self::Spend => "spend",
                Self::Payout => "payout",
                Self::CheckStatus => "check_status",
                Self::VoidSpend => "void_spend",
            }
        }
    }
    #[doc = "Calls of pallet `Utility`."]
    pub enum UtilityCall {
        Batch,
        AsDerivative,
        BatchAll,
        DispatchAs,
        ForceBatch,
        WithWeight,
    }
    impl CallInfo for UtilityCall {
        const PALLET: &'static str = "Utility";
        fn call_name(&self) -> &'static str {
            match self {
                Self::Batch => "batch",
                Self::AsDerivative => "as_derivative",
                Self::BatchAll => "batch_all",
                Self::DispatchAs => "dispatch_as",
                Self::ForceBatch => "force_batch",
                Self::WithWeight => "with_weight",
            }
        }
    }
    #[doc = "Calls of pallet `Vesting`."]
    pub enum VestingCall {
        Vest,
        VestOther,
        VestedTransfer,
        ForceVestedTransfer,
        MergeSchedules,
        ForceRemoveVestingSchedule,
    }
    impl CallInfo for VestingCall {
        const PALLET: &'static str = "Vesting";
        fn call_name(&self) -> &'static str {
            match self {
                Self::Vest => "vest",
                Self::VestOther => "vest_other",
                Self::VestedTransfer => "vested_transfer",
                Self::ForceVestedTransfer => "force_vested_transfer",
                Self::MergeSchedules => "merge_schedules",
                Self::ForceRemoveVestingSchedule => "force_remove_vesting_schedule",
            }
        }
    }
    #[doc = "Calls of pallet `Whitelist`."]
    pub enum WhitelistCall {
        WhitelistCall,
        RemoveWhitelistedCall,
        DispatchWhitelistedCall,
        DispatchWhitelistedCallWithPreimage,
    }
    impl CallInfo for WhitelistCall {
        const PALLET: &'static str = "Whitelist";
        fn call_name(&self) -> &'static str {
            match self {
                Self::WhitelistCall => "whitelist_call",
                Self::RemoveWhitelistedCall => "remove_whitelisted_call",
                Self::DispatchWhitelistedCall => "dispatch_whitelisted_call",
                Self::DispatchWhitelistedCallWithPreimage => {
                    "dispatch_whitelisted_call_with_preimage"
                }
            }
        }
    }
}
pub mod storage {
    #[doc = r" Show the call info."]
    pub trait StorageInfo {
        const PALLET: &'static str;
        #[doc = r" returns call name."]
        fn storage_name(&self) -> &'static str;
    }
    #[doc = "Storage of pallet `AuthorityDiscovery`."]
    pub enum AuthorityDiscoveryStorage {
        Keys,
        NextKeys,
    }
    impl StorageInfo for AuthorityDiscoveryStorage {
        const PALLET: &'static str = "AuthorityDiscovery";
        fn storage_name(&self) -> &'static str {
            match self {
                Self::Keys => "Keys",
                Self::NextKeys => "NextKeys",
            }
        }
    }
    #[doc = "Storage of pallet `Authorship`."]
    pub enum AuthorshipStorage {
        Author,
    }
    impl StorageInfo for AuthorshipStorage {
        const PALLET: &'static str = "Authorship";
        fn storage_name(&self) -> &'static str {
            match self {
                Self::Author => "Author",
            }
        }
    }
    #[doc = "Storage of pallet `Babe`."]
    pub enum BabeStorage {
        EpochIndex,
        Authorities,
        GenesisSlot,
        CurrentSlot,
        Randomness,
        PendingEpochConfigChange,
        NextRandomness,
        NextAuthorities,
        SegmentIndex,
        UnderConstruction,
        Initialized,
        AuthorVrfRandomness,
        EpochStart,
        Lateness,
        EpochConfig,
        NextEpochConfig,
        SkippedEpochs,
    }
    impl StorageInfo for BabeStorage {
        const PALLET: &'static str = "Babe";
        fn storage_name(&self) -> &'static str {
            match self {
                Self::EpochIndex => "EpochIndex",
                Self::Authorities => "Authorities",
                Self::GenesisSlot => "GenesisSlot",
                Self::CurrentSlot => "CurrentSlot",
                Self::Randomness => "Randomness",
                Self::PendingEpochConfigChange => "PendingEpochConfigChange",
                Self::NextRandomness => "NextRandomness",
                Self::NextAuthorities => "NextAuthorities",
                Self::SegmentIndex => "SegmentIndex",
                Self::UnderConstruction => "UnderConstruction",
                Self::Initialized => "Initialized",
                Self::AuthorVrfRandomness => "AuthorVrfRandomness",
                Self::EpochStart => "EpochStart",
                Self::Lateness => "Lateness",
                Self::EpochConfig => "EpochConfig",
                Self::NextEpochConfig => "NextEpochConfig",
                Self::SkippedEpochs => "SkippedEpochs",
            }
        }
    }
    #[doc = "Storage of pallet `BagsList`."]
    pub enum BagsListStorage {
        ListNodes,
        CounterForListNodes,
        ListBags,
    }
    impl StorageInfo for BagsListStorage {
        const PALLET: &'static str = "BagsList";
        fn storage_name(&self) -> &'static str {
            match self {
                Self::ListNodes => "ListNodes",
                Self::CounterForListNodes => "CounterForListNodes",
                Self::ListBags => "ListBags",
            }
        }
    }
    #[doc = "Storage of pallet `Balances`."]
    pub enum BalancesStorage {
        TotalIssuance,
        InactiveIssuance,
        Account,
        Locks,
        Reserves,
        Holds,
        Freezes,
    }
    impl StorageInfo for BalancesStorage {
        const PALLET: &'static str = "Balances";
        fn storage_name(&self) -> &'static str {
            match self {
                Self::TotalIssuance => "TotalIssuance",
                Self::InactiveIssuance => "InactiveIssuance",
                Self::Account => "Account",
                Self::Locks => "Locks",
                Self::Reserves => "Reserves",
                Self::Holds => "Holds",
                Self::Freezes => "Freezes",
            }
        }
    }
    #[doc = "Storage of pallet `Bounties`."]
    pub enum BountiesStorage {
        BountyCount,
        Bounties,
        BountyDescriptions,
        BountyApprovals,
    }
    impl StorageInfo for BountiesStorage {
        const PALLET: &'static str = "Bounties";
        fn storage_name(&self) -> &'static str {
            match self {
                Self::BountyCount => "BountyCount",
                Self::Bounties => "Bounties",
                Self::BountyDescriptions => "BountyDescriptions",
                Self::BountyApprovals => "BountyApprovals",
            }
        }
    }
    #[doc = "Storage of pallet `ChildBounties`."]
    pub enum ChildBountiesStorage {
        ChildBountyCount,
        ParentChildBounties,
        ChildBounties,
        ChildBountyDescriptions,
        ChildrenCuratorFees,
    }
    impl StorageInfo for ChildBountiesStorage {
        const PALLET: &'static str = "ChildBounties";
        fn storage_name(&self) -> &'static str {
            match self {
                Self::ChildBountyCount => "ChildBountyCount",
                Self::ParentChildBounties => "ParentChildBounties",
                Self::ChildBounties => "ChildBounties",
                Self::ChildBountyDescriptions => "ChildBountyDescriptions",
                Self::ChildrenCuratorFees => "ChildrenCuratorFees",
            }
        }
    }
    #[doc = "Storage of pallet `ConvictionVoting`."]
    pub enum ConvictionVotingStorage {
        VotingFor,
        ClassLocksFor,
    }
    impl StorageInfo for ConvictionVotingStorage {
        const PALLET: &'static str = "ConvictionVoting";
        fn storage_name(&self) -> &'static str {
            match self {
                Self::VotingFor => "VotingFor",
                Self::ClassLocksFor => "ClassLocksFor",
            }
        }
    }
    #[doc = "Storage of pallet `ElectionProviderMultiPhase`."]
    pub enum ElectionProviderMultiPhaseStorage {
        Round,
        CurrentPhase,
        QueuedSolution,
        Snapshot,
        DesiredTargets,
        SnapshotMetadata,
        SignedSubmissionNextIndex,
        SignedSubmissionIndices,
        SignedSubmissionsMap,
        MinimumUntrustedScore,
    }
    impl StorageInfo for ElectionProviderMultiPhaseStorage {
        const PALLET: &'static str = "ElectionProviderMultiPhase";
        fn storage_name(&self) -> &'static str {
            match self {
                Self::Round => "Round",
                Self::CurrentPhase => "CurrentPhase",
                Self::QueuedSolution => "QueuedSolution",
                Self::Snapshot => "Snapshot",
                Self::DesiredTargets => "DesiredTargets",
                Self::SnapshotMetadata => "SnapshotMetadata",
                Self::SignedSubmissionNextIndex => "SignedSubmissionNextIndex",
                Self::SignedSubmissionIndices => "SignedSubmissionIndices",
                Self::SignedSubmissionsMap => "SignedSubmissionsMap",
                Self::MinimumUntrustedScore => "MinimumUntrustedScore",
            }
        }
    }
    #[doc = "Storage of pallet `FellowshipCollective`."]
    pub enum FellowshipCollectiveStorage {
        MemberCount,
        Members,
        IdToIndex,
        IndexToId,
        Voting,
        VotingCleanup,
    }
    impl StorageInfo for FellowshipCollectiveStorage {
        const PALLET: &'static str = "FellowshipCollective";
        fn storage_name(&self) -> &'static str {
            match self {
                Self::MemberCount => "MemberCount",
                Self::Members => "Members",
                Self::IdToIndex => "IdToIndex",
                Self::IndexToId => "IndexToId",
                Self::Voting => "Voting",
                Self::VotingCleanup => "VotingCleanup",
            }
        }
    }
    #[doc = "Storage of pallet `FellowshipReferenda`."]
    pub enum FellowshipReferendaStorage {
        ReferendumCount,
        ReferendumInfoFor,
        TrackQueue,
        DecidingCount,
        MetadataOf,
    }
    impl StorageInfo for FellowshipReferendaStorage {
        const PALLET: &'static str = "FellowshipReferenda";
        fn storage_name(&self) -> &'static str {
            match self {
                Self::ReferendumCount => "ReferendumCount",
                Self::ReferendumInfoFor => "ReferendumInfoFor",
                Self::TrackQueue => "TrackQueue",
                Self::DecidingCount => "DecidingCount",
                Self::MetadataOf => "MetadataOf",
            }
        }
    }
    #[doc = "Storage of pallet `Gear`."]
    pub enum GearStorage {
        ExecuteInherent,
        BlockNumber,
        GearRunInBlock,
    }
    impl StorageInfo for GearStorage {
        const PALLET: &'static str = "Gear";
        fn storage_name(&self) -> &'static str {
            match self {
                Self::ExecuteInherent => "ExecuteInherent",
                Self::BlockNumber => "BlockNumber",
                Self::GearRunInBlock => "GearRunInBlock",
            }
        }
    }
    #[doc = "Storage of pallet `GearBank`."]
    pub enum GearBankStorage {
        Bank,
        BankAddress,
        UnusedValue,
        OnFinalizeTransfers,
        OnFinalizeValue,
    }
    impl StorageInfo for GearBankStorage {
        const PALLET: &'static str = "GearBank";
        fn storage_name(&self) -> &'static str {
            match self {
                Self::Bank => "Bank",
                Self::BankAddress => "BankAddress",
                Self::UnusedValue => "UnusedValue",
                Self::OnFinalizeTransfers => "OnFinalizeTransfers",
                Self::OnFinalizeValue => "OnFinalizeValue",
            }
        }
    }
    #[doc = "Storage of pallet `GearEthBridge`."]
    pub enum GearEthBridgeStorage {
        Initialized,
        Paused,
        AuthoritySetHash,
        QueueMerkleRoot,
        Queue,
        QueueId,
        QueuesInfo,
        SessionsTimer,
        ClearTimer,
        MessageNonce,
        QueueChanged,
        ResetQueueOnInit,
        TransportFee,
    }
    impl StorageInfo for GearEthBridgeStorage {
        const PALLET: &'static str = "GearEthBridge";
        fn storage_name(&self) -> &'static str {
            match self {
                Self::Initialized => "Initialized",
                Self::Paused => "Paused",
                Self::AuthoritySetHash => "AuthoritySetHash",
                Self::QueueMerkleRoot => "QueueMerkleRoot",
                Self::Queue => "Queue",
                Self::QueueId => "QueueId",
                Self::QueuesInfo => "QueuesInfo",
                Self::SessionsTimer => "SessionsTimer",
                Self::ClearTimer => "ClearTimer",
                Self::MessageNonce => "MessageNonce",
                Self::QueueChanged => "QueueChanged",
                Self::ResetQueueOnInit => "ResetQueueOnInit",
                Self::TransportFee => "TransportFee",
            }
        }
    }
    #[doc = "Storage of pallet `GearGas`."]
    pub enum GearGasStorage {
        TotalIssuance,
        GasNodes,
        Allowance,
    }
    impl StorageInfo for GearGasStorage {
        const PALLET: &'static str = "GearGas";
        fn storage_name(&self) -> &'static str {
            match self {
                Self::TotalIssuance => "TotalIssuance",
                Self::GasNodes => "GasNodes",
                Self::Allowance => "Allowance",
            }
        }
    }
    #[doc = "Storage of pallet `GearMessenger`."]
    pub enum GearMessengerStorage {
        Dequeued,
        Dispatches,
        CounterForDispatches,
        Head,
        Mailbox,
        QueueProcessing,
        Sent,
        Tail,
        Waitlist,
        DispatchStash,
    }
    impl StorageInfo for GearMessengerStorage {
        const PALLET: &'static str = "GearMessenger";
        fn storage_name(&self) -> &'static str {
            match self {
                Self::Dequeued => "Dequeued",
                Self::Dispatches => "Dispatches",
                Self::CounterForDispatches => "CounterForDispatches",
                Self::Head => "Head",
                Self::Mailbox => "Mailbox",
                Self::QueueProcessing => "QueueProcessing",
                Self::Sent => "Sent",
                Self::Tail => "Tail",
                Self::Waitlist => "Waitlist",
                Self::DispatchStash => "DispatchStash",
            }
        }
    }
    #[doc = "Storage of pallet `GearProgram`."]
    pub enum GearProgramStorage {
        InstrumentedCodeStorage,
        OriginalCodeStorage,
        CodeMetadataStorage,
        AllocationsStorage,
        ProgramStorage,
        MemoryPages,
    }
    impl StorageInfo for GearProgramStorage {
        const PALLET: &'static str = "GearProgram";
        fn storage_name(&self) -> &'static str {
            match self {
                Self::InstrumentedCodeStorage => "InstrumentedCodeStorage",
                Self::OriginalCodeStorage => "OriginalCodeStorage",
                Self::CodeMetadataStorage => "CodeMetadataStorage",
                Self::AllocationsStorage => "AllocationsStorage",
                Self::ProgramStorage => "ProgramStorage",
                Self::MemoryPages => "MemoryPages",
            }
        }
    }
    #[doc = "Storage of pallet `GearScheduler`."]
    pub enum GearSchedulerStorage {
        FirstIncompleteTasksBlock,
        TaskPool,
    }
    impl StorageInfo for GearSchedulerStorage {
        const PALLET: &'static str = "GearScheduler";
        fn storage_name(&self) -> &'static str {
            match self {
                Self::FirstIncompleteTasksBlock => "FirstIncompleteTasksBlock",
                Self::TaskPool => "TaskPool",
            }
        }
    }
    #[doc = "Storage of pallet `GearVoucher`."]
    pub enum GearVoucherStorage {
        Issued,
        Vouchers,
    }
    impl StorageInfo for GearVoucherStorage {
        const PALLET: &'static str = "GearVoucher";
        fn storage_name(&self) -> &'static str {
            match self {
                Self::Issued => "Issued",
                Self::Vouchers => "Vouchers",
            }
        }
    }
    #[doc = "Storage of pallet `Grandpa`."]
    pub enum GrandpaStorage {
        State,
        PendingChange,
        NextForced,
        Stalled,
        CurrentSetId,
        SetIdSession,
        Authorities,
    }
    impl StorageInfo for GrandpaStorage {
        const PALLET: &'static str = "Grandpa";
        fn storage_name(&self) -> &'static str {
            match self {
                Self::State => "State",
                Self::PendingChange => "PendingChange",
                Self::NextForced => "NextForced",
                Self::Stalled => "Stalled",
                Self::CurrentSetId => "CurrentSetId",
                Self::SetIdSession => "SetIdSession",
                Self::Authorities => "Authorities",
            }
        }
    }
    #[doc = "Storage of pallet `Historical`."]
    pub enum HistoricalStorage {
        HistoricalSessions,
        StoredRange,
    }
    impl StorageInfo for HistoricalStorage {
        const PALLET: &'static str = "Historical";
        fn storage_name(&self) -> &'static str {
            match self {
                Self::HistoricalSessions => "HistoricalSessions",
                Self::StoredRange => "StoredRange",
            }
        }
    }
    #[doc = "Storage of pallet `Identity`."]
    pub enum IdentityStorage {
        IdentityOf,
        SuperOf,
        SubsOf,
        Registrars,
        UsernameAuthorities,
        AccountOfUsername,
        PendingUsernames,
    }
    impl StorageInfo for IdentityStorage {
        const PALLET: &'static str = "Identity";
        fn storage_name(&self) -> &'static str {
            match self {
                Self::IdentityOf => "IdentityOf",
                Self::SuperOf => "SuperOf",
                Self::SubsOf => "SubsOf",
                Self::Registrars => "Registrars",
                Self::UsernameAuthorities => "UsernameAuthorities",
                Self::AccountOfUsername => "AccountOfUsername",
                Self::PendingUsernames => "PendingUsernames",
            }
        }
    }
    #[doc = "Storage of pallet `ImOnline`."]
    pub enum ImOnlineStorage {
        HeartbeatAfter,
        Keys,
        ReceivedHeartbeats,
        AuthoredBlocks,
    }
    impl StorageInfo for ImOnlineStorage {
        const PALLET: &'static str = "ImOnline";
        fn storage_name(&self) -> &'static str {
            match self {
                Self::HeartbeatAfter => "HeartbeatAfter",
                Self::Keys => "Keys",
                Self::ReceivedHeartbeats => "ReceivedHeartbeats",
                Self::AuthoredBlocks => "AuthoredBlocks",
            }
        }
    }
    #[doc = "Storage of pallet `Multisig`."]
    pub enum MultisigStorage {
        Multisigs,
    }
    impl StorageInfo for MultisigStorage {
        const PALLET: &'static str = "Multisig";
        fn storage_name(&self) -> &'static str {
            match self {
                Self::Multisigs => "Multisigs",
            }
        }
    }
    #[doc = "Storage of pallet `NominationPools`."]
    pub enum NominationPoolsStorage {
        TotalValueLocked,
        MinJoinBond,
        MinCreateBond,
        MaxPools,
        MaxPoolMembers,
        MaxPoolMembersPerPool,
        GlobalMaxCommission,
        PoolMembers,
        CounterForPoolMembers,
        BondedPools,
        CounterForBondedPools,
        RewardPools,
        CounterForRewardPools,
        SubPoolsStorage,
        CounterForSubPoolsStorage,
        Metadata,
        CounterForMetadata,
        LastPoolId,
        ReversePoolIdLookup,
        CounterForReversePoolIdLookup,
        ClaimPermissions,
    }
    impl StorageInfo for NominationPoolsStorage {
        const PALLET: &'static str = "NominationPools";
        fn storage_name(&self) -> &'static str {
            match self {
                Self::TotalValueLocked => "TotalValueLocked",
                Self::MinJoinBond => "MinJoinBond",
                Self::MinCreateBond => "MinCreateBond",
                Self::MaxPools => "MaxPools",
                Self::MaxPoolMembers => "MaxPoolMembers",
                Self::MaxPoolMembersPerPool => "MaxPoolMembersPerPool",
                Self::GlobalMaxCommission => "GlobalMaxCommission",
                Self::PoolMembers => "PoolMembers",
                Self::CounterForPoolMembers => "CounterForPoolMembers",
                Self::BondedPools => "BondedPools",
                Self::CounterForBondedPools => "CounterForBondedPools",
                Self::RewardPools => "RewardPools",
                Self::CounterForRewardPools => "CounterForRewardPools",
                Self::SubPoolsStorage => "SubPoolsStorage",
                Self::CounterForSubPoolsStorage => "CounterForSubPoolsStorage",
                Self::Metadata => "Metadata",
                Self::CounterForMetadata => "CounterForMetadata",
                Self::LastPoolId => "LastPoolId",
                Self::ReversePoolIdLookup => "ReversePoolIdLookup",
                Self::CounterForReversePoolIdLookup => "CounterForReversePoolIdLookup",
                Self::ClaimPermissions => "ClaimPermissions",
            }
        }
    }
    #[doc = "Storage of pallet `Offences`."]
    pub enum OffencesStorage {
        Reports,
        ConcurrentReportsIndex,
    }
    impl StorageInfo for OffencesStorage {
        const PALLET: &'static str = "Offences";
        fn storage_name(&self) -> &'static str {
            match self {
                Self::Reports => "Reports",
                Self::ConcurrentReportsIndex => "ConcurrentReportsIndex",
            }
        }
    }
    #[doc = "Storage of pallet `Preimage`."]
    pub enum PreimageStorage {
        StatusFor,
        RequestStatusFor,
        PreimageFor,
    }
    impl StorageInfo for PreimageStorage {
        const PALLET: &'static str = "Preimage";
        fn storage_name(&self) -> &'static str {
            match self {
                Self::StatusFor => "StatusFor",
                Self::RequestStatusFor => "RequestStatusFor",
                Self::PreimageFor => "PreimageFor",
            }
        }
    }
    #[doc = "Storage of pallet `Proxy`."]
    pub enum ProxyStorage {
        Proxies,
        Announcements,
    }
    impl StorageInfo for ProxyStorage {
        const PALLET: &'static str = "Proxy";
        fn storage_name(&self) -> &'static str {
            match self {
                Self::Proxies => "Proxies",
                Self::Announcements => "Announcements",
            }
        }
    }
    #[doc = "Storage of pallet `Referenda`."]
    pub enum ReferendaStorage {
        ReferendumCount,
        ReferendumInfoFor,
        TrackQueue,
        DecidingCount,
        MetadataOf,
    }
    impl StorageInfo for ReferendaStorage {
        const PALLET: &'static str = "Referenda";
        fn storage_name(&self) -> &'static str {
            match self {
                Self::ReferendumCount => "ReferendumCount",
                Self::ReferendumInfoFor => "ReferendumInfoFor",
                Self::TrackQueue => "TrackQueue",
                Self::DecidingCount => "DecidingCount",
                Self::MetadataOf => "MetadataOf",
            }
        }
    }
    #[doc = "Storage of pallet `Scheduler`."]
    pub enum SchedulerStorage {
        IncompleteSince,
        Agenda,
        Retries,
        Lookup,
    }
    impl StorageInfo for SchedulerStorage {
        const PALLET: &'static str = "Scheduler";
        fn storage_name(&self) -> &'static str {
            match self {
                Self::IncompleteSince => "IncompleteSince",
                Self::Agenda => "Agenda",
                Self::Retries => "Retries",
                Self::Lookup => "Lookup",
            }
        }
    }
    #[doc = "Storage of pallet `Session`."]
    pub enum SessionStorage {
        Validators,
        CurrentIndex,
        QueuedChanged,
        QueuedKeys,
        DisabledValidators,
        NextKeys,
        KeyOwner,
    }
    impl StorageInfo for SessionStorage {
        const PALLET: &'static str = "Session";
        fn storage_name(&self) -> &'static str {
            match self {
                Self::Validators => "Validators",
                Self::CurrentIndex => "CurrentIndex",
                Self::QueuedChanged => "QueuedChanged",
                Self::QueuedKeys => "QueuedKeys",
                Self::DisabledValidators => "DisabledValidators",
                Self::NextKeys => "NextKeys",
                Self::KeyOwner => "KeyOwner",
            }
        }
    }
    #[doc = "Storage of pallet `Staking`."]
    pub enum StakingStorage {
        ValidatorCount,
        MinimumValidatorCount,
        Invulnerables,
        Bonded,
        MinNominatorBond,
        MinValidatorBond,
        MinimumActiveStake,
        MinCommission,
        Ledger,
        Payee,
        Validators,
        CounterForValidators,
        MaxValidatorsCount,
        Nominators,
        CounterForNominators,
        VirtualStakers,
        CounterForVirtualStakers,
        MaxNominatorsCount,
        CurrentEra,
        ActiveEra,
        ErasStartSessionIndex,
        ErasStakers,
        ErasStakersOverview,
        ErasStakersClipped,
        ErasStakersPaged,
        ClaimedRewards,
        ErasValidatorPrefs,
        ErasValidatorReward,
        ErasRewardPoints,
        ErasTotalStake,
        ForceEra,
        MaxStakedRewards,
        SlashRewardFraction,
        CanceledSlashPayout,
        UnappliedSlashes,
        BondedEras,
        ValidatorSlashInEra,
        NominatorSlashInEra,
        SlashingSpans,
        SpanSlash,
        CurrentPlannedSession,
        DisabledValidators,
        ChillThreshold,
    }
    impl StorageInfo for StakingStorage {
        const PALLET: &'static str = "Staking";
        fn storage_name(&self) -> &'static str {
            match self {
                Self::ValidatorCount => "ValidatorCount",
                Self::MinimumValidatorCount => "MinimumValidatorCount",
                Self::Invulnerables => "Invulnerables",
                Self::Bonded => "Bonded",
                Self::MinNominatorBond => "MinNominatorBond",
                Self::MinValidatorBond => "MinValidatorBond",
                Self::MinimumActiveStake => "MinimumActiveStake",
                Self::MinCommission => "MinCommission",
                Self::Ledger => "Ledger",
                Self::Payee => "Payee",
                Self::Validators => "Validators",
                Self::CounterForValidators => "CounterForValidators",
                Self::MaxValidatorsCount => "MaxValidatorsCount",
                Self::Nominators => "Nominators",
                Self::CounterForNominators => "CounterForNominators",
                Self::VirtualStakers => "VirtualStakers",
                Self::CounterForVirtualStakers => "CounterForVirtualStakers",
                Self::MaxNominatorsCount => "MaxNominatorsCount",
                Self::CurrentEra => "CurrentEra",
                Self::ActiveEra => "ActiveEra",
                Self::ErasStartSessionIndex => "ErasStartSessionIndex",
                Self::ErasStakers => "ErasStakers",
                Self::ErasStakersOverview => "ErasStakersOverview",
                Self::ErasStakersClipped => "ErasStakersClipped",
                Self::ErasStakersPaged => "ErasStakersPaged",
                Self::ClaimedRewards => "ClaimedRewards",
                Self::ErasValidatorPrefs => "ErasValidatorPrefs",
                Self::ErasValidatorReward => "ErasValidatorReward",
                Self::ErasRewardPoints => "ErasRewardPoints",
                Self::ErasTotalStake => "ErasTotalStake",
                Self::ForceEra => "ForceEra",
                Self::MaxStakedRewards => "MaxStakedRewards",
                Self::SlashRewardFraction => "SlashRewardFraction",
                Self::CanceledSlashPayout => "CanceledSlashPayout",
                Self::UnappliedSlashes => "UnappliedSlashes",
                Self::BondedEras => "BondedEras",
                Self::ValidatorSlashInEra => "ValidatorSlashInEra",
                Self::NominatorSlashInEra => "NominatorSlashInEra",
                Self::SlashingSpans => "SlashingSpans",
                Self::SpanSlash => "SpanSlash",
                Self::CurrentPlannedSession => "CurrentPlannedSession",
                Self::DisabledValidators => "DisabledValidators",
                Self::ChillThreshold => "ChillThreshold",
            }
        }
    }
    #[doc = "Storage of pallet `StakingRewards`."]
    pub enum StakingRewardsStorage {
        TargetInflation,
        IdealStakingRatio,
        NonStakeableShare,
        FilteredAccounts,
    }
    impl StorageInfo for StakingRewardsStorage {
        const PALLET: &'static str = "StakingRewards";
        fn storage_name(&self) -> &'static str {
            match self {
                Self::TargetInflation => "TargetInflation",
                Self::IdealStakingRatio => "IdealStakingRatio",
                Self::NonStakeableShare => "NonStakeableShare",
                Self::FilteredAccounts => "FilteredAccounts",
            }
        }
    }
    #[doc = "Storage of pallet `Sudo`."]
    pub enum SudoStorage {
        Key,
    }
    impl StorageInfo for SudoStorage {
        const PALLET: &'static str = "Sudo";
        fn storage_name(&self) -> &'static str {
            match self {
                Self::Key => "Key",
            }
        }
    }
    #[doc = "Storage of pallet `System`."]
    pub enum SystemStorage {
        Account,
        ExtrinsicCount,
        InherentsApplied,
        BlockWeight,
        AllExtrinsicsLen,
        BlockHash,
        ExtrinsicData,
        Number,
        ParentHash,
        Digest,
        Events,
        EventCount,
        EventTopics,
        LastRuntimeUpgrade,
        UpgradedToU32RefCount,
        UpgradedToTripleRefCount,
        ExecutionPhase,
        AuthorizedUpgrade,
    }
    impl StorageInfo for SystemStorage {
        const PALLET: &'static str = "System";
        fn storage_name(&self) -> &'static str {
            match self {
                Self::Account => "Account",
                Self::ExtrinsicCount => "ExtrinsicCount",
                Self::InherentsApplied => "InherentsApplied",
                Self::BlockWeight => "BlockWeight",
                Self::AllExtrinsicsLen => "AllExtrinsicsLen",
                Self::BlockHash => "BlockHash",
                Self::ExtrinsicData => "ExtrinsicData",
                Self::Number => "Number",
                Self::ParentHash => "ParentHash",
                Self::Digest => "Digest",
                Self::Events => "Events",
                Self::EventCount => "EventCount",
                Self::EventTopics => "EventTopics",
                Self::LastRuntimeUpgrade => "LastRuntimeUpgrade",
                Self::UpgradedToU32RefCount => "UpgradedToU32RefCount",
                Self::UpgradedToTripleRefCount => "UpgradedToTripleRefCount",
                Self::ExecutionPhase => "ExecutionPhase",
                Self::AuthorizedUpgrade => "AuthorizedUpgrade",
            }
        }
    }
    #[doc = "Storage of pallet `Timestamp`."]
    pub enum TimestampStorage {
        Now,
        DidUpdate,
    }
    impl StorageInfo for TimestampStorage {
        const PALLET: &'static str = "Timestamp";
        fn storage_name(&self) -> &'static str {
            match self {
                Self::Now => "Now",
                Self::DidUpdate => "DidUpdate",
            }
        }
    }
    #[doc = "Storage of pallet `TransactionPayment`."]
    pub enum TransactionPaymentStorage {
        NextFeeMultiplier,
        StorageVersion,
    }
    impl StorageInfo for TransactionPaymentStorage {
        const PALLET: &'static str = "TransactionPayment";
        fn storage_name(&self) -> &'static str {
            match self {
                Self::NextFeeMultiplier => "NextFeeMultiplier",
                Self::StorageVersion => "StorageVersion",
            }
        }
    }
    #[doc = "Storage of pallet `Treasury`."]
    pub enum TreasuryStorage {
        ProposalCount,
        Proposals,
        Deactivated,
        Approvals,
        SpendCount,
        Spends,
    }
    impl StorageInfo for TreasuryStorage {
        const PALLET: &'static str = "Treasury";
        fn storage_name(&self) -> &'static str {
            match self {
                Self::ProposalCount => "ProposalCount",
                Self::Proposals => "Proposals",
                Self::Deactivated => "Deactivated",
                Self::Approvals => "Approvals",
                Self::SpendCount => "SpendCount",
                Self::Spends => "Spends",
            }
        }
    }
    #[doc = "Storage of pallet `Vesting`."]
    pub enum VestingStorage {
        Vesting,
        StorageVersion,
    }
    impl StorageInfo for VestingStorage {
        const PALLET: &'static str = "Vesting";
        fn storage_name(&self) -> &'static str {
            match self {
                Self::Vesting => "Vesting",
                Self::StorageVersion => "StorageVersion",
            }
        }
    }
    #[doc = "Storage of pallet `Whitelist`."]
    pub enum WhitelistStorage {
        WhitelistedCall,
    }
    impl StorageInfo for WhitelistStorage {
        const PALLET: &'static str = "Whitelist";
        fn storage_name(&self) -> &'static str {
            match self {
                Self::WhitelistedCall => "WhitelistedCall",
            }
        }
    }
}
pub mod exports {
    use crate::metadata::runtime_types;
    pub mod system {
        pub use super::runtime_types::frame_system::pallet::Event;
    }
    pub mod grandpa {
        pub use super::runtime_types::pallet_grandpa::pallet::Event;
    }
    pub mod balances {
        pub use super::runtime_types::pallet_balances::pallet::Event;
    }
    pub mod transaction_payment {
        pub use super::runtime_types::pallet_transaction_payment::pallet::Event;
    }
    pub mod session {
        pub use super::runtime_types::pallet_session::pallet::Event;
    }
    pub mod utility {
        pub use super::runtime_types::pallet_utility::pallet::Event;
    }
    pub mod vesting {
        pub use super::runtime_types::pallet_vesting::pallet::Event;
    }
    pub mod bags_list {
        pub use super::runtime_types::pallet_bags_list::pallet::Event;
    }
    pub mod im_online {
        pub use super::runtime_types::pallet_im_online::pallet::Event;
    }
    pub mod staking {
        pub use super::runtime_types::pallet_staking::pallet::pallet::Event;
    }
    pub mod treasury {
        pub use super::runtime_types::pallet_treasury::pallet::Event;
    }
    pub mod conviction_voting {
        pub use super::runtime_types::pallet_conviction_voting::pallet::Event;
    }
    pub mod referenda {
        pub use super::runtime_types::pallet_referenda::pallet::Event1 as Event;
    }
    pub mod fellowship_collective {
        pub use super::runtime_types::pallet_ranked_collective::pallet::Event;
    }
    pub mod fellowship_referenda {
        pub use super::runtime_types::pallet_referenda::pallet::Event2 as Event;
    }
    pub mod whitelist {
        pub use super::runtime_types::pallet_whitelist::pallet::Event;
    }
    pub mod scheduler {
        pub use super::runtime_types::pallet_scheduler::pallet::Event;
    }
    pub mod preimage {
        pub use super::runtime_types::pallet_preimage::pallet::Event;
    }
    pub mod identity {
        pub use super::runtime_types::pallet_identity::pallet::Event;
    }
    pub mod proxy {
        pub use super::runtime_types::pallet_proxy::pallet::Event;
    }
    pub mod multisig {
        pub use super::runtime_types::pallet_multisig::pallet::Event;
    }
    pub mod election_provider_multi_phase {
        pub use super::runtime_types::pallet_election_provider_multi_phase::pallet::Event;
    }
    pub mod offences {
        pub use super::runtime_types::pallet_offences::pallet::Event;
    }
    pub mod bounties {
        pub use super::runtime_types::pallet_bounties::pallet::Event;
    }
    pub mod child_bounties {
        pub use super::runtime_types::pallet_child_bounties::pallet::Event;
    }
    pub mod nomination_pools {
        pub use super::runtime_types::pallet_nomination_pools::pallet::Event;
    }
    pub mod sudo {
        pub use super::runtime_types::pallet_sudo::pallet::Event;
    }
    pub mod gear {
        pub use super::runtime_types::pallet_gear::pallet::Event;
    }
    pub mod staking_rewards {
        pub use super::runtime_types::pallet_gear_staking_rewards::pallet::Event;
    }
    pub mod gear_voucher {
        pub use super::runtime_types::pallet_gear_voucher::pallet::Event;
    }
    pub mod gear_eth_bridge {
        pub use super::runtime_types::pallet_gear_eth_bridge::pallet::Event;
    }
}
