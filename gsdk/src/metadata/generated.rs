// This file is part of Gear.
//
// Copyright (C) 2021-2024 Gear Technologies Inc.
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
                pub struct BoundedBTreeMap<_0, _1>(pub ::subxt::utils::KeyedVec<_0, _1>);
            }
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
                            ::subxt::ext::codec::CompactAs,
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
                    #[doc = "See [`Pallet::remark`]."]
                    remark {
                        remark: ::std::vec::Vec<::core::primitive::u8>,
                    },
                    #[codec(index = 1)]
                    #[doc = "See [`Pallet::set_heap_pages`]."]
                    set_heap_pages { pages: ::core::primitive::u64 },
                    #[codec(index = 2)]
                    #[doc = "See [`Pallet::set_code`]."]
                    set_code {
                        code: ::std::vec::Vec<::core::primitive::u8>,
                    },
                    #[codec(index = 3)]
                    #[doc = "See [`Pallet::set_code_without_checks`]."]
                    set_code_without_checks {
                        code: ::std::vec::Vec<::core::primitive::u8>,
                    },
                    #[codec(index = 4)]
                    #[doc = "See [`Pallet::set_storage`]."]
                    set_storage {
                        items: ::std::vec::Vec<(
                            ::std::vec::Vec<::core::primitive::u8>,
                            ::std::vec::Vec<::core::primitive::u8>,
                        )>,
                    },
                    #[codec(index = 5)]
                    #[doc = "See [`Pallet::kill_storage`]."]
                    kill_storage {
                        keys: ::std::vec::Vec<::std::vec::Vec<::core::primitive::u8>>,
                    },
                    #[codec(index = 6)]
                    #[doc = "See [`Pallet::kill_prefix`]."]
                    kill_prefix {
                        prefix: ::std::vec::Vec<::core::primitive::u8>,
                        subkeys: ::core::primitive::u32,
                    },
                    #[codec(index = 7)]
                    #[doc = "See [`Pallet::remark_with_event`]."]
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
                pub consumers: ::core::primitive::u32,
                pub providers: ::core::primitive::u32,
                pub sufficients: ::core::primitive::u32,
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
            pub mod scheduler {
                use super::runtime_types;
                pub mod task {
                    use super::runtime_types;
                    #[derive(
                        Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode,
                    )]
                    pub enum ScheduledTask<_0> {
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
                        SendDispatch(runtime_types::gprimitives::MessageId),
                        #[codec(index = 7)]
                        SendUserMessage {
                            message_id: runtime_types::gprimitives::MessageId,
                            to_mailbox: ::core::primitive::bool,
                        },
                        #[codec(index = 8)]
                        RemoveGasReservation(
                            runtime_types::gprimitives::ActorId,
                            runtime_types::gprimitives::ReservationId,
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
                pub allocations_tree_len: ::core::primitive::u32,
                pub memory_infix: runtime_types::gear_core::program::MemoryInfix,
                pub gas_reservation_map: ::subxt::utils::KeyedVec<
                    runtime_types::gear_core::ids::ReservationId,
                    runtime_types::gear_core::reservation::GasReservationSlot,
                >,
                pub code_hash: ::subxt::utils::H256,
                pub code_exports: ::std::vec::Vec<runtime_types::gear_core::message::DispatchKind>,
                pub static_pages: runtime_types::gear_core::pages::PagesAmount,
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
                pub mod instrumented {
                    use super::runtime_types;
                    #[derive(
                        Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode,
                    )]
                    pub struct InstrumentedCode {
                        pub code: ::std::vec::Vec<::core::primitive::u8>,
                        pub original_code_len: ::core::primitive::u32,
                        pub exports:
                            ::std::vec::Vec<runtime_types::gear_core::message::DispatchKind>,
                        pub static_pages: runtime_types::gear_core::pages::PagesAmount,
                        pub stack_end:
                            ::core::option::Option<runtime_types::gear_core::pages::Page2>,
                        pub version: ::core::primitive::u32,
                    }
                }
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
                        pub initialized: ::std::vec::Vec<runtime_types::gprimitives::ActorId>,
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
                        pub id: runtime_types::gprimitives::MessageId,
                        pub source: runtime_types::gprimitives::ActorId,
                        pub destination: runtime_types::gprimitives::ActorId,
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
                        pub id: runtime_types::gprimitives::MessageId,
                        pub source: runtime_types::gprimitives::ActorId,
                        pub destination: runtime_types::gprimitives::ActorId,
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
                pub struct Page(pub ::core::primitive::u32);
                #[derive(
                    ::subxt::ext::codec::CompactAs,
                    Debug,
                    crate::gp::Decode,
                    crate::gp::DecodeAsType,
                    crate::gp::Encode,
                )]
                pub struct Page2(pub ::core::primitive::u32);
                #[derive(
                    ::subxt::ext::codec::CompactAs,
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
                    ::subxt::ext::codec::CompactAs,
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
                    pub allocations: runtime_types::numerated::tree::IntervalsTree<
                        runtime_types::gear_core::pages::Page2,
                    >,
                    pub pages_with_data: runtime_types::numerated::tree::IntervalsTree<
                        runtime_types::gear_core::pages::Page,
                    >,
                    pub memory_infix: runtime_types::gear_core::program::MemoryInfix,
                    pub gas_reservation_map: ::subxt::utils::KeyedVec<
                        runtime_types::gprimitives::ReservationId,
                        runtime_types::gear_core::reservation::GasReservationSlot,
                    >,
                    pub code_hash: ::subxt::utils::H256,
                    pub code_exports:
                        ::std::vec::Vec<runtime_types::gear_core::message::DispatchKind>,
                    pub static_pages: runtime_types::gear_core::pages::PagesAmount,
                    pub state: runtime_types::gear_core::program::ProgramState,
                    pub expiration_block: _0,
                }
                #[derive(
                    ::subxt::ext::codec::CompactAs,
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
                    InactiveActor,
                    #[codec(index = 3)]
                    RemovedFromWaitlist,
                    #[codec(index = 4)]
                    ReinstrumentationFailure,
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
                    pub inner: ::subxt::utils::KeyedVec<_0, _0>,
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
                    #[doc = "See [`Pallet::report_equivocation`]."]
                    report_equivocation {
                        equivocation_proof: ::std::boxed::Box<
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
                    #[doc = "See [`Pallet::report_equivocation_unsigned`]."]
                    report_equivocation_unsigned {
                        equivocation_proof: ::std::boxed::Box<
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
                    #[doc = "See [`Pallet::plan_config_change`]."]
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
                    pub head: ::core::option::Option<::subxt::utils::AccountId32>,
                    pub tail: ::core::option::Option<::subxt::utils::AccountId32>,
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
                    pub id: ::subxt::utils::AccountId32,
                    pub prev: ::core::option::Option<::subxt::utils::AccountId32>,
                    pub next: ::core::option::Option<::subxt::utils::AccountId32>,
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
                    #[doc = "See [`Pallet::rebag`]."]
                    rebag {
                        dislocated: ::subxt::utils::MultiAddress<::subxt::utils::AccountId32, ()>,
                    },
                    #[codec(index = 1)]
                    #[doc = "See [`Pallet::put_in_front_of`]."]
                    put_in_front_of {
                        lighter: ::subxt::utils::MultiAddress<::subxt::utils::AccountId32, ()>,
                    },
                    #[codec(index = 2)]
                    #[doc = "See [`Pallet::put_in_front_of_other`]."]
                    put_in_front_of_other {
                        heavier: ::subxt::utils::MultiAddress<::subxt::utils::AccountId32, ()>,
                        lighter: ::subxt::utils::MultiAddress<::subxt::utils::AccountId32, ()>,
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
                        who: ::subxt::utils::AccountId32,
                        from: ::core::primitive::u64,
                        to: ::core::primitive::u64,
                    },
                    #[codec(index = 1)]
                    #[doc = "Updated the score of some account to the given amount."]
                    ScoreUpdated {
                        who: ::subxt::utils::AccountId32,
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
                    #[doc = "See [`Pallet::transfer_allow_death`]."]
                    transfer_allow_death {
                        dest: ::subxt::utils::MultiAddress<::subxt::utils::AccountId32, ()>,
                        #[codec(compact)]
                        value: ::core::primitive::u128,
                    },
                    #[codec(index = 2)]
                    #[doc = "See [`Pallet::force_transfer`]."]
                    force_transfer {
                        source: ::subxt::utils::MultiAddress<::subxt::utils::AccountId32, ()>,
                        dest: ::subxt::utils::MultiAddress<::subxt::utils::AccountId32, ()>,
                        #[codec(compact)]
                        value: ::core::primitive::u128,
                    },
                    #[codec(index = 3)]
                    #[doc = "See [`Pallet::transfer_keep_alive`]."]
                    transfer_keep_alive {
                        dest: ::subxt::utils::MultiAddress<::subxt::utils::AccountId32, ()>,
                        #[codec(compact)]
                        value: ::core::primitive::u128,
                    },
                    #[codec(index = 4)]
                    #[doc = "See [`Pallet::transfer_all`]."]
                    transfer_all {
                        dest: ::subxt::utils::MultiAddress<::subxt::utils::AccountId32, ()>,
                        keep_alive: ::core::primitive::bool,
                    },
                    #[codec(index = 5)]
                    #[doc = "See [`Pallet::force_unreserve`]."]
                    force_unreserve {
                        who: ::subxt::utils::MultiAddress<::subxt::utils::AccountId32, ()>,
                        amount: ::core::primitive::u128,
                    },
                    #[codec(index = 6)]
                    #[doc = "See [`Pallet::upgrade_accounts`]."]
                    upgrade_accounts {
                        who: ::std::vec::Vec<::subxt::utils::AccountId32>,
                    },
                    #[codec(index = 8)]
                    #[doc = "See [`Pallet::force_set_balance`]."]
                    force_set_balance {
                        who: ::subxt::utils::MultiAddress<::subxt::utils::AccountId32, ()>,
                        #[codec(compact)]
                        new_free: ::core::primitive::u128,
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
                    #[doc = "Number of holds exceed `MaxHolds`."]
                    TooManyHolds,
                    #[codec(index = 9)]
                    #[doc = "Number of freezes exceed `MaxFreezes`."]
                    TooManyFreezes,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "The `Event` enum of this pallet"]
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
                    #[codec(index = 10)]
                    #[doc = "Some amount was minted into an account."]
                    Minted {
                        who: ::subxt::utils::AccountId32,
                        amount: ::core::primitive::u128,
                    },
                    #[codec(index = 11)]
                    #[doc = "Some amount was burned from an account."]
                    Burned {
                        who: ::subxt::utils::AccountId32,
                        amount: ::core::primitive::u128,
                    },
                    #[codec(index = 12)]
                    #[doc = "Some amount was suspended from an account (it can be restored later)."]
                    Suspended {
                        who: ::subxt::utils::AccountId32,
                        amount: ::core::primitive::u128,
                    },
                    #[codec(index = 13)]
                    #[doc = "Some amount was restored into an account."]
                    Restored {
                        who: ::subxt::utils::AccountId32,
                        amount: ::core::primitive::u128,
                    },
                    #[codec(index = 14)]
                    #[doc = "An account was upgraded."]
                    Upgraded { who: ::subxt::utils::AccountId32 },
                    #[codec(index = 15)]
                    #[doc = "Total issuance was increased by `amount`, creating a credit to be balanced."]
                    Issued { amount: ::core::primitive::u128 },
                    #[codec(index = 16)]
                    #[doc = "Total issuance was decreased by `amount`, creating a debt to be balanced."]
                    Rescinded { amount: ::core::primitive::u128 },
                    #[codec(index = 17)]
                    #[doc = "Some balance was locked."]
                    Locked {
                        who: ::subxt::utils::AccountId32,
                        amount: ::core::primitive::u128,
                    },
                    #[codec(index = 18)]
                    #[doc = "Some balance was unlocked."]
                    Unlocked {
                        who: ::subxt::utils::AccountId32,
                        amount: ::core::primitive::u128,
                    },
                    #[codec(index = 19)]
                    #[doc = "Some balance was frozen."]
                    Frozen {
                        who: ::subxt::utils::AccountId32,
                        amount: ::core::primitive::u128,
                    },
                    #[codec(index = 20)]
                    #[doc = "Some balance was thawed."]
                    Thawed {
                        who: ::subxt::utils::AccountId32,
                        amount: ::core::primitive::u128,
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
                pub struct BalanceLock<_0> {
                    pub id: [::core::primitive::u8; 8usize],
                    pub amount: _0,
                    pub reasons: runtime_types::pallet_balances::types::Reasons,
                }
                #[derive(
                    ::subxt::ext::codec::CompactAs,
                    Debug,
                    crate::gp::Decode,
                    crate::gp::DecodeAsType,
                    crate::gp::Encode,
                )]
                pub struct ExtraFlags(pub ::core::primitive::u128);
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct IdAmount<_0, _1> {
                    pub id: _0,
                    pub amount: _1,
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
        }
        pub mod pallet_bounties {
            use super::runtime_types;
            pub mod pallet {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "Contains a variant per dispatchable extrinsic that this pallet has."]
                pub enum Call {
                    #[codec(index = 0)]
                    #[doc = "See [`Pallet::propose_bounty`]."]
                    propose_bounty {
                        #[codec(compact)]
                        value: ::core::primitive::u128,
                        description: ::std::vec::Vec<::core::primitive::u8>,
                    },
                    #[codec(index = 1)]
                    #[doc = "See [`Pallet::approve_bounty`]."]
                    approve_bounty {
                        #[codec(compact)]
                        bounty_id: ::core::primitive::u32,
                    },
                    #[codec(index = 2)]
                    #[doc = "See [`Pallet::propose_curator`]."]
                    propose_curator {
                        #[codec(compact)]
                        bounty_id: ::core::primitive::u32,
                        curator: ::subxt::utils::MultiAddress<::subxt::utils::AccountId32, ()>,
                        #[codec(compact)]
                        fee: ::core::primitive::u128,
                    },
                    #[codec(index = 3)]
                    #[doc = "See [`Pallet::unassign_curator`]."]
                    unassign_curator {
                        #[codec(compact)]
                        bounty_id: ::core::primitive::u32,
                    },
                    #[codec(index = 4)]
                    #[doc = "See [`Pallet::accept_curator`]."]
                    accept_curator {
                        #[codec(compact)]
                        bounty_id: ::core::primitive::u32,
                    },
                    #[codec(index = 5)]
                    #[doc = "See [`Pallet::award_bounty`]."]
                    award_bounty {
                        #[codec(compact)]
                        bounty_id: ::core::primitive::u32,
                        beneficiary: ::subxt::utils::MultiAddress<::subxt::utils::AccountId32, ()>,
                    },
                    #[codec(index = 6)]
                    #[doc = "See [`Pallet::claim_bounty`]."]
                    claim_bounty {
                        #[codec(compact)]
                        bounty_id: ::core::primitive::u32,
                    },
                    #[codec(index = 7)]
                    #[doc = "See [`Pallet::close_bounty`]."]
                    close_bounty {
                        #[codec(compact)]
                        bounty_id: ::core::primitive::u32,
                    },
                    #[codec(index = 8)]
                    #[doc = "See [`Pallet::extend_bounty_expiry`]."]
                    extend_bounty_expiry {
                        #[codec(compact)]
                        bounty_id: ::core::primitive::u32,
                        remark: ::std::vec::Vec<::core::primitive::u8>,
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
                        beneficiary: ::subxt::utils::AccountId32,
                    },
                    #[codec(index = 4)]
                    #[doc = "A bounty is claimed by beneficiary."]
                    BountyClaimed {
                        index: ::core::primitive::u32,
                        payout: ::core::primitive::u128,
                        beneficiary: ::subxt::utils::AccountId32,
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
                        curator: ::subxt::utils::AccountId32,
                    },
                    #[codec(index = 9)]
                    #[doc = "A bounty curator is unassigned."]
                    CuratorUnassigned { bounty_id: ::core::primitive::u32 },
                    #[codec(index = 10)]
                    #[doc = "A bounty curator is accepted."]
                    CuratorAccepted {
                        bounty_id: ::core::primitive::u32,
                        curator: ::subxt::utils::AccountId32,
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
                    #[doc = "See [`Pallet::add_child_bounty`]."]
                    add_child_bounty {
                        #[codec(compact)]
                        parent_bounty_id: ::core::primitive::u32,
                        #[codec(compact)]
                        value: ::core::primitive::u128,
                        description: ::std::vec::Vec<::core::primitive::u8>,
                    },
                    #[codec(index = 1)]
                    #[doc = "See [`Pallet::propose_curator`]."]
                    propose_curator {
                        #[codec(compact)]
                        parent_bounty_id: ::core::primitive::u32,
                        #[codec(compact)]
                        child_bounty_id: ::core::primitive::u32,
                        curator: ::subxt::utils::MultiAddress<::subxt::utils::AccountId32, ()>,
                        #[codec(compact)]
                        fee: ::core::primitive::u128,
                    },
                    #[codec(index = 2)]
                    #[doc = "See [`Pallet::accept_curator`]."]
                    accept_curator {
                        #[codec(compact)]
                        parent_bounty_id: ::core::primitive::u32,
                        #[codec(compact)]
                        child_bounty_id: ::core::primitive::u32,
                    },
                    #[codec(index = 3)]
                    #[doc = "See [`Pallet::unassign_curator`]."]
                    unassign_curator {
                        #[codec(compact)]
                        parent_bounty_id: ::core::primitive::u32,
                        #[codec(compact)]
                        child_bounty_id: ::core::primitive::u32,
                    },
                    #[codec(index = 4)]
                    #[doc = "See [`Pallet::award_child_bounty`]."]
                    award_child_bounty {
                        #[codec(compact)]
                        parent_bounty_id: ::core::primitive::u32,
                        #[codec(compact)]
                        child_bounty_id: ::core::primitive::u32,
                        beneficiary: ::subxt::utils::MultiAddress<::subxt::utils::AccountId32, ()>,
                    },
                    #[codec(index = 5)]
                    #[doc = "See [`Pallet::claim_child_bounty`]."]
                    claim_child_bounty {
                        #[codec(compact)]
                        parent_bounty_id: ::core::primitive::u32,
                        #[codec(compact)]
                        child_bounty_id: ::core::primitive::u32,
                    },
                    #[codec(index = 6)]
                    #[doc = "See [`Pallet::close_child_bounty`]."]
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
                        beneficiary: ::subxt::utils::AccountId32,
                    },
                    #[codec(index = 2)]
                    #[doc = "A child-bounty is claimed by beneficiary."]
                    Claimed {
                        index: ::core::primitive::u32,
                        child_index: ::core::primitive::u32,
                        payout: ::core::primitive::u128,
                        beneficiary: ::subxt::utils::AccountId32,
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
                    #[doc = "See [`Pallet::vote`]."]
                    vote {
                        #[codec(compact)]
                        poll_index: ::core::primitive::u32,
                        vote: runtime_types::pallet_conviction_voting::vote::AccountVote<
                            ::core::primitive::u128,
                        >,
                    },
                    #[codec(index = 1)]
                    #[doc = "See [`Pallet::delegate`]."]
                    delegate {
                        class: ::core::primitive::u16,
                        to: ::subxt::utils::MultiAddress<::subxt::utils::AccountId32, ()>,
                        conviction: runtime_types::pallet_conviction_voting::conviction::Conviction,
                        balance: ::core::primitive::u128,
                    },
                    #[codec(index = 2)]
                    #[doc = "See [`Pallet::undelegate`]."]
                    undelegate { class: ::core::primitive::u16 },
                    #[codec(index = 3)]
                    #[doc = "See [`Pallet::unlock`]."]
                    unlock {
                        class: ::core::primitive::u16,
                        target: ::subxt::utils::MultiAddress<::subxt::utils::AccountId32, ()>,
                    },
                    #[codec(index = 4)]
                    #[doc = "See [`Pallet::remove_vote`]."]
                    remove_vote {
                        class: ::core::option::Option<::core::primitive::u16>,
                        index: ::core::primitive::u32,
                    },
                    #[codec(index = 5)]
                    #[doc = "See [`Pallet::remove_other_vote`]."]
                    remove_other_vote {
                        target: ::subxt::utils::MultiAddress<::subxt::utils::AccountId32, ()>,
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
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "The `Event` enum of this pallet"]
                pub enum Event {
                    #[codec(index = 0)]
                    #[doc = "An account has delegated their vote to another account. \\[who, target\\]"]
                    Delegated(::subxt::utils::AccountId32, ::subxt::utils::AccountId32),
                    #[codec(index = 1)]
                    #[doc = "An \\[account\\] has cancelled a previous delegation operation."]
                    Undelegated(::subxt::utils::AccountId32),
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
                    pub __subxt_unused_type_params: ::core::marker::PhantomData<_2>,
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
                    ::subxt::ext::codec::CompactAs,
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
                    # [codec (index = 0)] # [doc = "See [`Pallet::submit_unsigned`]."] submit_unsigned { raw_solution: ::std::boxed::Box < runtime_types::pallet_election_provider_multi_phase::RawSolution < runtime_types::vara_runtime::NposSolution16 > > , witness : runtime_types::pallet_election_provider_multi_phase::SolutionOrSnapshotSize , } , # [codec (index = 1)] # [doc = "See [`Pallet::set_minimum_untrusted_score`]."] set_minimum_untrusted_score { maybe_next_score: ::core::option::Option < runtime_types::sp_npos_elections::ElectionScore > , } , # [codec (index = 2)] # [doc = "See [`Pallet::set_emergency_election_result`]."] set_emergency_election_result { supports: ::std::vec::Vec < (::subxt::utils::AccountId32 , runtime_types::sp_npos_elections::Support < ::subxt::utils::AccountId32 > ,) > , } , # [codec (index = 3)] # [doc = "See [`Pallet::submit`]."] submit { raw_solution: ::std::boxed::Box < runtime_types::pallet_election_provider_multi_phase::RawSolution < runtime_types::vara_runtime::NposSolution16 > > , } , # [codec (index = 4)] # [doc = "See [`Pallet::governance_fallback`]."] governance_fallback { maybe_max_voters: ::core::option::Option <::core::primitive::u32 > , maybe_max_targets: ::core::option::Option <::core::primitive::u32 > , } , }
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
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "The `Event` enum of this pallet"]
                pub enum Event {
                    #[codec(index = 0)]
                    #[doc = "A solution was stored with the given compute."]
                    #[doc = ""]
                    #[doc = "The `origin` indicates the origin of the solution. If `origin` is `Some(AccountId)`,"]
                    #[doc = "the stored solution was submited in the signed phase by a miner with the `AccountId`."]
                    #[doc = "Otherwise, the solution was stored either during the unsigned phase or by"]
                    #[doc = "`T::ForceOrigin`. The `bool` is `true` when a previous solution was ejected to make"]
                    #[doc = "room for this one."]
                    SolutionStored {
                        compute:
                            runtime_types::pallet_election_provider_multi_phase::ElectionCompute,
                        origin: ::core::option::Option<::subxt::utils::AccountId32>,
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
                        account: ::subxt::utils::AccountId32,
                        value: ::core::primitive::u128,
                    },
                    #[codec(index = 4)]
                    #[doc = "An account has been slashed for submitting an invalid signed submission."]
                    Slashed {
                        account: ::subxt::utils::AccountId32,
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
                    ::subxt::utils::AccountId32,
                    runtime_types::sp_npos_elections::Support<::subxt::utils::AccountId32>,
                )>,
                pub score: runtime_types::sp_npos_elections::ElectionScore,
                pub compute: runtime_types::pallet_election_provider_multi_phase::ElectionCompute,
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub struct RoundSnapshot<_0, _1> {
                pub voters: ::std::vec::Vec<_1>,
                pub targets: ::std::vec::Vec<_0>,
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
                    #[doc = "See [`Pallet::upload_code`]."]
                    upload_code {
                        code: ::std::vec::Vec<::core::primitive::u8>,
                    },
                    #[codec(index = 1)]
                    #[doc = "See [`Pallet::upload_program`]."]
                    upload_program {
                        code: ::std::vec::Vec<::core::primitive::u8>,
                        salt: ::std::vec::Vec<::core::primitive::u8>,
                        init_payload: ::std::vec::Vec<::core::primitive::u8>,
                        gas_limit: ::core::primitive::u64,
                        value: ::core::primitive::u128,
                        keep_alive: ::core::primitive::bool,
                    },
                    #[codec(index = 2)]
                    #[doc = "See [`Pallet::create_program`]."]
                    create_program {
                        code_id: runtime_types::gprimitives::CodeId,
                        salt: ::std::vec::Vec<::core::primitive::u8>,
                        init_payload: ::std::vec::Vec<::core::primitive::u8>,
                        gas_limit: ::core::primitive::u64,
                        value: ::core::primitive::u128,
                        keep_alive: ::core::primitive::bool,
                    },
                    #[codec(index = 3)]
                    #[doc = "See [`Pallet::send_message`]."]
                    send_message {
                        destination: runtime_types::gprimitives::ActorId,
                        payload: ::std::vec::Vec<::core::primitive::u8>,
                        gas_limit: ::core::primitive::u64,
                        value: ::core::primitive::u128,
                        keep_alive: ::core::primitive::bool,
                    },
                    #[codec(index = 4)]
                    #[doc = "See [`Pallet::send_reply`]."]
                    send_reply {
                        reply_to_id: runtime_types::gprimitives::MessageId,
                        payload: ::std::vec::Vec<::core::primitive::u8>,
                        gas_limit: ::core::primitive::u64,
                        value: ::core::primitive::u128,
                        keep_alive: ::core::primitive::bool,
                    },
                    #[codec(index = 5)]
                    #[doc = "See [`Pallet::claim_value`]."]
                    claim_value {
                        message_id: runtime_types::gprimitives::MessageId,
                    },
                    #[codec(index = 6)]
                    #[doc = "See [`Pallet::run`]."]
                    run {
                        max_gas: ::core::option::Option<::core::primitive::u64>,
                    },
                    #[codec(index = 7)]
                    #[doc = "See [`Pallet::set_execute_inherent`]."]
                    set_execute_inherent { value: ::core::primitive::bool },
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
                    #[doc = "Gear::run() already included in current block."]
                    GearRunAlreadyInBlock,
                    #[codec(index = 15)]
                    #[doc = "The program rent logic is disabled."]
                    ProgramRentDisabled,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "The `Event` enum of this pallet"]
                pub enum Event {
                    #[codec(index = 0)]
                    #[doc = "User sends message to program, which was successfully"]
                    #[doc = "added to the Gear message queue."]
                    MessageQueued {
                        id: runtime_types::gprimitives::MessageId,
                        source: ::subxt::utils::AccountId32,
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
                        statuses: ::subxt::utils::KeyedVec<
                            runtime_types::gprimitives::MessageId,
                            runtime_types::gear_common::event::DispatchStatus,
                        >,
                        state_changes: ::std::vec::Vec<runtime_types::gprimitives::ActorId>,
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
                    pub data_segments_amount: ::core::primitive::u32,
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
                    pub mem_grow_per_page: runtime_types::sp_weights::weight_v2::Weight,
                    pub parachain_read_heuristic: runtime_types::sp_weights::weight_v2::Weight,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct Schedule {
                    pub limits: runtime_types::pallet_gear::schedule::Limits,
                    pub instruction_weights:
                        runtime_types::pallet_gear::schedule::InstructionWeights,
                    pub syscall_weights: runtime_types::pallet_gear::schedule::SyscallWeights,
                    pub memory_weights: runtime_types::pallet_gear::schedule::MemoryWeights,
                    pub module_instantiation_per_byte: runtime_types::sp_weights::weight_v2::Weight,
                    pub db_write_per_byte: runtime_types::sp_weights::weight_v2::Weight,
                    pub db_read_per_byte: runtime_types::sp_weights::weight_v2::Weight,
                    pub code_instrumentation_cost: runtime_types::sp_weights::weight_v2::Weight,
                    pub code_instrumentation_byte_cost:
                        runtime_types::sp_weights::weight_v2::Weight,
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
                }
            }
        }
        pub mod pallet_gear_debug {
            use super::runtime_types;
            pub mod pallet {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "Contains a variant per dispatchable extrinsic that this pallet has."]
                pub enum Call {
                    #[codec(index = 0)]
                    #[doc = "See [`Pallet::enable_debug_mode`]."]
                    enable_debug_mode {
                        debug_mode_on: ::core::primitive::bool,
                    },
                    #[codec(index = 1)]
                    #[doc = "See [`Pallet::exhaust_block_resources`]."]
                    exhaust_block_resources {
                        fraction: runtime_types::sp_arithmetic::per_things::Percent,
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
                #[doc = "The `Error` enum of this pallet."]
                pub enum Error {}
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "The `Event` enum of this pallet"]
                pub enum Event {
                    #[codec(index = 0)]
                    DebugMode(::core::primitive::bool),
                    #[codec(index = 1)]
                    #[doc = "A snapshot of the debug data: programs and message queue ('debug mode' only)"]
                    DebugDataSnapshot(runtime_types::pallet_gear_debug::pallet::DebugData),
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct ProgramDetails {
                    pub id: runtime_types::gprimitives::ActorId,
                    pub state: runtime_types::pallet_gear_debug::pallet::ProgramState,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct ProgramInfo {
                    pub static_pages: runtime_types::gear_core::pages::PagesAmount,
                    pub persistent_pages: ::subxt::utils::KeyedVec<
                        runtime_types::gear_core::pages::Page,
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
                    NotSessionOwner,
                    #[codec(index = 5)]
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
                    #[doc = "See [`Pallet::refill`]."]
                    refill { value: ::core::primitive::u128 },
                    #[codec(index = 1)]
                    #[doc = "See [`Pallet::force_refill`]."]
                    force_refill {
                        from: ::subxt::utils::MultiAddress<::subxt::utils::AccountId32, ()>,
                        value: ::core::primitive::u128,
                    },
                    #[codec(index = 2)]
                    #[doc = "See [`Pallet::withdraw`]."]
                    withdraw {
                        to: ::subxt::utils::MultiAddress<::subxt::utils::AccountId32, ()>,
                        value: ::core::primitive::u128,
                    },
                    #[codec(index = 3)]
                    #[doc = "See [`Pallet::align_supply`]."]
                    align_supply { target: ::core::primitive::u128 },
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
                        payload: ::std::vec::Vec<::core::primitive::u8>,
                        gas_limit: ::core::primitive::u64,
                        value: _0,
                        keep_alive: ::core::primitive::bool,
                    },
                    #[codec(index = 1)]
                    SendReply {
                        reply_to_id: runtime_types::gprimitives::MessageId,
                        payload: ::std::vec::Vec<::core::primitive::u8>,
                        gas_limit: ::core::primitive::u64,
                        value: _0,
                        keep_alive: ::core::primitive::bool,
                    },
                    #[codec(index = 2)]
                    UploadCode {
                        code: ::std::vec::Vec<::core::primitive::u8>,
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
                        ::std::vec::Vec<runtime_types::gprimitives::ActorId>,
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
                    #[doc = "See [`Pallet::issue`]."]
                    issue {
                        spender: ::subxt::utils::AccountId32,
                        balance: ::core::primitive::u128,
                        programs: ::core::option::Option<
                            ::std::vec::Vec<runtime_types::gprimitives::ActorId>,
                        >,
                        code_uploading: ::core::primitive::bool,
                        duration: ::core::primitive::u32,
                    },
                    #[codec(index = 1)]
                    #[doc = "See [`Pallet::call`]."]
                    call {
                        voucher_id: runtime_types::pallet_gear_voucher::internal::VoucherId,
                        call: runtime_types::pallet_gear_voucher::internal::PrepaidCall<
                            ::core::primitive::u128,
                        >,
                    },
                    #[codec(index = 2)]
                    #[doc = "See [`Pallet::revoke`]."]
                    revoke {
                        spender: ::subxt::utils::AccountId32,
                        voucher_id: runtime_types::pallet_gear_voucher::internal::VoucherId,
                    },
                    #[codec(index = 3)]
                    #[doc = "See [`Pallet::update`]."]
                    update {
                        spender: ::subxt::utils::AccountId32,
                        voucher_id: runtime_types::pallet_gear_voucher::internal::VoucherId,
                        move_ownership: ::core::option::Option<::subxt::utils::AccountId32>,
                        balance_top_up: ::core::option::Option<::core::primitive::u128>,
                        append_programs: ::core::option::Option<
                            ::core::option::Option<
                                ::std::vec::Vec<runtime_types::gprimitives::ActorId>,
                            >,
                        >,
                        code_uploading: ::core::option::Option<::core::primitive::bool>,
                        prolong_duration: ::core::option::Option<::core::primitive::u32>,
                    },
                    #[codec(index = 4)]
                    #[doc = "See [`Pallet::call_deprecated`]."]
                    call_deprecated {
                        call: runtime_types::pallet_gear_voucher::internal::PrepaidCall<
                            ::core::primitive::u128,
                        >,
                    },
                    #[codec(index = 5)]
                    #[doc = "See [`Pallet::decline`]."]
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
                        owner: ::subxt::utils::AccountId32,
                        spender: ::subxt::utils::AccountId32,
                        voucher_id: runtime_types::pallet_gear_voucher::internal::VoucherId,
                    },
                    #[codec(index = 1)]
                    #[doc = "Voucher has been revoked by owner."]
                    #[doc = ""]
                    #[doc = "NOTE: currently means only \"refunded\"."]
                    VoucherRevoked {
                        spender: ::subxt::utils::AccountId32,
                        voucher_id: runtime_types::pallet_gear_voucher::internal::VoucherId,
                    },
                    #[codec(index = 2)]
                    #[doc = "Voucher has been updated."]
                    VoucherUpdated {
                        spender: ::subxt::utils::AccountId32,
                        voucher_id: runtime_types::pallet_gear_voucher::internal::VoucherId,
                        new_owner: ::core::option::Option<::subxt::utils::AccountId32>,
                    },
                    #[codec(index = 3)]
                    #[doc = "Voucher has been declined (set to expired state)."]
                    VoucherDeclined {
                        spender: ::subxt::utils::AccountId32,
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
                    #[doc = "See [`Pallet::report_equivocation`]."]
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
                    #[doc = "See [`Pallet::report_equivocation_unsigned`]."]
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
                    #[doc = "See [`Pallet::note_stalled`]."]
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
        pub mod pallet_identity {
            use super::runtime_types;
            pub mod pallet {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "Identity pallet declaration."]
                pub enum Call {
                    #[codec(index = 0)]
                    #[doc = "See [`Pallet::add_registrar`]."]
                    add_registrar {
                        account: ::subxt::utils::MultiAddress<::subxt::utils::AccountId32, ()>,
                    },
                    #[codec(index = 1)]
                    #[doc = "See [`Pallet::set_identity`]."]
                    set_identity {
                        info:
                            ::std::boxed::Box<runtime_types::pallet_identity::simple::IdentityInfo>,
                    },
                    #[codec(index = 2)]
                    #[doc = "See [`Pallet::set_subs`]."]
                    set_subs {
                        subs: ::std::vec::Vec<(
                            ::subxt::utils::AccountId32,
                            runtime_types::pallet_identity::types::Data,
                        )>,
                    },
                    #[codec(index = 3)]
                    #[doc = "See [`Pallet::clear_identity`]."]
                    clear_identity,
                    #[codec(index = 4)]
                    #[doc = "See [`Pallet::request_judgement`]."]
                    request_judgement {
                        #[codec(compact)]
                        reg_index: ::core::primitive::u32,
                        #[codec(compact)]
                        max_fee: ::core::primitive::u128,
                    },
                    #[codec(index = 5)]
                    #[doc = "See [`Pallet::cancel_request`]."]
                    cancel_request { reg_index: ::core::primitive::u32 },
                    #[codec(index = 6)]
                    #[doc = "See [`Pallet::set_fee`]."]
                    set_fee {
                        #[codec(compact)]
                        index: ::core::primitive::u32,
                        #[codec(compact)]
                        fee: ::core::primitive::u128,
                    },
                    #[codec(index = 7)]
                    #[doc = "See [`Pallet::set_account_id`]."]
                    set_account_id {
                        #[codec(compact)]
                        index: ::core::primitive::u32,
                        new: ::subxt::utils::MultiAddress<::subxt::utils::AccountId32, ()>,
                    },
                    #[codec(index = 8)]
                    #[doc = "See [`Pallet::set_fields`]."]
                    set_fields {
                        #[codec(compact)]
                        index: ::core::primitive::u32,
                        fields: runtime_types::pallet_identity::types::BitFlags<
                            runtime_types::pallet_identity::simple::IdentityField,
                        >,
                    },
                    #[codec(index = 9)]
                    #[doc = "See [`Pallet::provide_judgement`]."]
                    provide_judgement {
                        #[codec(compact)]
                        reg_index: ::core::primitive::u32,
                        target: ::subxt::utils::MultiAddress<::subxt::utils::AccountId32, ()>,
                        judgement: runtime_types::pallet_identity::types::Judgement<
                            ::core::primitive::u128,
                        >,
                        identity: ::subxt::utils::H256,
                    },
                    #[codec(index = 10)]
                    #[doc = "See [`Pallet::kill_identity`]."]
                    kill_identity {
                        target: ::subxt::utils::MultiAddress<::subxt::utils::AccountId32, ()>,
                    },
                    #[codec(index = 11)]
                    #[doc = "See [`Pallet::add_sub`]."]
                    add_sub {
                        sub: ::subxt::utils::MultiAddress<::subxt::utils::AccountId32, ()>,
                        data: runtime_types::pallet_identity::types::Data,
                    },
                    #[codec(index = 12)]
                    #[doc = "See [`Pallet::rename_sub`]."]
                    rename_sub {
                        sub: ::subxt::utils::MultiAddress<::subxt::utils::AccountId32, ()>,
                        data: runtime_types::pallet_identity::types::Data,
                    },
                    #[codec(index = 13)]
                    #[doc = "See [`Pallet::remove_sub`]."]
                    remove_sub {
                        sub: ::subxt::utils::MultiAddress<::subxt::utils::AccountId32, ()>,
                    },
                    #[codec(index = 14)]
                    #[doc = "See [`Pallet::quit_sub`]."]
                    quit_sub,
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
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "The `Event` enum of this pallet"]
                pub enum Event {
                    #[codec(index = 0)]
                    #[doc = "A name was set or reset (which will remove all judgements)."]
                    IdentitySet { who: ::subxt::utils::AccountId32 },
                    #[codec(index = 1)]
                    #[doc = "A name was cleared, and the given balance returned."]
                    IdentityCleared {
                        who: ::subxt::utils::AccountId32,
                        deposit: ::core::primitive::u128,
                    },
                    #[codec(index = 2)]
                    #[doc = "A name was removed and the given balance slashed."]
                    IdentityKilled {
                        who: ::subxt::utils::AccountId32,
                        deposit: ::core::primitive::u128,
                    },
                    #[codec(index = 3)]
                    #[doc = "A judgement was asked from a registrar."]
                    JudgementRequested {
                        who: ::subxt::utils::AccountId32,
                        registrar_index: ::core::primitive::u32,
                    },
                    #[codec(index = 4)]
                    #[doc = "A judgement request was retracted."]
                    JudgementUnrequested {
                        who: ::subxt::utils::AccountId32,
                        registrar_index: ::core::primitive::u32,
                    },
                    #[codec(index = 5)]
                    #[doc = "A judgement was given by a registrar."]
                    JudgementGiven {
                        target: ::subxt::utils::AccountId32,
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
                        sub: ::subxt::utils::AccountId32,
                        main: ::subxt::utils::AccountId32,
                        deposit: ::core::primitive::u128,
                    },
                    #[codec(index = 8)]
                    #[doc = "A sub-identity was removed from an identity and the deposit freed."]
                    SubIdentityRemoved {
                        sub: ::subxt::utils::AccountId32,
                        main: ::subxt::utils::AccountId32,
                        deposit: ::core::primitive::u128,
                    },
                    #[codec(index = 9)]
                    #[doc = "A sub-identity was cleared, and the given deposit repatriated from the"]
                    #[doc = "main identity account to the sub-identity account."]
                    SubIdentityRevoked {
                        sub: ::subxt::utils::AccountId32,
                        main: ::subxt::utils::AccountId32,
                        deposit: ::core::primitive::u128,
                    },
                }
            }
            pub mod simple {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum IdentityField {
                    #[codec(index = 0)]
                    Display,
                    #[codec(index = 1)]
                    Legal,
                    #[codec(index = 2)]
                    Web,
                    #[codec(index = 3)]
                    Riot,
                    #[codec(index = 4)]
                    Email,
                    #[codec(index = 5)]
                    PgpFingerprint,
                    #[codec(index = 6)]
                    Image,
                    #[codec(index = 7)]
                    Twitter,
                }
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
            pub mod types {
                use super::runtime_types;
                #[derive(
                    ::subxt::ext::codec::CompactAs,
                    Debug,
                    crate::gp::Decode,
                    crate::gp::DecodeAsType,
                    crate::gp::Encode,
                )]
                pub struct BitFlags<_0>(
                    pub ::core::primitive::u64,
                    #[codec(skip)] pub ::core::marker::PhantomData<_0>,
                );
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
                    pub fields: runtime_types::pallet_identity::types::BitFlags<_2>,
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
                    #[doc = "See [`Pallet::heartbeat`]."]
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
                        offline: ::std::vec::Vec<(
                            ::subxt::utils::AccountId32,
                            runtime_types::pallet_staking::Exposure<
                                ::subxt::utils::AccountId32,
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
                    pub struct Public(pub runtime_types::sp_core::sr25519::Public);
                    #[derive(
                        Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode,
                    )]
                    pub struct Signature(pub runtime_types::sp_core::sr25519::Signature);
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
                    #[doc = "See [`Pallet::as_multi_threshold_1`]."]
                    as_multi_threshold_1 {
                        other_signatories: ::std::vec::Vec<::subxt::utils::AccountId32>,
                        call: ::std::boxed::Box<runtime_types::vara_runtime::RuntimeCall>,
                    },
                    #[codec(index = 1)]
                    #[doc = "See [`Pallet::as_multi`]."]
                    as_multi {
                        threshold: ::core::primitive::u16,
                        other_signatories: ::std::vec::Vec<::subxt::utils::AccountId32>,
                        maybe_timepoint: ::core::option::Option<
                            runtime_types::pallet_multisig::Timepoint<::core::primitive::u32>,
                        >,
                        call: ::std::boxed::Box<runtime_types::vara_runtime::RuntimeCall>,
                        max_weight: runtime_types::sp_weights::weight_v2::Weight,
                    },
                    #[codec(index = 2)]
                    #[doc = "See [`Pallet::approve_as_multi`]."]
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
                    #[doc = "See [`Pallet::cancel_as_multi`]."]
                    cancel_as_multi {
                        threshold: ::core::primitive::u16,
                        other_signatories: ::std::vec::Vec<::subxt::utils::AccountId32>,
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
                    #[doc = "See [`Pallet::join`]."]
                    join {
                        #[codec(compact)]
                        amount: ::core::primitive::u128,
                        pool_id: ::core::primitive::u32,
                    },
                    #[codec(index = 1)]
                    #[doc = "See [`Pallet::bond_extra`]."]
                    bond_extra {
                        extra: runtime_types::pallet_nomination_pools::BondExtra<
                            ::core::primitive::u128,
                        >,
                    },
                    #[codec(index = 2)]
                    #[doc = "See [`Pallet::claim_payout`]."]
                    claim_payout,
                    #[codec(index = 3)]
                    #[doc = "See [`Pallet::unbond`]."]
                    unbond {
                        member_account:
                            ::subxt::utils::MultiAddress<::subxt::utils::AccountId32, ()>,
                        #[codec(compact)]
                        unbonding_points: ::core::primitive::u128,
                    },
                    #[codec(index = 4)]
                    #[doc = "See [`Pallet::pool_withdraw_unbonded`]."]
                    pool_withdraw_unbonded {
                        pool_id: ::core::primitive::u32,
                        num_slashing_spans: ::core::primitive::u32,
                    },
                    #[codec(index = 5)]
                    #[doc = "See [`Pallet::withdraw_unbonded`]."]
                    withdraw_unbonded {
                        member_account:
                            ::subxt::utils::MultiAddress<::subxt::utils::AccountId32, ()>,
                        num_slashing_spans: ::core::primitive::u32,
                    },
                    #[codec(index = 6)]
                    #[doc = "See [`Pallet::create`]."]
                    create {
                        #[codec(compact)]
                        amount: ::core::primitive::u128,
                        root: ::subxt::utils::MultiAddress<::subxt::utils::AccountId32, ()>,
                        nominator: ::subxt::utils::MultiAddress<::subxt::utils::AccountId32, ()>,
                        bouncer: ::subxt::utils::MultiAddress<::subxt::utils::AccountId32, ()>,
                    },
                    #[codec(index = 7)]
                    #[doc = "See [`Pallet::create_with_pool_id`]."]
                    create_with_pool_id {
                        #[codec(compact)]
                        amount: ::core::primitive::u128,
                        root: ::subxt::utils::MultiAddress<::subxt::utils::AccountId32, ()>,
                        nominator: ::subxt::utils::MultiAddress<::subxt::utils::AccountId32, ()>,
                        bouncer: ::subxt::utils::MultiAddress<::subxt::utils::AccountId32, ()>,
                        pool_id: ::core::primitive::u32,
                    },
                    #[codec(index = 8)]
                    #[doc = "See [`Pallet::nominate`]."]
                    nominate {
                        pool_id: ::core::primitive::u32,
                        validators: ::std::vec::Vec<::subxt::utils::AccountId32>,
                    },
                    #[codec(index = 9)]
                    #[doc = "See [`Pallet::set_state`]."]
                    set_state {
                        pool_id: ::core::primitive::u32,
                        state: runtime_types::pallet_nomination_pools::PoolState,
                    },
                    #[codec(index = 10)]
                    #[doc = "See [`Pallet::set_metadata`]."]
                    set_metadata {
                        pool_id: ::core::primitive::u32,
                        metadata: ::std::vec::Vec<::core::primitive::u8>,
                    },
                    #[codec(index = 11)]
                    #[doc = "See [`Pallet::set_configs`]."]
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
                    #[doc = "See [`Pallet::update_roles`]."]
                    update_roles {
                        pool_id: ::core::primitive::u32,
                        new_root: runtime_types::pallet_nomination_pools::ConfigOp<
                            ::subxt::utils::AccountId32,
                        >,
                        new_nominator: runtime_types::pallet_nomination_pools::ConfigOp<
                            ::subxt::utils::AccountId32,
                        >,
                        new_bouncer: runtime_types::pallet_nomination_pools::ConfigOp<
                            ::subxt::utils::AccountId32,
                        >,
                    },
                    #[codec(index = 13)]
                    #[doc = "See [`Pallet::chill`]."]
                    chill { pool_id: ::core::primitive::u32 },
                    #[codec(index = 14)]
                    #[doc = "See [`Pallet::bond_extra_other`]."]
                    bond_extra_other {
                        member: ::subxt::utils::MultiAddress<::subxt::utils::AccountId32, ()>,
                        extra: runtime_types::pallet_nomination_pools::BondExtra<
                            ::core::primitive::u128,
                        >,
                    },
                    #[codec(index = 15)]
                    #[doc = "See [`Pallet::set_claim_permission`]."]
                    set_claim_permission {
                        permission: runtime_types::pallet_nomination_pools::ClaimPermission,
                    },
                    #[codec(index = 16)]
                    #[doc = "See [`Pallet::claim_payout_other`]."]
                    claim_payout_other { other: ::subxt::utils::AccountId32 },
                    #[codec(index = 17)]
                    #[doc = "See [`Pallet::set_commission`]."]
                    set_commission {
                        pool_id: ::core::primitive::u32,
                        new_commission: ::core::option::Option<(
                            runtime_types::sp_arithmetic::per_things::Perbill,
                            ::subxt::utils::AccountId32,
                        )>,
                    },
                    #[codec(index = 18)]
                    #[doc = "See [`Pallet::set_commission_max`]."]
                    set_commission_max {
                        pool_id: ::core::primitive::u32,
                        max_commission: runtime_types::sp_arithmetic::per_things::Perbill,
                    },
                    #[codec(index = 19)]
                    #[doc = "See [`Pallet::set_commission_change_rate`]."]
                    set_commission_change_rate {
                        pool_id: ::core::primitive::u32,
                        change_rate: runtime_types::pallet_nomination_pools::CommissionChangeRate<
                            ::core::primitive::u32,
                        >,
                    },
                    #[codec(index = 20)]
                    #[doc = "See [`Pallet::claim_commission`]."]
                    claim_commission { pool_id: ::core::primitive::u32 },
                    #[codec(index = 21)]
                    #[doc = "See [`Pallet::adjust_pool_deposit`]."]
                    adjust_pool_deposit { pool_id: ::core::primitive::u32 },
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
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "Events of this pallet."]
                pub enum Event {
                    #[codec(index = 0)]
                    #[doc = "A pool has been created."]
                    Created {
                        depositor: ::subxt::utils::AccountId32,
                        pool_id: ::core::primitive::u32,
                    },
                    #[codec(index = 1)]
                    #[doc = "A member has became bonded in a pool."]
                    Bonded {
                        member: ::subxt::utils::AccountId32,
                        pool_id: ::core::primitive::u32,
                        bonded: ::core::primitive::u128,
                        joined: ::core::primitive::bool,
                    },
                    #[codec(index = 2)]
                    #[doc = "A payout has been made to a member."]
                    PaidOut {
                        member: ::subxt::utils::AccountId32,
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
                        member: ::subxt::utils::AccountId32,
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
                        member: ::subxt::utils::AccountId32,
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
                    MemberRemoved {
                        pool_id: ::core::primitive::u32,
                        member: ::subxt::utils::AccountId32,
                    },
                    #[codec(index = 8)]
                    #[doc = "The roles of a pool have been updated to the given new roles. Note that the depositor"]
                    #[doc = "can never change."]
                    RolesUpdated {
                        root: ::core::option::Option<::subxt::utils::AccountId32>,
                        bouncer: ::core::option::Option<::subxt::utils::AccountId32>,
                        nominator: ::core::option::Option<::subxt::utils::AccountId32>,
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
                            ::subxt::utils::AccountId32,
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
                    #[doc = "Pool commission has been claimed."]
                    PoolCommissionClaimed {
                        pool_id: ::core::primitive::u32,
                        commission: ::core::primitive::u128,
                    },
                    #[codec(index = 15)]
                    #[doc = "Topped up deficit in frozen ED of the reward pool."]
                    MinBalanceDeficitAdjusted {
                        pool_id: ::core::primitive::u32,
                        amount: ::core::primitive::u128,
                    },
                    #[codec(index = 16)]
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
                pub roles:
                    runtime_types::pallet_nomination_pools::PoolRoles<::subxt::utils::AccountId32>,
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
                    ::subxt::utils::AccountId32,
                )>,
                pub max: ::core::option::Option<runtime_types::sp_arithmetic::per_things::Perbill>,
                pub change_rate: ::core::option::Option<
                    runtime_types::pallet_nomination_pools::CommissionChangeRate<
                        ::core::primitive::u32,
                    >,
                >,
                pub throttle_from: ::core::option::Option<::core::primitive::u32>,
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub struct CommissionChangeRate<_0> {
                pub max_increase: runtime_types::sp_arithmetic::per_things::Perbill,
                pub min_delay: _0,
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
                        timeslot: ::std::vec::Vec<::core::primitive::u8>,
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
                    #[doc = "See [`Pallet::note_preimage`]."]
                    note_preimage {
                        bytes: ::std::vec::Vec<::core::primitive::u8>,
                    },
                    #[codec(index = 1)]
                    #[doc = "See [`Pallet::unnote_preimage`]."]
                    unnote_preimage { hash: ::subxt::utils::H256 },
                    #[codec(index = 2)]
                    #[doc = "See [`Pallet::request_preimage`]."]
                    request_preimage { hash: ::subxt::utils::H256 },
                    #[codec(index = 3)]
                    #[doc = "See [`Pallet::unrequest_preimage`]."]
                    unrequest_preimage { hash: ::subxt::utils::H256 },
                    #[codec(index = 4)]
                    #[doc = "See [`Pallet::ensure_updated`]."]
                    ensure_updated {
                        hashes: ::std::vec::Vec<::subxt::utils::H256>,
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
                    Noted { hash: ::subxt::utils::H256 },
                    #[codec(index = 1)]
                    #[doc = "A preimage has been requested."]
                    Requested { hash: ::subxt::utils::H256 },
                    #[codec(index = 2)]
                    #[doc = "A preimage has ben cleared."]
                    Cleared { hash: ::subxt::utils::H256 },
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
                    #[doc = "See [`Pallet::proxy`]."]
                    proxy {
                        real: ::subxt::utils::MultiAddress<::subxt::utils::AccountId32, ()>,
                        force_proxy_type:
                            ::core::option::Option<runtime_types::vara_runtime::ProxyType>,
                        call: ::std::boxed::Box<runtime_types::vara_runtime::RuntimeCall>,
                    },
                    #[codec(index = 1)]
                    #[doc = "See [`Pallet::add_proxy`]."]
                    add_proxy {
                        delegate: ::subxt::utils::MultiAddress<::subxt::utils::AccountId32, ()>,
                        proxy_type: runtime_types::vara_runtime::ProxyType,
                        delay: ::core::primitive::u32,
                    },
                    #[codec(index = 2)]
                    #[doc = "See [`Pallet::remove_proxy`]."]
                    remove_proxy {
                        delegate: ::subxt::utils::MultiAddress<::subxt::utils::AccountId32, ()>,
                        proxy_type: runtime_types::vara_runtime::ProxyType,
                        delay: ::core::primitive::u32,
                    },
                    #[codec(index = 3)]
                    #[doc = "See [`Pallet::remove_proxies`]."]
                    remove_proxies,
                    #[codec(index = 4)]
                    #[doc = "See [`Pallet::create_pure`]."]
                    create_pure {
                        proxy_type: runtime_types::vara_runtime::ProxyType,
                        delay: ::core::primitive::u32,
                        index: ::core::primitive::u16,
                    },
                    #[codec(index = 5)]
                    #[doc = "See [`Pallet::kill_pure`]."]
                    kill_pure {
                        spawner: ::subxt::utils::MultiAddress<::subxt::utils::AccountId32, ()>,
                        proxy_type: runtime_types::vara_runtime::ProxyType,
                        index: ::core::primitive::u16,
                        #[codec(compact)]
                        height: ::core::primitive::u32,
                        #[codec(compact)]
                        ext_index: ::core::primitive::u32,
                    },
                    #[codec(index = 6)]
                    #[doc = "See [`Pallet::announce`]."]
                    announce {
                        real: ::subxt::utils::MultiAddress<::subxt::utils::AccountId32, ()>,
                        call_hash: ::subxt::utils::H256,
                    },
                    #[codec(index = 7)]
                    #[doc = "See [`Pallet::remove_announcement`]."]
                    remove_announcement {
                        real: ::subxt::utils::MultiAddress<::subxt::utils::AccountId32, ()>,
                        call_hash: ::subxt::utils::H256,
                    },
                    #[codec(index = 8)]
                    #[doc = "See [`Pallet::reject_announcement`]."]
                    reject_announcement {
                        delegate: ::subxt::utils::MultiAddress<::subxt::utils::AccountId32, ()>,
                        call_hash: ::subxt::utils::H256,
                    },
                    #[codec(index = 9)]
                    #[doc = "See [`Pallet::proxy_announced`]."]
                    proxy_announced {
                        delegate: ::subxt::utils::MultiAddress<::subxt::utils::AccountId32, ()>,
                        real: ::subxt::utils::MultiAddress<::subxt::utils::AccountId32, ()>,
                        force_proxy_type:
                            ::core::option::Option<runtime_types::vara_runtime::ProxyType>,
                        call: ::std::boxed::Box<runtime_types::vara_runtime::RuntimeCall>,
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
                        pure: ::subxt::utils::AccountId32,
                        who: ::subxt::utils::AccountId32,
                        proxy_type: runtime_types::vara_runtime::ProxyType,
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
                        proxy_type: runtime_types::vara_runtime::ProxyType,
                        delay: ::core::primitive::u32,
                    },
                    #[codec(index = 4)]
                    #[doc = "A proxy was removed."]
                    ProxyRemoved {
                        delegator: ::subxt::utils::AccountId32,
                        delegatee: ::subxt::utils::AccountId32,
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
                    #[doc = "See [`Pallet::add_member`]."]
                    add_member {
                        who: ::subxt::utils::MultiAddress<::subxt::utils::AccountId32, ()>,
                    },
                    #[codec(index = 1)]
                    #[doc = "See [`Pallet::promote_member`]."]
                    promote_member {
                        who: ::subxt::utils::MultiAddress<::subxt::utils::AccountId32, ()>,
                    },
                    #[codec(index = 2)]
                    #[doc = "See [`Pallet::demote_member`]."]
                    demote_member {
                        who: ::subxt::utils::MultiAddress<::subxt::utils::AccountId32, ()>,
                    },
                    #[codec(index = 3)]
                    #[doc = "See [`Pallet::remove_member`]."]
                    remove_member {
                        who: ::subxt::utils::MultiAddress<::subxt::utils::AccountId32, ()>,
                        min_rank: ::core::primitive::u16,
                    },
                    #[codec(index = 4)]
                    #[doc = "See [`Pallet::vote`]."]
                    vote {
                        poll: ::core::primitive::u32,
                        aye: ::core::primitive::bool,
                    },
                    #[codec(index = 5)]
                    #[doc = "See [`Pallet::cleanup_poll`]."]
                    cleanup_poll {
                        poll_index: ::core::primitive::u32,
                        max: ::core::primitive::u32,
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
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "The `Event` enum of this pallet"]
                pub enum Event {
                    #[codec(index = 0)]
                    #[doc = "A member `who` has been added."]
                    MemberAdded { who: ::subxt::utils::AccountId32 },
                    #[codec(index = 1)]
                    #[doc = "The member `who`se rank has been changed to the given `rank`."]
                    RankChanged {
                        who: ::subxt::utils::AccountId32,
                        rank: ::core::primitive::u16,
                    },
                    #[codec(index = 2)]
                    #[doc = "The member `who` of given `rank` has been removed from the collective."]
                    MemberRemoved {
                        who: ::subxt::utils::AccountId32,
                        rank: ::core::primitive::u16,
                    },
                    #[codec(index = 3)]
                    #[doc = "The member `who` has voted for the `poll` with the given `vote` leading to an updated"]
                    #[doc = "`tally`."]
                    Voted {
                        who: ::subxt::utils::AccountId32,
                        poll: ::core::primitive::u32,
                        vote: runtime_types::pallet_ranked_collective::VoteRecord,
                        tally: runtime_types::pallet_ranked_collective::Tally,
                    },
                }
            }
            #[derive(
                ::subxt::ext::codec::CompactAs,
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
                    #[doc = "See [`Pallet::submit`]."]
                    submit {
                        proposal_origin:
                            ::std::boxed::Box<runtime_types::vara_runtime::OriginCaller>,
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
                    #[doc = "See [`Pallet::place_decision_deposit`]."]
                    place_decision_deposit { index: ::core::primitive::u32 },
                    #[codec(index = 2)]
                    #[doc = "See [`Pallet::refund_decision_deposit`]."]
                    refund_decision_deposit { index: ::core::primitive::u32 },
                    #[codec(index = 3)]
                    #[doc = "See [`Pallet::cancel`]."]
                    cancel { index: ::core::primitive::u32 },
                    #[codec(index = 4)]
                    #[doc = "See [`Pallet::kill`]."]
                    kill { index: ::core::primitive::u32 },
                    #[codec(index = 5)]
                    #[doc = "See [`Pallet::nudge_referendum`]."]
                    nudge_referendum { index: ::core::primitive::u32 },
                    #[codec(index = 6)]
                    #[doc = "See [`Pallet::one_fewer_deciding`]."]
                    one_fewer_deciding { track: ::core::primitive::u16 },
                    #[codec(index = 7)]
                    #[doc = "See [`Pallet::refund_submission_deposit`]."]
                    refund_submission_deposit { index: ::core::primitive::u32 },
                    #[codec(index = 8)]
                    #[doc = "See [`Pallet::set_metadata`]."]
                    set_metadata {
                        index: ::core::primitive::u32,
                        maybe_hash: ::core::option::Option<::subxt::utils::H256>,
                    },
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "Contains a variant per dispatchable extrinsic that this pallet has."]
                pub enum Call2 {
                    #[codec(index = 0)]
                    #[doc = "See [`Pallet::submit`]."]
                    submit {
                        proposal_origin:
                            ::std::boxed::Box<runtime_types::vara_runtime::OriginCaller>,
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
                    #[doc = "See [`Pallet::place_decision_deposit`]."]
                    place_decision_deposit { index: ::core::primitive::u32 },
                    #[codec(index = 2)]
                    #[doc = "See [`Pallet::refund_decision_deposit`]."]
                    refund_decision_deposit { index: ::core::primitive::u32 },
                    #[codec(index = 3)]
                    #[doc = "See [`Pallet::cancel`]."]
                    cancel { index: ::core::primitive::u32 },
                    #[codec(index = 4)]
                    #[doc = "See [`Pallet::kill`]."]
                    kill { index: ::core::primitive::u32 },
                    #[codec(index = 5)]
                    #[doc = "See [`Pallet::nudge_referendum`]."]
                    nudge_referendum { index: ::core::primitive::u32 },
                    #[codec(index = 6)]
                    #[doc = "See [`Pallet::one_fewer_deciding`]."]
                    one_fewer_deciding { track: ::core::primitive::u16 },
                    #[codec(index = 7)]
                    #[doc = "See [`Pallet::refund_submission_deposit`]."]
                    refund_submission_deposit { index: ::core::primitive::u32 },
                    #[codec(index = 8)]
                    #[doc = "See [`Pallet::set_metadata`]."]
                    set_metadata {
                        index: ::core::primitive::u32,
                        maybe_hash: ::core::option::Option<::subxt::utils::H256>,
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
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "The `Error` enum of this pallet."]
                pub enum Error2 {
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
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "The `Event` enum of this pallet"]
                pub enum Event {
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
                        who: ::subxt::utils::AccountId32,
                        amount: ::core::primitive::u128,
                    },
                    #[codec(index = 2)]
                    #[doc = "The decision deposit has been refunded."]
                    DecisionDepositRefunded {
                        index: ::core::primitive::u32,
                        who: ::subxt::utils::AccountId32,
                        amount: ::core::primitive::u128,
                    },
                    #[codec(index = 3)]
                    #[doc = "A deposit has been slashaed."]
                    DepositSlashed {
                        who: ::subxt::utils::AccountId32,
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
                        who: ::subxt::utils::AccountId32,
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
                        who: ::subxt::utils::AccountId32,
                        amount: ::core::primitive::u128,
                    },
                    #[codec(index = 2)]
                    #[doc = "The decision deposit has been refunded."]
                    DecisionDepositRefunded {
                        index: ::core::primitive::u32,
                        who: ::subxt::utils::AccountId32,
                        amount: ::core::primitive::u128,
                    },
                    #[codec(index = 3)]
                    #[doc = "A deposit has been slashaed."]
                    DepositSlashed {
                        who: ::subxt::utils::AccountId32,
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
                        who: ::subxt::utils::AccountId32,
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
                    pub name: ::std::string::String,
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
                    #[doc = "See [`Pallet::schedule`]."]
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
                    #[doc = "See [`Pallet::cancel`]."]
                    cancel {
                        when: ::core::primitive::u32,
                        index: ::core::primitive::u32,
                    },
                    #[codec(index = 2)]
                    #[doc = "See [`Pallet::schedule_named`]."]
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
                    #[doc = "See [`Pallet::cancel_named`]."]
                    cancel_named {
                        id: [::core::primitive::u8; 32usize],
                    },
                    #[codec(index = 4)]
                    #[doc = "See [`Pallet::schedule_after`]."]
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
                    #[doc = "See [`Pallet::schedule_named_after`]."]
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
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
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
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "Contains a variant per dispatchable extrinsic that this pallet has."]
                pub enum Call {
                    #[codec(index = 0)]
                    #[doc = "See [`Pallet::set_keys`]."]
                    set_keys {
                        keys: runtime_types::vara_runtime::SessionKeys,
                        proof: ::std::vec::Vec<::core::primitive::u8>,
                    },
                    #[codec(index = 1)]
                    #[doc = "See [`Pallet::purge_keys`]."]
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
                        #[doc = "See [`Pallet::bond`]."]
                        bond {
                            #[codec(compact)]
                            value: ::core::primitive::u128,
                            payee: runtime_types::pallet_staking::RewardDestination<
                                ::subxt::utils::AccountId32,
                            >,
                        },
                        #[codec(index = 1)]
                        #[doc = "See [`Pallet::bond_extra`]."]
                        bond_extra {
                            #[codec(compact)]
                            max_additional: ::core::primitive::u128,
                        },
                        #[codec(index = 2)]
                        #[doc = "See [`Pallet::unbond`]."]
                        unbond {
                            #[codec(compact)]
                            value: ::core::primitive::u128,
                        },
                        #[codec(index = 3)]
                        #[doc = "See [`Pallet::withdraw_unbonded`]."]
                        withdraw_unbonded {
                            num_slashing_spans: ::core::primitive::u32,
                        },
                        #[codec(index = 4)]
                        #[doc = "See [`Pallet::validate`]."]
                        validate {
                            prefs: runtime_types::pallet_staking::ValidatorPrefs,
                        },
                        #[codec(index = 5)]
                        #[doc = "See [`Pallet::nominate`]."]
                        nominate {
                            targets: ::std::vec::Vec<
                                ::subxt::utils::MultiAddress<::subxt::utils::AccountId32, ()>,
                            >,
                        },
                        #[codec(index = 6)]
                        #[doc = "See [`Pallet::chill`]."]
                        chill,
                        #[codec(index = 7)]
                        #[doc = "See [`Pallet::set_payee`]."]
                        set_payee {
                            payee: runtime_types::pallet_staking::RewardDestination<
                                ::subxt::utils::AccountId32,
                            >,
                        },
                        #[codec(index = 8)]
                        #[doc = "See [`Pallet::set_controller`]."]
                        set_controller,
                        #[codec(index = 9)]
                        #[doc = "See [`Pallet::set_validator_count`]."]
                        set_validator_count {
                            #[codec(compact)]
                            new: ::core::primitive::u32,
                        },
                        #[codec(index = 10)]
                        #[doc = "See [`Pallet::increase_validator_count`]."]
                        increase_validator_count {
                            #[codec(compact)]
                            additional: ::core::primitive::u32,
                        },
                        #[codec(index = 11)]
                        #[doc = "See [`Pallet::scale_validator_count`]."]
                        scale_validator_count {
                            factor: runtime_types::sp_arithmetic::per_things::Percent,
                        },
                        #[codec(index = 12)]
                        #[doc = "See [`Pallet::force_no_eras`]."]
                        force_no_eras,
                        #[codec(index = 13)]
                        #[doc = "See [`Pallet::force_new_era`]."]
                        force_new_era,
                        #[codec(index = 14)]
                        #[doc = "See [`Pallet::set_invulnerables`]."]
                        set_invulnerables {
                            invulnerables: ::std::vec::Vec<::subxt::utils::AccountId32>,
                        },
                        #[codec(index = 15)]
                        #[doc = "See [`Pallet::force_unstake`]."]
                        force_unstake {
                            stash: ::subxt::utils::AccountId32,
                            num_slashing_spans: ::core::primitive::u32,
                        },
                        #[codec(index = 16)]
                        #[doc = "See [`Pallet::force_new_era_always`]."]
                        force_new_era_always,
                        #[codec(index = 17)]
                        #[doc = "See [`Pallet::cancel_deferred_slash`]."]
                        cancel_deferred_slash {
                            era: ::core::primitive::u32,
                            slash_indices: ::std::vec::Vec<::core::primitive::u32>,
                        },
                        #[codec(index = 18)]
                        #[doc = "See [`Pallet::payout_stakers`]."]
                        payout_stakers {
                            validator_stash: ::subxt::utils::AccountId32,
                            era: ::core::primitive::u32,
                        },
                        #[codec(index = 19)]
                        #[doc = "See [`Pallet::rebond`]."]
                        rebond {
                            #[codec(compact)]
                            value: ::core::primitive::u128,
                        },
                        #[codec(index = 20)]
                        #[doc = "See [`Pallet::reap_stash`]."]
                        reap_stash {
                            stash: ::subxt::utils::AccountId32,
                            num_slashing_spans: ::core::primitive::u32,
                        },
                        #[codec(index = 21)]
                        #[doc = "See [`Pallet::kick`]."]
                        kick {
                            who: ::std::vec::Vec<
                                ::subxt::utils::MultiAddress<::subxt::utils::AccountId32, ()>,
                            >,
                        },
                        #[codec(index = 22)]
                        #[doc = "See [`Pallet::set_staking_configs`]."]
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
                        #[doc = "See [`Pallet::chill_other`]."]
                        chill_other {
                            controller: ::subxt::utils::AccountId32,
                        },
                        #[codec(index = 24)]
                        #[doc = "See [`Pallet::force_apply_min_commission`]."]
                        force_apply_min_commission {
                            validator_stash: ::subxt::utils::AccountId32,
                        },
                        #[codec(index = 25)]
                        #[doc = "See [`Pallet::set_min_commission`]."]
                        set_min_commission {
                            new: runtime_types::sp_arithmetic::per_things::Perbill,
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
                            stash: ::subxt::utils::AccountId32,
                            dest: runtime_types::pallet_staking::RewardDestination<
                                ::subxt::utils::AccountId32,
                            >,
                            amount: ::core::primitive::u128,
                        },
                        #[codec(index = 2)]
                        #[doc = "A staker (validator or nominator) has been slashed by the given amount."]
                        Slashed {
                            staker: ::subxt::utils::AccountId32,
                            amount: ::core::primitive::u128,
                        },
                        #[codec(index = 3)]
                        #[doc = "A slash for the given validator, for the given percentage of their stake, at the given"]
                        #[doc = "era as been reported."]
                        SlashReported {
                            validator: ::subxt::utils::AccountId32,
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
                            stash: ::subxt::utils::AccountId32,
                            amount: ::core::primitive::u128,
                        },
                        #[codec(index = 7)]
                        #[doc = "An account has unbonded this amount."]
                        Unbonded {
                            stash: ::subxt::utils::AccountId32,
                            amount: ::core::primitive::u128,
                        },
                        #[codec(index = 8)]
                        #[doc = "An account has called `withdraw_unbonded` and removed unbonding chunks worth `Balance`"]
                        #[doc = "from the unlocking queue."]
                        Withdrawn {
                            stash: ::subxt::utils::AccountId32,
                            amount: ::core::primitive::u128,
                        },
                        #[codec(index = 9)]
                        #[doc = "A nominator has been kicked from a validator."]
                        Kicked {
                            nominator: ::subxt::utils::AccountId32,
                            stash: ::subxt::utils::AccountId32,
                        },
                        #[codec(index = 10)]
                        #[doc = "The election failed. No new era is planned."]
                        StakingElectionFailed,
                        #[codec(index = 11)]
                        #[doc = "An account has stopped participating as either a validator or nominator."]
                        Chilled { stash: ::subxt::utils::AccountId32 },
                        #[codec(index = 12)]
                        #[doc = "The stakers' rewards are getting paid."]
                        PayoutStarted {
                            era_index: ::core::primitive::u32,
                            validator_stash: ::subxt::utils::AccountId32,
                        },
                        #[codec(index = 13)]
                        #[doc = "A validator has set their preferences."]
                        ValidatorPrefsSet {
                            stash: ::subxt::utils::AccountId32,
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
                    pub prior: ::std::vec::Vec<::core::primitive::u32>,
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
                pub individual: ::subxt::utils::KeyedVec<_0, ::core::primitive::u32>,
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub struct Exposure<_0, _1> {
                #[codec(compact)]
                pub total: _1,
                #[codec(compact)]
                pub own: _1,
                pub others:
                    ::std::vec::Vec<runtime_types::pallet_staking::IndividualExposure<_0, _1>>,
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
            pub struct IndividualExposure<_0, _1> {
                pub who: _0,
                #[codec(compact)]
                pub value: _1,
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub struct Nominations {
                pub targets: runtime_types::bounded_collections::bounded_vec::BoundedVec<
                    ::subxt::utils::AccountId32,
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
                pub stash: ::subxt::utils::AccountId32,
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
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub struct UnappliedSlash<_0, _1> {
                pub validator: _0,
                pub own: _1,
                pub others: ::std::vec::Vec<(_0, _1)>,
                pub reporters: ::std::vec::Vec<_0>,
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
                    #[doc = "See [`Pallet::sudo`]."]
                    sudo {
                        call: ::std::boxed::Box<runtime_types::vara_runtime::RuntimeCall>,
                    },
                    #[codec(index = 1)]
                    #[doc = "See [`Pallet::sudo_unchecked_weight`]."]
                    sudo_unchecked_weight {
                        call: ::std::boxed::Box<runtime_types::vara_runtime::RuntimeCall>,
                        weight: runtime_types::sp_weights::weight_v2::Weight,
                    },
                    #[codec(index = 2)]
                    #[doc = "See [`Pallet::set_key`]."]
                    set_key {
                        new: ::subxt::utils::MultiAddress<::subxt::utils::AccountId32, ()>,
                    },
                    #[codec(index = 3)]
                    #[doc = "See [`Pallet::sudo_as`]."]
                    sudo_as {
                        who: ::subxt::utils::MultiAddress<::subxt::utils::AccountId32, ()>,
                        call: ::std::boxed::Box<runtime_types::vara_runtime::RuntimeCall>,
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
                        old_sudoer: ::core::option::Option<::subxt::utils::AccountId32>,
                    },
                    #[codec(index = 2)]
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
                    #[doc = "See [`Pallet::set`]."]
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
        pub mod pallet_treasury {
            use super::runtime_types;
            pub mod pallet {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "Contains a variant per dispatchable extrinsic that this pallet has."]
                pub enum Call {
                    #[codec(index = 0)]
                    #[doc = "See [`Pallet::propose_spend`]."]
                    propose_spend {
                        #[codec(compact)]
                        value: ::core::primitive::u128,
                        beneficiary: ::subxt::utils::MultiAddress<::subxt::utils::AccountId32, ()>,
                    },
                    #[codec(index = 1)]
                    #[doc = "See [`Pallet::reject_proposal`]."]
                    reject_proposal {
                        #[codec(compact)]
                        proposal_id: ::core::primitive::u32,
                    },
                    #[codec(index = 2)]
                    #[doc = "See [`Pallet::approve_proposal`]."]
                    approve_proposal {
                        #[codec(compact)]
                        proposal_id: ::core::primitive::u32,
                    },
                    #[codec(index = 3)]
                    #[doc = "See [`Pallet::spend_local`]."]
                    spend_local {
                        #[codec(compact)]
                        amount: ::core::primitive::u128,
                        beneficiary: ::subxt::utils::MultiAddress<::subxt::utils::AccountId32, ()>,
                    },
                    #[codec(index = 4)]
                    #[doc = "See [`Pallet::remove_approval`]."]
                    remove_approval {
                        #[codec(compact)]
                        proposal_id: ::core::primitive::u32,
                    },
                    #[codec(index = 5)]
                    #[doc = "See [`Pallet::spend`]."]
                    spend {
                        asset_kind: ::std::boxed::Box<()>,
                        #[codec(compact)]
                        amount: ::core::primitive::u128,
                        beneficiary: ::std::boxed::Box<::subxt::utils::AccountId32>,
                        valid_from: ::core::option::Option<::core::primitive::u32>,
                    },
                    #[codec(index = 6)]
                    #[doc = "See [`Pallet::payout`]."]
                    payout { index: ::core::primitive::u32 },
                    #[codec(index = 7)]
                    #[doc = "See [`Pallet::check_status`]."]
                    check_status { index: ::core::primitive::u32 },
                    #[codec(index = 8)]
                    #[doc = "See [`Pallet::void_spend`]."]
                    void_spend { index: ::core::primitive::u32 },
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "Error for the treasury pallet."]
                pub enum Error {
                    #[codec(index = 0)]
                    #[doc = "Proposer's balance is too low."]
                    InsufficientProposersBalance,
                    #[codec(index = 1)]
                    #[doc = "No proposal, bounty or spend at that index."]
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
                    #[codec(index = 5)]
                    #[doc = "The balance of the asset kind is not convertible to the balance of the native asset."]
                    FailedToConvertBalance,
                    #[codec(index = 6)]
                    #[doc = "The spend has expired and cannot be claimed."]
                    SpendExpired,
                    #[codec(index = 7)]
                    #[doc = "The spend is not yet eligible for payout."]
                    EarlyPayout,
                    #[codec(index = 8)]
                    #[doc = "The payment has already been attempted."]
                    AlreadyAttempted,
                    #[codec(index = 9)]
                    #[doc = "There was some issue with the mechanism of payment."]
                    PayoutError,
                    #[codec(index = 10)]
                    #[doc = "The payout was not yet attempted/claimed."]
                    NotAttempted,
                    #[codec(index = 11)]
                    #[doc = "The payment has neither failed nor succeeded yet."]
                    Inconclusive,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                #[doc = "The `Event` enum of this pallet"]
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
                        account: ::subxt::utils::AccountId32,
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
                        beneficiary: ::subxt::utils::AccountId32,
                    },
                    #[codec(index = 8)]
                    #[doc = "The inactive funds of the pallet have been updated."]
                    UpdatedInactive {
                        reactivated: ::core::primitive::u128,
                        deactivated: ::core::primitive::u128,
                    },
                    #[codec(index = 9)]
                    #[doc = "A new asset spend proposal has been approved."]
                    AssetSpendApproved {
                        index: ::core::primitive::u32,
                        asset_kind: (),
                        amount: ::core::primitive::u128,
                        beneficiary: ::subxt::utils::AccountId32,
                        valid_from: ::core::primitive::u32,
                        expire_at: ::core::primitive::u32,
                    },
                    #[codec(index = 10)]
                    #[doc = "An approved spend was voided."]
                    AssetSpendVoided { index: ::core::primitive::u32 },
                    #[codec(index = 11)]
                    #[doc = "A payment happened."]
                    Paid {
                        index: ::core::primitive::u32,
                        payment_id: (),
                    },
                    #[codec(index = 12)]
                    #[doc = "A payment failed and can be retried."]
                    PaymentFailed {
                        index: ::core::primitive::u32,
                        payment_id: (),
                    },
                    #[codec(index = 13)]
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
                pub __subxt_unused_type_params: ::core::marker::PhantomData<_4>,
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
                    #[doc = "See [`Pallet::batch`]."]
                    batch {
                        calls: ::std::vec::Vec<runtime_types::vara_runtime::RuntimeCall>,
                    },
                    #[codec(index = 1)]
                    #[doc = "See [`Pallet::as_derivative`]."]
                    as_derivative {
                        index: ::core::primitive::u16,
                        call: ::std::boxed::Box<runtime_types::vara_runtime::RuntimeCall>,
                    },
                    #[codec(index = 2)]
                    #[doc = "See [`Pallet::batch_all`]."]
                    batch_all {
                        calls: ::std::vec::Vec<runtime_types::vara_runtime::RuntimeCall>,
                    },
                    #[codec(index = 3)]
                    #[doc = "See [`Pallet::dispatch_as`]."]
                    dispatch_as {
                        as_origin: ::std::boxed::Box<runtime_types::vara_runtime::OriginCaller>,
                        call: ::std::boxed::Box<runtime_types::vara_runtime::RuntimeCall>,
                    },
                    #[codec(index = 4)]
                    #[doc = "See [`Pallet::force_batch`]."]
                    force_batch {
                        calls: ::std::vec::Vec<runtime_types::vara_runtime::RuntimeCall>,
                    },
                    #[codec(index = 5)]
                    #[doc = "See [`Pallet::with_weight`]."]
                    with_weight {
                        call: ::std::boxed::Box<runtime_types::vara_runtime::RuntimeCall>,
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
                    #[doc = "See [`Pallet::vest`]."]
                    vest,
                    #[codec(index = 1)]
                    #[doc = "See [`Pallet::vest_other`]."]
                    vest_other {
                        target: ::subxt::utils::MultiAddress<::subxt::utils::AccountId32, ()>,
                    },
                    #[codec(index = 2)]
                    #[doc = "See [`Pallet::vested_transfer`]."]
                    vested_transfer {
                        target: ::subxt::utils::MultiAddress<::subxt::utils::AccountId32, ()>,
                        schedule: runtime_types::pallet_vesting::vesting_info::VestingInfo<
                            ::core::primitive::u128,
                            ::core::primitive::u32,
                        >,
                    },
                    #[codec(index = 3)]
                    #[doc = "See [`Pallet::force_vested_transfer`]."]
                    force_vested_transfer {
                        source: ::subxt::utils::MultiAddress<::subxt::utils::AccountId32, ()>,
                        target: ::subxt::utils::MultiAddress<::subxt::utils::AccountId32, ()>,
                        schedule: runtime_types::pallet_vesting::vesting_info::VestingInfo<
                            ::core::primitive::u128,
                            ::core::primitive::u32,
                        >,
                    },
                    #[codec(index = 4)]
                    #[doc = "See [`Pallet::merge_schedules`]."]
                    merge_schedules {
                        schedule1_index: ::core::primitive::u32,
                        schedule2_index: ::core::primitive::u32,
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
                        account: ::subxt::utils::AccountId32,
                        unvested: ::core::primitive::u128,
                    },
                    #[codec(index = 1)]
                    #[doc = "An \\[account\\] has become fully vested."]
                    VestingCompleted {
                        account: ::subxt::utils::AccountId32,
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
                    #[doc = "See [`Pallet::whitelist_call`]."]
                    whitelist_call { call_hash: ::subxt::utils::H256 },
                    #[codec(index = 1)]
                    #[doc = "See [`Pallet::remove_whitelisted_call`]."]
                    remove_whitelisted_call { call_hash: ::subxt::utils::H256 },
                    #[codec(index = 2)]
                    #[doc = "See [`Pallet::dispatch_whitelisted_call`]."]
                    dispatch_whitelisted_call {
                        call_hash: ::subxt::utils::H256,
                        call_encoded_len: ::core::primitive::u32,
                        call_weight_witness: runtime_types::sp_weights::weight_v2::Weight,
                    },
                    #[codec(index = 3)]
                    #[doc = "See [`Pallet::dispatch_whitelisted_call_with_preimage`]."]
                    dispatch_whitelisted_call_with_preimage {
                        call: ::std::boxed::Box<runtime_types::vara_runtime::RuntimeCall>,
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
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct FixedI64(pub ::core::primitive::i64);
                #[derive(
                    ::subxt::ext::codec::CompactAs,
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
                    ::subxt::ext::codec::CompactAs,
                    Debug,
                    crate::gp::Decode,
                    crate::gp::DecodeAsType,
                    crate::gp::Encode,
                )]
                pub struct PerU16(pub ::core::primitive::u16);
                #[derive(
                    ::subxt::ext::codec::CompactAs,
                    Debug,
                    crate::gp::Decode,
                    crate::gp::DecodeAsType,
                    crate::gp::Encode,
                )]
                pub struct Perbill(pub ::core::primitive::u32);
                #[derive(
                    ::subxt::ext::codec::CompactAs,
                    Debug,
                    crate::gp::Decode,
                    crate::gp::DecodeAsType,
                    crate::gp::Encode,
                )]
                pub struct Percent(pub ::core::primitive::u8);
                #[derive(
                    ::subxt::ext::codec::CompactAs,
                    Debug,
                    crate::gp::Decode,
                    crate::gp::DecodeAsType,
                    crate::gp::Encode,
                )]
                pub struct Permill(pub ::core::primitive::u32);
                #[derive(
                    ::subxt::ext::codec::CompactAs,
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
                pub struct Public(pub runtime_types::sp_core::sr25519::Public);
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
                pub mod vrf {
                    use super::runtime_types;
                    #[derive(
                        Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode,
                    )]
                    pub struct VrfSignature {
                        pub output: [::core::primitive::u8; 32usize],
                        pub proof: [::core::primitive::u8; 64usize],
                    }
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct Public(pub [::core::primitive::u8; 32usize]);
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct Signature(pub [::core::primitive::u8; 64usize]);
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
                pub voters: ::std::vec::Vec<(_0, ::core::primitive::u128)>,
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
                    pub struct Header<_0> {
                        pub parent_hash: ::subxt::utils::H256,
                        #[codec(compact)]
                        pub number: _0,
                        pub state_root: ::subxt::utils::H256,
                        pub extrinsics_root: ::subxt::utils::H256,
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
                Ed25519(runtime_types::sp_core::ed25519::Signature),
                #[codec(index = 1)]
                Sr25519(runtime_types::sp_core::sr25519::Signature),
                #[codec(index = 2)]
                Ecdsa(runtime_types::sp_core::ecdsa::Signature),
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
                pub trie_nodes: ::std::vec::Vec<::std::vec::Vec<::core::primitive::u8>>,
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
                    pub reporters: ::std::vec::Vec<_0>,
                }
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
                pub votes1: ::std::vec::Vec<(
                    ::subxt::ext::codec::Compact<::core::primitive::u32>,
                    ::subxt::ext::codec::Compact<::core::primitive::u16>,
                )>,
                pub votes2: ::std::vec::Vec<(
                    ::subxt::ext::codec::Compact<::core::primitive::u32>,
                    (
                        ::subxt::ext::codec::Compact<::core::primitive::u16>,
                        ::subxt::ext::codec::Compact<
                            runtime_types::sp_arithmetic::per_things::PerU16,
                        >,
                    ),
                    ::subxt::ext::codec::Compact<::core::primitive::u16>,
                )>,
                pub votes3: ::std::vec::Vec<(
                    ::subxt::ext::codec::Compact<::core::primitive::u32>,
                    [(
                        ::subxt::ext::codec::Compact<::core::primitive::u16>,
                        ::subxt::ext::codec::Compact<
                            runtime_types::sp_arithmetic::per_things::PerU16,
                        >,
                    ); 2usize],
                    ::subxt::ext::codec::Compact<::core::primitive::u16>,
                )>,
                pub votes4: ::std::vec::Vec<(
                    ::subxt::ext::codec::Compact<::core::primitive::u32>,
                    [(
                        ::subxt::ext::codec::Compact<::core::primitive::u16>,
                        ::subxt::ext::codec::Compact<
                            runtime_types::sp_arithmetic::per_things::PerU16,
                        >,
                    ); 3usize],
                    ::subxt::ext::codec::Compact<::core::primitive::u16>,
                )>,
                pub votes5: ::std::vec::Vec<(
                    ::subxt::ext::codec::Compact<::core::primitive::u32>,
                    [(
                        ::subxt::ext::codec::Compact<::core::primitive::u16>,
                        ::subxt::ext::codec::Compact<
                            runtime_types::sp_arithmetic::per_things::PerU16,
                        >,
                    ); 4usize],
                    ::subxt::ext::codec::Compact<::core::primitive::u16>,
                )>,
                pub votes6: ::std::vec::Vec<(
                    ::subxt::ext::codec::Compact<::core::primitive::u32>,
                    [(
                        ::subxt::ext::codec::Compact<::core::primitive::u16>,
                        ::subxt::ext::codec::Compact<
                            runtime_types::sp_arithmetic::per_things::PerU16,
                        >,
                    ); 5usize],
                    ::subxt::ext::codec::Compact<::core::primitive::u16>,
                )>,
                pub votes7: ::std::vec::Vec<(
                    ::subxt::ext::codec::Compact<::core::primitive::u32>,
                    [(
                        ::subxt::ext::codec::Compact<::core::primitive::u16>,
                        ::subxt::ext::codec::Compact<
                            runtime_types::sp_arithmetic::per_things::PerU16,
                        >,
                    ); 6usize],
                    ::subxt::ext::codec::Compact<::core::primitive::u16>,
                )>,
                pub votes8: ::std::vec::Vec<(
                    ::subxt::ext::codec::Compact<::core::primitive::u32>,
                    [(
                        ::subxt::ext::codec::Compact<::core::primitive::u16>,
                        ::subxt::ext::codec::Compact<
                            runtime_types::sp_arithmetic::per_things::PerU16,
                        >,
                    ); 7usize],
                    ::subxt::ext::codec::Compact<::core::primitive::u16>,
                )>,
                pub votes9: ::std::vec::Vec<(
                    ::subxt::ext::codec::Compact<::core::primitive::u32>,
                    [(
                        ::subxt::ext::codec::Compact<::core::primitive::u16>,
                        ::subxt::ext::codec::Compact<
                            runtime_types::sp_arithmetic::per_things::PerU16,
                        >,
                    ); 8usize],
                    ::subxt::ext::codec::Compact<::core::primitive::u16>,
                )>,
                pub votes10: ::std::vec::Vec<(
                    ::subxt::ext::codec::Compact<::core::primitive::u32>,
                    [(
                        ::subxt::ext::codec::Compact<::core::primitive::u16>,
                        ::subxt::ext::codec::Compact<
                            runtime_types::sp_arithmetic::per_things::PerU16,
                        >,
                    ); 9usize],
                    ::subxt::ext::codec::Compact<::core::primitive::u16>,
                )>,
                pub votes11: ::std::vec::Vec<(
                    ::subxt::ext::codec::Compact<::core::primitive::u32>,
                    [(
                        ::subxt::ext::codec::Compact<::core::primitive::u16>,
                        ::subxt::ext::codec::Compact<
                            runtime_types::sp_arithmetic::per_things::PerU16,
                        >,
                    ); 10usize],
                    ::subxt::ext::codec::Compact<::core::primitive::u16>,
                )>,
                pub votes12: ::std::vec::Vec<(
                    ::subxt::ext::codec::Compact<::core::primitive::u32>,
                    [(
                        ::subxt::ext::codec::Compact<::core::primitive::u16>,
                        ::subxt::ext::codec::Compact<
                            runtime_types::sp_arithmetic::per_things::PerU16,
                        >,
                    ); 11usize],
                    ::subxt::ext::codec::Compact<::core::primitive::u16>,
                )>,
                pub votes13: ::std::vec::Vec<(
                    ::subxt::ext::codec::Compact<::core::primitive::u32>,
                    [(
                        ::subxt::ext::codec::Compact<::core::primitive::u16>,
                        ::subxt::ext::codec::Compact<
                            runtime_types::sp_arithmetic::per_things::PerU16,
                        >,
                    ); 12usize],
                    ::subxt::ext::codec::Compact<::core::primitive::u16>,
                )>,
                pub votes14: ::std::vec::Vec<(
                    ::subxt::ext::codec::Compact<::core::primitive::u32>,
                    [(
                        ::subxt::ext::codec::Compact<::core::primitive::u16>,
                        ::subxt::ext::codec::Compact<
                            runtime_types::sp_arithmetic::per_things::PerU16,
                        >,
                    ); 13usize],
                    ::subxt::ext::codec::Compact<::core::primitive::u16>,
                )>,
                pub votes15: ::std::vec::Vec<(
                    ::subxt::ext::codec::Compact<::core::primitive::u32>,
                    [(
                        ::subxt::ext::codec::Compact<::core::primitive::u16>,
                        ::subxt::ext::codec::Compact<
                            runtime_types::sp_arithmetic::per_things::PerU16,
                        >,
                    ); 14usize],
                    ::subxt::ext::codec::Compact<::core::primitive::u16>,
                )>,
                pub votes16: ::std::vec::Vec<(
                    ::subxt::ext::codec::Compact<::core::primitive::u32>,
                    [(
                        ::subxt::ext::codec::Compact<::core::primitive::u16>,
                        ::subxt::ext::codec::Compact<
                            runtime_types::sp_arithmetic::per_things::PerU16,
                        >,
                    ); 15usize],
                    ::subxt::ext::codec::Compact<::core::primitive::u16>,
                )>,
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
            pub enum OriginCaller {
                #[codec(index = 0)]
                system(
                    runtime_types::frame_support::dispatch::RawOrigin<::subxt::utils::AccountId32>,
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
                #[codec(index = 8)]
                Utility(runtime_types::pallet_utility::pallet::Call),
                #[codec(index = 16)]
                ConvictionVoting(runtime_types::pallet_conviction_voting::pallet::Call),
                #[codec(index = 17)]
                Referenda(runtime_types::pallet_referenda::pallet::Call),
                #[codec(index = 18)]
                FellowshipCollective(runtime_types::pallet_ranked_collective::pallet::Call),
                #[codec(index = 19)]
                FellowshipReferenda(runtime_types::pallet_referenda::pallet::Call2),
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
                #[codec(index = 104)]
                Gear(runtime_types::pallet_gear::pallet::Call),
                #[codec(index = 106)]
                StakingRewards(runtime_types::pallet_gear_staking_rewards::pallet::Call),
                #[codec(index = 107)]
                GearVoucher(runtime_types::pallet_gear_voucher::pallet::Call),
                #[codec(index = 99)]
                Sudo(runtime_types::pallet_sudo::pallet::Call),
                #[codec(index = 199)]
                GearDebug(runtime_types::pallet_gear_debug::pallet::Call),
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
                #[codec(index = 10)]
                Vesting(runtime_types::pallet_vesting::pallet::Error),
                #[codec(index = 11)]
                BagsList(runtime_types::pallet_bags_list::pallet::Error),
                #[codec(index = 12)]
                ImOnline(runtime_types::pallet_im_online::pallet::Error),
                #[codec(index = 13)]
                Staking(runtime_types::pallet_staking::pallet::pallet::Error),
                #[codec(index = 7)]
                Session(runtime_types::pallet_session::pallet::Error),
                #[codec(index = 14)]
                Treasury(runtime_types::pallet_treasury::pallet::Error),
                #[codec(index = 8)]
                Utility(runtime_types::pallet_utility::pallet::Error),
                #[codec(index = 16)]
                ConvictionVoting(runtime_types::pallet_conviction_voting::pallet::Error),
                #[codec(index = 17)]
                Referenda(runtime_types::pallet_referenda::pallet::Error),
                #[codec(index = 18)]
                FellowshipCollective(runtime_types::pallet_ranked_collective::pallet::Error),
                #[codec(index = 19)]
                FellowshipReferenda(runtime_types::pallet_referenda::pallet::Error2),
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
                #[codec(index = 99)]
                Sudo(runtime_types::pallet_sudo::pallet::Error),
                #[codec(index = 199)]
                GearDebug(runtime_types::pallet_gear_debug::pallet::Error),
            }
            #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
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
                #[codec(index = 8)]
                Utility(runtime_types::pallet_utility::pallet::Event),
                #[codec(index = 16)]
                ConvictionVoting(runtime_types::pallet_conviction_voting::pallet::Event),
                #[codec(index = 17)]
                Referenda(runtime_types::pallet_referenda::pallet::Event),
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
                #[codec(index = 104)]
                Gear(runtime_types::pallet_gear::pallet::Event),
                #[codec(index = 106)]
                StakingRewards(runtime_types::pallet_gear_staking_rewards::pallet::Event),
                #[codec(index = 107)]
                GearVoucher(runtime_types::pallet_gear_voucher::pallet::Event),
                #[codec(index = 99)]
                Sudo(runtime_types::pallet_sudo::pallet::Event),
                #[codec(index = 199)]
                GearDebug(runtime_types::pallet_gear_debug::pallet::Event),
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
            }
        }
    }
    #[doc = "Calls of pallet `GearDebug`."]
    pub enum GearDebugCall {
        EnableDebugMode,
        ExhaustBlockResources,
    }
    impl CallInfo for GearDebugCall {
        const PALLET: &'static str = "GearDebug";
        fn call_name(&self) -> &'static str {
            match self {
                Self::EnableDebugMode => "enable_debug_mode",
                Self::ExhaustBlockResources => "exhaust_block_resources",
            }
        }
    }
    #[doc = "Calls of pallet `GearVoucher`."]
    pub enum GearVoucherCall {
        Issue,
        Call,
        Revoke,
        Update,
        CallDeprecated,
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
                Self::CallDeprecated => "call_deprecated",
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
            }
        }
    }
    #[doc = "Calls of pallet `StakingRewards`."]
    pub enum StakingRewardsCall {
        Refill,
        ForceRefill,
        Withdraw,
        AlignSupply,
    }
    impl CallInfo for StakingRewardsCall {
        const PALLET: &'static str = "StakingRewards";
        fn call_name(&self) -> &'static str {
            match self {
                Self::Refill => "refill",
                Self::ForceRefill => "force_refill",
                Self::Withdraw => "withdraw",
                Self::AlignSupply => "align_supply",
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
    #[doc = "Calls of pallet `Treasury`."]
    pub enum TreasuryCall {
        ProposeSpend,
        RejectProposal,
        ApproveProposal,
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
                Self::ProposeSpend => "propose_spend",
                Self::RejectProposal => "reject_proposal",
                Self::ApproveProposal => "approve_proposal",
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
        UnusedValue,
        OnFinalizeTransfers,
        OnFinalizeValue,
    }
    impl StorageInfo for GearBankStorage {
        const PALLET: &'static str = "GearBank";
        fn storage_name(&self) -> &'static str {
            match self {
                Self::Bank => "Bank",
                Self::UnusedValue => "UnusedValue",
                Self::OnFinalizeTransfers => "OnFinalizeTransfers",
                Self::OnFinalizeValue => "OnFinalizeValue",
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
        AllocationsStorage,
        PagesWithDataStorage,
        ProgramStorage,
        MemoryPages,
    }
    impl StorageInfo for GearProgramStorage {
        const PALLET: &'static str = "GearProgram";
        fn storage_name(&self) -> &'static str {
            match self {
                Self::CodeStorage => "CodeStorage",
                Self::CodeLenStorage => "CodeLenStorage",
                Self::OriginalCodeStorage => "OriginalCodeStorage",
                Self::MetadataStorage => "MetadataStorage",
                Self::AllocationsStorage => "AllocationsStorage",
                Self::PagesWithDataStorage => "PagesWithDataStorage",
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
    }
    impl StorageInfo for IdentityStorage {
        const PALLET: &'static str = "Identity";
        fn storage_name(&self) -> &'static str {
            match self {
                Self::IdentityOf => "IdentityOf",
                Self::SuperOf => "SuperOf",
                Self::SubsOf => "SubsOf",
                Self::Registrars => "Registrars",
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
        Lookup,
    }
    impl StorageInfo for SchedulerStorage {
        const PALLET: &'static str = "Scheduler";
        fn storage_name(&self) -> &'static str {
            match self {
                Self::IncompleteSince => "IncompleteSince",
                Self::Agenda => "Agenda",
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
        MaxNominatorsCount,
        CurrentEra,
        ActiveEra,
        ErasStartSessionIndex,
        ErasStakers,
        ErasStakersClipped,
        ErasValidatorPrefs,
        ErasValidatorReward,
        ErasRewardPoints,
        ErasTotalStake,
        ForceEra,
        SlashRewardFraction,
        CanceledSlashPayout,
        UnappliedSlashes,
        BondedEras,
        ValidatorSlashInEra,
        NominatorSlashInEra,
        SlashingSpans,
        SpanSlash,
        CurrentPlannedSession,
        OffendingValidators,
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
                Self::MaxNominatorsCount => "MaxNominatorsCount",
                Self::CurrentEra => "CurrentEra",
                Self::ActiveEra => "ActiveEra",
                Self::ErasStartSessionIndex => "ErasStartSessionIndex",
                Self::ErasStakers => "ErasStakers",
                Self::ErasStakersClipped => "ErasStakersClipped",
                Self::ErasValidatorPrefs => "ErasValidatorPrefs",
                Self::ErasValidatorReward => "ErasValidatorReward",
                Self::ErasRewardPoints => "ErasRewardPoints",
                Self::ErasTotalStake => "ErasTotalStake",
                Self::ForceEra => "ForceEra",
                Self::SlashRewardFraction => "SlashRewardFraction",
                Self::CanceledSlashPayout => "CanceledSlashPayout",
                Self::UnappliedSlashes => "UnappliedSlashes",
                Self::BondedEras => "BondedEras",
                Self::ValidatorSlashInEra => "ValidatorSlashInEra",
                Self::NominatorSlashInEra => "NominatorSlashInEra",
                Self::SlashingSpans => "SlashingSpans",
                Self::SpanSlash => "SpanSlash",
                Self::CurrentPlannedSession => "CurrentPlannedSession",
                Self::OffendingValidators => "OffendingValidators",
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
    pub mod vesting {
        pub use super::runtime_types::pallet_vesting::pallet::Event;
    }
    pub mod transaction_payment {
        pub use super::runtime_types::pallet_transaction_payment::pallet::Event;
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
    pub mod session {
        pub use super::runtime_types::pallet_session::pallet::Event;
    }
    pub mod treasury {
        pub use super::runtime_types::pallet_treasury::pallet::Event;
    }
    pub mod utility {
        pub use super::runtime_types::pallet_utility::pallet::Event;
    }
    pub mod conviction_voting {
        pub use super::runtime_types::pallet_conviction_voting::pallet::Event;
    }
    pub mod referenda {
        pub use super::runtime_types::pallet_referenda::pallet::Event;
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
    pub mod gear {
        pub use super::runtime_types::pallet_gear::pallet::Event;
    }
    pub mod staking_rewards {
        pub use super::runtime_types::pallet_gear_staking_rewards::pallet::Event;
    }
    pub mod gear_voucher {
        pub use super::runtime_types::pallet_gear_voucher::pallet::Event;
    }
    pub mod sudo {
        pub use super::runtime_types::pallet_sudo::pallet::Event;
    }
    pub mod gear_debug {
        pub use super::runtime_types::pallet_gear_debug::pallet::Event;
    }
}
