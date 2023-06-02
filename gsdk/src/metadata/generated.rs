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
pub mod runtime_types {
    use super::runtime_types as root_mod;
    pub mod runtime_types {
        use super::runtime_types;
        pub mod bounded_collections {
            use super::runtime_types;
            pub mod bounded_vec {
                use super::runtime_types;
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                pub struct BoundedVec<_0>(pub ::std::vec::Vec<_0>);
            }
            pub mod weak_bounded_vec {
                use super::runtime_types;
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                pub struct WeakBoundedVec<_0>(pub ::std::vec::Vec<_0>);
            }
        }
        pub mod finality_grandpa {
            use super::runtime_types;
            #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
            pub struct Equivocation<_0, _1, _2> {
                pub round_number: ::core::primitive::u64,
                pub identity: _0,
                pub first: (_1, _2),
                pub second: (_1, _2),
            }
            #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
            pub struct Precommit<_0, _1> {
                pub target_hash: _0,
                pub target_number: _1,
            }
            #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
            pub struct Prevote<_0, _1> {
                pub target_hash: _0,
                pub target_number: _1,
            }
        }
        pub mod frame_support {
            use super::runtime_types;
            pub mod dispatch {
                use super::runtime_types;
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                pub enum DispatchClass {
                    #[codec(index = 0)]
                    Normal,
                    #[codec(index = 1)]
                    Operational,
                    #[codec(index = 2)]
                    Mandatory,
                }
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                pub struct DispatchInfo {
                    pub weight: runtime_types::sp_weights::weight_v2::Weight,
                    pub class: runtime_types::frame_support::dispatch::DispatchClass,
                    pub pays_fee: runtime_types::frame_support::dispatch::Pays,
                }
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                pub enum Pays {
                    #[codec(index = 0)]
                    Yes,
                    #[codec(index = 1)]
                    No,
                }
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                pub struct PerDispatchClass<_0> {
                    pub normal: _0,
                    pub operational: _0,
                    pub mandatory: _0,
                }
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                pub struct PostDispatchInfo {
                    pub actual_weight:
                        ::core::option::Option<runtime_types::sp_weights::weight_v2::Weight>,
                    pub pays_fee: runtime_types::frame_support::dispatch::Pays,
                }
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
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
                pub mod misc {
                    use super::runtime_types;
                    #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                    pub struct WrapperOpaque<_0>(
                        #[codec(compact)] pub ::core::primitive::u32,
                        pub _0,
                    );
                }
                pub mod preimages {
                    use super::runtime_types;
                    #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                    pub enum Bounded<_0> {
                        #[codec(index = 0)]
                        Legacy {
                            hash: ::subxt::utils::H256,
                        },
                        #[codec(index = 1)]
                        Inline(
                            runtime_types::bounded_collections::bounded_vec::BoundedVec<
                                ::core::primitive::u8,
                            >,
                        ),
                        #[codec(index = 2)]
                        Lookup {
                            hash: ::subxt::utils::H256,
                            len: ::core::primitive::u32,
                        },
                        __Ignore(::core::marker::PhantomData<_0>),
                    }
                }
                pub mod schedule {
                    use super::runtime_types;
                    #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                    pub enum DispatchTime<_0> {
                        #[codec(index = 0)]
                        At(_0),
                        #[codec(index = 1)]
                        After(_0),
                    }
                }
                pub mod tokens {
                    use super::runtime_types;
                    pub mod misc {
                        use super::runtime_types;
                        #[derive(
                            ::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug,
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
            #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
            pub struct PalletId(pub [::core::primitive::u8; 8usize]);
        }
        pub mod frame_system {
            use super::runtime_types;
            pub mod extensions {
                use super::runtime_types;
                pub mod check_genesis {
                    use super::runtime_types;
                    #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                    pub struct CheckGenesis;
                }
                pub mod check_mortality {
                    use super::runtime_types;
                    #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                    pub struct CheckMortality(pub runtime_types::sp_runtime::generic::era::Era);
                }
                pub mod check_non_zero_sender {
                    use super::runtime_types;
                    #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                    pub struct CheckNonZeroSender;
                }
                pub mod check_nonce {
                    use super::runtime_types;
                    #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                    pub struct CheckNonce(#[codec(compact)] pub ::core::primitive::u32);
                }
                pub mod check_spec_version {
                    use super::runtime_types;
                    #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                    pub struct CheckSpecVersion;
                }
                pub mod check_tx_version {
                    use super::runtime_types;
                    #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                    pub struct CheckTxVersion;
                }
                pub mod check_weight {
                    use super::runtime_types;
                    #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                    pub struct CheckWeight;
                }
            }
            pub mod limits {
                use super::runtime_types;
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                pub struct BlockLength {
                    pub max: runtime_types::frame_support::dispatch::PerDispatchClass<
                        ::core::primitive::u32,
                    >,
                }
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                pub struct BlockWeights {
                    pub base_block: runtime_types::sp_weights::weight_v2::Weight,
                    pub max_block: runtime_types::sp_weights::weight_v2::Weight,
                    pub per_class: runtime_types::frame_support::dispatch::PerDispatchClass<
                        runtime_types::frame_system::limits::WeightsPerClass,
                    >,
                }
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
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
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
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
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
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
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
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
                    NewAccount { account: sp_runtime::AccountId32 },
                    #[codec(index = 4)]
                    #[doc = "An account was reaped."]
                    KilledAccount { account: sp_runtime::AccountId32 },
                    #[codec(index = 5)]
                    #[doc = "On on-chain remark happened."]
                    Remarked {
                        sender: sp_runtime::AccountId32,
                        hash: ::subxt::utils::H256,
                    },
                }
            }
            #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
            pub struct AccountInfo<_0, _1> {
                pub nonce: _0,
                pub consumers: _0,
                pub providers: _0,
                pub sufficients: _0,
                pub data: _1,
            }
            #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
            pub struct EventRecord<_0, _1> {
                pub phase: runtime_types::frame_system::Phase,
                pub event: _0,
                pub topics: ::std::vec::Vec<_1>,
            }
            #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
            pub struct LastRuntimeUpgradeInfo {
                #[codec(compact)]
                pub spec_version: ::core::primitive::u32,
                pub spec_name: ::std::string::String,
            }
            #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
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
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
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
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                pub enum DispatchStatus {
                    #[codec(index = 0)]
                    Success,
                    #[codec(index = 1)]
                    Failed,
                    #[codec(index = 2)]
                    NotExecuted,
                }
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
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
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
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
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                pub enum MessageWaitedSystemReason {
                    #[codec(index = 0)]
                    ProgramIsNotInitialized,
                }
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                pub enum MessageWokenRuntimeReason {
                    #[codec(index = 0)]
                    WakeCalled,
                }
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                pub enum MessageWokenSystemReason {
                    #[codec(index = 0)]
                    ProgramGotInitialized,
                    #[codec(index = 1)]
                    TimeoutHasCome,
                    #[codec(index = 2)]
                    OutOfRent,
                }
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
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
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                pub enum Reason<_0, _1> {
                    #[codec(index = 0)]
                    Runtime(_0),
                    #[codec(index = 1)]
                    System(_1),
                }
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                pub enum UserMessageReadRuntimeReason {
                    #[codec(index = 0)]
                    MessageReplied,
                    #[codec(index = 1)]
                    MessageClaimed,
                }
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                pub enum UserMessageReadSystemReason {
                    #[codec(index = 0)]
                    OutOfRent,
                }
            }
            pub mod gas_provider {
                use super::runtime_types;
                pub mod node {
                    use super::runtime_types;
                    #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                    pub struct ChildrenRefs {
                        pub spec_refs: ::core::primitive::u32,
                        pub unspec_refs: ::core::primitive::u32,
                    }
                    #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                    pub enum GasNode<_0, _1, _2> {
                        #[codec(index = 0)]
                        External {
                            id: _0,
                            value: _2,
                            lock: runtime_types::gear_common::gas_provider::node::NodeLock<_2>,
                            system_reserve: _2,
                            refs: runtime_types::gear_common::gas_provider::node::ChildrenRefs,
                            consumed: ::core::primitive::bool,
                        },
                        #[codec(index = 1)]
                        Cut {
                            id: _0,
                            value: _2,
                            lock: runtime_types::gear_common::gas_provider::node::NodeLock<_2>,
                        },
                        #[codec(index = 2)]
                        Reserved {
                            id: _0,
                            value: _2,
                            lock: runtime_types::gear_common::gas_provider::node::NodeLock<_2>,
                            refs: runtime_types::gear_common::gas_provider::node::ChildrenRefs,
                            consumed: ::core::primitive::bool,
                        },
                        #[codec(index = 3)]
                        SpecifiedLocal {
                            parent: _1,
                            value: _2,
                            lock: runtime_types::gear_common::gas_provider::node::NodeLock<_2>,
                            system_reserve: _2,
                            refs: runtime_types::gear_common::gas_provider::node::ChildrenRefs,
                            consumed: ::core::primitive::bool,
                        },
                        #[codec(index = 4)]
                        UnspecifiedLocal {
                            parent: _1,
                            lock: runtime_types::gear_common::gas_provider::node::NodeLock<_2>,
                            system_reserve: _2,
                        },
                    }
                    #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                    pub enum GasNodeId<_0, _1> {
                        #[codec(index = 0)]
                        Node(_0),
                        #[codec(index = 1)]
                        Reservation(_1),
                    }
                    #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                    pub struct NodeLock<_0>(pub [_0; 4usize]);
                }
            }
            pub mod scheduler {
                use super::runtime_types;
                pub mod task {
                    use super::runtime_types;
                    #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
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
                            ::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug,
                        )]
                        pub struct LinkedNode<_0, _1> {
                            pub next: ::core::option::Option<_0>,
                            pub value: _1,
                        }
                    }
                }
                pub mod primitives {
                    use super::runtime_types;
                    #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                    pub struct Interval<_0> {
                        pub start: _0,
                        pub finish: _0,
                    }
                }
            }
            #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
            pub struct ActiveProgram<_0> {
                pub allocations: ::std::vec::Vec<runtime_types::gear_core::memory::WasmPage>,
                pub pages_with_data: ::std::vec::Vec<runtime_types::gear_core::memory::GearPage>,
                pub gas_reservation_map: ::subxt::utils::KeyedVec<
                    runtime_types::gear_core::ids::ReservationId,
                    runtime_types::gear_core::reservation::GasReservationSlot,
                >,
                pub code_hash: ::subxt::utils::H256,
                pub code_exports: ::std::vec::Vec<runtime_types::gear_core::message::DispatchKind>,
                pub static_pages: runtime_types::gear_core::memory::WasmPage,
                pub state: runtime_types::gear_common::ProgramState,
                pub expiration_block: _0,
            }
            #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
            pub struct CodeMetadata {
                pub author: ::subxt::utils::H256,
                #[codec(compact)]
                pub block_number: ::core::primitive::u32,
            }
            #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
            pub enum Program<_0> {
                #[codec(index = 0)]
                Active(runtime_types::gear_common::ActiveProgram<_0>),
                #[codec(index = 1)]
                Exited(runtime_types::gear_core::ids::ProgramId),
                #[codec(index = 2)]
                Terminated(runtime_types::gear_core::ids::ProgramId),
            }
            #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
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
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                pub struct LimitedVec<_0, _1>(
                    pub ::std::vec::Vec<_0>,
                    #[codec(skip)] pub ::core::marker::PhantomData<_1>,
                );
            }
            pub mod code {
                use super::runtime_types;
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                pub struct InstrumentedCode {
                    pub code: ::std::vec::Vec<::core::primitive::u8>,
                    pub original_code_len: ::core::primitive::u32,
                    pub exports: ::std::vec::Vec<runtime_types::gear_core::message::DispatchKind>,
                    pub static_pages: runtime_types::gear_core::memory::WasmPage,
                    pub version: ::core::primitive::u32,
                }
            }
            pub mod ids {
                use super::runtime_types;
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug, Copy)]
                pub struct CodeId(pub [::core::primitive::u8; 32usize]);
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug, Copy)]
                pub struct MessageId(pub [::core::primitive::u8; 32usize]);
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug, Copy)]
                pub struct ProgramId(pub [::core::primitive::u8; 32usize]);
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug, Copy)]
                pub struct ReservationId(pub [::core::primitive::u8; 32usize]);
            }
            pub mod memory {
                use super::runtime_types;
                #[derive(
                    ::subxt::ext::codec::CompactAs,
                    ::subxt::ext::codec::Decode,
                    ::subxt::ext::codec::Encode,
                    Debug,
                )]
                pub struct GearPage(pub ::core::primitive::u32);
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                pub struct PageBuf(
                    pub runtime_types::gear_core::buffer::LimitedVec<::core::primitive::u8, ()>,
                );
                #[derive(
                    ::subxt::ext::codec::CompactAs,
                    ::subxt::ext::codec::Decode,
                    ::subxt::ext::codec::Encode,
                    Debug,
                )]
                pub struct WasmPage(pub ::core::primitive::u32);
            }
            pub mod message {
                use super::runtime_types;
                pub mod common {
                    use super::runtime_types;
                    #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                    pub enum MessageDetails {
                        #[codec(index = 0)]
                        Reply(runtime_types::gear_core::message::common::ReplyDetails),
                        #[codec(index = 1)]
                        Signal(runtime_types::gear_core::message::common::SignalDetails),
                    }
                    #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                    pub struct ReplyDetails {
                        pub reply_to: runtime_types::gear_core::ids::MessageId,
                        pub status_code: ::core::primitive::i32,
                    }
                    #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                    pub struct SignalDetails {
                        pub from: runtime_types::gear_core::ids::MessageId,
                        pub status_code: ::core::primitive::i32,
                    }
                }
                pub mod context {
                    use super::runtime_types;
                    #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
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
                        pub reservation_nonce: ::core::primitive::u64,
                        pub system_reservation: ::core::option::Option<::core::primitive::u64>,
                    }
                }
                pub mod stored {
                    use super::runtime_types;
                    #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                    pub struct StoredDispatch {
                        pub kind: runtime_types::gear_core::message::DispatchKind,
                        pub message: runtime_types::gear_core::message::stored::StoredMessage,
                        pub context: ::core::option::Option<
                            runtime_types::gear_core::message::context::ContextStore,
                        >,
                    }
                    #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
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
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
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
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                pub struct PayloadSizeError;
            }
            pub mod reservation {
                use super::runtime_types;
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                pub struct GasReservationSlot {
                    pub amount: ::core::primitive::u64,
                    pub start: ::core::primitive::u32,
                    pub finish: ::core::primitive::u32,
                }
            }
        }
        pub mod pallet_airdrop {
            use super::runtime_types;
            pub mod pallet {
                use super::runtime_types;
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                #[doc = "Contains one variant per dispatchable that can be called by an extrinsic."]
                pub enum Call {
                    #[codec(index = 0)]
                    #[doc = "Transfer tokens from pre-funded `source` to `dest` account."]
                    #[doc = ""]
                    #[doc = "The origin must be the root."]
                    #[doc = ""]
                    #[doc = "Parameters:"]
                    #[doc = "- `source`: the pre-funded account (i.e. root),"]
                    #[doc = "- `dest`: the beneficiary account,"]
                    #[doc = "- `amount`: the amount of tokens to be minted."]
                    #[doc = ""]
                    #[doc = "Emits the following events:"]
                    #[doc = "- `TokensDeposited{ dest, amount }`"]
                    transfer {
                        source: sp_runtime::AccountId32,
                        dest: sp_runtime::AccountId32,
                        amount: ::core::primitive::u128,
                    },
                }
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                #[doc = "\n\t\t\tThe [event](https://docs.substrate.io/main-docs/build/events-errors/) emitted\n\t\t\tby this pallet.\n\t\t\t"]
                pub enum Event {
                    #[codec(index = 0)]
                    TokensDeposited {
                        account: sp_runtime::AccountId32,
                        amount: ::core::primitive::u128,
                    },
                }
            }
        }
        pub mod pallet_babe {
            use super::runtime_types;
            pub mod pallet {
                use super::runtime_types;
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
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
                        equivocation_proof: ::std::boxed::Box<
                            runtime_types::sp_consensus_slots::EquivocationProof<
                                runtime_types::sp_runtime::generic::header::Header<
                                    ::core::primitive::u32,
                                    runtime_types::sp_runtime::traits::BlakeTwo256,
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
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
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
        pub mod pallet_bags_list {
            use super::runtime_types;
            pub mod list {
                use super::runtime_types;
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                pub struct Bag {
                    pub head: ::core::option::Option<sp_runtime::AccountId32>,
                    pub tail: ::core::option::Option<sp_runtime::AccountId32>,
                }
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
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
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                pub struct Node {
                    pub id: sp_runtime::AccountId32,
                    pub prev: ::core::option::Option<sp_runtime::AccountId32>,
                    pub next: ::core::option::Option<sp_runtime::AccountId32>,
                    pub bag_upper: ::core::primitive::u64,
                    pub score: ::core::primitive::u64,
                }
            }
            pub mod pallet {
                use super::runtime_types;
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                #[doc = "Contains one variant per dispatchable that can be called by an extrinsic."]
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
                        dislocated: sp_runtime::MultiAddress<sp_runtime::AccountId32, ()>,
                    },
                    #[codec(index = 1)]
                    #[doc = "Move the caller's Id directly in front of `lighter`."]
                    #[doc = ""]
                    #[doc = "The dispatch origin for this call must be _Signed_ and can only be called by the Id of"]
                    #[doc = "the account going in front of `lighter`."]
                    #[doc = ""]
                    #[doc = "Only works if"]
                    #[doc = "- both nodes are within the same bag,"]
                    #[doc = "- and `origin` has a greater `Score` than `lighter`."]
                    put_in_front_of {
                        lighter: sp_runtime::MultiAddress<sp_runtime::AccountId32, ()>,
                    },
                }
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                #[doc = "\n\t\t\tCustom [dispatch errors](https://docs.substrate.io/main-docs/build/events-errors/)\n\t\t\tof this pallet.\n\t\t\t"]
                pub enum Error {
                    #[codec(index = 0)]
                    #[doc = "A error in the list interface implementation."]
                    List(runtime_types::pallet_bags_list::list::ListError),
                }
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                #[doc = "\n\t\t\tThe [event](https://docs.substrate.io/main-docs/build/events-errors/) emitted\n\t\t\tby this pallet.\n\t\t\t"]
                pub enum Event {
                    #[codec(index = 0)]
                    #[doc = "Moved an account from one bag to another."]
                    Rebagged {
                        who: sp_runtime::AccountId32,
                        from: ::core::primitive::u64,
                        to: ::core::primitive::u64,
                    },
                    #[codec(index = 1)]
                    #[doc = "Updated the score of some account to the given amount."]
                    ScoreUpdated {
                        who: sp_runtime::AccountId32,
                        new_score: ::core::primitive::u64,
                    },
                }
            }
        }
        pub mod pallet_balances {
            use super::runtime_types;
            pub mod pallet {
                use super::runtime_types;
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
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
                        dest: sp_runtime::MultiAddress<sp_runtime::AccountId32, ()>,
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
                        who: sp_runtime::MultiAddress<sp_runtime::AccountId32, ()>,
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
                        source: sp_runtime::MultiAddress<sp_runtime::AccountId32, ()>,
                        dest: sp_runtime::MultiAddress<sp_runtime::AccountId32, ()>,
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
                        dest: sp_runtime::MultiAddress<sp_runtime::AccountId32, ()>,
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
                        dest: sp_runtime::MultiAddress<sp_runtime::AccountId32, ()>,
                        keep_alive: ::core::primitive::bool,
                    },
                    #[codec(index = 5)]
                    #[doc = "Unreserve some balance from a user by force."]
                    #[doc = ""]
                    #[doc = "Can only be called by ROOT."]
                    force_unreserve {
                        who: sp_runtime::MultiAddress<sp_runtime::AccountId32, ()>,
                        amount: ::core::primitive::u128,
                    },
                }
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
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
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                #[doc = "\n\t\t\tThe [event](https://docs.substrate.io/main-docs/build/events-errors/) emitted\n\t\t\tby this pallet.\n\t\t\t"]
                pub enum Event {
                    #[codec(index = 0)]
                    #[doc = "An account was created with some free balance."]
                    Endowed {
                        account: sp_runtime::AccountId32,
                        free_balance: ::core::primitive::u128,
                    },
                    #[codec(index = 1)]
                    #[doc = "An account was removed whose balance was non-zero but below ExistentialDeposit,"]
                    #[doc = "resulting in an outright loss."]
                    DustLost {
                        account: sp_runtime::AccountId32,
                        amount: ::core::primitive::u128,
                    },
                    #[codec(index = 2)]
                    #[doc = "Transfer succeeded."]
                    Transfer {
                        from: sp_runtime::AccountId32,
                        to: sp_runtime::AccountId32,
                        amount: ::core::primitive::u128,
                    },
                    #[codec(index = 3)]
                    #[doc = "A balance was set by root."]
                    BalanceSet {
                        who: sp_runtime::AccountId32,
                        free: ::core::primitive::u128,
                        reserved: ::core::primitive::u128,
                    },
                    #[codec(index = 4)]
                    #[doc = "Some balance was reserved (moved from free to reserved)."]
                    Reserved {
                        who: sp_runtime::AccountId32,
                        amount: ::core::primitive::u128,
                    },
                    #[codec(index = 5)]
                    #[doc = "Some balance was unreserved (moved from reserved to free)."]
                    Unreserved {
                        who: sp_runtime::AccountId32,
                        amount: ::core::primitive::u128,
                    },
                    #[codec(index = 6)]
                    #[doc = "Some balance was moved from the reserve of the first account to the second account."]
                    #[doc = "Final argument indicates the destination balance type."]
                    ReserveRepatriated {
                        from: sp_runtime::AccountId32,
                        to: sp_runtime::AccountId32,
                        amount: ::core::primitive::u128,
                        destination_status:
                            runtime_types::frame_support::traits::tokens::misc::BalanceStatus,
                    },
                    #[codec(index = 7)]
                    #[doc = "Some amount was deposited (e.g. for transaction fees)."]
                    Deposit {
                        who: sp_runtime::AccountId32,
                        amount: ::core::primitive::u128,
                    },
                    #[codec(index = 8)]
                    #[doc = "Some amount was withdrawn from the account (e.g. for transaction fees)."]
                    Withdraw {
                        who: sp_runtime::AccountId32,
                        amount: ::core::primitive::u128,
                    },
                    #[codec(index = 9)]
                    #[doc = "Some amount was removed from the account (e.g. for misbehavior)."]
                    Slashed {
                        who: sp_runtime::AccountId32,
                        amount: ::core::primitive::u128,
                    },
                }
            }
            #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
            pub struct AccountData<_0> {
                pub free: _0,
                pub reserved: _0,
                pub misc_frozen: _0,
                pub fee_frozen: _0,
            }
            #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
            pub struct BalanceLock<_0> {
                pub id: [::core::primitive::u8; 8usize],
                pub amount: _0,
                pub reasons: runtime_types::pallet_balances::Reasons,
            }
            #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
            pub enum Reasons {
                #[codec(index = 0)]
                Fee,
                #[codec(index = 1)]
                Misc,
                #[codec(index = 2)]
                All,
            }
            #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
            pub struct ReserveData<_0, _1> {
                pub id: _0,
                pub amount: _1,
            }
        }
        pub mod pallet_conviction_voting {
            use super::runtime_types;
            pub mod conviction {
                use super::runtime_types;
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
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
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                #[doc = "Contains one variant per dispatchable that can be called by an extrinsic."]
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
                    #[doc = "  - have no voting activity (if there is, then it will need to be removed/consolidated"]
                    #[doc = "    through `reap_vote` or `unvote`)."]
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
                        to: sp_runtime::MultiAddress<sp_runtime::AccountId32, ()>,
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
                        target: sp_runtime::MultiAddress<sp_runtime::AccountId32, ()>,
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
                        target: sp_runtime::MultiAddress<sp_runtime::AccountId32, ()>,
                        class: ::core::primitive::u16,
                        index: ::core::primitive::u32,
                    },
                }
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                #[doc = "\n\t\t\tCustom [dispatch errors](https://docs.substrate.io/main-docs/build/events-errors/)\n\t\t\tof this pallet.\n\t\t\t"]
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
                    #[doc = "these are removed, either through `unvote` or `reap_vote`."]
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
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                #[doc = "\n\t\t\tThe [event](https://docs.substrate.io/main-docs/build/events-errors/) emitted\n\t\t\tby this pallet.\n\t\t\t"]
                pub enum Event {
                    #[codec(index = 0)]
                    #[doc = "An account has delegated their vote to another account. \\[who, target\\]"]
                    Delegated(sp_runtime::AccountId32, sp_runtime::AccountId32),
                    #[codec(index = 1)]
                    #[doc = "An \\[account\\] has cancelled a previous delegation operation."]
                    Undelegated(sp_runtime::AccountId32),
                }
            }
            pub mod types {
                use super::runtime_types;
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                pub struct Delegations<_0> {
                    pub votes: _0,
                    pub capital: _0,
                }
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                pub struct Tally<_0> {
                    pub ayes: _0,
                    pub nays: _0,
                    pub support: _0,
                }
            }
            pub mod vote {
                use super::runtime_types;
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
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
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                pub struct Casting<_0, _1, _2> {
                    pub votes: runtime_types::bounded_collections::bounded_vec::BoundedVec<(
                        _1,
                        runtime_types::pallet_conviction_voting::vote::AccountVote<_0>,
                    )>,
                    pub delegations:
                        runtime_types::pallet_conviction_voting::types::Delegations<_0>,
                    pub prior: runtime_types::pallet_conviction_voting::vote::PriorLock<_1, _0>,
                    #[codec(skip)]
                    pub __subxt_unused_type_params: ::core::marker::PhantomData<_2>,
                }
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                pub struct Delegating<_0, _1, _2> {
                    pub balance: _0,
                    pub target: _1,
                    pub conviction: runtime_types::pallet_conviction_voting::conviction::Conviction,
                    pub delegations:
                        runtime_types::pallet_conviction_voting::types::Delegations<_0>,
                    pub prior: runtime_types::pallet_conviction_voting::vote::PriorLock<_2, _0>,
                }
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                pub struct PriorLock<_0, _1>(pub _0, pub _1);
                #[derive(
                    ::subxt::ext::codec::CompactAs,
                    ::subxt::ext::codec::Decode,
                    ::subxt::ext::codec::Encode,
                    Debug,
                )]
                pub struct Vote(pub ::core::primitive::u8);
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
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
        pub mod pallet_gear {
            use super::runtime_types;
            pub mod pallet {
                use super::runtime_types;
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
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
                    #[doc = "Parameters:"]
                    #[doc = "- `destination`: the message destination."]
                    #[doc = "- `payload`: in case of a program destination, parameters of the `handle` function."]
                    #[doc = "- `gas_limit`: maximum amount of gas the program can spend before it is halted."]
                    #[doc = "- `value`: balance to be transferred to the program once it's been created."]
                    #[doc = ""]
                    #[doc = "Emits the following events:"]
                    #[doc = "- `DispatchMessageEnqueued(MessageInfo)` when dispatch message is placed in the queue."]
                    send_message {
                        destination: runtime_types::gear_core::ids::ProgramId,
                        payload: ::std::vec::Vec<::core::primitive::u8>,
                        gas_limit: ::core::primitive::u64,
                        value: ::core::primitive::u128,
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
                        reply_to_id: runtime_types::gear_core::ids::MessageId,
                        payload: ::std::vec::Vec<::core::primitive::u8>,
                        gas_limit: ::core::primitive::u64,
                        value: ::core::primitive::u128,
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
                    run,
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
                }
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                #[doc = "\n\t\t\tCustom [dispatch errors](https://docs.substrate.io/main-docs/build/events-errors/)\n\t\t\tof this pallet.\n\t\t\t"]
                pub enum Error {
                    #[codec(index = 0)]
                    #[doc = "Message wasn't found in the mailbox."]
                    MessageNotFound,
                    #[codec(index = 1)]
                    #[doc = "Not enough balance to reserve."]
                    #[doc = ""]
                    #[doc = "Usually occurs when the gas_limit specified is such that the origin account can't afford the message."]
                    InsufficientBalanceForReserve,
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
                    #[doc = "Messages storage corrupted."]
                    MessagesStorageCorrupted,
                    #[codec(index = 12)]
                    #[doc = "Message queue processing is disabled."]
                    MessageQueueProcessingDisabled,
                    #[codec(index = 13)]
                    #[doc = "Program with the specified id is not found."]
                    ProgramNotFound,
                }
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                #[doc = "\n\t\t\tThe [event](https://docs.substrate.io/main-docs/build/events-errors/) emitted\n\t\t\tby this pallet.\n\t\t\t"]
                pub enum Event {
                    #[codec(index = 0)]
                    #[doc = "User sends message to program, which was successfully"]
                    #[doc = "added to the Gear message queue."]
                    MessageQueued {
                        id: runtime_types::gear_core::ids::MessageId,
                        source: sp_runtime::AccountId32,
                        destination: runtime_types::gear_core::ids::ProgramId,
                        entry: runtime_types::gear_common::event::MessageEntry,
                    },
                    #[codec(index = 1)]
                    #[doc = "Somebody sent a message to the user."]
                    UserMessageSent {
                        message: runtime_types::gear_core::message::stored::StoredMessage,
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
                    QueueProcessingReverted,
                }
            }
            pub mod schedule {
                use super::runtime_types;
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                pub struct HostFnWeights {
                    pub alloc: runtime_types::sp_weights::weight_v2::Weight,
                    pub free: runtime_types::sp_weights::weight_v2::Weight,
                    pub gr_reserve_gas: runtime_types::sp_weights::weight_v2::Weight,
                    pub gr_unreserve_gas: runtime_types::sp_weights::weight_v2::Weight,
                    pub gr_system_reserve_gas: runtime_types::sp_weights::weight_v2::Weight,
                    pub gr_gas_available: runtime_types::sp_weights::weight_v2::Weight,
                    pub gr_message_id: runtime_types::sp_weights::weight_v2::Weight,
                    pub gr_origin: runtime_types::sp_weights::weight_v2::Weight,
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
                    pub gr_error: runtime_types::sp_weights::weight_v2::Weight,
                    pub gr_status_code: runtime_types::sp_weights::weight_v2::Weight,
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
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
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
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
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
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
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
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
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
        pub mod pallet_gear_debug {
            use super::runtime_types;
            pub mod pallet {
                use super::runtime_types;
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
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
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                pub struct DebugData {
                    pub dispatch_queue:
                        ::std::vec::Vec<runtime_types::gear_core::message::stored::StoredDispatch>,
                    pub programs:
                        ::std::vec::Vec<runtime_types::pallet_gear_debug::pallet::ProgramDetails>,
                }
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                #[doc = "\n\t\t\tCustom [dispatch errors](https://docs.substrate.io/main-docs/build/events-errors/)\n\t\t\tof this pallet.\n\t\t\t"]
                pub enum Error {}
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                #[doc = "\n\t\t\tThe [event](https://docs.substrate.io/main-docs/build/events-errors/) emitted\n\t\t\tby this pallet.\n\t\t\t"]
                pub enum Event {
                    #[codec(index = 0)]
                    DebugMode(::core::primitive::bool),
                    #[codec(index = 1)]
                    #[doc = "A snapshot of the debug data: programs and message queue ('debug mode' only)"]
                    DebugDataSnapshot(runtime_types::pallet_gear_debug::pallet::DebugData),
                }
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                pub struct ProgramDetails {
                    pub id: runtime_types::gear_core::ids::ProgramId,
                    pub state: runtime_types::pallet_gear_debug::pallet::ProgramState,
                }
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                pub struct ProgramInfo {
                    pub static_pages: runtime_types::gear_core::memory::WasmPage,
                    pub persistent_pages: ::subxt::utils::KeyedVec<
                        runtime_types::gear_core::memory::GearPage,
                        runtime_types::gear_core::memory::PageBuf,
                    >,
                    pub code_hash: ::subxt::utils::H256,
                }
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
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
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
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
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
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
            #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
            pub struct CustomChargeTransactionPayment<_0>(
                pub runtime_types::pallet_transaction_payment::ChargeTransactionPayment,
                #[codec(skip)] pub ::core::marker::PhantomData<_0>,
            );
        }
        pub mod pallet_gear_program {
            use super::runtime_types;
            pub mod pallet {
                use super::runtime_types;
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                #[doc = "\n\t\t\tCustom [dispatch errors](https://docs.substrate.io/main-docs/build/events-errors/)\n\t\t\tof this pallet.\n\t\t\t"]
                pub enum Error {
                    #[codec(index = 0)]
                    DuplicateItem,
                    #[codec(index = 1)]
                    ItemNotFound,
                    #[codec(index = 2)]
                    NotActiveProgram,
                    #[codec(index = 3)]
                    CannotFindDataForPage,
                }
            }
        }
        pub mod pallet_gear_scheduler {
            use super::runtime_types;
            pub mod pallet {
                use super::runtime_types;
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
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
        pub mod pallet_gear_staking_rewards {
            use super::runtime_types;
            pub mod extension {
                use super::runtime_types;
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                pub struct StakingBlackList;
            }
            pub mod pallet {
                use super::runtime_types;
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                #[doc = "Contains one variant per dispatchable that can be called by an extrinsic."]
                pub enum Call {
                    #[codec(index = 0)]
                    refill { value: ::core::primitive::u128 },
                    #[codec(index = 1)]
                    force_refill {
                        from: sp_runtime::MultiAddress<sp_runtime::AccountId32, ()>,
                        value: ::core::primitive::u128,
                    },
                    #[codec(index = 2)]
                    withdraw {
                        to: sp_runtime::MultiAddress<sp_runtime::AccountId32, ()>,
                        value: ::core::primitive::u128,
                    },
                }
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                #[doc = "Error for the staking rewards pallet."]
                pub enum Error {
                    #[codec(index = 0)]
                    #[doc = "Pool not replenished due to error."]
                    FailureToRefillPool,
                    #[codec(index = 1)]
                    #[doc = "Failure to withdraw funds from the rewards pool."]
                    FailureToWithdrawFromPool,
                }
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                #[doc = "\n\t\t\tThe [event](https://docs.substrate.io/main-docs/build/events-errors/) emitted\n\t\t\tby this pallet.\n\t\t\t"]
                pub enum Event {
                    #[codec(index = 0)]
                    #[doc = "Transferred to the pool from an external account."]
                    Refilled { amount: ::core::primitive::u128 },
                    #[codec(index = 1)]
                    #[doc = "Transferred from the pool to an external account."]
                    Withdrawn { amount: ::core::primitive::u128 },
                    #[codec(index = 2)]
                    #[doc = "Burned from the pool."]
                    Burned { amount: ::core::primitive::u128 },
                }
            }
        }
        pub mod pallet_grandpa {
            use super::runtime_types;
            pub mod pallet {
                use super::runtime_types;
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
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
                        equivocation_proof: ::std::boxed::Box<
                            runtime_types::sp_consensus_grandpa::EquivocationProof<
                                ::subxt::utils::H256,
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
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
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
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
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
            #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
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
            #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
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
            pub mod pallet {
                use super::runtime_types;
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
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
                    #[doc = ""]
                    #[doc = "## Complexity"]
                    #[doc = "- `O(R)` where `R` registrar-count (governance-bounded and code-bounded)."]
                    add_registrar {
                        account: sp_runtime::MultiAddress<sp_runtime::AccountId32, ()>,
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
                    #[doc = ""]
                    #[doc = "## Complexity"]
                    #[doc = "- `O(X + X' + R)`"]
                    #[doc = "  - where `X` additional-field-count (deposit-bounded and code-bounded)"]
                    #[doc = "  - where `R` judgements-count (registrar-count-bounded)"]
                    set_identity {
                        info:
                            ::std::boxed::Box<runtime_types::pallet_identity::types::IdentityInfo>,
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
                    #[doc = ""]
                    #[doc = "## Complexity"]
                    #[doc = "- `O(P + S)`"]
                    #[doc = "  - where `P` old-subs-count (hard- and deposit-bounded)."]
                    #[doc = "  - where `S` subs-count (hard- and deposit-bounded)."]
                    set_subs {
                        subs: ::std::vec::Vec<(
                            sp_runtime::AccountId32,
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
                    #[doc = ""]
                    #[doc = "## Complexity"]
                    #[doc = "- `O(R + S + X)`"]
                    #[doc = "  - where `R` registrar-count (governance-bounded)."]
                    #[doc = "  - where `S` subs-count (hard- and deposit-bounded)."]
                    #[doc = "  - where `X` additional-field-count (deposit-bounded and code-bounded)."]
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
                    #[doc = "Self::registrars().get(reg_index).unwrap().fee"]
                    #[doc = "```"]
                    #[doc = ""]
                    #[doc = "Emits `JudgementRequested` if successful."]
                    #[doc = ""]
                    #[doc = "## Complexity"]
                    #[doc = "- `O(R + X)`."]
                    #[doc = "  - where `R` registrar-count (governance-bounded)."]
                    #[doc = "  - where `X` additional-field-count (deposit-bounded and code-bounded)."]
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
                    #[doc = ""]
                    #[doc = "## Complexity"]
                    #[doc = "- `O(R + X)`."]
                    #[doc = "  - where `R` registrar-count (governance-bounded)."]
                    #[doc = "  - where `X` additional-field-count (deposit-bounded and code-bounded)."]
                    cancel_request { reg_index: ::core::primitive::u32 },
                    #[codec(index = 6)]
                    #[doc = "Set the fee required for a judgement to be requested from a registrar."]
                    #[doc = ""]
                    #[doc = "The dispatch origin for this call must be _Signed_ and the sender must be the account"]
                    #[doc = "of the registrar whose index is `index`."]
                    #[doc = ""]
                    #[doc = "- `index`: the index of the registrar whose fee is to be set."]
                    #[doc = "- `fee`: the new fee."]
                    #[doc = ""]
                    #[doc = "## Complexity"]
                    #[doc = "- `O(R)`."]
                    #[doc = "  - where `R` registrar-count (governance-bounded)."]
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
                    #[doc = ""]
                    #[doc = "## Complexity"]
                    #[doc = "- `O(R)`."]
                    #[doc = "  - where `R` registrar-count (governance-bounded)."]
                    set_account_id {
                        #[codec(compact)]
                        index: ::core::primitive::u32,
                        new: sp_runtime::MultiAddress<sp_runtime::AccountId32, ()>,
                    },
                    #[codec(index = 8)]
                    #[doc = "Set the field information for a registrar."]
                    #[doc = ""]
                    #[doc = "The dispatch origin for this call must be _Signed_ and the sender must be the account"]
                    #[doc = "of the registrar whose index is `index`."]
                    #[doc = ""]
                    #[doc = "- `index`: the index of the registrar whose fee is to be set."]
                    #[doc = "- `fields`: the fields that the registrar concerns themselves with."]
                    #[doc = ""]
                    #[doc = "## Complexity"]
                    #[doc = "- `O(R)`."]
                    #[doc = "  - where `R` registrar-count (governance-bounded)."]
                    set_fields {
                        #[codec(compact)]
                        index: ::core::primitive::u32,
                        fields: runtime_types::pallet_identity::types::BitFlags<
                            runtime_types::pallet_identity::types::IdentityField,
                        >,
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
                    #[doc = "- `identity`: The hash of the [`IdentityInfo`] for that the judgement is provided."]
                    #[doc = ""]
                    #[doc = "Emits `JudgementGiven` if successful."]
                    #[doc = ""]
                    #[doc = "## Complexity"]
                    #[doc = "- `O(R + X)`."]
                    #[doc = "  - where `R` registrar-count (governance-bounded)."]
                    #[doc = "  - where `X` additional-field-count (deposit-bounded and code-bounded)."]
                    provide_judgement {
                        #[codec(compact)]
                        reg_index: ::core::primitive::u32,
                        target: sp_runtime::MultiAddress<sp_runtime::AccountId32, ()>,
                        judgement: runtime_types::pallet_identity::types::Judgement<
                            ::core::primitive::u128,
                        >,
                        identity: ::subxt::utils::H256,
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
                    #[doc = ""]
                    #[doc = "## Complexity"]
                    #[doc = "- `O(R + S + X)`"]
                    #[doc = "  - where `R` registrar-count (governance-bounded)."]
                    #[doc = "  - where `S` subs-count (hard- and deposit-bounded)."]
                    #[doc = "  - where `X` additional-field-count (deposit-bounded and code-bounded)."]
                    kill_identity {
                        target: sp_runtime::MultiAddress<sp_runtime::AccountId32, ()>,
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
                        sub: sp_runtime::MultiAddress<sp_runtime::AccountId32, ()>,
                        data: runtime_types::pallet_identity::types::Data,
                    },
                    #[codec(index = 12)]
                    #[doc = "Alter the associated name of the given sub-account."]
                    #[doc = ""]
                    #[doc = "The dispatch origin for this call must be _Signed_ and the sender must have a registered"]
                    #[doc = "sub identity of `sub`."]
                    rename_sub {
                        sub: sp_runtime::MultiAddress<sp_runtime::AccountId32, ()>,
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
                        sub: sp_runtime::MultiAddress<sp_runtime::AccountId32, ()>,
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
                }
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                #[doc = "\n\t\t\tCustom [dispatch errors](https://docs.substrate.io/main-docs/build/events-errors/)\n\t\t\tof this pallet.\n\t\t\t"]
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
                    #[doc = "Too many additional fields."]
                    TooManyFields,
                    #[codec(index = 12)]
                    #[doc = "Maximum amount of registrars reached. Cannot add any more."]
                    TooManyRegistrars,
                    #[codec(index = 13)]
                    #[doc = "Account ID is already named."]
                    AlreadyClaimed,
                    #[codec(index = 14)]
                    #[doc = "Sender is not a sub-account."]
                    NotSub,
                    #[codec(index = 15)]
                    #[doc = "Sub-account isn't owned by sender."]
                    NotOwned,
                    #[codec(index = 16)]
                    #[doc = "The provided judgement was for a different identity."]
                    JudgementForDifferentIdentity,
                    #[codec(index = 17)]
                    #[doc = "Error that occurs when there is an issue paying for judgement."]
                    JudgementPaymentFailed,
                }
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                #[doc = "\n\t\t\tThe [event](https://docs.substrate.io/main-docs/build/events-errors/) emitted\n\t\t\tby this pallet.\n\t\t\t"]
                pub enum Event {
                    #[codec(index = 0)]
                    #[doc = "A name was set or reset (which will remove all judgements)."]
                    IdentitySet { who: sp_runtime::AccountId32 },
                    #[codec(index = 1)]
                    #[doc = "A name was cleared, and the given balance returned."]
                    IdentityCleared {
                        who: sp_runtime::AccountId32,
                        deposit: ::core::primitive::u128,
                    },
                    #[codec(index = 2)]
                    #[doc = "A name was removed and the given balance slashed."]
                    IdentityKilled {
                        who: sp_runtime::AccountId32,
                        deposit: ::core::primitive::u128,
                    },
                    #[codec(index = 3)]
                    #[doc = "A judgement was asked from a registrar."]
                    JudgementRequested {
                        who: sp_runtime::AccountId32,
                        registrar_index: ::core::primitive::u32,
                    },
                    #[codec(index = 4)]
                    #[doc = "A judgement request was retracted."]
                    JudgementUnrequested {
                        who: sp_runtime::AccountId32,
                        registrar_index: ::core::primitive::u32,
                    },
                    #[codec(index = 5)]
                    #[doc = "A judgement was given by a registrar."]
                    JudgementGiven {
                        target: sp_runtime::AccountId32,
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
                        sub: sp_runtime::AccountId32,
                        main: sp_runtime::AccountId32,
                        deposit: ::core::primitive::u128,
                    },
                    #[codec(index = 8)]
                    #[doc = "A sub-identity was removed from an identity and the deposit freed."]
                    SubIdentityRemoved {
                        sub: sp_runtime::AccountId32,
                        main: sp_runtime::AccountId32,
                        deposit: ::core::primitive::u128,
                    },
                    #[codec(index = 9)]
                    #[doc = "A sub-identity was cleared, and the given deposit repatriated from the"]
                    #[doc = "main identity account to the sub-identity account."]
                    SubIdentityRevoked {
                        sub: sp_runtime::AccountId32,
                        main: sp_runtime::AccountId32,
                        deposit: ::core::primitive::u128,
                    },
                }
            }
            pub mod types {
                use super::runtime_types;
                #[derive(
                    ::subxt::ext::codec::CompactAs,
                    ::subxt::ext::codec::Decode,
                    ::subxt::ext::codec::Encode,
                    Debug,
                )]
                pub struct BitFlags<_0>(
                    pub ::core::primitive::u64,
                    #[codec(skip)] pub ::core::marker::PhantomData<_0>,
                );
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
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
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                pub enum IdentityField {
                    #[codec(index = 1)]
                    Display,
                    #[codec(index = 2)]
                    Legal,
                    #[codec(index = 4)]
                    Web,
                    #[codec(index = 8)]
                    Riot,
                    #[codec(index = 16)]
                    Email,
                    #[codec(index = 32)]
                    PgpFingerprint,
                    #[codec(index = 64)]
                    Image,
                    #[codec(index = 128)]
                    Twitter,
                }
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
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
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
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
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                pub struct RegistrarInfo<_0, _1> {
                    pub account: _1,
                    pub fee: _0,
                    pub fields: runtime_types::pallet_identity::types::BitFlags<
                        runtime_types::pallet_identity::types::IdentityField,
                    >,
                }
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                pub struct Registration<_0> {
                    pub judgements: runtime_types::bounded_collections::bounded_vec::BoundedVec<(
                        ::core::primitive::u32,
                        runtime_types::pallet_identity::types::Judgement<_0>,
                    )>,
                    pub deposit: _0,
                    pub info: runtime_types::pallet_identity::types::IdentityInfo,
                }
            }
        }
        pub mod pallet_im_online {
            use super::runtime_types;
            pub mod pallet {
                use super::runtime_types;
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                #[doc = "Contains one variant per dispatchable that can be called by an extrinsic."]
                pub enum Call {
                    #[codec(index = 0)]
                    #[doc = "## Complexity:"]
                    #[doc = "- `O(K + E)` where K is length of `Keys` (heartbeat.validators_len) and E is length of"]
                    #[doc = "  `heartbeat.network_state.external_address`"]
                    #[doc = "  - `O(K)`: decoding of length `K`"]
                    #[doc = "  - `O(E)`: decoding/encoding of length `E`"]
                    heartbeat {
                        heartbeat:
                            runtime_types::pallet_im_online::Heartbeat<::core::primitive::u32>,
                        signature: runtime_types::pallet_im_online::sr25519::app_sr25519::Signature,
                    },
                }
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                #[doc = "\n\t\t\tCustom [dispatch errors](https://docs.substrate.io/main-docs/build/events-errors/)\n\t\t\tof this pallet.\n\t\t\t"]
                pub enum Error {
                    #[codec(index = 0)]
                    #[doc = "Non existent public key."]
                    InvalidKey,
                    #[codec(index = 1)]
                    #[doc = "Duplicated heartbeat."]
                    DuplicatedHeartbeat,
                }
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                #[doc = "\n\t\t\tThe [event](https://docs.substrate.io/main-docs/build/events-errors/) emitted\n\t\t\tby this pallet.\n\t\t\t"]
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
                        offline: ::std::vec::Vec<(
                            sp_runtime::AccountId32,
                            runtime_types::pallet_staking::Exposure<
                                sp_runtime::AccountId32,
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
                    #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                    pub struct Public(pub runtime_types::sp_core::sr25519::Public);
                    #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                    pub struct Signature(pub runtime_types::sp_core::sr25519::Signature);
                }
            }
            #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
            pub struct BoundedOpaqueNetworkState {
                pub peer_id: runtime_types::bounded_collections::weak_bounded_vec::WeakBoundedVec<
                    ::core::primitive::u8,
                >,
                pub external_addresses:
                    runtime_types::bounded_collections::weak_bounded_vec::WeakBoundedVec<
                        runtime_types::bounded_collections::weak_bounded_vec::WeakBoundedVec<
                            ::core::primitive::u8,
                        >,
                    >,
            }
            #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
            pub struct Heartbeat<_0> {
                pub block_number: _0,
                pub network_state: runtime_types::sp_core::offchain::OpaqueNetworkState,
                pub session_index: _0,
                pub authority_index: _0,
                pub validators_len: _0,
            }
        }
        pub mod pallet_multisig {
            use super::runtime_types;
            pub mod pallet {
                use super::runtime_types;
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
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
                        other_signatories: ::std::vec::Vec<sp_runtime::AccountId32>,
                        call: ::std::boxed::Box<runtime_types::vara_runtime::RuntimeCall>,
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
                        other_signatories: ::std::vec::Vec<sp_runtime::AccountId32>,
                        maybe_timepoint: ::core::option::Option<
                            runtime_types::pallet_multisig::Timepoint<::core::primitive::u32>,
                        >,
                        call: ::std::boxed::Box<runtime_types::vara_runtime::RuntimeCall>,
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
                        other_signatories: ::std::vec::Vec<sp_runtime::AccountId32>,
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
                        other_signatories: ::std::vec::Vec<sp_runtime::AccountId32>,
                        timepoint:
                            runtime_types::pallet_multisig::Timepoint<::core::primitive::u32>,
                        call_hash: [::core::primitive::u8; 32usize],
                    },
                }
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
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
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                #[doc = "\n\t\t\tThe [event](https://docs.substrate.io/main-docs/build/events-errors/) emitted\n\t\t\tby this pallet.\n\t\t\t"]
                pub enum Event {
                    #[codec(index = 0)]
                    #[doc = "A new multisig operation has begun."]
                    NewMultisig {
                        approving: sp_runtime::AccountId32,
                        multisig: sp_runtime::AccountId32,
                        call_hash: [::core::primitive::u8; 32usize],
                    },
                    #[codec(index = 1)]
                    #[doc = "A multisig operation has been approved by someone."]
                    MultisigApproval {
                        approving: sp_runtime::AccountId32,
                        timepoint:
                            runtime_types::pallet_multisig::Timepoint<::core::primitive::u32>,
                        multisig: sp_runtime::AccountId32,
                        call_hash: [::core::primitive::u8; 32usize],
                    },
                    #[codec(index = 2)]
                    #[doc = "A multisig operation has been executed."]
                    MultisigExecuted {
                        approving: sp_runtime::AccountId32,
                        timepoint:
                            runtime_types::pallet_multisig::Timepoint<::core::primitive::u32>,
                        multisig: sp_runtime::AccountId32,
                        call_hash: [::core::primitive::u8; 32usize],
                        result:
                            ::core::result::Result<(), runtime_types::sp_runtime::DispatchError>,
                    },
                    #[codec(index = 3)]
                    #[doc = "A multisig operation has been cancelled."]
                    MultisigCancelled {
                        cancelling: sp_runtime::AccountId32,
                        timepoint:
                            runtime_types::pallet_multisig::Timepoint<::core::primitive::u32>,
                        multisig: sp_runtime::AccountId32,
                        call_hash: [::core::primitive::u8; 32usize],
                    },
                }
            }
            #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
            pub struct Multisig<_0, _1, _2> {
                pub when: runtime_types::pallet_multisig::Timepoint<_0>,
                pub deposit: _1,
                pub depositor: _2,
                pub approvals: runtime_types::bounded_collections::bounded_vec::BoundedVec<_2>,
            }
            #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
            pub struct Timepoint<_0> {
                pub height: _0,
                pub index: _0,
            }
        }
        pub mod pallet_preimage {
            use super::runtime_types;
            pub mod pallet {
                use super::runtime_types;
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                #[doc = "Contains one variant per dispatchable that can be called by an extrinsic."]
                pub enum Call {
                    #[codec(index = 0)]
                    #[doc = "Register a preimage on-chain."]
                    #[doc = ""]
                    #[doc = "If the preimage was previously requested, no fees or deposits are taken for providing"]
                    #[doc = "the preimage. Otherwise, a deposit is taken proportional to the size of the preimage."]
                    note_preimage {
                        bytes: ::std::vec::Vec<::core::primitive::u8>,
                    },
                    #[codec(index = 1)]
                    #[doc = "Clear an unrequested preimage from the runtime storage."]
                    #[doc = ""]
                    #[doc = "If `len` is provided, then it will be a much cheaper operation."]
                    #[doc = ""]
                    #[doc = "- `hash`: The hash of the preimage to be removed from the store."]
                    #[doc = "- `len`: The length of the preimage of `hash`."]
                    unnote_preimage { hash: ::subxt::utils::H256 },
                    #[codec(index = 2)]
                    #[doc = "Request a preimage be uploaded to the chain without paying any fees or deposits."]
                    #[doc = ""]
                    #[doc = "If the preimage requests has already been provided on-chain, we unreserve any deposit"]
                    #[doc = "a user may have paid, and take the control of the preimage out of their hands."]
                    request_preimage { hash: ::subxt::utils::H256 },
                    #[codec(index = 3)]
                    #[doc = "Clear a previously made request for a preimage."]
                    #[doc = ""]
                    #[doc = "NOTE: THIS MUST NOT BE CALLED ON `hash` MORE TIMES THAN `request_preimage`."]
                    unrequest_preimage { hash: ::subxt::utils::H256 },
                }
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                #[doc = "\n\t\t\tCustom [dispatch errors](https://docs.substrate.io/main-docs/build/events-errors/)\n\t\t\tof this pallet.\n\t\t\t"]
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
                }
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                #[doc = "\n\t\t\tThe [event](https://docs.substrate.io/main-docs/build/events-errors/) emitted\n\t\t\tby this pallet.\n\t\t\t"]
                pub enum Event {
                    #[codec(index = 0)]
                    #[doc = "A preimage has been noted."]
                    Noted { hash: ::subxt::utils::H256 },
                    #[codec(index = 1)]
                    #[doc = "A preimage has been requested."]
                    Requested { hash: ::subxt::utils::H256 },
                    #[codec(index = 2)]
                    #[doc = "A preimage has ben cleared."]
                    Cleared { hash: ::subxt::utils::H256 },
                }
            }
            #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
            pub enum RequestStatus<_0, _1> {
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
        }
        pub mod pallet_proxy {
            use super::runtime_types;
            pub mod pallet {
                use super::runtime_types;
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
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
                        real: sp_runtime::MultiAddress<sp_runtime::AccountId32, ()>,
                        force_proxy_type:
                            ::core::option::Option<runtime_types::vara_runtime::ProxyType>,
                        call: ::std::boxed::Box<runtime_types::vara_runtime::RuntimeCall>,
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
                        delegate: sp_runtime::MultiAddress<sp_runtime::AccountId32, ()>,
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
                        delegate: sp_runtime::MultiAddress<sp_runtime::AccountId32, ()>,
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
                        spawner: sp_runtime::MultiAddress<sp_runtime::AccountId32, ()>,
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
                        real: sp_runtime::MultiAddress<sp_runtime::AccountId32, ()>,
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
                        real: sp_runtime::MultiAddress<sp_runtime::AccountId32, ()>,
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
                        delegate: sp_runtime::MultiAddress<sp_runtime::AccountId32, ()>,
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
                        delegate: sp_runtime::MultiAddress<sp_runtime::AccountId32, ()>,
                        real: sp_runtime::MultiAddress<sp_runtime::AccountId32, ()>,
                        force_proxy_type:
                            ::core::option::Option<runtime_types::vara_runtime::ProxyType>,
                        call: ::std::boxed::Box<runtime_types::vara_runtime::RuntimeCall>,
                    },
                }
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
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
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
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
                        pure: sp_runtime::AccountId32,
                        who: sp_runtime::AccountId32,
                        proxy_type: runtime_types::vara_runtime::ProxyType,
                        disambiguation_index: ::core::primitive::u16,
                    },
                    #[codec(index = 2)]
                    #[doc = "An announcement was placed to make a call in the future."]
                    Announced {
                        real: sp_runtime::AccountId32,
                        proxy: sp_runtime::AccountId32,
                        call_hash: ::subxt::utils::H256,
                    },
                    #[codec(index = 3)]
                    #[doc = "A proxy was added."]
                    ProxyAdded {
                        delegator: sp_runtime::AccountId32,
                        delegatee: sp_runtime::AccountId32,
                        proxy_type: runtime_types::vara_runtime::ProxyType,
                        delay: ::core::primitive::u32,
                    },
                    #[codec(index = 4)]
                    #[doc = "A proxy was removed."]
                    ProxyRemoved {
                        delegator: sp_runtime::AccountId32,
                        delegatee: sp_runtime::AccountId32,
                        proxy_type: runtime_types::vara_runtime::ProxyType,
                        delay: ::core::primitive::u32,
                    },
                }
            }
            #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
            pub struct Announcement<_0, _1, _2> {
                pub real: _0,
                pub call_hash: _1,
                pub height: _2,
            }
            #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
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
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                #[doc = "Contains one variant per dispatchable that can be called by an extrinsic."]
                pub enum Call {
                    #[codec(index = 0)]
                    #[doc = "Introduce a new member."]
                    #[doc = ""]
                    #[doc = "- `origin`: Must be the `AdminOrigin`."]
                    #[doc = "- `who`: Account of non-member which will become a member."]
                    #[doc = "- `rank`: The rank to give the new member."]
                    #[doc = ""]
                    #[doc = "Weight: `O(1)`"]
                    add_member {
                        who: sp_runtime::MultiAddress<sp_runtime::AccountId32, ()>,
                    },
                    #[codec(index = 1)]
                    #[doc = "Increment the rank of an existing member by one."]
                    #[doc = ""]
                    #[doc = "- `origin`: Must be the `AdminOrigin`."]
                    #[doc = "- `who`: Account of existing member."]
                    #[doc = ""]
                    #[doc = "Weight: `O(1)`"]
                    promote_member {
                        who: sp_runtime::MultiAddress<sp_runtime::AccountId32, ()>,
                    },
                    #[codec(index = 2)]
                    #[doc = "Decrement the rank of an existing member by one. If the member is already at rank zero,"]
                    #[doc = "then they are removed entirely."]
                    #[doc = ""]
                    #[doc = "- `origin`: Must be the `AdminOrigin`."]
                    #[doc = "- `who`: Account of existing member of rank greater than zero."]
                    #[doc = ""]
                    #[doc = "Weight: `O(1)`, less if the member's index is highest in its rank."]
                    demote_member {
                        who: sp_runtime::MultiAddress<sp_runtime::AccountId32, ()>,
                    },
                    #[codec(index = 3)]
                    #[doc = "Remove the member entirely."]
                    #[doc = ""]
                    #[doc = "- `origin`: Must be the `AdminOrigin`."]
                    #[doc = "- `who`: Account of existing member of rank greater than zero."]
                    #[doc = "- `min_rank`: The rank of the member or greater."]
                    #[doc = ""]
                    #[doc = "Weight: `O(min_rank)`."]
                    remove_member {
                        who: sp_runtime::MultiAddress<sp_runtime::AccountId32, ()>,
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
                }
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                #[doc = "\n\t\t\tCustom [dispatch errors](https://docs.substrate.io/main-docs/build/events-errors/)\n\t\t\tof this pallet.\n\t\t\t"]
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
                }
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                #[doc = "\n\t\t\tThe [event](https://docs.substrate.io/main-docs/build/events-errors/) emitted\n\t\t\tby this pallet.\n\t\t\t"]
                pub enum Event {
                    #[codec(index = 0)]
                    #[doc = "A member `who` has been added."]
                    MemberAdded { who: sp_runtime::AccountId32 },
                    #[codec(index = 1)]
                    #[doc = "The member `who`se rank has been changed to the given `rank`."]
                    RankChanged {
                        who: sp_runtime::AccountId32,
                        rank: ::core::primitive::u16,
                    },
                    #[codec(index = 2)]
                    #[doc = "The member `who` of given `rank` has been removed from the collective."]
                    MemberRemoved {
                        who: sp_runtime::AccountId32,
                        rank: ::core::primitive::u16,
                    },
                    #[codec(index = 3)]
                    #[doc = "The member `who` has voted for the `poll` with the given `vote` leading to an updated"]
                    #[doc = "`tally`."]
                    Voted {
                        who: sp_runtime::AccountId32,
                        poll: ::core::primitive::u32,
                        vote: runtime_types::pallet_ranked_collective::VoteRecord,
                        tally: runtime_types::pallet_ranked_collective::Tally,
                    },
                }
            }
            #[derive(
                ::subxt::ext::codec::CompactAs,
                ::subxt::ext::codec::Decode,
                ::subxt::ext::codec::Encode,
                Debug,
            )]
            pub struct MemberRecord {
                pub rank: ::core::primitive::u16,
            }
            #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
            pub struct Tally {
                pub bare_ayes: ::core::primitive::u32,
                pub ayes: ::core::primitive::u32,
                pub nays: ::core::primitive::u32,
            }
            #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
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
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                #[doc = "Contains one variant per dispatchable that can be called by an extrinsic."]
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
                        proposal_origin:
                            ::std::boxed::Box<runtime_types::vara_runtime::OriginCaller>,
                        proposal: runtime_types::frame_support::traits::preimages::Bounded<
                            runtime_types::vara_runtime::RuntimeCall,
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
                        maybe_hash: ::core::option::Option<::subxt::utils::H256>,
                    },
                }
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                #[doc = "\n\t\t\tCustom [dispatch errors](https://docs.substrate.io/main-docs/build/events-errors/)\n\t\t\tof this pallet.\n\t\t\t"]
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
                }
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                #[doc = "\n\t\t\tThe [event](https://docs.substrate.io/main-docs/build/events-errors/) emitted\n\t\t\tby this pallet.\n\t\t\t"]
                pub enum Event {
                    #[codec(index = 0)]
                    #[doc = "A referendum has been submitted."]
                    Submitted {
                        index: ::core::primitive::u32,
                        track: ::core::primitive::u16,
                        proposal: runtime_types::frame_support::traits::preimages::Bounded<
                            runtime_types::vara_runtime::RuntimeCall,
                        >,
                    },
                    #[codec(index = 1)]
                    #[doc = "The decision deposit has been placed."]
                    DecisionDepositPlaced {
                        index: ::core::primitive::u32,
                        who: sp_runtime::AccountId32,
                        amount: ::core::primitive::u128,
                    },
                    #[codec(index = 2)]
                    #[doc = "The decision deposit has been refunded."]
                    DecisionDepositRefunded {
                        index: ::core::primitive::u32,
                        who: sp_runtime::AccountId32,
                        amount: ::core::primitive::u128,
                    },
                    #[codec(index = 3)]
                    #[doc = "A deposit has been slashaed."]
                    DepositSlashed {
                        who: sp_runtime::AccountId32,
                        amount: ::core::primitive::u128,
                    },
                    #[codec(index = 4)]
                    #[doc = "A referendum has moved into the deciding phase."]
                    DecisionStarted {
                        index: ::core::primitive::u32,
                        track: ::core::primitive::u16,
                        proposal: runtime_types::frame_support::traits::preimages::Bounded<
                            runtime_types::vara_runtime::RuntimeCall,
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
                        who: sp_runtime::AccountId32,
                        amount: ::core::primitive::u128,
                    },
                    #[codec(index = 14)]
                    #[doc = "Metadata for a referendum has been set."]
                    MetadataSet {
                        index: ::core::primitive::u32,
                        hash: ::subxt::utils::H256,
                    },
                    #[codec(index = 15)]
                    #[doc = "Metadata for a referendum has been cleared."]
                    MetadataCleared {
                        index: ::core::primitive::u32,
                        hash: ::subxt::utils::H256,
                    },
                }
            }
            pub mod types {
                use super::runtime_types;
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
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
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                pub struct DecidingStatus<_0> {
                    pub since: _0,
                    pub confirming: ::core::option::Option<_0>,
                }
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                pub struct Deposit<_0, _1> {
                    pub who: _0,
                    pub amount: _1,
                }
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
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
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
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
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                pub struct TrackInfo<_0, _1> {
                    pub name: ::std::string::String,
                    pub max_deciding: _1,
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
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                #[doc = "Contains one variant per dispatchable that can be called by an extrinsic."]
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
                        call: ::std::boxed::Box<runtime_types::vara_runtime::RuntimeCall>,
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
                        call: ::std::boxed::Box<runtime_types::vara_runtime::RuntimeCall>,
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
                        call: ::std::boxed::Box<runtime_types::vara_runtime::RuntimeCall>,
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
                        call: ::std::boxed::Box<runtime_types::vara_runtime::RuntimeCall>,
                    },
                }
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                #[doc = "\n\t\t\tCustom [dispatch errors](https://docs.substrate.io/main-docs/build/events-errors/)\n\t\t\tof this pallet.\n\t\t\t"]
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
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
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
                    #[doc = "The call for the provided hash was not found so the task has been aborted."]
                    CallUnavailable {
                        task: (::core::primitive::u32, ::core::primitive::u32),
                        id: ::core::option::Option<[::core::primitive::u8; 32usize]>,
                    },
                    #[codec(index = 4)]
                    #[doc = "The given task was unable to be renewed since the agenda is full at that block."]
                    PeriodicFailed {
                        task: (::core::primitive::u32, ::core::primitive::u32),
                        id: ::core::option::Option<[::core::primitive::u8; 32usize]>,
                    },
                    #[codec(index = 5)]
                    #[doc = "The given task can never be executed since it is overweight."]
                    PermanentlyOverweight {
                        task: (::core::primitive::u32, ::core::primitive::u32),
                        id: ::core::option::Option<[::core::primitive::u8; 32usize]>,
                    },
                }
            }
            #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
            pub struct Scheduled<_0, _1, _2, _3, _4> {
                pub maybe_id: ::core::option::Option<_0>,
                pub priority: ::core::primitive::u8,
                pub call: _1,
                pub maybe_periodic: ::core::option::Option<(_2, _2)>,
                pub origin: _3,
                #[codec(skip)]
                pub __subxt_unused_type_params: ::core::marker::PhantomData<_4>,
            }
        }
        pub mod pallet_session {
            use super::runtime_types;
            pub mod pallet {
                use super::runtime_types;
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
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
                        keys: runtime_types::vara_runtime::SessionKeys,
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
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
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
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
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
        pub mod pallet_staking {
            use super::runtime_types;
            pub mod pallet {
                use super::runtime_types;
                pub mod pallet {
                    use super::runtime_types;
                    #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                    #[doc = "Contains one variant per dispatchable that can be called by an extrinsic."]
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
                        #[doc = "unless the `origin` falls below _existential deposit_ and gets removed as dust."]
                        bond {
                            controller: sp_runtime::MultiAddress<sp_runtime::AccountId32, ()>,
                            #[codec(compact)]
                            value: ::core::primitive::u128,
                            payee: runtime_types::pallet_staking::RewardDestination<
                                sp_runtime::AccountId32,
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
                        #[doc = "This essentially frees up that balance to be used by the stash account to do"]
                        #[doc = "whatever it wants."]
                        #[doc = ""]
                        #[doc = "The dispatch origin for this call must be _Signed_ by the controller."]
                        #[doc = ""]
                        #[doc = "Emits `Withdrawn`."]
                        #[doc = ""]
                        #[doc = "See also [`Call::unbond`]."]
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
                            targets: ::std::vec::Vec<
                                sp_runtime::MultiAddress<sp_runtime::AccountId32, ()>,
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
                                sp_runtime::AccountId32,
                            >,
                        },
                        #[codec(index = 8)]
                        #[doc = "(Re-)set the controller of a stash."]
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
                        set_controller {
                            controller: sp_runtime::MultiAddress<sp_runtime::AccountId32, ()>,
                        },
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
                        #[doc = "Increments the ideal number of validators upto maximum of"]
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
                        #[doc = "Scale up the ideal number of validators by a factor upto maximum of"]
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
                            invulnerables: ::std::vec::Vec<sp_runtime::AccountId32>,
                        },
                        #[codec(index = 15)]
                        #[doc = "Force a current staker to become completely unstaked, immediately."]
                        #[doc = ""]
                        #[doc = "The dispatch origin must be Root."]
                        force_unstake {
                            stash: sp_runtime::AccountId32,
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
                            slash_indices: ::std::vec::Vec<::core::primitive::u32>,
                        },
                        #[codec(index = 18)]
                        #[doc = "Pay out all the stakers behind a single validator for a single era."]
                        #[doc = ""]
                        #[doc = "- `validator_stash` is the stash account of the validator. Their nominators, up to"]
                        #[doc = "  `T::MaxNominatorRewardedPerValidator`, will also receive their rewards."]
                        #[doc = "- `era` may be any era between `[current_era - history_depth; current_era]`."]
                        #[doc = ""]
                        #[doc = "The origin of this call must be _Signed_. Any account can call this function, even if"]
                        #[doc = "it is not one of the stakers."]
                        #[doc = ""]
                        #[doc = "## Complexity"]
                        #[doc = "- At most O(MaxNominatorRewardedPerValidator)."]
                        payout_stakers {
                            validator_stash: sp_runtime::AccountId32,
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
                        #[doc = ""]
                        #[doc = "The former can happen in cases like a slash; the latter when a fully unbonded account"]
                        #[doc = "is still receiving staking rewards in `RewardDestination::Staked`."]
                        #[doc = ""]
                        #[doc = "It can be called by anyone, as long as `stash` meets the above requirements."]
                        #[doc = ""]
                        #[doc = "Refunds the transaction fees upon successful execution."]
                        reap_stash {
                            stash: sp_runtime::AccountId32,
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
                            who: ::std::vec::Vec<
                                sp_runtime::MultiAddress<sp_runtime::AccountId32, ()>,
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
                        chill_other { controller: sp_runtime::AccountId32 },
                        #[codec(index = 24)]
                        #[doc = "Force a validator to have at least the minimum commission. This will not affect a"]
                        #[doc = "validator who already has a commission greater than or equal to the minimum. Any account"]
                        #[doc = "can call this."]
                        force_apply_min_commission {
                            validator_stash: sp_runtime::AccountId32,
                        },
                        #[codec(index = 25)]
                        #[doc = "Sets the minimum amount of commission that each validators must maintain."]
                        #[doc = ""]
                        #[doc = "This call has lower privilege requirements than `set_staking_config` and can be called"]
                        #[doc = "by the `T::AdminOrigin`. Root can always call this."]
                        set_min_commission {
                            new: runtime_types::sp_arithmetic::per_things::Perbill,
                        },
                    }
                    #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                    pub enum ConfigOp<_0> {
                        #[codec(index = 0)]
                        Noop,
                        #[codec(index = 1)]
                        Set(_0),
                        #[codec(index = 2)]
                        Remove,
                    }
                    #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                    #[doc = "\n\t\t\tCustom [dispatch errors](https://docs.substrate.io/main-docs/build/events-errors/)\n\t\t\tof this pallet.\n\t\t\t"]
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
                        #[doc = "Incorrect previous history depth input provided."]
                        IncorrectHistoryDepth,
                        #[codec(index = 16)]
                        #[doc = "Incorrect number of slashing spans provided."]
                        IncorrectSlashingSpans,
                        #[codec(index = 17)]
                        #[doc = "Internal state has become somehow corrupted and the operation cannot continue."]
                        BadState,
                        #[codec(index = 18)]
                        #[doc = "Too many nomination targets supplied."]
                        TooManyTargets,
                        #[codec(index = 19)]
                        #[doc = "A nomination target was supplied that was blocked or otherwise not a validator."]
                        BadTarget,
                        #[codec(index = 20)]
                        #[doc = "The user has enough bond and thus cannot be chilled forcefully by an external person."]
                        CannotChillOther,
                        #[codec(index = 21)]
                        #[doc = "There are too many nominators in the system. Governance needs to adjust the staking"]
                        #[doc = "settings to keep things safe for the runtime."]
                        TooManyNominators,
                        #[codec(index = 22)]
                        #[doc = "There are too many validator candidates in the system. Governance needs to adjust the"]
                        #[doc = "staking settings to keep things safe for the runtime."]
                        TooManyValidators,
                        #[codec(index = 23)]
                        #[doc = "Commission is too low. Must be at least `MinCommission`."]
                        CommissionTooLow,
                        #[codec(index = 24)]
                        #[doc = "Some bound is not met."]
                        BoundNotMet,
                    }
                    #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                    #[doc = "\n\t\t\tThe [event](https://docs.substrate.io/main-docs/build/events-errors/) emitted\n\t\t\tby this pallet.\n\t\t\t"]
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
                        #[doc = "The nominator has been rewarded by this amount."]
                        Rewarded {
                            stash: sp_runtime::AccountId32,
                            amount: ::core::primitive::u128,
                        },
                        #[codec(index = 2)]
                        #[doc = "A staker (validator or nominator) has been slashed by the given amount."]
                        Slashed {
                            staker: sp_runtime::AccountId32,
                            amount: ::core::primitive::u128,
                        },
                        #[codec(index = 3)]
                        #[doc = "A slash for the given validator, for the given percentage of their stake, at the given"]
                        #[doc = "era as been reported."]
                        SlashReported {
                            validator: sp_runtime::AccountId32,
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
                            stash: sp_runtime::AccountId32,
                            amount: ::core::primitive::u128,
                        },
                        #[codec(index = 7)]
                        #[doc = "An account has unbonded this amount."]
                        Unbonded {
                            stash: sp_runtime::AccountId32,
                            amount: ::core::primitive::u128,
                        },
                        #[codec(index = 8)]
                        #[doc = "An account has called `withdraw_unbonded` and removed unbonding chunks worth `Balance`"]
                        #[doc = "from the unlocking queue."]
                        Withdrawn {
                            stash: sp_runtime::AccountId32,
                            amount: ::core::primitive::u128,
                        },
                        #[codec(index = 9)]
                        #[doc = "A nominator has been kicked from a validator."]
                        Kicked {
                            nominator: sp_runtime::AccountId32,
                            stash: sp_runtime::AccountId32,
                        },
                        #[codec(index = 10)]
                        #[doc = "The election failed. No new era is planned."]
                        StakingElectionFailed,
                        #[codec(index = 11)]
                        #[doc = "An account has stopped participating as either a validator or nominator."]
                        Chilled { stash: sp_runtime::AccountId32 },
                        #[codec(index = 12)]
                        #[doc = "The stakers' rewards are getting paid."]
                        PayoutStarted {
                            era_index: ::core::primitive::u32,
                            validator_stash: sp_runtime::AccountId32,
                        },
                        #[codec(index = 13)]
                        #[doc = "A validator has set their preferences."]
                        ValidatorPrefsSet {
                            stash: sp_runtime::AccountId32,
                            prefs: runtime_types::pallet_staking::ValidatorPrefs,
                        },
                        #[codec(index = 14)]
                        #[doc = "A new force era mode was set."]
                        ForceEra {
                            mode: runtime_types::pallet_staking::Forcing,
                        },
                    }
                }
            }
            pub mod slashing {
                use super::runtime_types;
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                pub struct SlashingSpans {
                    pub span_index: ::core::primitive::u32,
                    pub last_start: ::core::primitive::u32,
                    pub last_nonzero_slash: ::core::primitive::u32,
                    pub prior: ::std::vec::Vec<::core::primitive::u32>,
                }
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                pub struct SpanRecord<_0> {
                    pub slashed: _0,
                    pub paid_out: _0,
                }
            }
            #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
            pub struct ActiveEraInfo {
                pub index: ::core::primitive::u32,
                pub start: ::core::option::Option<::core::primitive::u64>,
            }
            #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
            pub struct EraRewardPoints<_0> {
                pub total: ::core::primitive::u32,
                pub individual: ::subxt::utils::KeyedVec<_0, ::core::primitive::u32>,
            }
            #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
            pub struct Exposure<_0, _1> {
                #[codec(compact)]
                pub total: _1,
                #[codec(compact)]
                pub own: _1,
                pub others:
                    ::std::vec::Vec<runtime_types::pallet_staking::IndividualExposure<_0, _1>>,
            }
            #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
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
            #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
            pub struct IndividualExposure<_0, _1> {
                pub who: _0,
                #[codec(compact)]
                pub value: _1,
            }
            #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
            pub struct Nominations {
                pub targets: runtime_types::bounded_collections::bounded_vec::BoundedVec<
                    sp_runtime::AccountId32,
                >,
                pub submitted_in: ::core::primitive::u32,
                pub suppressed: ::core::primitive::bool,
            }
            #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
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
            #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
            pub struct StakingLedger {
                pub stash: sp_runtime::AccountId32,
                #[codec(compact)]
                pub total: ::core::primitive::u128,
                #[codec(compact)]
                pub active: ::core::primitive::u128,
                pub unlocking: runtime_types::bounded_collections::bounded_vec::BoundedVec<
                    runtime_types::pallet_staking::UnlockChunk<::core::primitive::u128>,
                >,
                pub claimed_rewards: runtime_types::bounded_collections::bounded_vec::BoundedVec<
                    ::core::primitive::u32,
                >,
            }
            #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
            pub struct UnappliedSlash<_0, _1> {
                pub validator: _0,
                pub own: _1,
                pub others: ::std::vec::Vec<(_0, _1)>,
                pub reporters: ::std::vec::Vec<_0>,
                pub payout: _1,
            }
            #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
            pub struct UnlockChunk<_0> {
                #[codec(compact)]
                pub value: _0,
                #[codec(compact)]
                pub era: ::core::primitive::u32,
            }
            #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
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
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
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
                        call: ::std::boxed::Box<runtime_types::vara_runtime::RuntimeCall>,
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
                        call: ::std::boxed::Box<runtime_types::vara_runtime::RuntimeCall>,
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
                        new: sp_runtime::MultiAddress<sp_runtime::AccountId32, ()>,
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
                        who: sp_runtime::MultiAddress<sp_runtime::AccountId32, ()>,
                        call: ::std::boxed::Box<runtime_types::vara_runtime::RuntimeCall>,
                    },
                }
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                #[doc = "Error for the Sudo pallet"]
                pub enum Error {
                    #[codec(index = 0)]
                    #[doc = "Sender must be the Sudo account"]
                    RequireSudo,
                }
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
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
                        old_sudoer: ::core::option::Option<sp_runtime::AccountId32>,
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
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
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
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                #[doc = "\n\t\t\tThe [event](https://docs.substrate.io/main-docs/build/events-errors/) emitted\n\t\t\tby this pallet.\n\t\t\t"]
                pub enum Event {
                    #[codec(index = 0)]
                    #[doc = "A transaction fee `actual_fee`, of which `tip` was added to the minimum inclusion fee,"]
                    #[doc = "has been paid by `who`."]
                    TransactionFeePaid {
                        who: sp_runtime::AccountId32,
                        actual_fee: ::core::primitive::u128,
                        tip: ::core::primitive::u128,
                    },
                }
            }
            #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
            pub struct ChargeTransactionPayment(#[codec(compact)] pub ::core::primitive::u128);
            #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
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
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                #[doc = "Contains one variant per dispatchable that can be called by an extrinsic."]
                pub enum Call {
                    #[codec(index = 0)]
                    #[doc = "Put forward a suggestion for spending. A deposit proportional to the value"]
                    #[doc = "is reserved and slashed if the proposal is rejected. It is returned once the"]
                    #[doc = "proposal is awarded."]
                    #[doc = ""]
                    #[doc = "## Complexity"]
                    #[doc = "- O(1)"]
                    propose_spend {
                        #[codec(compact)]
                        value: ::core::primitive::u128,
                        beneficiary: sp_runtime::MultiAddress<sp_runtime::AccountId32, ()>,
                    },
                    #[codec(index = 1)]
                    #[doc = "Reject a proposed spend. The original deposit will be slashed."]
                    #[doc = ""]
                    #[doc = "May only be called from `T::RejectOrigin`."]
                    #[doc = ""]
                    #[doc = "## Complexity"]
                    #[doc = "- O(1)"]
                    reject_proposal {
                        #[codec(compact)]
                        proposal_id: ::core::primitive::u32,
                    },
                    #[codec(index = 2)]
                    #[doc = "Approve a proposal. At a later time, the proposal will be allocated to the beneficiary"]
                    #[doc = "and the original deposit will be returned."]
                    #[doc = ""]
                    #[doc = "May only be called from `T::ApproveOrigin`."]
                    #[doc = ""]
                    #[doc = "## Complexity"]
                    #[doc = " - O(1)."]
                    approve_proposal {
                        #[codec(compact)]
                        proposal_id: ::core::primitive::u32,
                    },
                    #[codec(index = 3)]
                    #[doc = "Propose and approve a spend of treasury funds."]
                    #[doc = ""]
                    #[doc = "- `origin`: Must be `SpendOrigin` with the `Success` value being at least `amount`."]
                    #[doc = "- `amount`: The amount to be transferred from the treasury to the `beneficiary`."]
                    #[doc = "- `beneficiary`: The destination account for the transfer."]
                    #[doc = ""]
                    #[doc = "NOTE: For record-keeping purposes, the proposer is deemed to be equivalent to the"]
                    #[doc = "beneficiary."]
                    spend {
                        #[codec(compact)]
                        amount: ::core::primitive::u128,
                        beneficiary: sp_runtime::MultiAddress<sp_runtime::AccountId32, ()>,
                    },
                    #[codec(index = 4)]
                    #[doc = "Force a previously approved proposal to be removed from the approval queue."]
                    #[doc = "The original deposit will no longer be returned."]
                    #[doc = ""]
                    #[doc = "May only be called from `T::RejectOrigin`."]
                    #[doc = "- `proposal_id`: The index of a proposal"]
                    #[doc = ""]
                    #[doc = "## Complexity"]
                    #[doc = "- O(A) where `A` is the number of approvals"]
                    #[doc = ""]
                    #[doc = "Errors:"]
                    #[doc = "- `ProposalNotApproved`: The `proposal_id` supplied was not found in the approval queue,"]
                    #[doc = "i.e., the proposal has not been approved. This could also mean the proposal does not"]
                    #[doc = "exist altogether, thus there is no way it would have been approved in the first place."]
                    remove_approval {
                        #[codec(compact)]
                        proposal_id: ::core::primitive::u32,
                    },
                }
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                #[doc = "Error for the treasury pallet."]
                pub enum Error {
                    #[codec(index = 0)]
                    #[doc = "Proposer's balance is too low."]
                    InsufficientProposersBalance,
                    #[codec(index = 1)]
                    #[doc = "No proposal or bounty at that index."]
                    InvalidIndex,
                    #[codec(index = 2)]
                    #[doc = "Too many approvals in the queue."]
                    TooManyApprovals,
                    #[codec(index = 3)]
                    #[doc = "The spend origin is valid but the amount it is allowed to spend is lower than the"]
                    #[doc = "amount to be spent."]
                    InsufficientPermission,
                    #[codec(index = 4)]
                    #[doc = "Proposal has not been approved."]
                    ProposalNotApproved,
                }
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                #[doc = "\n\t\t\tThe [event](https://docs.substrate.io/main-docs/build/events-errors/) emitted\n\t\t\tby this pallet.\n\t\t\t"]
                pub enum Event {
                    #[codec(index = 0)]
                    #[doc = "New proposal."]
                    Proposed {
                        proposal_index: ::core::primitive::u32,
                    },
                    #[codec(index = 1)]
                    #[doc = "We have ended a spend period and will now allocate funds."]
                    Spending {
                        budget_remaining: ::core::primitive::u128,
                    },
                    #[codec(index = 2)]
                    #[doc = "Some funds have been allocated."]
                    Awarded {
                        proposal_index: ::core::primitive::u32,
                        award: ::core::primitive::u128,
                        account: sp_runtime::AccountId32,
                    },
                    #[codec(index = 3)]
                    #[doc = "A proposal was rejected; funds were slashed."]
                    Rejected {
                        proposal_index: ::core::primitive::u32,
                        slashed: ::core::primitive::u128,
                    },
                    #[codec(index = 4)]
                    #[doc = "Some of our funds have been burnt."]
                    Burnt {
                        burnt_funds: ::core::primitive::u128,
                    },
                    #[codec(index = 5)]
                    #[doc = "Spending has finished; this is the amount that rolls over until next spend."]
                    Rollover {
                        rollover_balance: ::core::primitive::u128,
                    },
                    #[codec(index = 6)]
                    #[doc = "Some funds have been deposited."]
                    Deposit { value: ::core::primitive::u128 },
                    #[codec(index = 7)]
                    #[doc = "A new spend proposal has been approved."]
                    SpendApproved {
                        proposal_index: ::core::primitive::u32,
                        amount: ::core::primitive::u128,
                        beneficiary: sp_runtime::AccountId32,
                    },
                    #[codec(index = 8)]
                    #[doc = "The inactive funds of the pallet have been updated."]
                    UpdatedInactive {
                        reactivated: ::core::primitive::u128,
                        deactivated: ::core::primitive::u128,
                    },
                }
            }
            #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
            pub struct Proposal<_0, _1> {
                pub proposer: _0,
                pub value: _1,
                pub beneficiary: _0,
                pub bond: _1,
            }
        }
        pub mod pallet_utility {
            use super::runtime_types;
            pub mod pallet {
                use super::runtime_types;
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
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
                        calls: ::std::vec::Vec<runtime_types::vara_runtime::RuntimeCall>,
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
                        call: ::std::boxed::Box<runtime_types::vara_runtime::RuntimeCall>,
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
                        calls: ::std::vec::Vec<runtime_types::vara_runtime::RuntimeCall>,
                    },
                    #[codec(index = 3)]
                    #[doc = "Dispatches a function call with a provided origin."]
                    #[doc = ""]
                    #[doc = "The dispatch origin for this call must be _Root_."]
                    #[doc = ""]
                    #[doc = "## Complexity"]
                    #[doc = "- O(1)."]
                    dispatch_as {
                        as_origin: ::std::boxed::Box<runtime_types::vara_runtime::OriginCaller>,
                        call: ::std::boxed::Box<runtime_types::vara_runtime::RuntimeCall>,
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
                        calls: ::std::vec::Vec<runtime_types::vara_runtime::RuntimeCall>,
                    },
                    #[codec(index = 5)]
                    #[doc = "Dispatch a function call with a specified weight."]
                    #[doc = ""]
                    #[doc = "This function does not check the weight of the call, and instead allows the"]
                    #[doc = "Root origin to specify the weight of the call."]
                    #[doc = ""]
                    #[doc = "The dispatch origin for this call must be _Root_."]
                    with_weight {
                        call: ::std::boxed::Box<runtime_types::vara_runtime::RuntimeCall>,
                        weight: runtime_types::sp_weights::weight_v2::Weight,
                    },
                }
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                #[doc = "\n\t\t\tCustom [dispatch errors](https://docs.substrate.io/main-docs/build/events-errors/)\n\t\t\tof this pallet.\n\t\t\t"]
                pub enum Error {
                    #[codec(index = 0)]
                    #[doc = "Too many calls batched."]
                    TooManyCalls,
                }
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
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
        pub mod pallet_vesting {
            use super::runtime_types;
            pub mod pallet {
                use super::runtime_types;
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                #[doc = "Contains one variant per dispatchable that can be called by an extrinsic."]
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
                        target: sp_runtime::MultiAddress<sp_runtime::AccountId32, ()>,
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
                        target: sp_runtime::MultiAddress<sp_runtime::AccountId32, ()>,
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
                        source: sp_runtime::MultiAddress<sp_runtime::AccountId32, ()>,
                        target: sp_runtime::MultiAddress<sp_runtime::AccountId32, ()>,
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
                }
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
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
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                #[doc = "\n\t\t\tThe [event](https://docs.substrate.io/main-docs/build/events-errors/) emitted\n\t\t\tby this pallet.\n\t\t\t"]
                pub enum Event {
                    #[codec(index = 0)]
                    #[doc = "The amount vested has been updated. This could indicate a change in funds available."]
                    #[doc = "The balance given is the amount which is left unvested (and thus locked)."]
                    VestingUpdated {
                        account: sp_runtime::AccountId32,
                        unvested: ::core::primitive::u128,
                    },
                    #[codec(index = 1)]
                    #[doc = "An \\[account\\] has become fully vested."]
                    VestingCompleted { account: sp_runtime::AccountId32 },
                }
            }
            pub mod vesting_info {
                use super::runtime_types;
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                pub struct VestingInfo<_0, _1> {
                    pub locked: _0,
                    pub per_block: _0,
                    pub starting_block: _1,
                }
            }
            #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
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
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                #[doc = "Contains one variant per dispatchable that can be called by an extrinsic."]
                pub enum Call {
                    #[codec(index = 0)]
                    whitelist_call { call_hash: ::subxt::utils::H256 },
                    #[codec(index = 1)]
                    remove_whitelisted_call { call_hash: ::subxt::utils::H256 },
                    #[codec(index = 2)]
                    dispatch_whitelisted_call {
                        call_hash: ::subxt::utils::H256,
                        call_encoded_len: ::core::primitive::u32,
                        call_weight_witness: runtime_types::sp_weights::weight_v2::Weight,
                    },
                    #[codec(index = 3)]
                    dispatch_whitelisted_call_with_preimage {
                        call: ::std::boxed::Box<runtime_types::vara_runtime::RuntimeCall>,
                    },
                }
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                #[doc = "\n\t\t\tCustom [dispatch errors](https://docs.substrate.io/main-docs/build/events-errors/)\n\t\t\tof this pallet.\n\t\t\t"]
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
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                #[doc = "\n\t\t\tThe [event](https://docs.substrate.io/main-docs/build/events-errors/) emitted\n\t\t\tby this pallet.\n\t\t\t"]
                pub enum Event {
                    #[codec(index = 0)]
                    CallWhitelisted { call_hash: ::subxt::utils::H256 },
                    #[codec(index = 1)]
                    WhitelistedCallRemoved { call_hash: ::subxt::utils::H256 },
                    #[codec(index = 2)]
                    WhitelistedCallDispatched {
                        call_hash: ::subxt::utils::H256,
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
        pub mod sp_arithmetic {
            use super::runtime_types;
            pub mod fixed_point {
                use super::runtime_types;
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                pub struct FixedI64(pub ::core::primitive::i64);
                #[derive(
                    ::subxt::ext::codec::CompactAs,
                    ::subxt::ext::codec::Decode,
                    ::subxt::ext::codec::Encode,
                    Debug,
                )]
                pub struct FixedU128(pub ::core::primitive::u128);
            }
            pub mod per_things {
                use super::runtime_types;
                #[derive(
                    ::subxt::ext::codec::CompactAs,
                    ::subxt::ext::codec::Decode,
                    ::subxt::ext::codec::Encode,
                    Debug,
                )]
                pub struct Perbill(pub ::core::primitive::u32);
                #[derive(
                    ::subxt::ext::codec::CompactAs,
                    ::subxt::ext::codec::Decode,
                    ::subxt::ext::codec::Encode,
                    Debug,
                )]
                pub struct Percent(pub ::core::primitive::u8);
                #[derive(
                    ::subxt::ext::codec::CompactAs,
                    ::subxt::ext::codec::Decode,
                    ::subxt::ext::codec::Encode,
                    Debug,
                )]
                pub struct Permill(pub ::core::primitive::u32);
                #[derive(
                    ::subxt::ext::codec::CompactAs,
                    ::subxt::ext::codec::Decode,
                    ::subxt::ext::codec::Encode,
                    Debug,
                )]
                pub struct Perquintill(pub ::core::primitive::u64);
            }
            #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
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
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                pub struct Public(pub runtime_types::sp_core::sr25519::Public);
            }
        }
        pub mod sp_consensus_babe {
            use super::runtime_types;
            pub mod app {
                use super::runtime_types;
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                pub struct Public(pub runtime_types::sp_core::sr25519::Public);
            }
            pub mod digests {
                use super::runtime_types;
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                pub enum NextConfigDescriptor {
                    #[codec(index = 1)]
                    V1 {
                        c: (::core::primitive::u64, ::core::primitive::u64),
                        allowed_slots: runtime_types::sp_consensus_babe::AllowedSlots,
                    },
                }
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
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
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                pub struct PrimaryPreDigest {
                    pub authority_index: ::core::primitive::u32,
                    pub slot: runtime_types::sp_consensus_slots::Slot,
                    pub vrf_output: [::core::primitive::u8; 32usize],
                    pub vrf_proof: [::core::primitive::u8; 64usize],
                }
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                pub struct SecondaryPlainPreDigest {
                    pub authority_index: ::core::primitive::u32,
                    pub slot: runtime_types::sp_consensus_slots::Slot,
                }
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                pub struct SecondaryVRFPreDigest {
                    pub authority_index: ::core::primitive::u32,
                    pub slot: runtime_types::sp_consensus_slots::Slot,
                    pub vrf_output: [::core::primitive::u8; 32usize],
                    pub vrf_proof: [::core::primitive::u8; 64usize],
                }
            }
            #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
            pub enum AllowedSlots {
                #[codec(index = 0)]
                PrimarySlots,
                #[codec(index = 1)]
                PrimaryAndSecondaryPlainSlots,
                #[codec(index = 2)]
                PrimaryAndSecondaryVRFSlots,
            }
            #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
            pub struct BabeEpochConfiguration {
                pub c: (::core::primitive::u64, ::core::primitive::u64),
                pub allowed_slots: runtime_types::sp_consensus_babe::AllowedSlots,
            }
        }
        pub mod sp_consensus_grandpa {
            use super::runtime_types;
            pub mod app {
                use super::runtime_types;
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                pub struct Public(pub runtime_types::sp_core::ed25519::Public);
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                pub struct Signature(pub runtime_types::sp_core::ed25519::Signature);
            }
            #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
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
            #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
            pub struct EquivocationProof<_0, _1> {
                pub set_id: ::core::primitive::u64,
                pub equivocation: runtime_types::sp_consensus_grandpa::Equivocation<_0, _1>,
            }
        }
        pub mod sp_consensus_slots {
            use super::runtime_types;
            #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
            pub struct EquivocationProof<_0, _1> {
                pub offender: _1,
                pub slot: runtime_types::sp_consensus_slots::Slot,
                pub first_header: _0,
                pub second_header: _0,
            }
            #[derive(
                ::subxt::ext::codec::CompactAs,
                ::subxt::ext::codec::Decode,
                ::subxt::ext::codec::Encode,
                Debug,
            )]
            pub struct Slot(pub ::core::primitive::u64);
        }
        pub mod sp_core {
            use super::runtime_types;
            pub mod crypto {
                use super::runtime_types;
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                pub struct KeyTypeId(pub [::core::primitive::u8; 4usize]);
            }
            pub mod ecdsa {
                use super::runtime_types;
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                pub struct Signature(pub [::core::primitive::u8; 65usize]);
            }
            pub mod ed25519 {
                use super::runtime_types;
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                pub struct Public(pub [::core::primitive::u8; 32usize]);
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                pub struct Signature(pub [::core::primitive::u8; 64usize]);
            }
            pub mod offchain {
                use super::runtime_types;
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                pub struct OpaqueMultiaddr(pub ::std::vec::Vec<::core::primitive::u8>);
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                pub struct OpaqueNetworkState {
                    pub peer_id: runtime_types::sp_core::OpaquePeerId,
                    pub external_addresses:
                        ::std::vec::Vec<runtime_types::sp_core::offchain::OpaqueMultiaddr>,
                }
            }
            pub mod sr25519 {
                use super::runtime_types;
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                pub struct Public(pub [::core::primitive::u8; 32usize]);
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                pub struct Signature(pub [::core::primitive::u8; 64usize]);
            }
            #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
            pub struct OpaquePeerId(pub ::std::vec::Vec<::core::primitive::u8>);
            #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
            pub enum Void {}
        }
        pub mod sp_runtime {
            use super::runtime_types;
            pub mod generic {
                use super::runtime_types;
                pub mod digest {
                    use super::runtime_types;
                    #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                    pub struct Digest {
                        pub logs:
                            ::std::vec::Vec<runtime_types::sp_runtime::generic::digest::DigestItem>,
                    }
                    #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
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
                    #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
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
                    #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
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
                    #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                    pub struct UncheckedExtrinsic<_0, _1, _2, _3>(
                        pub ::std::vec::Vec<::core::primitive::u8>,
                        #[codec(skip)] pub ::core::marker::PhantomData<(_1, _0, _2, _3)>,
                    );
                }
            }
            pub mod traits {
                use super::runtime_types;
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                pub struct BlakeTwo256;
            }
            #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
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
            #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
            pub struct DispatchErrorWithPostInfo<_0> {
                pub post_info: _0,
                pub error: runtime_types::sp_runtime::DispatchError,
            }
            #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
            pub struct ModuleError {
                pub index: ::core::primitive::u8,
                pub error: [::core::primitive::u8; 4usize],
            }
            #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
            pub enum MultiSignature {
                #[codec(index = 0)]
                Ed25519(runtime_types::sp_core::ed25519::Signature),
                #[codec(index = 1)]
                Sr25519(runtime_types::sp_core::sr25519::Signature),
                #[codec(index = 2)]
                Ecdsa(runtime_types::sp_core::ecdsa::Signature),
            }
            #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
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
            #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
            pub enum TransactionalError {
                #[codec(index = 0)]
                LimitReached,
                #[codec(index = 1)]
                NoLayer,
            }
        }
        pub mod sp_session {
            use super::runtime_types;
            #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
            pub struct MembershipProof {
                pub session: ::core::primitive::u32,
                pub trie_nodes: ::std::vec::Vec<::std::vec::Vec<::core::primitive::u8>>,
                pub validator_count: ::core::primitive::u32,
            }
        }
        pub mod sp_version {
            use super::runtime_types;
            #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
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
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                pub struct Weight {
                    #[codec(compact)]
                    pub ref_time: ::core::primitive::u64,
                    #[codec(compact)]
                    pub proof_size: ::core::primitive::u64,
                }
            }
            #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
            pub struct RuntimeDbWeight {
                pub read: ::core::primitive::u64,
                pub write: ::core::primitive::u64,
            }
        }
        pub mod substrate_validator_set {
            use super::runtime_types;
            pub mod pallet {
                use super::runtime_types;
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
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
                        validator_id: sp_runtime::AccountId32,
                    },
                    #[codec(index = 1)]
                    #[doc = "Remove a validator."]
                    #[doc = ""]
                    #[doc = "The origin can be configured using the `AddRemoveOrigin` type in the"]
                    #[doc = "host runtime. Can also be set to sudo/root."]
                    remove_validator {
                        validator_id: sp_runtime::AccountId32,
                    },
                    #[codec(index = 2)]
                    #[doc = "Add an approved validator again when it comes back online."]
                    #[doc = ""]
                    #[doc = "For this call, the dispatch origin must be the validator itself."]
                    add_validator_again {
                        validator_id: sp_runtime::AccountId32,
                    },
                }
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
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
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                #[doc = "\n\t\t\tThe [event](https://docs.substrate.io/main-docs/build/events-errors/) emitted\n\t\t\tby this pallet.\n\t\t\t"]
                pub enum Event {
                    #[codec(index = 0)]
                    #[doc = "New validator addition initiated. Effective in ~2 sessions."]
                    ValidatorAdditionInitiated(sp_runtime::AccountId32),
                    #[codec(index = 1)]
                    #[doc = "Validator removal initiated. Effective in ~2 sessions."]
                    ValidatorRemovalInitiated(sp_runtime::AccountId32),
                }
            }
        }
        pub mod vara_runtime {
            use super::runtime_types;
            pub mod extensions {
                use super::runtime_types;
                #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
                pub struct DisableValueTransfers;
            }
            pub mod governance {
                use super::runtime_types;
                pub mod origins {
                    use super::runtime_types;
                    pub mod pallet_custom_origins {
                        use super::runtime_types;
                        #[derive(
                            ::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug,
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
            #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
            pub enum OriginCaller {
                #[codec(index = 0)]
                system(runtime_types::frame_support::dispatch::RawOrigin<sp_runtime::AccountId32>),
                #[codec(index = 20)]
                Origins(
                    runtime_types::vara_runtime::governance::origins::pallet_custom_origins::Origin,
                ),
                #[codec(index = 2)]
                Void(runtime_types::sp_core::Void),
            }
            #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
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
            #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
            pub struct Runtime;
            #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
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
                #[codec(index = 10)]
                Vesting(runtime_types::pallet_vesting::pallet::Call),
                #[codec(index = 11)]
                BagsList(runtime_types::pallet_bags_list::pallet::Call),
                #[codec(index = 12)]
                ImOnline(runtime_types::pallet_im_online::pallet::Call),
                #[codec(index = 13)]
                Staking(runtime_types::pallet_staking::pallet::pallet::Call),
                #[codec(index = 7)]
                Session(runtime_types::pallet_session::pallet::Call),
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
                #[codec(index = 98)]
                ValidatorSet(runtime_types::substrate_validator_set::pallet::Call),
                #[codec(index = 99)]
                Sudo(runtime_types::pallet_sudo::pallet::Call),
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
                #[codec(index = 8)]
                Utility(runtime_types::pallet_utility::pallet::Call),
                #[codec(index = 104)]
                Gear(runtime_types::pallet_gear::pallet::Call),
                #[codec(index = 106)]
                StakingRewards(runtime_types::pallet_gear_staking_rewards::pallet::Call),
                #[codec(index = 198)]
                Airdrop(runtime_types::pallet_airdrop::pallet::Call),
                #[codec(index = 199)]
                GearDebug(runtime_types::pallet_gear_debug::pallet::Call),
            }
            #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
            pub enum RuntimeEvent {
                #[codec(index = 0)]
                System(runtime_types::frame_system::pallet::Event),
                #[codec(index = 4)]
                Grandpa(runtime_types::pallet_grandpa::pallet::Event),
                #[codec(index = 5)]
                Balances(runtime_types::pallet_balances::pallet::Event),
                #[codec(index = 10)]
                Vesting(runtime_types::pallet_vesting::pallet::Event),
                #[codec(index = 6)]
                TransactionPayment(runtime_types::pallet_transaction_payment::pallet::Event),
                #[codec(index = 11)]
                BagsList(runtime_types::pallet_bags_list::pallet::Event),
                #[codec(index = 12)]
                ImOnline(runtime_types::pallet_im_online::pallet::Event),
                #[codec(index = 13)]
                Staking(runtime_types::pallet_staking::pallet::pallet::Event),
                #[codec(index = 7)]
                Session(runtime_types::pallet_session::pallet::Event),
                #[codec(index = 14)]
                Treasury(runtime_types::pallet_treasury::pallet::Event),
                #[codec(index = 16)]
                ConvictionVoting(runtime_types::pallet_conviction_voting::pallet::Event),
                #[codec(index = 17)]
                Referenda(runtime_types::pallet_referenda::pallet::Event),
                #[codec(index = 18)]
                FellowshipCollective(runtime_types::pallet_ranked_collective::pallet::Event),
                #[codec(index = 19)]
                FellowshipReferenda(runtime_types::pallet_referenda::pallet::Event),
                #[codec(index = 21)]
                Whitelist(runtime_types::pallet_whitelist::pallet::Event),
                #[codec(index = 98)]
                ValidatorSet(runtime_types::substrate_validator_set::pallet::Event),
                #[codec(index = 99)]
                Sudo(runtime_types::pallet_sudo::pallet::Event),
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
                #[codec(index = 8)]
                Utility(runtime_types::pallet_utility::pallet::Event),
                #[codec(index = 104)]
                Gear(runtime_types::pallet_gear::pallet::Event),
                #[codec(index = 106)]
                StakingRewards(runtime_types::pallet_gear_staking_rewards::pallet::Event),
                #[codec(index = 198)]
                Airdrop(runtime_types::pallet_airdrop::pallet::Event),
                #[codec(index = 199)]
                GearDebug(runtime_types::pallet_gear_debug::pallet::Event),
            }
            #[derive(::subxt::ext::codec::Decode, ::subxt::ext::codec::Encode, Debug)]
            pub struct SessionKeys {
                pub babe: runtime_types::sp_consensus_babe::app::Public,
                pub grandpa: runtime_types::sp_consensus_grandpa::app::Public,
                pub im_online: runtime_types::pallet_im_online::sr25519::app_sr25519::Public,
                pub authority_discovery: runtime_types::sp_authority_discovery::app::Public,
            }
        }
    }
}
