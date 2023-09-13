// This file is part of Gear.
//
// Copyright (C) 2021-2023 Gear Technologies Inc.
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
#[allow(rustdoc::broken_intra_doc_links)] //subxt-codegen produces incorrect docs
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
            pub mod bounded_vec {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct BoundedVec<_0>(pub ::std::vec::Vec<_0>);
            }
            pub mod weak_bounded_vec {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct WeakBoundedVec<_0>(pub ::std::vec::Vec<_0>);
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
                pub mod tokens {
                    use super::runtime_types;
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
                pub mod check_nonce {
                    use super::runtime_types;
                    #[derive(
                        Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode,
                    )]
                    pub struct CheckNonce(#[codec(compact)] pub ::core::primitive::u32);
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
                #[doc = "Contains one variant per dispatchable that can be called by an extrinsic."]
                pub enum Call {
                    #[codec(index = 0)]
                    #[doc = "Make some on-chain remark."]
                    #[doc = ""]
                    #[doc = "## Complexity"]
                    #[doc = "- `O(1)`"]
                    remark {
                        remark: ::std::vec::Vec<::core::primitive::u8>,
                    },
                    #[codec(index = 1)]
                    #[doc = "Set the number of pages in the WebAssembly environment's heap."]
                    set_heap_pages { pages: ::core::primitive::u64 },
                    #[codec(index = 2)]
                    #[doc = "Set the new runtime code."]
                    #[doc = ""]
                    #[doc = "## Complexity"]
                    #[doc = "- `O(C + S)` where `C` length of `code` and `S` complexity of `can_set_code`"]
                    set_code {
                        code: ::std::vec::Vec<::core::primitive::u8>,
                    },
                    #[codec(index = 3)]
                    #[doc = "Set the new runtime code without doing any checks of the given `code`."]
                    #[doc = ""]
                    #[doc = "## Complexity"]
                    #[doc = "- `O(C)` where `C` length of `code`"]
                    set_code_without_checks {
                        code: ::std::vec::Vec<::core::primitive::u8>,
                    },
                    #[codec(index = 4)]
                    #[doc = "Set some items of storage."]
                    set_storage {
                        items: ::std::vec::Vec<(
                            ::std::vec::Vec<::core::primitive::u8>,
                            ::std::vec::Vec<::core::primitive::u8>,
                        )>,
                    },
                    #[codec(index = 5)]
                    #[doc = "Kill some items from storage."]
                    kill_storage {
                        keys: ::std::vec::Vec<::std::vec::Vec<::core::primitive::u8>>,
                    },
                    #[codec(index = 6)]
                    #[doc = "Kill all storage items with a key that starts with the given prefix."]
                    #[doc = ""]
                    #[doc = "**NOTE:** We rely on the Root origin to provide us the number of subkeys under"]
                    #[doc = "the prefix we are removing to accurately calculate the weight of this function."]
                    kill_prefix {
                        prefix: ::std::vec::Vec<::core::primitive::u8>,
                        subkeys: ::core::primitive::u32,
                    },
                    #[codec(index = 7)]
                    #[doc = "Make some on-chain remark and emit event."]
                    remark_with_event {
                        remark: ::std::vec::Vec<::core::primitive::u8>,
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
                        account: ::subxt::utils::AccountId32,
                    },
                    #[codec(index = 4)]
                    #[doc = "An account was reaped."]
                    KilledAccount {
                        account: ::subxt::utils::AccountId32,
                    },
                    #[codec(index = 5)]
                    #[doc = "On on-chain remark happened."]
                    Remarked {
                        sender: ::subxt::utils::AccountId32,
                        hash: ::subxt::utils::H256,
                    },
                }
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub struct AccountInfo<_0, _1> {
                pub nonce: _0,
                pub consumers: _0,
                pub providers: _0,
                pub sufficients: _0,
                pub data: _1,
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub struct EventRecord<_0, _1> {
                pub phase: runtime_types::frame_system::Phase,
                pub event: _0,
                pub topics: ::std::vec::Vec<_1>,
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub struct LastRuntimeUpgradeInfo {
                #[codec(compact)]
                pub spec_version: ::core::primitive::u32,
                pub spec_name: ::std::string::String,
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
                    Reply(runtime_types::gear_core::ids::MessageId),
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
                pub enum MessageWaitedSystemReason {
                    #[codec(index = 0)]
                    ProgramIsNotInitialized,
                }
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
            pub mod paused_program_storage {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct ResumeSession<_0, _1> {
                    pub page_count: _1,
                    pub user: _0,
                    pub program_id: runtime_types::gear_core::ids::ProgramId,
                    pub allocations: ::std::vec::Vec<runtime_types::gear_core::pages::WasmPage>,
                    pub code_hash: runtime_types::gear_core::ids::CodeId,
                    pub end_block: _1,
                }
            }
            pub mod scheduler {
                use super::runtime_types;
                pub mod task {
                    use super::runtime_types;
                    #[derive(
                        Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode,
                    )]
                    pub enum ScheduledTask<_0> {
                        #[codec(index = 0)]
                        PauseProgram(runtime_types::gear_core::ids::ProgramId),
                        #[codec(index = 1)]
                        RemoveCode(runtime_types::gear_core::ids::CodeId),
                        #[codec(index = 2)]
                        RemoveFromMailbox(_0, runtime_types::gear_core::ids::MessageId),
                        #[codec(index = 3)]
                        RemoveFromWaitlist(
                            runtime_types::gear_core::ids::ProgramId,
                            runtime_types::gear_core::ids::MessageId,
                        ),
                        #[codec(index = 4)]
                        RemovePausedProgram(runtime_types::gear_core::ids::ProgramId),
                        #[codec(index = 5)]
                        WakeMessage(
                            runtime_types::gear_core::ids::ProgramId,
                            runtime_types::gear_core::ids::MessageId,
                        ),
                        #[codec(index = 6)]
                        SendDispatch(runtime_types::gear_core::ids::MessageId),
                        #[codec(index = 7)]
                        SendUserMessage {
                            message_id: runtime_types::gear_core::ids::MessageId,
                            to_mailbox: ::core::primitive::bool,
                        },
                        #[codec(index = 8)]
                        RemoveGasReservation(
                            runtime_types::gear_core::ids::ProgramId,
                            runtime_types::gear_core::ids::ReservationId,
                        ),
                        #[codec(index = 9)]
                        RemoveResumeSession(::core::primitive::u128),
                    }
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
            pub struct ActiveProgram<_0> {
                pub allocations: ::std::vec::Vec<runtime_types::gear_core::pages::WasmPage>,
                pub pages_with_data: ::std::vec::Vec<runtime_types::gear_core::pages::GearPage>,
                pub gas_reservation_map: ::subxt::utils::KeyedVec<
                    runtime_types::gear_core::ids::ReservationId,
                    runtime_types::gear_core::reservation::GasReservationSlot,
                >,
                pub code_hash: ::subxt::utils::H256,
                pub code_exports: ::std::vec::Vec<runtime_types::gear_core::message::DispatchKind>,
                pub static_pages: runtime_types::gear_core::pages::WasmPage,
                pub state: runtime_types::gear_common::ProgramState,
                pub expiration_block: _0,
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub struct CodeMetadata {
                pub author: ::subxt::utils::H256,
                #[codec(compact)]
                pub block_number: ::core::primitive::u32,
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub enum GasMultiplier<_0, _1> {
                #[codec(index = 0)]
                ValuePerGas(_0),
                #[codec(index = 1)]
                GasPerValue(_1),
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub enum Program<_0> {
                #[codec(index = 0)]
                Active(runtime_types::gear_common::ActiveProgram<_0>),
                #[codec(index = 1)]
                Exited(runtime_types::gear_core::ids::ProgramId),
                #[codec(index = 2)]
                Terminated(runtime_types::gear_core::ids::ProgramId),
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub enum ProgramState {
                #[codec(index = 0)]
                Uninitialized {
                    message_id: runtime_types::gear_core::ids::MessageId,
                },
                #[codec(index = 1)]
                Initialized,
            }
        }
        pub mod gear_core {
            use super::runtime_types;
            pub mod buffer {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct LimitedVec<_0, _1>(
                    pub ::std::vec::Vec<_0>,
                    #[codec(skip)] pub ::core::marker::PhantomData<_1>,
                );
            }
            pub mod code {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct InstrumentedCode {
                    pub code: ::std::vec::Vec<::core::primitive::u8>,
                    pub original_code_len: ::core::primitive::u32,
                    pub exports: ::std::vec::Vec<runtime_types::gear_core::message::DispatchKind>,
                    pub static_pages: runtime_types::gear_core::pages::WasmPage,
                    pub version: ::core::primitive::u32,
                }
            }
            pub mod ids {
                use super::runtime_types;
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
                pub struct ProgramId(pub [::core::primitive::u8; 32usize]);
                #[derive(
                    Copy, Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode,
                )]
                pub struct ReservationId(pub [::core::primitive::u8; 32usize]);
            }
            pub mod memory {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct PageBuf(
                    pub runtime_types::gear_core::buffer::LimitedVec<::core::primitive::u8, ()>,
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
                        pub to: runtime_types::gear_core::ids::MessageId,
                        pub code: runtime_types::gear_core_errors::simple::ReplyCode,
                    }
                    #[derive(
                        Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode,
                    )]
                    pub struct SignalDetails {
                        pub to: runtime_types::gear_core::ids::MessageId,
                        pub code: runtime_types::gear_core_errors::simple::SignalCode,
                    }
                }
                pub mod context {
                    use super::runtime_types;
                    #[derive(
                        Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode,
                    )]
                    pub struct ContextStore {
                        pub outgoing: ::subxt::utils::KeyedVec<
                            ::core::primitive::u32,
                            ::core::option::Option<
                                runtime_types::gear_core::buffer::LimitedVec<
                                    ::core::primitive::u8,
                                    runtime_types::gear_core::message::PayloadSizeError,
                                >,
                            >,
                        >,
                        pub reply: ::core::option::Option<
                            runtime_types::gear_core::buffer::LimitedVec<
                                ::core::primitive::u8,
                                runtime_types::gear_core::message::PayloadSizeError,
                            >,
                        >,
                        pub initialized: ::std::vec::Vec<runtime_types::gear_core::ids::ProgramId>,
                        pub awaken: ::std::vec::Vec<runtime_types::gear_core::ids::MessageId>,
                        pub reply_sent: ::core::primitive::bool,
                        pub reservation_nonce:
                            runtime_types::gear_core::reservation::ReservationNonce,
                        pub system_reservation: ::core::option::Option<::core::primitive::u64>,
                    }
                }
                pub mod stored {
                    use super::runtime_types;
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
                        pub id: runtime_types::gear_core::ids::MessageId,
                        pub source: runtime_types::gear_core::ids::ProgramId,
                        pub destination: runtime_types::gear_core::ids::ProgramId,
                        pub payload: runtime_types::gear_core::buffer::LimitedVec<
                            ::core::primitive::u8,
                            runtime_types::gear_core::message::PayloadSizeError,
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
                        pub id: runtime_types::gear_core::ids::MessageId,
                        pub source: runtime_types::gear_core::ids::ProgramId,
                        pub destination: runtime_types::gear_core::ids::ProgramId,
                        pub payload: runtime_types::gear_core::buffer::LimitedVec<
                            ::core::primitive::u8,
                            runtime_types::gear_core::message::PayloadSizeError,
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
                        pub id: runtime_types::gear_core::ids::MessageId,
                        pub source: runtime_types::gear_core::ids::ProgramId,
                        pub destination: runtime_types::gear_core::ids::ProgramId,
                        pub payload: runtime_types::gear_core::buffer::LimitedVec<
                            ::core::primitive::u8,
                            runtime_types::gear_core::message::PayloadSizeError,
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
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct PayloadSizeError;
            }
            pub mod pages {
                use super::runtime_types;
                #[derive(
                    ::subxt::ext::codec::CompactAs,
                    Debug,
                    crate::gp::Decode,
                    crate::gp::DecodeAsType,
                    crate::gp::Encode,
                )]
                pub struct GearPage(pub ::core::primitive::u32);
                #[derive(
                    ::subxt::ext::codec::CompactAs,
                    Debug,
                    crate::gp::Decode,
                    crate::gp::DecodeAsType,
                    crate::gp::Encode,
                )]
                pub struct WasmPage(pub ::core::primitive::u32);
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
                    ::subxt::ext::codec::CompactAs,
                    Debug,
                    crate::gp::Decode,
                    crate::gp::DecodeAsType,
                    crate::gp::Encode,
                )]
                pub struct ReservationNonce(pub ::core::primitive::u64);
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
                    #[codec(index = 1)]
                    FailedToCreateProgram(
                        runtime_types::gear_core_errors::simple::SimpleProgramCreationError,
                    ),
                    #[codec(index = 2)]
                    InactiveProgram,
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
                    #[codec(index = 255)]
                    Unsupported,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum SimpleProgramCreationError {
                    #[codec(index = 0)]
                    CodeNotExists,
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
        pub mod gear_runtime {
            use super::runtime_types;
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub enum OriginCaller {
                #[codec(index = 0)]
                system(
                    runtime_types::frame_support::dispatch::RawOrigin<::subxt::utils::AccountId32>,
                ),
                #[codec(index = 1)]
                Void(runtime_types::sp_core::Void),
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub enum ProxyType {
                #[codec(index = 0)]
                Any,
                #[codec(index = 1)]
                NonTransfer,
                #[codec(index = 2)]
                CancelProxy,
                #[codec(index = 3)]
                SudoBalances,
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
                #[codec(index = 9)]
                Proxy(runtime_types::pallet_proxy::pallet::Call),
                #[codec(index = 10)]
                Multisig(runtime_types::pallet_multisig::pallet::Call),
                #[codec(index = 98)]
                ValidatorSet(runtime_types::substrate_validator_set::pallet::Call),
                #[codec(index = 99)]
                Sudo(runtime_types::pallet_sudo::pallet::Call),
                #[codec(index = 104)]
                Gear(runtime_types::pallet_gear::pallet::Call),
                #[codec(index = 106)]
                GearVoucher(runtime_types::pallet_gear_voucher::pallet::Call),
                #[codec(index = 199)]
                GearDebug(runtime_types::pallet_gear_debug::pallet::Call),
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
                #[codec(index = 9)]
                Proxy(runtime_types::pallet_proxy::pallet::Event),
                #[codec(index = 10)]
                Multisig(runtime_types::pallet_multisig::pallet::Event),
                #[codec(index = 98)]
                ValidatorSet(runtime_types::substrate_validator_set::pallet::Event),
                #[codec(index = 99)]
                Sudo(runtime_types::pallet_sudo::pallet::Event),
                #[codec(index = 104)]
                Gear(runtime_types::pallet_gear::pallet::Event),
                #[codec(index = 106)]
                GearVoucher(runtime_types::pallet_gear_voucher::pallet::Event),
                #[codec(index = 199)]
                GearDebug(runtime_types::pallet_gear_debug::pallet::Event),
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub struct SessionKeys {
                pub babe: runtime_types::sp_consensus_babe::app::Public,
                pub grandpa: runtime_types::sp_consensus_grandpa::app::Public,
            }
        }
        pub mod pallet_babe {
            use super::runtime_types;
            pub mod pallet {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "Contains one variant per dispatchable that can be called by an extrinsic."]
                pub enum Call {
                    #[codec(index = 0)]
                    #[doc = "Report authority equivocation/misbehavior. This method will verify"]
                    #[doc = "the equivocation proof and validate the given key ownership proof"]
                    #[doc = "against the extracted offender. If both are valid, the offence will"]
                    #[doc = "be reported."]
                    report_equivocation {
                        equivocation_proof: ::std::boxed::Box<
                            runtime_types::sp_consensus_slots::EquivocationProof<
                                runtime_types::sp_runtime::generic::header::Header<
                                    ::core::primitive::u32,
                                    runtime_types::sp_runtime::traits::BlakeTwo256,
                                >,
                                runtime_types::sp_consensus_babe::app::Public,
                            >,
                        >,
                        key_owner_proof: runtime_types::sp_core::Void,
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
                        equivocation_proof: ::std::boxed::Box<
                            runtime_types::sp_consensus_slots::EquivocationProof<
                                runtime_types::sp_runtime::generic::header::Header<
                                    ::core::primitive::u32,
                                    runtime_types::sp_runtime::traits::BlakeTwo256,
                                >,
                                runtime_types::sp_consensus_babe::app::Public,
                            >,
                        >,
                        key_owner_proof: runtime_types::sp_core::Void,
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
                #[doc = "\n\t\t\tCustom [dispatch errors](https://docs.substrate.io/main-docs/build/events-errors/)\n\t\t\tof this pallet.\n\t\t\t"]
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
        pub mod pallet_balances {
            use super::runtime_types;
            pub mod pallet {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "Contains one variant per dispatchable that can be called by an extrinsic."]
                pub enum Call {
                    #[codec(index = 0)]
                    #[doc = "Transfer some liquid free balance to another account."]
                    #[doc = ""]
                    #[doc = "`transfer` will set the `FreeBalance` of the sender and receiver."]
                    #[doc = "If the sender's account is below the existential deposit as a result"]
                    #[doc = "of the transfer, the account will be reaped."]
                    #[doc = ""]
                    #[doc = "The dispatch origin for this call must be `Signed` by the transactor."]
                    #[doc = ""]
                    #[doc = "## Complexity"]
                    #[doc = "- Dependent on arguments but not critical, given proper implementations for input config"]
                    #[doc = "  types. See related functions below."]
                    #[doc = "- It contains a limited number of reads and writes internally and no complex"]
                    #[doc = "  computation."]
                    #[doc = ""]
                    #[doc = "Related functions:"]
                    #[doc = ""]
                    #[doc = "  - `ensure_can_withdraw` is always called internally but has a bounded complexity."]
                    #[doc = "  - Transferring balances to accounts that did not exist before will cause"]
                    #[doc = "    `T::OnNewAccount::on_new_account` to be called."]
                    #[doc = "  - Removing enough funds from an account will trigger `T::DustRemoval::on_unbalanced`."]
                    #[doc = "  - `transfer_keep_alive` works the same way as `transfer`, but has an additional check"]
                    #[doc = "    that the transfer will not kill the origin account."]
                    transfer {
                        dest: ::subxt::utils::MultiAddress<::subxt::utils::AccountId32, ()>,
                        #[codec(compact)]
                        value: ::core::primitive::u128,
                    },
                    #[codec(index = 1)]
                    #[doc = "Set the balances of a given account."]
                    #[doc = ""]
                    #[doc = "This will alter `FreeBalance` and `ReservedBalance` in storage. it will"]
                    #[doc = "also alter the total issuance of the system (`TotalIssuance`) appropriately."]
                    #[doc = "If the new free or reserved balance is below the existential deposit,"]
                    #[doc = "it will reset the account nonce (`frame_system::AccountNonce`)."]
                    #[doc = ""]
                    #[doc = "The dispatch origin for this call is `root`."]
                    set_balance {
                        who: ::subxt::utils::MultiAddress<::subxt::utils::AccountId32, ()>,
                        #[codec(compact)]
                        new_free: ::core::primitive::u128,
                        #[codec(compact)]
                        new_reserved: ::core::primitive::u128,
                    },
                    #[codec(index = 2)]
                    #[doc = "Exactly as `transfer`, except the origin must be root and the source account may be"]
                    #[doc = "specified."]
                    #[doc = "## Complexity"]
                    #[doc = "- Same as transfer, but additional read and write because the source account is not"]
                    #[doc = "  assumed to be in the overlay."]
                    force_transfer {
                        source: ::subxt::utils::MultiAddress<::subxt::utils::AccountId32, ()>,
                        dest: ::subxt::utils::MultiAddress<::subxt::utils::AccountId32, ()>,
                        #[codec(compact)]
                        value: ::core::primitive::u128,
                    },
                    #[codec(index = 3)]
                    #[doc = "Same as the [`transfer`] call, but with a check that the transfer will not kill the"]
                    #[doc = "origin account."]
                    #[doc = ""]
                    #[doc = "99% of the time you want [`transfer`] instead."]
                    #[doc = ""]
                    #[doc = "[`transfer`]: struct.Pallet.html#method.transfer"]
                    transfer_keep_alive {
                        dest: ::subxt::utils::MultiAddress<::subxt::utils::AccountId32, ()>,
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
                    #[doc = "  keep the sender account alive (true). ## Complexity"]
                    #[doc = "- O(1). Just like transfer, but reading the user's transferable balance first."]
                    transfer_all {
                        dest: ::subxt::utils::MultiAddress<::subxt::utils::AccountId32, ()>,
                        keep_alive: ::core::primitive::bool,
                    },
                    #[codec(index = 5)]
                    #[doc = "Unreserve some balance from a user by force."]
                    #[doc = ""]
                    #[doc = "Can only be called by ROOT."]
                    force_unreserve {
                        who: ::subxt::utils::MultiAddress<::subxt::utils::AccountId32, ()>,
                        amount: ::core::primitive::u128,
                    },
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "\n\t\t\tCustom [dispatch errors](https://docs.substrate.io/main-docs/build/events-errors/)\n\t\t\tof this pallet.\n\t\t\t"]
                pub enum Error {
                    #[codec(index = 0)]
                    #[doc = "Vesting balance too high to send value"]
                    VestingBalance,
                    #[codec(index = 1)]
                    #[doc = "Account liquidity restrictions prevent withdrawal"]
                    LiquidityRestrictions,
                    #[codec(index = 2)]
                    #[doc = "Balance too low to send value."]
                    InsufficientBalance,
                    #[codec(index = 3)]
                    #[doc = "Value too low to create account due to existential deposit"]
                    ExistentialDeposit,
                    #[codec(index = 4)]
                    #[doc = "Transfer/payment would kill account"]
                    KeepAlive,
                    #[codec(index = 5)]
                    #[doc = "A vesting schedule already exists for this account"]
                    ExistingVestingSchedule,
                    #[codec(index = 6)]
                    #[doc = "Beneficiary account must pre-exist"]
                    DeadAccount,
                    #[codec(index = 7)]
                    #[doc = "Number of named reserves exceed MaxReserves"]
                    TooManyReserves,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "\n\t\t\tThe [event](https://docs.substrate.io/main-docs/build/events-errors/) emitted\n\t\t\tby this pallet.\n\t\t\t"]
                pub enum Event {
                    #[codec(index = 0)]
                    #[doc = "An account was created with some free balance."]
                    Endowed {
                        account: ::subxt::utils::AccountId32,
                        free_balance: ::core::primitive::u128,
                    },
                    #[codec(index = 1)]
                    #[doc = "An account was removed whose balance was non-zero but below ExistentialDeposit,"]
                    #[doc = "resulting in an outright loss."]
                    DustLost {
                        account: ::subxt::utils::AccountId32,
                        amount: ::core::primitive::u128,
                    },
                    #[codec(index = 2)]
                    #[doc = "Transfer succeeded."]
                    Transfer {
                        from: ::subxt::utils::AccountId32,
                        to: ::subxt::utils::AccountId32,
                        amount: ::core::primitive::u128,
                    },
                    #[codec(index = 3)]
                    #[doc = "A balance was set by root."]
                    BalanceSet {
                        who: ::subxt::utils::AccountId32,
                        free: ::core::primitive::u128,
                        reserved: ::core::primitive::u128,
                    },
                    #[codec(index = 4)]
                    #[doc = "Some balance was reserved (moved from free to reserved)."]
                    Reserved {
                        who: ::subxt::utils::AccountId32,
                        amount: ::core::primitive::u128,
                    },
                    #[codec(index = 5)]
                    #[doc = "Some balance was unreserved (moved from reserved to free)."]
                    Unreserved {
                        who: ::subxt::utils::AccountId32,
                        amount: ::core::primitive::u128,
                    },
                    #[codec(index = 6)]
                    #[doc = "Some balance was moved from the reserve of the first account to the second account."]
                    #[doc = "Final argument indicates the destination balance type."]
                    ReserveRepatriated {
                        from: ::subxt::utils::AccountId32,
                        to: ::subxt::utils::AccountId32,
                        amount: ::core::primitive::u128,
                        destination_status:
                            runtime_types::frame_support::traits::tokens::misc::BalanceStatus,
                    },
                    #[codec(index = 7)]
                    #[doc = "Some amount was deposited (e.g. for transaction fees)."]
                    Deposit {
                        who: ::subxt::utils::AccountId32,
                        amount: ::core::primitive::u128,
                    },
                    #[codec(index = 8)]
                    #[doc = "Some amount was withdrawn from the account (e.g. for transaction fees)."]
                    Withdraw {
                        who: ::subxt::utils::AccountId32,
                        amount: ::core::primitive::u128,
                    },
                    #[codec(index = 9)]
                    #[doc = "Some amount was removed from the account (e.g. for misbehavior)."]
                    Slashed {
                        who: ::subxt::utils::AccountId32,
                        amount: ::core::primitive::u128,
                    },
                }
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub struct AccountData<_0> {
                pub free: _0,
                pub reserved: _0,
                pub misc_frozen: _0,
                pub fee_frozen: _0,
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub struct BalanceLock<_0> {
                pub id: [::core::primitive::u8; 8usize],
                pub amount: _0,
                pub reasons: runtime_types::pallet_balances::Reasons,
            }
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
        pub mod pallet_gear {
            use super::runtime_types;
            pub mod pallet {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "Contains one variant per dispatchable that can be called by an extrinsic."]
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
                        code: ::std::vec::Vec<::core::primitive::u8>,
                    },
                    #[codec(index = 1)]
                    #[doc = "Creates program initialization request (message), that is scheduled to be run in the same block."]
                    #[doc = ""]
                    #[doc = "There are no guarantees that initialization message will be run in the same block due to block"]
                    #[doc = "gas limit restrictions. For example, when it will be the message's turn, required gas limit for it"]
                    #[doc = "could be more than remaining block gas limit. Therefore, the message processing will be postponed"]
                    #[doc = "until the next block."]
                    #[doc = ""]
                    #[doc = "`ProgramId` is computed as Blake256 hash of concatenated bytes of `code` + `salt`. (todo #512 `code_hash` + `salt`)"]
                    #[doc = "Such `ProgramId` must not exist in the Program Storage at the time of this call."]
                    #[doc = ""]
                    #[doc = "There is the same guarantee here as in `upload_code`. That is, future program's"]
                    #[doc = "`code` and metadata are stored before message was added to the queue and processed."]
                    #[doc = ""]
                    #[doc = "The origin must be Signed and the sender must have sufficient funds to pay"]
                    #[doc = "for `gas` and `value` (in case the latter is being transferred)."]
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
                        code: ::std::vec::Vec<::core::primitive::u8>,
                        salt: ::std::vec::Vec<::core::primitive::u8>,
                        init_payload: ::std::vec::Vec<::core::primitive::u8>,
                        gas_limit: ::core::primitive::u64,
                        value: ::core::primitive::u128,
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
                        code_id: runtime_types::gear_core::ids::CodeId,
                        salt: ::std::vec::Vec<::core::primitive::u8>,
                        init_payload: ::std::vec::Vec<::core::primitive::u8>,
                        gas_limit: ::core::primitive::u64,
                        value: ::core::primitive::u128,
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
                    #[doc = "If `prepaid` flag is set, the transaction fee and the gas cost will be"]
                    #[doc = "charged against a `voucher` that must have been issued for the sender"]
                    #[doc = "in conjunction with the `destination` program. That means that the"]
                    #[doc = "synthetic account corresponding to the (`AccountId`, `ProgramId`) pair must"]
                    #[doc = "exist and have sufficient funds in it. Otherwise, the call is invalidated."]
                    #[doc = ""]
                    #[doc = "Parameters:"]
                    #[doc = "- `destination`: the message destination."]
                    #[doc = "- `payload`: in case of a program destination, parameters of the `handle` function."]
                    #[doc = "- `gas_limit`: maximum amount of gas the program can spend before it is halted."]
                    #[doc = "- `value`: balance to be transferred to the program once it's been created."]
                    #[doc = "- `prepaid`: a flag that indicates whether a voucher should be used."]
                    #[doc = ""]
                    #[doc = "Emits the following events:"]
                    #[doc = "- `DispatchMessageEnqueued(MessageInfo)` when dispatch message is placed in the queue."]
                    send_message {
                        destination: runtime_types::gear_core::ids::ProgramId,
                        payload: ::std::vec::Vec<::core::primitive::u8>,
                        gas_limit: ::core::primitive::u64,
                        value: ::core::primitive::u128,
                        prepaid: ::core::primitive::bool,
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
                    #[doc = ""]
                    #[doc = "If `prepaid` flag is set, the transaction fee and the gas cost will be"]
                    #[doc = "charged against a `voucher` that must have been issued for the sender"]
                    #[doc = "in conjunction with the mailboxed message source program. That means that the"]
                    #[doc = "synthetic account corresponding to the (`AccountId`, `ProgramId`) pair must"]
                    #[doc = "exist and have sufficient funds in it. Otherwise, the call is invalidated."]
                    send_reply {
                        reply_to_id: runtime_types::gear_core::ids::MessageId,
                        payload: ::std::vec::Vec<::core::primitive::u8>,
                        gas_limit: ::core::primitive::u64,
                        value: ::core::primitive::u128,
                        prepaid: ::core::primitive::bool,
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
                        message_id: runtime_types::gear_core::ids::MessageId,
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
                    #[doc = "Pay additional rent for the program."]
                    pay_program_rent {
                        program_id: runtime_types::gear_core::ids::ProgramId,
                        block_count: ::core::primitive::u32,
                    },
                    #[codec(index = 9)]
                    #[doc = "Starts a resume session of the previously paused program."]
                    #[doc = ""]
                    #[doc = "The origin must be Signed."]
                    #[doc = ""]
                    #[doc = "Parameters:"]
                    #[doc = "- `program_id`: id of the program to resume."]
                    #[doc = "- `allocations`: memory allocations of program prior to stop."]
                    #[doc = "- `code_hash`: id of the program binary code."]
                    resume_session_init {
                        program_id: runtime_types::gear_core::ids::ProgramId,
                        allocations: ::std::vec::Vec<runtime_types::gear_core::pages::WasmPage>,
                        code_hash: runtime_types::gear_core::ids::CodeId,
                    },
                    #[codec(index = 10)]
                    #[doc = "Appends memory pages to the resume session."]
                    #[doc = ""]
                    #[doc = "The origin must be Signed and should be the owner of the session."]
                    #[doc = ""]
                    #[doc = "Parameters:"]
                    #[doc = "- `session_id`: id of the resume session."]
                    #[doc = "- `memory_pages`: program memory (or its part) before it was paused."]
                    resume_session_push {
                        session_id: ::core::primitive::u128,
                        memory_pages: ::std::vec::Vec<(
                            runtime_types::gear_core::pages::GearPage,
                            runtime_types::gear_core::memory::PageBuf,
                        )>,
                    },
                    #[codec(index = 11)]
                    #[doc = "Finishes the program resume session."]
                    #[doc = ""]
                    #[doc = "The origin must be Signed and should be the owner of the session."]
                    #[doc = ""]
                    #[doc = "Parameters:"]
                    #[doc = "- `session_id`: id of the resume session."]
                    #[doc = "- `block_count`: the specified period of rent."]
                    resume_session_commit {
                        session_id: ::core::primitive::u128,
                        block_count: ::core::primitive::u32,
                    },
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "\n\t\t\tCustom [dispatch errors](https://docs.substrate.io/main-docs/build/events-errors/)\n\t\t\tof this pallet.\n\t\t\t"]
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
                    #[doc = "Value doesn't cover ExistentialDeposit."]
                    ValueLessThanMinimal,
                    #[codec(index = 11)]
                    #[doc = "Message queue processing is disabled."]
                    MessageQueueProcessingDisabled,
                    #[codec(index = 12)]
                    #[doc = "Block count doesn't cover MinimalResumePeriod."]
                    ResumePeriodLessThanMinimal,
                    #[codec(index = 13)]
                    #[doc = "Program with the specified id is not found."]
                    ProgramNotFound,
                    #[codec(index = 14)]
                    #[doc = "Voucher can't be redeemed"]
                    FailureRedeemingVoucher,
                    #[codec(index = 15)]
                    #[doc = "Gear::run() already included in current block."]
                    GearRunAlreadyInBlock,
                    #[codec(index = 16)]
                    #[doc = "The program rent logic is disabled."]
                    ProgramRentDisabled,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "\n\t\t\tThe [event](https://docs.substrate.io/main-docs/build/events-errors/) emitted\n\t\t\tby this pallet.\n\t\t\t"]
                pub enum Event {
                    #[codec(index = 0)]
                    #[doc = "User sends message to program, which was successfully"]
                    #[doc = "added to the Gear message queue."]
                    MessageQueued {
                        id: runtime_types::gear_core::ids::MessageId,
                        source: ::subxt::utils::AccountId32,
                        destination: runtime_types::gear_core::ids::ProgramId,
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
                        id: runtime_types::gear_core::ids::MessageId,
                        reason: runtime_types::gear_common::event::Reason<
                            runtime_types::gear_common::event::UserMessageReadRuntimeReason,
                            runtime_types::gear_common::event::UserMessageReadSystemReason,
                        >,
                    },
                    #[codec(index = 3)]
                    #[doc = "The result of processing the messages within the block."]
                    MessagesDispatched {
                        total: ::core::primitive::u32,
                        statuses: ::subxt::utils::KeyedVec<
                            runtime_types::gear_core::ids::MessageId,
                            runtime_types::gear_common::event::DispatchStatus,
                        >,
                        state_changes: ::std::vec::Vec<runtime_types::gear_core::ids::ProgramId>,
                    },
                    #[codec(index = 4)]
                    #[doc = "Messages execution delayed (waited) and successfully"]
                    #[doc = "added to gear waitlist."]
                    MessageWaited {
                        id: runtime_types::gear_core::ids::MessageId,
                        origin: ::core::option::Option<
                            runtime_types::gear_common::gas_provider::node::GasNodeId<
                                runtime_types::gear_core::ids::MessageId,
                                runtime_types::gear_core::ids::ReservationId,
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
                        id: runtime_types::gear_core::ids::MessageId,
                        reason: runtime_types::gear_common::event::Reason<
                            runtime_types::gear_common::event::MessageWokenRuntimeReason,
                            runtime_types::gear_common::event::MessageWokenSystemReason,
                        >,
                    },
                    #[codec(index = 6)]
                    #[doc = "Any data related to program codes changed."]
                    CodeChanged {
                        id: runtime_types::gear_core::ids::CodeId,
                        change: runtime_types::gear_common::event::CodeChangeKind<
                            ::core::primitive::u32,
                        >,
                    },
                    #[codec(index = 7)]
                    #[doc = "Any data related to programs changed."]
                    ProgramChanged {
                        id: runtime_types::gear_core::ids::ProgramId,
                        change: runtime_types::gear_common::event::ProgramChangeKind<
                            ::core::primitive::u32,
                        >,
                    },
                    #[codec(index = 8)]
                    #[doc = "The pseudo-inherent extrinsic that runs queue processing rolled back or not executed."]
                    QueueNotProcessed,
                    #[codec(index = 9)]
                    #[doc = "Program resume session has been started."]
                    ProgramResumeSessionStarted {
                        session_id: ::core::primitive::u128,
                        account_id: ::subxt::utils::AccountId32,
                        program_id: runtime_types::gear_core::ids::ProgramId,
                        session_end_block: ::core::primitive::u32,
                    },
                }
            }
            pub mod schedule {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct HostFnWeights {
                    pub alloc: runtime_types::sp_weights::weight_v2::Weight,
                    pub alloc_per_page: runtime_types::sp_weights::weight_v2::Weight,
                    pub free: runtime_types::sp_weights::weight_v2::Weight,
                    pub gr_reserve_gas: runtime_types::sp_weights::weight_v2::Weight,
                    pub gr_unreserve_gas: runtime_types::sp_weights::weight_v2::Weight,
                    pub gr_system_reserve_gas: runtime_types::sp_weights::weight_v2::Weight,
                    pub gr_gas_available: runtime_types::sp_weights::weight_v2::Weight,
                    pub gr_message_id: runtime_types::sp_weights::weight_v2::Weight,
                    pub gr_pay_program_rent: runtime_types::sp_weights::weight_v2::Weight,
                    pub gr_program_id: runtime_types::sp_weights::weight_v2::Weight,
                    pub gr_source: runtime_types::sp_weights::weight_v2::Weight,
                    pub gr_value: runtime_types::sp_weights::weight_v2::Weight,
                    pub gr_value_available: runtime_types::sp_weights::weight_v2::Weight,
                    pub gr_size: runtime_types::sp_weights::weight_v2::Weight,
                    pub gr_read: runtime_types::sp_weights::weight_v2::Weight,
                    pub gr_read_per_byte: runtime_types::sp_weights::weight_v2::Weight,
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
                    pub static_page: runtime_types::sp_weights::weight_v2::Weight,
                    pub mem_grow: runtime_types::sp_weights::weight_v2::Weight,
                    pub parachain_read_heuristic: runtime_types::sp_weights::weight_v2::Weight,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct Schedule {
                    pub limits: runtime_types::pallet_gear::schedule::Limits,
                    pub instruction_weights:
                        runtime_types::pallet_gear::schedule::InstructionWeights,
                    pub host_fn_weights: runtime_types::pallet_gear::schedule::HostFnWeights,
                    pub memory_weights: runtime_types::pallet_gear::schedule::MemoryWeights,
                    pub module_instantiation_per_byte: runtime_types::sp_weights::weight_v2::Weight,
                    pub db_write_per_byte: runtime_types::sp_weights::weight_v2::Weight,
                    pub db_read_per_byte: runtime_types::sp_weights::weight_v2::Weight,
                    pub code_instrumentation_cost: runtime_types::sp_weights::weight_v2::Weight,
                    pub code_instrumentation_byte_cost:
                        runtime_types::sp_weights::weight_v2::Weight,
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
                #[doc = "\n\t\t\tCustom [dispatch errors](https://docs.substrate.io/main-docs/build/events-errors/)\n\t\t\tof this pallet.\n\t\t\t"]
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
                }
            }
        }
        pub mod pallet_gear_debug {
            use super::runtime_types;
            pub mod pallet {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "Contains one variant per dispatchable that can be called by an extrinsic."]
                pub enum Call {
                    #[codec(index = 0)]
                    #[doc = "Turn the debug mode on and off."]
                    #[doc = ""]
                    #[doc = "The origin must be the root."]
                    #[doc = ""]
                    #[doc = "Parameters:"]
                    #[doc = "- `debug_mode_on`: if true, debug mode will be turned on, turned off otherwise."]
                    #[doc = ""]
                    #[doc = "Emits the following events:"]
                    #[doc = "- `DebugMode(debug_mode_on)."]
                    enable_debug_mode {
                        debug_mode_on: ::core::primitive::bool,
                    },
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct DebugData {
                    pub dispatch_queue:
                        ::std::vec::Vec<runtime_types::gear_core::message::stored::StoredDispatch>,
                    pub programs:
                        ::std::vec::Vec<runtime_types::pallet_gear_debug::pallet::ProgramDetails>,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "\n\t\t\tCustom [dispatch errors](https://docs.substrate.io/main-docs/build/events-errors/)\n\t\t\tof this pallet.\n\t\t\t"]
                pub enum Error {}
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "\n\t\t\tThe [event](https://docs.substrate.io/main-docs/build/events-errors/) emitted\n\t\t\tby this pallet.\n\t\t\t"]
                pub enum Event {
                    #[codec(index = 0)]
                    DebugMode(::core::primitive::bool),
                    #[codec(index = 1)]
                    #[doc = "A snapshot of the debug data: programs and message queue ('debug mode' only)"]
                    DebugDataSnapshot(runtime_types::pallet_gear_debug::pallet::DebugData),
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct ProgramDetails {
                    pub id: runtime_types::gear_core::ids::ProgramId,
                    pub state: runtime_types::pallet_gear_debug::pallet::ProgramState,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct ProgramInfo {
                    pub static_pages: runtime_types::gear_core::pages::WasmPage,
                    pub persistent_pages: ::subxt::utils::KeyedVec<
                        runtime_types::gear_core::pages::GearPage,
                        runtime_types::gear_core::memory::PageBuf,
                    >,
                    pub code_hash: ::subxt::utils::H256,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum ProgramState {
                    #[codec(index = 0)]
                    Active(runtime_types::pallet_gear_debug::pallet::ProgramInfo),
                    #[codec(index = 1)]
                    Terminated,
                }
            }
        }
        pub mod pallet_gear_gas {
            use super::runtime_types;
            pub mod pallet {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "\n\t\t\tCustom [dispatch errors](https://docs.substrate.io/main-docs/build/events-errors/)\n\t\t\tof this pallet.\n\t\t\t"]
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
                #[doc = "\n\t\t\tCustom [dispatch errors](https://docs.substrate.io/main-docs/build/events-errors/)\n\t\t\tof this pallet.\n\t\t\t"]
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
                #[doc = "\n\t\t\tCustom [dispatch errors](https://docs.substrate.io/main-docs/build/events-errors/)\n\t\t\tof this pallet.\n\t\t\t"]
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
                    ResumeSessionNotFound,
                    #[codec(index = 5)]
                    NotSessionOwner,
                    #[codec(index = 6)]
                    ResumeSessionFailed,
                    #[codec(index = 7)]
                    ProgramCodeNotFound,
                    #[codec(index = 8)]
                    DuplicateResumeSession,
                }
            }
        }
        pub mod pallet_gear_scheduler {
            use super::runtime_types;
            pub mod pallet {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "\n\t\t\tCustom [dispatch errors](https://docs.substrate.io/main-docs/build/events-errors/)\n\t\t\tof this pallet.\n\t\t\t"]
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
        pub mod pallet_gear_voucher {
            use super::runtime_types;
            pub mod pallet {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "Contains one variant per dispatchable that can be called by an extrinsic."]
                pub enum Call {
                    #[codec(index = 0)]
                    #[doc = "Issue a new voucher for a `user` to be used to pay for sending messages"]
                    #[doc = "to `program_id` program."]
                    #[doc = ""]
                    #[doc = "The dispatch origin for this call must be _Signed_."]
                    #[doc = ""]
                    #[doc = "- `to`: The voucher holder account id."]
                    #[doc = "- `program`: The program id, messages to whom can be paid with the voucher."]
                    #[doc = "NOTE: the fact a program with such id exists in storage is not checked - it's"]
                    #[doc = "a caller's responsibility to ensure the consistency of the input parameters."]
                    #[doc = "- `amount`: The voucher amount."]
                    #[doc = ""]
                    #[doc = "## Complexity"]
                    #[doc = "O(Z + C) where Z is the length of the call and C its execution weight."]
                    issue {
                        to: ::subxt::utils::MultiAddress<::subxt::utils::AccountId32, ()>,
                        program: runtime_types::gear_core::ids::ProgramId,
                        value: ::core::primitive::u128,
                    },
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "\n\t\t\tCustom [dispatch errors](https://docs.substrate.io/main-docs/build/events-errors/)\n\t\t\tof this pallet.\n\t\t\t"]
                pub enum Error {
                    #[codec(index = 0)]
                    FailureToCreateVoucher,
                    #[codec(index = 1)]
                    FailureToRedeemVoucher,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "\n\t\t\tThe [event](https://docs.substrate.io/main-docs/build/events-errors/) emitted\n\t\t\tby this pallet.\n\t\t\t"]
                pub enum Event {
                    #[codec(index = 0)]
                    #[doc = "A new voucher issued."]
                    VoucherIssued {
                        holder: ::subxt::utils::AccountId32,
                        program: runtime_types::gear_core::ids::ProgramId,
                        value: ::core::primitive::u128,
                    },
                }
            }
        }
        pub mod pallet_grandpa {
            use super::runtime_types;
            pub mod pallet {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "Contains one variant per dispatchable that can be called by an extrinsic."]
                pub enum Call {
                    #[codec(index = 0)]
                    #[doc = "Report voter equivocation/misbehavior. This method will verify the"]
                    #[doc = "equivocation proof and validate the given key ownership proof"]
                    #[doc = "against the extracted offender. If both are valid, the offence"]
                    #[doc = "will be reported."]
                    report_equivocation {
                        equivocation_proof: ::std::boxed::Box<
                            runtime_types::sp_consensus_grandpa::EquivocationProof<
                                ::subxt::utils::H256,
                                ::core::primitive::u32,
                            >,
                        >,
                        key_owner_proof: runtime_types::sp_core::Void,
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
                        equivocation_proof: ::std::boxed::Box<
                            runtime_types::sp_consensus_grandpa::EquivocationProof<
                                ::subxt::utils::H256,
                                ::core::primitive::u32,
                            >,
                        >,
                        key_owner_proof: runtime_types::sp_core::Void,
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
                #[doc = "\n\t\t\tCustom [dispatch errors](https://docs.substrate.io/main-docs/build/events-errors/)\n\t\t\tof this pallet.\n\t\t\t"]
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
                #[doc = "\n\t\t\tThe [event](https://docs.substrate.io/main-docs/build/events-errors/) emitted\n\t\t\tby this pallet.\n\t\t\t"]
                pub enum Event {
                    #[codec(index = 0)]
                    #[doc = "New authority set has been applied."]
                    NewAuthorities {
                        authority_set: ::std::vec::Vec<(
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
        pub mod pallet_multisig {
            use super::runtime_types;
            pub mod pallet {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "Contains one variant per dispatchable that can be called by an extrinsic."]
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
                        other_signatories: ::std::vec::Vec<::subxt::utils::AccountId32>,
                        call: ::std::boxed::Box<runtime_types::gear_runtime::RuntimeCall>,
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
                        other_signatories: ::std::vec::Vec<::subxt::utils::AccountId32>,
                        maybe_timepoint: ::core::option::Option<
                            runtime_types::pallet_multisig::Timepoint<::core::primitive::u32>,
                        >,
                        call: ::std::boxed::Box<runtime_types::gear_runtime::RuntimeCall>,
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
                        other_signatories: ::std::vec::Vec<::subxt::utils::AccountId32>,
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
                        other_signatories: ::std::vec::Vec<::subxt::utils::AccountId32>,
                        timepoint:
                            runtime_types::pallet_multisig::Timepoint<::core::primitive::u32>,
                        call_hash: [::core::primitive::u8; 32usize],
                    },
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "\n\t\t\tCustom [dispatch errors](https://docs.substrate.io/main-docs/build/events-errors/)\n\t\t\tof this pallet.\n\t\t\t"]
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
                #[doc = "\n\t\t\tThe [event](https://docs.substrate.io/main-docs/build/events-errors/) emitted\n\t\t\tby this pallet.\n\t\t\t"]
                pub enum Event {
                    #[codec(index = 0)]
                    #[doc = "A new multisig operation has begun."]
                    NewMultisig {
                        approving: ::subxt::utils::AccountId32,
                        multisig: ::subxt::utils::AccountId32,
                        call_hash: [::core::primitive::u8; 32usize],
                    },
                    #[codec(index = 1)]
                    #[doc = "A multisig operation has been approved by someone."]
                    MultisigApproval {
                        approving: ::subxt::utils::AccountId32,
                        timepoint:
                            runtime_types::pallet_multisig::Timepoint<::core::primitive::u32>,
                        multisig: ::subxt::utils::AccountId32,
                        call_hash: [::core::primitive::u8; 32usize],
                    },
                    #[codec(index = 2)]
                    #[doc = "A multisig operation has been executed."]
                    MultisigExecuted {
                        approving: ::subxt::utils::AccountId32,
                        timepoint:
                            runtime_types::pallet_multisig::Timepoint<::core::primitive::u32>,
                        multisig: ::subxt::utils::AccountId32,
                        call_hash: [::core::primitive::u8; 32usize],
                        result:
                            ::core::result::Result<(), runtime_types::sp_runtime::DispatchError>,
                    },
                    #[codec(index = 3)]
                    #[doc = "A multisig operation has been cancelled."]
                    MultisigCancelled {
                        cancelling: ::subxt::utils::AccountId32,
                        timepoint:
                            runtime_types::pallet_multisig::Timepoint<::core::primitive::u32>,
                        multisig: ::subxt::utils::AccountId32,
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
                pub index: _0,
            }
        }
        pub mod pallet_proxy {
            use super::runtime_types;
            pub mod pallet {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "Contains one variant per dispatchable that can be called by an extrinsic."]
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
                        real: ::subxt::utils::MultiAddress<::subxt::utils::AccountId32, ()>,
                        force_proxy_type:
                            ::core::option::Option<runtime_types::gear_runtime::ProxyType>,
                        call: ::std::boxed::Box<runtime_types::gear_runtime::RuntimeCall>,
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
                        delegate: ::subxt::utils::MultiAddress<::subxt::utils::AccountId32, ()>,
                        proxy_type: runtime_types::gear_runtime::ProxyType,
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
                        delegate: ::subxt::utils::MultiAddress<::subxt::utils::AccountId32, ()>,
                        proxy_type: runtime_types::gear_runtime::ProxyType,
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
                        proxy_type: runtime_types::gear_runtime::ProxyType,
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
                        spawner: ::subxt::utils::MultiAddress<::subxt::utils::AccountId32, ()>,
                        proxy_type: runtime_types::gear_runtime::ProxyType,
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
                        real: ::subxt::utils::MultiAddress<::subxt::utils::AccountId32, ()>,
                        call_hash: ::subxt::utils::H256,
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
                        real: ::subxt::utils::MultiAddress<::subxt::utils::AccountId32, ()>,
                        call_hash: ::subxt::utils::H256,
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
                        delegate: ::subxt::utils::MultiAddress<::subxt::utils::AccountId32, ()>,
                        call_hash: ::subxt::utils::H256,
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
                        delegate: ::subxt::utils::MultiAddress<::subxt::utils::AccountId32, ()>,
                        real: ::subxt::utils::MultiAddress<::subxt::utils::AccountId32, ()>,
                        force_proxy_type:
                            ::core::option::Option<runtime_types::gear_runtime::ProxyType>,
                        call: ::std::boxed::Box<runtime_types::gear_runtime::RuntimeCall>,
                    },
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "\n\t\t\tCustom [dispatch errors](https://docs.substrate.io/main-docs/build/events-errors/)\n\t\t\tof this pallet.\n\t\t\t"]
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
                #[doc = "\n\t\t\tThe [event](https://docs.substrate.io/main-docs/build/events-errors/) emitted\n\t\t\tby this pallet.\n\t\t\t"]
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
                        pure: ::subxt::utils::AccountId32,
                        who: ::subxt::utils::AccountId32,
                        proxy_type: runtime_types::gear_runtime::ProxyType,
                        disambiguation_index: ::core::primitive::u16,
                    },
                    #[codec(index = 2)]
                    #[doc = "An announcement was placed to make a call in the future."]
                    Announced {
                        real: ::subxt::utils::AccountId32,
                        proxy: ::subxt::utils::AccountId32,
                        call_hash: ::subxt::utils::H256,
                    },
                    #[codec(index = 3)]
                    #[doc = "A proxy was added."]
                    ProxyAdded {
                        delegator: ::subxt::utils::AccountId32,
                        delegatee: ::subxt::utils::AccountId32,
                        proxy_type: runtime_types::gear_runtime::ProxyType,
                        delay: ::core::primitive::u32,
                    },
                    #[codec(index = 4)]
                    #[doc = "A proxy was removed."]
                    ProxyRemoved {
                        delegator: ::subxt::utils::AccountId32,
                        delegatee: ::subxt::utils::AccountId32,
                        proxy_type: runtime_types::gear_runtime::ProxyType,
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
        pub mod pallet_session {
            use super::runtime_types;
            pub mod pallet {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "Contains one variant per dispatchable that can be called by an extrinsic."]
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
                        keys: runtime_types::gear_runtime::SessionKeys,
                        proof: ::std::vec::Vec<::core::primitive::u8>,
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
                #[doc = "\n\t\t\tThe [event](https://docs.substrate.io/main-docs/build/events-errors/) emitted\n\t\t\tby this pallet.\n\t\t\t"]
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
        pub mod pallet_sudo {
            use super::runtime_types;
            pub mod pallet {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "Contains one variant per dispatchable that can be called by an extrinsic."]
                pub enum Call {
                    #[codec(index = 0)]
                    #[doc = "Authenticates the sudo key and dispatches a function call with `Root` origin."]
                    #[doc = ""]
                    #[doc = "The dispatch origin for this call must be _Signed_."]
                    #[doc = ""]
                    #[doc = "## Complexity"]
                    #[doc = "- O(1)."]
                    sudo {
                        call: ::std::boxed::Box<runtime_types::gear_runtime::RuntimeCall>,
                    },
                    #[codec(index = 1)]
                    #[doc = "Authenticates the sudo key and dispatches a function call with `Root` origin."]
                    #[doc = "This function does not check the weight of the call, and instead allows the"]
                    #[doc = "Sudo user to specify the weight of the call."]
                    #[doc = ""]
                    #[doc = "The dispatch origin for this call must be _Signed_."]
                    #[doc = ""]
                    #[doc = "## Complexity"]
                    #[doc = "- O(1)."]
                    sudo_unchecked_weight {
                        call: ::std::boxed::Box<runtime_types::gear_runtime::RuntimeCall>,
                        weight: runtime_types::sp_weights::weight_v2::Weight,
                    },
                    #[codec(index = 2)]
                    #[doc = "Authenticates the current sudo key and sets the given AccountId (`new`) as the new sudo"]
                    #[doc = "key."]
                    #[doc = ""]
                    #[doc = "The dispatch origin for this call must be _Signed_."]
                    #[doc = ""]
                    #[doc = "## Complexity"]
                    #[doc = "- O(1)."]
                    set_key {
                        new: ::subxt::utils::MultiAddress<::subxt::utils::AccountId32, ()>,
                    },
                    #[codec(index = 3)]
                    #[doc = "Authenticates the sudo key and dispatches a function call with `Signed` origin from"]
                    #[doc = "a given account."]
                    #[doc = ""]
                    #[doc = "The dispatch origin for this call must be _Signed_."]
                    #[doc = ""]
                    #[doc = "## Complexity"]
                    #[doc = "- O(1)."]
                    sudo_as {
                        who: ::subxt::utils::MultiAddress<::subxt::utils::AccountId32, ()>,
                        call: ::std::boxed::Box<runtime_types::gear_runtime::RuntimeCall>,
                    },
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "Error for the Sudo pallet"]
                pub enum Error {
                    #[codec(index = 0)]
                    #[doc = "Sender must be the Sudo account"]
                    RequireSudo,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "\n\t\t\tThe [event](https://docs.substrate.io/main-docs/build/events-errors/) emitted\n\t\t\tby this pallet.\n\t\t\t"]
                pub enum Event {
                    #[codec(index = 0)]
                    #[doc = "A sudo just took place. \\[result\\]"]
                    Sudid {
                        sudo_result:
                            ::core::result::Result<(), runtime_types::sp_runtime::DispatchError>,
                    },
                    #[codec(index = 1)]
                    #[doc = "The \\[sudoer\\] just switched identity; the old key is supplied if one existed."]
                    KeyChanged {
                        old_sudoer: ::core::option::Option<::subxt::utils::AccountId32>,
                    },
                    #[codec(index = 2)]
                    #[doc = "A sudo just took place. \\[result\\]"]
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
                #[doc = "Contains one variant per dispatchable that can be called by an extrinsic."]
                pub enum Call {
                    #[codec(index = 0)]
                    #[doc = "Set the current time."]
                    #[doc = ""]
                    #[doc = "This call should be invoked exactly once per block. It will panic at the finalization"]
                    #[doc = "phase, if this call hasn't been invoked by that time."]
                    #[doc = ""]
                    #[doc = "The timestamp should be greater than the previous one by the amount specified by"]
                    #[doc = "`MinimumPeriod`."]
                    #[doc = ""]
                    #[doc = "The dispatch origin for this call must be `Inherent`."]
                    #[doc = ""]
                    #[doc = "## Complexity"]
                    #[doc = "- `O(1)` (Note that implementations of `OnTimestampSet` must also be `O(1)`)"]
                    #[doc = "- 1 storage read and 1 storage mutation (codec `O(1)`). (because of `DidUpdate::take` in"]
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
                #[doc = "\n\t\t\tThe [event](https://docs.substrate.io/main-docs/build/events-errors/) emitted\n\t\t\tby this pallet.\n\t\t\t"]
                pub enum Event {
                    #[codec(index = 0)]
                    #[doc = "A transaction fee `actual_fee`, of which `tip` was added to the minimum inclusion fee,"]
                    #[doc = "has been paid by `who`."]
                    TransactionFeePaid {
                        who: ::subxt::utils::AccountId32,
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
        pub mod pallet_utility {
            use super::runtime_types;
            pub mod pallet {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "Contains one variant per dispatchable that can be called by an extrinsic."]
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
                        calls: ::std::vec::Vec<runtime_types::gear_runtime::RuntimeCall>,
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
                        call: ::std::boxed::Box<runtime_types::gear_runtime::RuntimeCall>,
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
                        calls: ::std::vec::Vec<runtime_types::gear_runtime::RuntimeCall>,
                    },
                    #[codec(index = 3)]
                    #[doc = "Dispatches a function call with a provided origin."]
                    #[doc = ""]
                    #[doc = "The dispatch origin for this call must be _Root_."]
                    #[doc = ""]
                    #[doc = "## Complexity"]
                    #[doc = "- O(1)."]
                    dispatch_as {
                        as_origin: ::std::boxed::Box<runtime_types::gear_runtime::OriginCaller>,
                        call: ::std::boxed::Box<runtime_types::gear_runtime::RuntimeCall>,
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
                        calls: ::std::vec::Vec<runtime_types::gear_runtime::RuntimeCall>,
                    },
                    #[codec(index = 5)]
                    #[doc = "Dispatch a function call with a specified weight."]
                    #[doc = ""]
                    #[doc = "This function does not check the weight of the call, and instead allows the"]
                    #[doc = "Root origin to specify the weight of the call."]
                    #[doc = ""]
                    #[doc = "The dispatch origin for this call must be _Root_."]
                    with_weight {
                        call: ::std::boxed::Box<runtime_types::gear_runtime::RuntimeCall>,
                        weight: runtime_types::sp_weights::weight_v2::Weight,
                    },
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "\n\t\t\tCustom [dispatch errors](https://docs.substrate.io/main-docs/build/events-errors/)\n\t\t\tof this pallet.\n\t\t\t"]
                pub enum Error {
                    #[codec(index = 0)]
                    #[doc = "Too many calls batched."]
                    TooManyCalls,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "\n\t\t\tThe [event](https://docs.substrate.io/main-docs/build/events-errors/) emitted\n\t\t\tby this pallet.\n\t\t\t"]
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
        pub mod sp_arithmetic {
            use super::runtime_types;
            pub mod fixed_point {
                use super::runtime_types;
                #[derive(
                    ::subxt::ext::codec::CompactAs,
                    Debug,
                    crate::gp::Decode,
                    crate::gp::DecodeAsType,
                    crate::gp::Encode,
                )]
                pub struct FixedU128(pub ::core::primitive::u128);
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
        pub mod sp_consensus_babe {
            use super::runtime_types;
            pub mod app {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct Public(pub runtime_types::sp_core::sr25519::Public);
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
                    pub vrf_output: [::core::primitive::u8; 32usize],
                    pub vrf_proof: [::core::primitive::u8; 64usize],
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
                    pub vrf_output: [::core::primitive::u8; 32usize],
                    pub vrf_proof: [::core::primitive::u8; 64usize],
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
                pub struct Public(pub runtime_types::sp_core::ed25519::Public);
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct Signature(pub runtime_types::sp_core::ed25519::Signature);
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
                ::subxt::ext::codec::CompactAs,
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
            pub mod ecdsa {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct Signature(pub [::core::primitive::u8; 65usize]);
            }
            pub mod ed25519 {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct Public(pub [::core::primitive::u8; 32usize]);
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct Signature(pub [::core::primitive::u8; 64usize]);
            }
            pub mod sr25519 {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct Public(pub [::core::primitive::u8; 32usize]);
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct Signature(pub [::core::primitive::u8; 64usize]);
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub enum Void {}
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
                        pub logs:
                            ::std::vec::Vec<runtime_types::sp_runtime::generic::digest::DigestItem>,
                    }
                    #[derive(
                        Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode,
                    )]
                    pub enum DigestItem {
                        #[codec(index = 6)]
                        PreRuntime(
                            [::core::primitive::u8; 4usize],
                            ::std::vec::Vec<::core::primitive::u8>,
                        ),
                        #[codec(index = 4)]
                        Consensus(
                            [::core::primitive::u8; 4usize],
                            ::std::vec::Vec<::core::primitive::u8>,
                        ),
                        #[codec(index = 5)]
                        Seal(
                            [::core::primitive::u8; 4usize],
                            ::std::vec::Vec<::core::primitive::u8>,
                        ),
                        #[codec(index = 0)]
                        Other(::std::vec::Vec<::core::primitive::u8>),
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
                    pub struct Header<_0, _1> {
                        pub parent_hash: ::subxt::utils::H256,
                        #[codec(compact)]
                        pub number: _0,
                        pub state_root: ::subxt::utils::H256,
                        pub extrinsics_root: ::subxt::utils::H256,
                        pub digest: runtime_types::sp_runtime::generic::digest::Digest,
                        #[codec(skip)]
                        pub __subxt_unused_type_params: ::core::marker::PhantomData<_1>,
                    }
                }
                pub mod unchecked_extrinsic {
                    use super::runtime_types;
                    #[derive(
                        Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode,
                    )]
                    pub struct UncheckedExtrinsic<_0, _1, _2, _3>(
                        pub ::std::vec::Vec<::core::primitive::u8>,
                        #[codec(skip)] pub ::core::marker::PhantomData<(_0, _1, _2, _3)>,
                    );
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
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub struct ModuleError {
                pub index: ::core::primitive::u8,
                pub error: [::core::primitive::u8; 4usize],
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub enum MultiSignature {
                #[codec(index = 0)]
                Ed25519(runtime_types::sp_core::ed25519::Signature),
                #[codec(index = 1)]
                Sr25519(runtime_types::sp_core::sr25519::Signature),
                #[codec(index = 2)]
                Ecdsa(runtime_types::sp_core::ecdsa::Signature),
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub enum TokenError {
                #[codec(index = 0)]
                NoFunds,
                #[codec(index = 1)]
                WouldDie,
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
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub enum TransactionalError {
                #[codec(index = 0)]
                LimitReached,
                #[codec(index = 1)]
                NoLayer,
            }
        }
        pub mod sp_version {
            use super::runtime_types;
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub struct RuntimeVersion {
                pub spec_name: ::std::string::String,
                pub impl_name: ::std::string::String,
                pub authoring_version: ::core::primitive::u32,
                pub spec_version: ::core::primitive::u32,
                pub impl_version: ::core::primitive::u32,
                pub apis:
                    ::std::vec::Vec<([::core::primitive::u8; 8usize], ::core::primitive::u32)>,
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
        pub mod substrate_validator_set {
            use super::runtime_types;
            pub mod pallet {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "Contains one variant per dispatchable that can be called by an extrinsic."]
                pub enum Call {
                    #[codec(index = 0)]
                    #[doc = "Add a new validator."]
                    #[doc = ""]
                    #[doc = "New validator's session keys should be set in Session pallet before"]
                    #[doc = "calling this."]
                    #[doc = ""]
                    #[doc = "The origin can be configured using the `AddRemoveOrigin` type in the"]
                    #[doc = "host runtime. Can also be set to sudo/root."]
                    add_validator {
                        validator_id: ::subxt::utils::AccountId32,
                    },
                    #[codec(index = 1)]
                    #[doc = "Remove a validator."]
                    #[doc = ""]
                    #[doc = "The origin can be configured using the `AddRemoveOrigin` type in the"]
                    #[doc = "host runtime. Can also be set to sudo/root."]
                    remove_validator {
                        validator_id: ::subxt::utils::AccountId32,
                    },
                    #[codec(index = 2)]
                    #[doc = "Add an approved validator again when it comes back online."]
                    #[doc = ""]
                    #[doc = "For this call, the dispatch origin must be the validator itself."]
                    add_validator_again {
                        validator_id: ::subxt::utils::AccountId32,
                    },
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "\n\t\t\tCustom [dispatch errors](https://docs.substrate.io/main-docs/build/events-errors/)\n\t\t\tof this pallet.\n\t\t\t"]
                pub enum Error {
                    #[codec(index = 0)]
                    #[doc = "Target (post-removal) validator count is below the minimum."]
                    TooLowValidatorCount,
                    #[codec(index = 1)]
                    #[doc = "Validator is already in the validator set."]
                    Duplicate,
                    #[codec(index = 2)]
                    #[doc = "Validator is not approved for re-addition."]
                    ValidatorNotApproved,
                    #[codec(index = 3)]
                    #[doc = "Only the validator can add itself back after coming online."]
                    BadOrigin,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "\n\t\t\tThe [event](https://docs.substrate.io/main-docs/build/events-errors/) emitted\n\t\t\tby this pallet.\n\t\t\t"]
                pub enum Event {
                    #[codec(index = 0)]
                    #[doc = "New validator addition initiated. Effective in ~2 sessions."]
                    ValidatorAdditionInitiated(::subxt::utils::AccountId32),
                    #[codec(index = 1)]
                    #[doc = "Validator removal initiated. Effective in ~2 sessions."]
                    ValidatorRemovalInitiated(::subxt::utils::AccountId32),
                }
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
    #[doc = "Calls of pallet `Balances`."]
    pub enum BalancesCall {
        Transfer,
        SetBalance,
        ForceTransfer,
        TransferKeepAlive,
        TransferAll,
        ForceUnreserve,
    }
    impl CallInfo for BalancesCall {
        const PALLET: &'static str = "Balances";
        fn call_name(&self) -> &'static str {
            match self {
                Self::Transfer => "transfer",
                Self::SetBalance => "set_balance",
                Self::ForceTransfer => "force_transfer",
                Self::TransferKeepAlive => "transfer_keep_alive",
                Self::TransferAll => "transfer_all",
                Self::ForceUnreserve => "force_unreserve",
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
        PayProgramRent,
        ResumeSessionInit,
        ResumeSessionPush,
        ResumeSessionCommit,
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
                Self::PayProgramRent => "pay_program_rent",
                Self::ResumeSessionInit => "resume_session_init",
                Self::ResumeSessionPush => "resume_session_push",
                Self::ResumeSessionCommit => "resume_session_commit",
            }
        }
    }
    #[doc = "Calls of pallet `GearDebug`."]
    pub enum GearDebugCall {
        EnableDebugMode,
    }
    impl CallInfo for GearDebugCall {
        const PALLET: &'static str = "GearDebug";
        fn call_name(&self) -> &'static str {
            match self {
                Self::EnableDebugMode => "enable_debug_mode",
            }
        }
    }
    #[doc = "Calls of pallet `GearVoucher`."]
    pub enum GearVoucherCall {
        Issue,
    }
    impl CallInfo for GearVoucherCall {
        const PALLET: &'static str = "GearVoucher";
        fn call_name(&self) -> &'static str {
            match self {
                Self::Issue => "issue",
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
    #[doc = "Calls of pallet `Sudo`."]
    pub enum SudoCall {
        Sudo,
        SudoUncheckedWeight,
        SetKey,
        SudoAs,
    }
    impl CallInfo for SudoCall {
        const PALLET: &'static str = "Sudo";
        fn call_name(&self) -> &'static str {
            match self {
                Self::Sudo => "sudo",
                Self::SudoUncheckedWeight => "sudo_unchecked_weight",
                Self::SetKey => "set_key",
                Self::SudoAs => "sudo_as",
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
    #[doc = "Calls of pallet `ValidatorSet`."]
    pub enum ValidatorSetCall {
        AddValidator,
        RemoveValidator,
        AddValidatorAgain,
    }
    impl CallInfo for ValidatorSetCall {
        const PALLET: &'static str = "ValidatorSet";
        fn call_name(&self) -> &'static str {
            match self {
                Self::AddValidator => "add_validator",
                Self::RemoveValidator => "remove_validator",
                Self::AddValidatorAgain => "add_validator_again",
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
    #[doc = "Storage of pallet `Balances`."]
    pub enum BalancesStorage {
        TotalIssuance,
        InactiveIssuance,
        Account,
        Locks,
        Reserves,
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
        UnusedValue,
    }
    impl StorageInfo for GearBankStorage {
        const PALLET: &'static str = "GearBank";
        fn storage_name(&self) -> &'static str {
            match self {
                Self::Bank => "Bank",
                Self::UnusedValue => "UnusedValue",
            }
        }
    }
    #[doc = "Storage of pallet `GearDebug`."]
    pub enum GearDebugStorage {
        DebugMode,
        RemapId,
        ProgramsMap,
    }
    impl StorageInfo for GearDebugStorage {
        const PALLET: &'static str = "GearDebug";
        fn storage_name(&self) -> &'static str {
            match self {
                Self::DebugMode => "DebugMode",
                Self::RemapId => "RemapId",
                Self::ProgramsMap => "ProgramsMap",
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
        CodeStorage,
        CodeLenStorage,
        OriginalCodeStorage,
        MetadataStorage,
        ProgramStorage,
        MemoryPageStorage,
        WaitingInitStorage,
        PausedProgramStorage,
        ResumeSessionsNonce,
        ResumeSessions,
        SessionMemoryPages,
    }
    impl StorageInfo for GearProgramStorage {
        const PALLET: &'static str = "GearProgram";
        fn storage_name(&self) -> &'static str {
            match self {
                Self::CodeStorage => "CodeStorage",
                Self::CodeLenStorage => "CodeLenStorage",
                Self::OriginalCodeStorage => "OriginalCodeStorage",
                Self::MetadataStorage => "MetadataStorage",
                Self::ProgramStorage => "ProgramStorage",
                Self::MemoryPageStorage => "MemoryPageStorage",
                Self::WaitingInitStorage => "WaitingInitStorage",
                Self::PausedProgramStorage => "PausedProgramStorage",
                Self::ResumeSessionsNonce => "ResumeSessionsNonce",
                Self::ResumeSessions => "ResumeSessions",
                Self::SessionMemoryPages => "SessionMemoryPages",
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
    #[doc = "Storage of pallet `Grandpa`."]
    pub enum GrandpaStorage {
        State,
        PendingChange,
        NextForced,
        Stalled,
        CurrentSetId,
        SetIdSession,
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
    }
    impl StorageInfo for SystemStorage {
        const PALLET: &'static str = "System";
        fn storage_name(&self) -> &'static str {
            match self {
                Self::Account => "Account",
                Self::ExtrinsicCount => "ExtrinsicCount",
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
    #[doc = "Storage of pallet `ValidatorSet`."]
    pub enum ValidatorSetStorage {
        Validators,
        ApprovedValidators,
        OfflineValidators,
    }
    impl StorageInfo for ValidatorSetStorage {
        const PALLET: &'static str = "ValidatorSet";
        fn storage_name(&self) -> &'static str {
            match self {
                Self::Validators => "Validators",
                Self::ApprovedValidators => "ApprovedValidators",
                Self::OfflineValidators => "OfflineValidators",
            }
        }
    }
}
pub mod impls {
    use crate::metadata::Event;
    impl subxt::events::RootEvent for Event {
        fn root_event(
            pallet_bytes: &[u8],
            pallet_name: &str,
            pallet_ty: u32,
            metadata: &subxt::Metadata,
        ) -> Result<Self, subxt::Error> {
            use subxt::metadata::DecodeWithMetadata;
            if pallet_name == "System" {
                return Ok(Event::System(
                    crate::metadata::system::Event::decode_with_metadata(
                        &mut &*pallet_bytes,
                        pallet_ty,
                        metadata,
                    )?,
                ));
            }
            if pallet_name == "Grandpa" {
                return Ok(Event::Grandpa(
                    crate::metadata::grandpa::Event::decode_with_metadata(
                        &mut &*pallet_bytes,
                        pallet_ty,
                        metadata,
                    )?,
                ));
            }
            if pallet_name == "Balances" {
                return Ok(Event::Balances(
                    crate::metadata::balances::Event::decode_with_metadata(
                        &mut &*pallet_bytes,
                        pallet_ty,
                        metadata,
                    )?,
                ));
            }
            if pallet_name == "TransactionPayment" {
                return Ok(Event::TransactionPayment(
                    crate::metadata::transaction_payment::Event::decode_with_metadata(
                        &mut &*pallet_bytes,
                        pallet_ty,
                        metadata,
                    )?,
                ));
            }
            if pallet_name == "Session" {
                return Ok(Event::Session(
                    crate::metadata::session::Event::decode_with_metadata(
                        &mut &*pallet_bytes,
                        pallet_ty,
                        metadata,
                    )?,
                ));
            }
            if pallet_name == "Utility" {
                return Ok(Event::Utility(
                    crate::metadata::utility::Event::decode_with_metadata(
                        &mut &*pallet_bytes,
                        pallet_ty,
                        metadata,
                    )?,
                ));
            }
            if pallet_name == "Proxy" {
                return Ok(Event::Proxy(
                    crate::metadata::proxy::Event::decode_with_metadata(
                        &mut &*pallet_bytes,
                        pallet_ty,
                        metadata,
                    )?,
                ));
            }
            if pallet_name == "Multisig" {
                return Ok(Event::Multisig(
                    crate::metadata::multisig::Event::decode_with_metadata(
                        &mut &*pallet_bytes,
                        pallet_ty,
                        metadata,
                    )?,
                ));
            }
            if pallet_name == "ValidatorSet" {
                return Ok(Event::ValidatorSet(
                    crate::metadata::validator_set::Event::decode_with_metadata(
                        &mut &*pallet_bytes,
                        pallet_ty,
                        metadata,
                    )?,
                ));
            }
            if pallet_name == "Sudo" {
                return Ok(Event::Sudo(
                    crate::metadata::sudo::Event::decode_with_metadata(
                        &mut &*pallet_bytes,
                        pallet_ty,
                        metadata,
                    )?,
                ));
            }
            if pallet_name == "Gear" {
                return Ok(Event::Gear(
                    crate::metadata::gear::Event::decode_with_metadata(
                        &mut &*pallet_bytes,
                        pallet_ty,
                        metadata,
                    )?,
                ));
            }
            if pallet_name == "GearVoucher" {
                return Ok(Event::GearVoucher(
                    crate::metadata::gear_voucher::Event::decode_with_metadata(
                        &mut &*pallet_bytes,
                        pallet_ty,
                        metadata,
                    )?,
                ));
            }
            if pallet_name == "GearDebug" {
                return Ok(Event::GearDebug(
                    crate::metadata::gear_debug::Event::decode_with_metadata(
                        &mut &*pallet_bytes,
                        pallet_ty,
                        metadata,
                    )?,
                ));
            }
            Err(subxt::ext::scale_decode::Error::custom(format!(
                "Pallet name '{}' not found in root Event enum",
                pallet_name
            ))
            .into())
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
    pub mod proxy {
        pub use super::runtime_types::pallet_proxy::pallet::Event;
    }
    pub mod multisig {
        pub use super::runtime_types::pallet_multisig::pallet::Event;
    }
    pub mod validator_set {
        pub use super::runtime_types::substrate_validator_set::pallet::Event;
    }
    pub mod sudo {
        pub use super::runtime_types::pallet_sudo::pallet::Event;
    }
    pub mod gear {
        pub use super::runtime_types::pallet_gear::pallet::Event;
    }
    pub mod gear_voucher {
        pub use super::runtime_types::pallet_gear_voucher::pallet::Event;
    }
    pub mod gear_debug {
        pub use super::runtime_types::pallet_gear_debug::pallet::Event;
    }
}
