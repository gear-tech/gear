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
                pub enum Call {
                    #[codec(index = 0)]
                    remark {
                        remark: ::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
                    },
                    #[codec(index = 1)]
                    set_heap_pages { pages: ::core::primitive::u64 },
                    #[codec(index = 2)]
                    set_code {
                        code: ::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
                    },
                    #[codec(index = 3)]
                    set_code_without_checks {
                        code: ::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
                    },
                    #[codec(index = 4)]
                    set_storage {
                        items: ::subxt::ext::subxt_core::alloc::vec::Vec<(
                            ::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
                            ::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
                        )>,
                    },
                    #[codec(index = 5)]
                    kill_storage {
                        keys: ::subxt::ext::subxt_core::alloc::vec::Vec<
                            ::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
                        >,
                    },
                    #[codec(index = 6)]
                    kill_prefix {
                        prefix: ::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
                        subkeys: ::core::primitive::u32,
                    },
                    #[codec(index = 7)]
                    remark_with_event {
                        remark: ::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
                    },
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum Error {
                    #[codec(index = 0)]
                    InvalidSpecName,
                    #[codec(index = 1)]
                    SpecVersionNeedsToIncrease,
                    #[codec(index = 2)]
                    FailedToExtractRuntimeVersion,
                    #[codec(index = 3)]
                    NonDefaultComposite,
                    #[codec(index = 4)]
                    NonZeroRefCount,
                    #[codec(index = 5)]
                    CallFiltered,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum Event {
                    #[codec(index = 0)]
                    ExtrinsicSuccess {
                        dispatch_info: runtime_types::frame_support::dispatch::DispatchInfo,
                    },
                    #[codec(index = 1)]
                    ExtrinsicFailed {
                        dispatch_error: runtime_types::sp_runtime::DispatchError,
                        dispatch_info: runtime_types::frame_support::dispatch::DispatchInfo,
                    },
                    #[codec(index = 2)]
                    CodeUpdated,
                    #[codec(index = 3)]
                    NewAccount {
                        account: ::subxt::ext::subxt_core::utils::AccountId32,
                    },
                    #[codec(index = 4)]
                    KilledAccount {
                        account: ::subxt::ext::subxt_core::utils::AccountId32,
                    },
                    #[codec(index = 5)]
                    Remarked {
                        sender: ::subxt::ext::subxt_core::utils::AccountId32,
                        hash: ::subxt::ext::subxt_core::utils::H256,
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
                        #[codec(index = 9)]
                        RemoveResumeSession(::core::primitive::u32),
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
            pub struct CodeMetadata {
                pub author: ::subxt::ext::subxt_core::utils::H256,
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
                    pub ::subxt::ext::subxt_core::alloc::vec::Vec<_0>,
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
                        pub code: ::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
                        pub original_code_len: ::core::primitive::u32,
                        pub exports: ::subxt::ext::subxt_core::alloc::vec::Vec<
                            runtime_types::gear_core::message::DispatchKind,
                        >,
                        pub static_pages: runtime_types::gear_core::pages::PagesAmount,
                        pub stack_end:
                            ::core::option::Option<runtime_types::gear_core::pages::Page>,
                        pub instantiated_section_sizes:
                            runtime_types::gear_core::code::instrumented::InstantiatedSectionSizes,
                        pub version: ::core::primitive::u32,
                    }
                }
            }
            pub mod memory {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct IntoPageBufError;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct PageBuf(
                    pub  runtime_types::gear_core::buffer::LimitedVec<
                        ::core::primitive::u8,
                        runtime_types::gear_core::memory::IntoPageBufError,
                    >,
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
                        pub outgoing: ::subxt::ext::subxt_core::utils::KeyedVec<
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
                        pub initialized: ::subxt::ext::subxt_core::alloc::vec::Vec<
                            runtime_types::gprimitives::ActorId,
                        >,
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
                    pub code_hash: ::subxt::ext::subxt_core::utils::H256,
                    pub code_exports: ::subxt::ext::subxt_core::alloc::vec::Vec<
                        runtime_types::gear_core::message::DispatchKind,
                    >,
                    pub static_pages: runtime_types::gear_core::pages::PagesAmount,
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
                    pub inner: ::subxt::ext::subxt_core::utils::KeyedVec<_0, _0>,
                }
            }
        }
        pub mod pallet_babe {
            use super::runtime_types;
            pub mod pallet {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum Call {
                    #[codec(index = 0)]
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
                    plan_config_change {
                        config: runtime_types::sp_consensus_babe::digests::NextConfigDescriptor,
                    },
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum Error {
                    #[codec(index = 0)]
                    InvalidEquivocationProof,
                    #[codec(index = 1)]
                    InvalidKeyOwnershipProof,
                    #[codec(index = 2)]
                    DuplicateOffenceReport,
                    #[codec(index = 3)]
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
                pub enum Call {
                    #[codec(index = 0)]
                    rebag {
                        dislocated: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                    },
                    #[codec(index = 1)]
                    put_in_front_of {
                        lighter: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                    },
                    #[codec(index = 2)]
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
                pub enum Error {
                    #[codec(index = 0)]
                    List(runtime_types::pallet_bags_list::list::ListError),
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum Event {
                    #[codec(index = 0)]
                    Rebagged {
                        who: ::subxt::ext::subxt_core::utils::AccountId32,
                        from: ::core::primitive::u64,
                        to: ::core::primitive::u64,
                    },
                    #[codec(index = 1)]
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
                pub enum Call {
                    #[codec(index = 0)]
                    transfer_allow_death {
                        dest: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                        #[codec(compact)]
                        value: ::core::primitive::u128,
                    },
                    #[codec(index = 2)]
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
                    transfer_keep_alive {
                        dest: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                        #[codec(compact)]
                        value: ::core::primitive::u128,
                    },
                    #[codec(index = 4)]
                    transfer_all {
                        dest: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                        keep_alive: ::core::primitive::bool,
                    },
                    #[codec(index = 5)]
                    force_unreserve {
                        who: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                        amount: ::core::primitive::u128,
                    },
                    #[codec(index = 6)]
                    upgrade_accounts {
                        who: ::subxt::ext::subxt_core::alloc::vec::Vec<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                        >,
                    },
                    #[codec(index = 8)]
                    force_set_balance {
                        who: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                        #[codec(compact)]
                        new_free: ::core::primitive::u128,
                    },
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum Error {
                    #[codec(index = 0)]
                    VestingBalance,
                    #[codec(index = 1)]
                    LiquidityRestrictions,
                    #[codec(index = 2)]
                    InsufficientBalance,
                    #[codec(index = 3)]
                    ExistentialDeposit,
                    #[codec(index = 4)]
                    Expendability,
                    #[codec(index = 5)]
                    ExistingVestingSchedule,
                    #[codec(index = 6)]
                    DeadAccount,
                    #[codec(index = 7)]
                    TooManyReserves,
                    #[codec(index = 8)]
                    TooManyHolds,
                    #[codec(index = 9)]
                    TooManyFreezes,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum Event {
                    #[codec(index = 0)]
                    Endowed {
                        account: ::subxt::ext::subxt_core::utils::AccountId32,
                        free_balance: ::core::primitive::u128,
                    },
                    #[codec(index = 1)]
                    DustLost {
                        account: ::subxt::ext::subxt_core::utils::AccountId32,
                        amount: ::core::primitive::u128,
                    },
                    #[codec(index = 2)]
                    Transfer {
                        from: ::subxt::ext::subxt_core::utils::AccountId32,
                        to: ::subxt::ext::subxt_core::utils::AccountId32,
                        amount: ::core::primitive::u128,
                    },
                    #[codec(index = 3)]
                    BalanceSet {
                        who: ::subxt::ext::subxt_core::utils::AccountId32,
                        free: ::core::primitive::u128,
                    },
                    #[codec(index = 4)]
                    Reserved {
                        who: ::subxt::ext::subxt_core::utils::AccountId32,
                        amount: ::core::primitive::u128,
                    },
                    #[codec(index = 5)]
                    Unreserved {
                        who: ::subxt::ext::subxt_core::utils::AccountId32,
                        amount: ::core::primitive::u128,
                    },
                    #[codec(index = 6)]
                    ReserveRepatriated {
                        from: ::subxt::ext::subxt_core::utils::AccountId32,
                        to: ::subxt::ext::subxt_core::utils::AccountId32,
                        amount: ::core::primitive::u128,
                        destination_status:
                            runtime_types::frame_support::traits::tokens::misc::BalanceStatus,
                    },
                    #[codec(index = 7)]
                    Deposit {
                        who: ::subxt::ext::subxt_core::utils::AccountId32,
                        amount: ::core::primitive::u128,
                    },
                    #[codec(index = 8)]
                    Withdraw {
                        who: ::subxt::ext::subxt_core::utils::AccountId32,
                        amount: ::core::primitive::u128,
                    },
                    #[codec(index = 9)]
                    Slashed {
                        who: ::subxt::ext::subxt_core::utils::AccountId32,
                        amount: ::core::primitive::u128,
                    },
                    #[codec(index = 10)]
                    Minted {
                        who: ::subxt::ext::subxt_core::utils::AccountId32,
                        amount: ::core::primitive::u128,
                    },
                    #[codec(index = 11)]
                    Burned {
                        who: ::subxt::ext::subxt_core::utils::AccountId32,
                        amount: ::core::primitive::u128,
                    },
                    #[codec(index = 12)]
                    Suspended {
                        who: ::subxt::ext::subxt_core::utils::AccountId32,
                        amount: ::core::primitive::u128,
                    },
                    #[codec(index = 13)]
                    Restored {
                        who: ::subxt::ext::subxt_core::utils::AccountId32,
                        amount: ::core::primitive::u128,
                    },
                    #[codec(index = 14)]
                    Upgraded {
                        who: ::subxt::ext::subxt_core::utils::AccountId32,
                    },
                    #[codec(index = 15)]
                    Issued { amount: ::core::primitive::u128 },
                    #[codec(index = 16)]
                    Rescinded { amount: ::core::primitive::u128 },
                    #[codec(index = 17)]
                    Locked {
                        who: ::subxt::ext::subxt_core::utils::AccountId32,
                        amount: ::core::primitive::u128,
                    },
                    #[codec(index = 18)]
                    Unlocked {
                        who: ::subxt::ext::subxt_core::utils::AccountId32,
                        amount: ::core::primitive::u128,
                    },
                    #[codec(index = 19)]
                    Frozen {
                        who: ::subxt::ext::subxt_core::utils::AccountId32,
                        amount: ::core::primitive::u128,
                    },
                    #[codec(index = 20)]
                    Thawed {
                        who: ::subxt::ext::subxt_core::utils::AccountId32,
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
                    ::subxt::ext ::subxt_core::ext::codec::CompactAs,
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
                pub enum Call {
                    #[codec(index = 0)]
                    propose_bounty {
                        #[codec(compact)]
                        value: ::core::primitive::u128,
                        description:
                            ::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
                    },
                    #[codec(index = 1)]
                    approve_bounty {
                        #[codec(compact)]
                        bounty_id: ::core::primitive::u32,
                    },
                    #[codec(index = 2)]
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
                    unassign_curator {
                        #[codec(compact)]
                        bounty_id: ::core::primitive::u32,
                    },
                    #[codec(index = 4)]
                    accept_curator {
                        #[codec(compact)]
                        bounty_id: ::core::primitive::u32,
                    },
                    #[codec(index = 5)]
                    award_bounty {
                        #[codec(compact)]
                        bounty_id: ::core::primitive::u32,
                        beneficiary: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                    },
                    #[codec(index = 6)]
                    claim_bounty {
                        #[codec(compact)]
                        bounty_id: ::core::primitive::u32,
                    },
                    #[codec(index = 7)]
                    close_bounty {
                        #[codec(compact)]
                        bounty_id: ::core::primitive::u32,
                    },
                    #[codec(index = 8)]
                    extend_bounty_expiry {
                        #[codec(compact)]
                        bounty_id: ::core::primitive::u32,
                        remark: ::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
                    },
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum Error {
                    #[codec(index = 0)]
                    InsufficientProposersBalance,
                    #[codec(index = 1)]
                    InvalidIndex,
                    #[codec(index = 2)]
                    ReasonTooBig,
                    #[codec(index = 3)]
                    UnexpectedStatus,
                    #[codec(index = 4)]
                    RequireCurator,
                    #[codec(index = 5)]
                    InvalidValue,
                    #[codec(index = 6)]
                    InvalidFee,
                    #[codec(index = 7)]
                    PendingPayout,
                    #[codec(index = 8)]
                    Premature,
                    #[codec(index = 9)]
                    HasActiveChildBounty,
                    #[codec(index = 10)]
                    TooManyQueued,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum Event {
                    #[codec(index = 0)]
                    BountyProposed { index: ::core::primitive::u32 },
                    #[codec(index = 1)]
                    BountyRejected {
                        index: ::core::primitive::u32,
                        bond: ::core::primitive::u128,
                    },
                    #[codec(index = 2)]
                    BountyBecameActive { index: ::core::primitive::u32 },
                    #[codec(index = 3)]
                    BountyAwarded {
                        index: ::core::primitive::u32,
                        beneficiary: ::subxt::ext::subxt_core::utils::AccountId32,
                    },
                    #[codec(index = 4)]
                    BountyClaimed {
                        index: ::core::primitive::u32,
                        payout: ::core::primitive::u128,
                        beneficiary: ::subxt::ext::subxt_core::utils::AccountId32,
                    },
                    #[codec(index = 5)]
                    BountyCanceled { index: ::core::primitive::u32 },
                    #[codec(index = 6)]
                    BountyExtended { index: ::core::primitive::u32 },
                    #[codec(index = 7)]
                    BountyApproved { index: ::core::primitive::u32 },
                    #[codec(index = 8)]
                    CuratorProposed {
                        bounty_id: ::core::primitive::u32,
                        curator: ::subxt::ext::subxt_core::utils::AccountId32,
                    },
                    #[codec(index = 9)]
                    CuratorUnassigned { bounty_id: ::core::primitive::u32 },
                    #[codec(index = 10)]
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
                pub enum Call {
                    #[codec(index = 0)]
                    add_child_bounty {
                        #[codec(compact)]
                        parent_bounty_id: ::core::primitive::u32,
                        #[codec(compact)]
                        value: ::core::primitive::u128,
                        description:
                            ::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
                    },
                    #[codec(index = 1)]
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
                    accept_curator {
                        #[codec(compact)]
                        parent_bounty_id: ::core::primitive::u32,
                        #[codec(compact)]
                        child_bounty_id: ::core::primitive::u32,
                    },
                    #[codec(index = 3)]
                    unassign_curator {
                        #[codec(compact)]
                        parent_bounty_id: ::core::primitive::u32,
                        #[codec(compact)]
                        child_bounty_id: ::core::primitive::u32,
                    },
                    #[codec(index = 4)]
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
                    claim_child_bounty {
                        #[codec(compact)]
                        parent_bounty_id: ::core::primitive::u32,
                        #[codec(compact)]
                        child_bounty_id: ::core::primitive::u32,
                    },
                    #[codec(index = 6)]
                    close_child_bounty {
                        #[codec(compact)]
                        parent_bounty_id: ::core::primitive::u32,
                        #[codec(compact)]
                        child_bounty_id: ::core::primitive::u32,
                    },
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum Error {
                    #[codec(index = 0)]
                    ParentBountyNotActive,
                    #[codec(index = 1)]
                    InsufficientBountyBalance,
                    #[codec(index = 2)]
                    TooManyChildBounties,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum Event {
                    #[codec(index = 0)]
                    Added {
                        index: ::core::primitive::u32,
                        child_index: ::core::primitive::u32,
                    },
                    #[codec(index = 1)]
                    Awarded {
                        index: ::core::primitive::u32,
                        child_index: ::core::primitive::u32,
                        beneficiary: ::subxt::ext::subxt_core::utils::AccountId32,
                    },
                    #[codec(index = 2)]
                    Claimed {
                        index: ::core::primitive::u32,
                        child_index: ::core::primitive::u32,
                        payout: ::core::primitive::u128,
                        beneficiary: ::subxt::ext::subxt_core::utils::AccountId32,
                    },
                    #[codec(index = 3)]
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
                pub enum Call {
                    #[codec(index = 0)]
                    vote {
                        #[codec(compact)]
                        poll_index: ::core::primitive::u32,
                        vote: runtime_types::pallet_conviction_voting::vote::AccountVote<
                            ::core::primitive::u128,
                        >,
                    },
                    #[codec(index = 1)]
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
                    undelegate { class: ::core::primitive::u16 },
                    #[codec(index = 3)]
                    unlock {
                        class: ::core::primitive::u16,
                        target: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                    },
                    #[codec(index = 4)]
                    remove_vote {
                        class: ::core::option::Option<::core::primitive::u16>,
                        index: ::core::primitive::u32,
                    },
                    #[codec(index = 5)]
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
                pub enum Error {
                    #[codec(index = 0)]
                    NotOngoing,
                    #[codec(index = 1)]
                    NotVoter,
                    #[codec(index = 2)]
                    NoPermission,
                    #[codec(index = 3)]
                    NoPermissionYet,
                    #[codec(index = 4)]
                    AlreadyDelegating,
                    #[codec(index = 5)]
                    AlreadyVoting,
                    #[codec(index = 6)]
                    InsufficientFunds,
                    #[codec(index = 7)]
                    NotDelegating,
                    #[codec(index = 8)]
                    Nonsense,
                    #[codec(index = 9)]
                    MaxVotesReached,
                    #[codec(index = 10)]
                    ClassNeeded,
                    #[codec(index = 11)]
                    BadClass,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum Event {
                    #[codec(index = 0)]
                    Delegated(
                        ::subxt::ext::subxt_core::utils::AccountId32,
                        ::subxt::ext::subxt_core::utils::AccountId32,
                    ),
                    #[codec(index = 1)]
                    Undelegated(::subxt::ext::subxt_core::utils::AccountId32),
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
                pub enum Call {
                    # [codec (index = 0)] submit_unsigned { raw_solution : ::subxt::ext ::subxt_core::alloc::boxed::Box < runtime_types::pallet_election_provider_multi_phase::RawSolution < runtime_types::vara_runtime::NposSolution16 > > , witness : runtime_types::pallet_election_provider_multi_phase::SolutionOrSnapshotSize , } , # [codec (index = 1)] set_minimum_untrusted_score { maybe_next_score: ::core::option::Option < runtime_types::sp_npos_elections::ElectionScore > , } , # [codec (index = 2)] set_emergency_election_result { supports : ::subxt::ext ::subxt_core::alloc::vec::Vec < (::subxt::ext ::subxt_core::utils::AccountId32 , runtime_types::sp_npos_elections::Support < ::subxt::ext ::subxt_core::utils::AccountId32 > ,) > , } , # [codec (index = 3)] submit { raw_solution : ::subxt::ext ::subxt_core::alloc::boxed::Box < runtime_types::pallet_election_provider_multi_phase::RawSolution < runtime_types::vara_runtime::NposSolution16 > > , } , # [codec (index = 4)] governance_fallback { maybe_max_voters: ::core::option::Option <::core::primitive::u32 > , maybe_max_targets: ::core::option::Option <::core::primitive::u32 > , } , }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum Error {
                    #[codec(index = 0)]
                    PreDispatchEarlySubmission,
                    #[codec(index = 1)]
                    PreDispatchWrongWinnerCount,
                    #[codec(index = 2)]
                    PreDispatchWeakSubmission,
                    #[codec(index = 3)]
                    SignedQueueFull,
                    #[codec(index = 4)]
                    SignedCannotPayDeposit,
                    #[codec(index = 5)]
                    SignedInvalidWitness,
                    #[codec(index = 6)]
                    SignedTooMuchWeight,
                    #[codec(index = 7)]
                    OcwCallWrongEra,
                    #[codec(index = 8)]
                    MissingSnapshotMetadata,
                    #[codec(index = 9)]
                    InvalidSubmissionIndex,
                    #[codec(index = 10)]
                    CallNotAllowed,
                    #[codec(index = 11)]
                    FallbackFailed,
                    #[codec(index = 12)]
                    BoundNotMet,
                    #[codec(index = 13)]
                    TooManyWinners,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum Event {
                    #[codec(index = 0)]
                    SolutionStored {
                        compute:
                            runtime_types::pallet_election_provider_multi_phase::ElectionCompute,
                        origin:
                            ::core::option::Option<::subxt::ext::subxt_core::utils::AccountId32>,
                        prev_ejected: ::core::primitive::bool,
                    },
                    #[codec(index = 1)]
                    ElectionFinalized {
                        compute:
                            runtime_types::pallet_election_provider_multi_phase::ElectionCompute,
                        score: runtime_types::sp_npos_elections::ElectionScore,
                    },
                    #[codec(index = 2)]
                    ElectionFailed,
                    #[codec(index = 3)]
                    Rewarded {
                        account: ::subxt::ext::subxt_core::utils::AccountId32,
                        value: ::core::primitive::u128,
                    },
                    #[codec(index = 4)]
                    Slashed {
                        account: ::subxt::ext::subxt_core::utils::AccountId32,
                        value: ::core::primitive::u128,
                    },
                    #[codec(index = 5)]
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
                pub enum Call {
                    #[codec(index = 0)]
                    upload_code {
                        code: ::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
                    },
                    #[codec(index = 1)]
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
                    send_message {
                        destination: runtime_types::gprimitives::ActorId,
                        payload: ::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
                        gas_limit: ::core::primitive::u64,
                        value: ::core::primitive::u128,
                        keep_alive: ::core::primitive::bool,
                    },
                    #[codec(index = 4)]
                    send_reply {
                        reply_to_id: runtime_types::gprimitives::MessageId,
                        payload: ::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
                        gas_limit: ::core::primitive::u64,
                        value: ::core::primitive::u128,
                        keep_alive: ::core::primitive::bool,
                    },
                    #[codec(index = 5)]
                    claim_value {
                        message_id: runtime_types::gprimitives::MessageId,
                    },
                    #[codec(index = 6)]
                    run {
                        max_gas: ::core::option::Option<::core::primitive::u64>,
                    },
                    #[codec(index = 7)]
                    set_execute_inherent { value: ::core::primitive::bool },
                    #[codec(index = 8)]
                    claim_value_to_inheritor {
                        program_id: runtime_types::gprimitives::ActorId,
                        depth: ::core::num::NonZeroU32,
                    },
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum Error {
                    #[codec(index = 0)]
                    MessageNotFound,
                    #[codec(index = 1)]
                    InsufficientBalance,
                    #[codec(index = 2)]
                    GasLimitTooHigh,
                    #[codec(index = 3)]
                    ProgramAlreadyExists,
                    #[codec(index = 4)]
                    InactiveProgram,
                    #[codec(index = 5)]
                    NoMessageTree,
                    #[codec(index = 6)]
                    CodeAlreadyExists,
                    #[codec(index = 7)]
                    CodeDoesntExist,
                    #[codec(index = 8)]
                    CodeTooLarge,
                    #[codec(index = 9)]
                    ProgramConstructionFailed,
                    #[codec(index = 10)]
                    MessageQueueProcessingDisabled,
                    #[codec(index = 11)]
                    ResumePeriodLessThanMinimal,
                    #[codec(index = 12)]
                    ProgramNotFound,
                    #[codec(index = 13)]
                    GearRunAlreadyInBlock,
                    #[codec(index = 14)]
                    ProgramRentDisabled,
                    #[codec(index = 15)]
                    ActiveProgram,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum Event {
                    #[codec(index = 0)]
                    MessageQueued {
                        id: runtime_types::gprimitives::MessageId,
                        source: ::subxt::ext::subxt_core::utils::AccountId32,
                        destination: runtime_types::gprimitives::ActorId,
                        entry: runtime_types::gear_common::event::MessageEntry,
                    },
                    #[codec(index = 1)]
                    UserMessageSent {
                        message: runtime_types::gear_core::message::user::UserMessage,
                        expiration: ::core::option::Option<::core::primitive::u32>,
                    },
                    #[codec(index = 2)]
                    UserMessageRead {
                        id: runtime_types::gprimitives::MessageId,
                        reason: runtime_types::gear_common::event::Reason<
                            runtime_types::gear_common::event::UserMessageReadRuntimeReason,
                            runtime_types::gear_common::event::UserMessageReadSystemReason,
                        >,
                    },
                    #[codec(index = 3)]
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
                    MessageWoken {
                        id: runtime_types::gprimitives::MessageId,
                        reason: runtime_types::gear_common::event::Reason<
                            runtime_types::gear_common::event::MessageWokenRuntimeReason,
                            runtime_types::gear_common::event::MessageWokenSystemReason,
                        >,
                    },
                    #[codec(index = 6)]
                    CodeChanged {
                        id: runtime_types::gprimitives::CodeId,
                        change: runtime_types::gear_common::event::CodeChangeKind<
                            ::core::primitive::u32,
                        >,
                    },
                    #[codec(index = 7)]
                    ProgramChanged {
                        id: runtime_types::gprimitives::ActorId,
                        change: runtime_types::gear_common::event::ProgramChangeKind<
                            ::core::primitive::u32,
                        >,
                    },
                    #[codec(index = 8)]
                    QueueNotProcessed,
                }
            }
            pub mod schedule {
                use super::runtime_types;
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
                pub struct Limits {
                    pub stack_height: ::core::option::Option<::core::primitive::u32>,
                    pub globals: ::core::primitive::u32,
                    pub locals: ::core::primitive::u32,
                    pub parameters: ::core::primitive::u32,
                    pub memory_pages: ::core::primitive::u16,
                    pub table_size: ::core::primitive::u32,
                    pub table_number: ::core::primitive::u32,
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
                    pub instantiation_weights:
                        runtime_types::pallet_gear::schedule::InstantiationWeights,
                    pub db_write_per_byte: runtime_types::sp_weights::weight_v2::Weight,
                    pub db_read_per_byte: runtime_types::sp_weights::weight_v2::Weight,
                    pub code_instrumentation_cost: runtime_types::sp_weights::weight_v2::Weight,
                    pub code_instrumentation_byte_cost:
                        runtime_types::sp_weights::weight_v2::Weight,
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
                pub enum Error {
                    #[codec(index = 0)]
                    InsufficientBalance,
                    #[codec(index = 1)]
                    InsufficientGasBalance,
                    #[codec(index = 2)]
                    InsufficientValueBalance,
                    #[codec(index = 3)]
                    InsufficientBankBalance,
                    #[codec(index = 4)]
                    InsufficientDeposit,
                    #[codec(index = 5)]
                    Overflow,
                }
            }
        }
        pub mod pallet_gear_debug {
            use super::runtime_types;
            pub mod pallet {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum Call {
                    #[codec(index = 0)]
                    enable_debug_mode {
                        debug_mode_on: ::core::primitive::bool,
                    },
                    #[codec(index = 1)]
                    exhaust_block_resources {
                        fraction: runtime_types::sp_arithmetic::per_things::Percent,
                    },
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct DebugData {
                    pub dispatch_queue: ::subxt::ext::subxt_core::alloc::vec::Vec<
                        runtime_types::gear_core::message::stored::StoredDispatch,
                    >,
                    pub programs: ::subxt::ext::subxt_core::alloc::vec::Vec<
                        runtime_types::pallet_gear_debug::pallet::ProgramDetails,
                    >,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum Error {}
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum Event {
                    #[codec(index = 0)]
                    DebugMode(::core::primitive::bool),
                    #[codec(index = 1)]
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
                    pub persistent_pages: ::subxt::ext::subxt_core::utils::KeyedVec<
                        runtime_types::gear_core::pages::Page,
                        runtime_types::gear_core::memory::PageBuf,
                    >,
                    pub code_hash: ::subxt::ext::subxt_core::utils::H256,
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
        pub mod pallet_gear_eth_bridge {
            use super::runtime_types;
            pub mod internal {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub struct EthMessage {
                    pub nonce: runtime_types::primitive_types::U256,
                    pub source: ::subxt::ext::subxt_core::utils::H256,
                    pub destination: ::subxt::ext::subxt_core::utils::H160,
                    pub payload: ::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
                }
            }
            pub mod pallet {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum Call {
                    #[codec(index = 0)]
                    pause,
                    #[codec(index = 1)]
                    unpause,
                    #[codec(index = 2)]
                    send_eth_message {
                        destination: ::subxt::ext::subxt_core::utils::H160,
                        payload: ::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
                    },
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum Error {
                    #[codec(index = 0)]
                    BridgeIsNotYetInitialized,
                    #[codec(index = 1)]
                    BridgeIsPaused,
                    #[codec(index = 2)]
                    MaxPayloadSizeExceeded,
                    #[codec(index = 3)]
                    QueueCapacityExceeded,
                    #[codec(index = 4)]
                    IncorrectValueApplied,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum Event {
                    #[codec(index = 0)]
                    AuthoritySetHashChanged(::subxt::ext::subxt_core::utils::H256),
                    #[codec(index = 1)]
                    BridgeCleared,
                    #[codec(index = 2)]
                    BridgeInitialized,
                    #[codec(index = 3)]
                    BridgePaused,
                    #[codec(index = 4)]
                    BridgeUnpaused,
                    #[codec(index = 5)]
                    MessageQueued {
                        message: runtime_types::pallet_gear_eth_bridge::internal::EthMessage,
                        hash: ::subxt::ext::subxt_core::utils::H256,
                    },
                    #[codec(index = 6)]
                    QueueMerkleRootChanged(::subxt::ext::subxt_core::utils::H256),
                }
            }
        }
        pub mod pallet_gear_gas {
            use super::runtime_types;
            pub mod pallet {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
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
                    ParentIsLost,
                    #[codec(index = 6)]
                    ParentHasNoChildren,
                    #[codec(index = 7)]
                    UnexpectedConsumeOutput,
                    #[codec(index = 8)]
                    UnexpectedNodeType,
                    #[codec(index = 9)]
                    ValueIsNotCaught,
                    #[codec(index = 10)]
                    ValueIsBlocked,
                    #[codec(index = 11)]
                    ValueIsNotBlocked,
                    #[codec(index = 12)]
                    ConsumedWithLock,
                    #[codec(index = 13)]
                    ConsumedWithSystemReservation,
                    #[codec(index = 14)]
                    TotalValueIsOverflowed,
                    #[codec(index = 15)]
                    TotalValueIsUnderflowed,
                }
            }
        }
        pub mod pallet_gear_messenger {
            use super::runtime_types;
            pub mod pallet {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum Error {
                    #[codec(index = 0)]
                    QueueDuplicateKey,
                    #[codec(index = 1)]
                    QueueElementNotFound,
                    #[codec(index = 2)]
                    QueueHeadShouldBeSet,
                    #[codec(index = 3)]
                    QueueHeadShouldNotBeSet,
                    #[codec(index = 4)]
                    QueueTailHasNextKey,
                    #[codec(index = 5)]
                    QueueTailParentNotFound,
                    #[codec(index = 6)]
                    QueueTailShouldBeSet,
                    #[codec(index = 7)]
                    QueueTailShouldNotBeSet,
                    #[codec(index = 8)]
                    MailboxDuplicateKey,
                    #[codec(index = 9)]
                    MailboxElementNotFound,
                    #[codec(index = 10)]
                    WaitlistDuplicateKey,
                    #[codec(index = 11)]
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
                pub enum Error {
                    #[codec(index = 0)]
                    DuplicateTask,
                    #[codec(index = 1)]
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
                    align_supply { target: ::core::primitive::u128 },
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum Error {
                    #[codec(index = 0)]
                    FailureToRefillPool,
                    #[codec(index = 1)]
                    FailureToWithdrawFromPool,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum Event {
                    #[codec(index = 0)]
                    Deposited { amount: ::core::primitive::u128 },
                    #[codec(index = 1)]
                    Withdrawn { amount: ::core::primitive::u128 },
                    #[codec(index = 2)]
                    Burned { amount: ::core::primitive::u128 },
                    #[codec(index = 3)]
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
                pub enum Call {
                    #[codec(index = 0)]
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
                    call {
                        voucher_id: runtime_types::pallet_gear_voucher::internal::VoucherId,
                        call: runtime_types::pallet_gear_voucher::internal::PrepaidCall<
                            ::core::primitive::u128,
                        >,
                    },
                    #[codec(index = 2)]
                    revoke {
                        spender: ::subxt::ext::subxt_core::utils::AccountId32,
                        voucher_id: runtime_types::pallet_gear_voucher::internal::VoucherId,
                    },
                    #[codec(index = 3)]
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
                    call_deprecated {
                        call: runtime_types::pallet_gear_voucher::internal::PrepaidCall<
                            ::core::primitive::u128,
                        >,
                    },
                    #[codec(index = 5)]
                    decline {
                        voucher_id: runtime_types::pallet_gear_voucher::internal::VoucherId,
                    },
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum Error {
                    #[codec(index = 0)]
                    BadOrigin,
                    #[codec(index = 1)]
                    BalanceTransfer,
                    #[codec(index = 2)]
                    InappropriateDestination,
                    #[codec(index = 3)]
                    InexistentVoucher,
                    #[codec(index = 4)]
                    IrrevocableYet,
                    #[codec(index = 5)]
                    MaxProgramsLimitExceeded,
                    #[codec(index = 6)]
                    UnknownDestination,
                    #[codec(index = 7)]
                    VoucherExpired,
                    #[codec(index = 8)]
                    DurationOutOfBounds,
                    #[codec(index = 9)]
                    CodeUploadingEnabled,
                    #[codec(index = 10)]
                    CodeUploadingDisabled,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum Event {
                    #[codec(index = 0)]
                    VoucherIssued {
                        owner: ::subxt::ext::subxt_core::utils::AccountId32,
                        spender: ::subxt::ext::subxt_core::utils::AccountId32,
                        voucher_id: runtime_types::pallet_gear_voucher::internal::VoucherId,
                    },
                    #[codec(index = 1)]
                    VoucherRevoked {
                        spender: ::subxt::ext::subxt_core::utils::AccountId32,
                        voucher_id: runtime_types::pallet_gear_voucher::internal::VoucherId,
                    },
                    #[codec(index = 2)]
                    VoucherUpdated {
                        spender: ::subxt::ext::subxt_core::utils::AccountId32,
                        voucher_id: runtime_types::pallet_gear_voucher::internal::VoucherId,
                        new_owner:
                            ::core::option::Option<::subxt::ext::subxt_core::utils::AccountId32>,
                    },
                    #[codec(index = 3)]
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
                pub enum Call {
                    #[codec(index = 0)]
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
                    note_stalled {
                        delay: ::core::primitive::u32,
                        best_finalized_block_number: ::core::primitive::u32,
                    },
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum Error {
                    #[codec(index = 0)]
                    PauseFailed,
                    #[codec(index = 1)]
                    ResumeFailed,
                    #[codec(index = 2)]
                    ChangePending,
                    #[codec(index = 3)]
                    TooSoon,
                    #[codec(index = 4)]
                    InvalidKeyOwnershipProof,
                    #[codec(index = 5)]
                    InvalidEquivocationProof,
                    #[codec(index = 6)]
                    DuplicateOffenceReport,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum Event {
                    #[codec(index = 0)]
                    NewAuthorities {
                        authority_set: ::subxt::ext::subxt_core::alloc::vec::Vec<(
                            runtime_types::sp_consensus_grandpa::app::Public,
                            ::core::primitive::u64,
                        )>,
                    },
                    #[codec(index = 1)]
                    Paused,
                    #[codec(index = 2)]
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
                pub enum Call {
                    #[codec(index = 0)]
                    add_registrar {
                        account: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                    },
                    #[codec(index = 1)]
                    set_identity {
                        info: ::subxt::ext::subxt_core::alloc::boxed::Box<
                            runtime_types::pallet_identity::legacy::IdentityInfo,
                        >,
                    },
                    #[codec(index = 2)]
                    set_subs {
                        subs: ::subxt::ext::subxt_core::alloc::vec::Vec<(
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            runtime_types::pallet_identity::types::Data,
                        )>,
                    },
                    #[codec(index = 3)]
                    clear_identity,
                    #[codec(index = 4)]
                    request_judgement {
                        #[codec(compact)]
                        reg_index: ::core::primitive::u32,
                        #[codec(compact)]
                        max_fee: ::core::primitive::u128,
                    },
                    #[codec(index = 5)]
                    cancel_request { reg_index: ::core::primitive::u32 },
                    #[codec(index = 6)]
                    set_fee {
                        #[codec(compact)]
                        index: ::core::primitive::u32,
                        #[codec(compact)]
                        fee: ::core::primitive::u128,
                    },
                    #[codec(index = 7)]
                    set_account_id {
                        #[codec(compact)]
                        index: ::core::primitive::u32,
                        new: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                    },
                    #[codec(index = 8)]
                    set_fields {
                        #[codec(compact)]
                        index: ::core::primitive::u32,
                        fields: ::core::primitive::u64,
                    },
                    #[codec(index = 9)]
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
                    kill_identity {
                        target: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                    },
                    #[codec(index = 11)]
                    add_sub {
                        sub: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                        data: runtime_types::pallet_identity::types::Data,
                    },
                    #[codec(index = 12)]
                    rename_sub {
                        sub: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                        data: runtime_types::pallet_identity::types::Data,
                    },
                    #[codec(index = 13)]
                    remove_sub {
                        sub: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                    },
                    #[codec(index = 14)]
                    quit_sub,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum Error {
                    #[codec(index = 0)]
                    TooManySubAccounts,
                    #[codec(index = 1)]
                    NotFound,
                    #[codec(index = 2)]
                    NotNamed,
                    #[codec(index = 3)]
                    EmptyIndex,
                    #[codec(index = 4)]
                    FeeChanged,
                    #[codec(index = 5)]
                    NoIdentity,
                    #[codec(index = 6)]
                    StickyJudgement,
                    #[codec(index = 7)]
                    JudgementGiven,
                    #[codec(index = 8)]
                    InvalidJudgement,
                    #[codec(index = 9)]
                    InvalidIndex,
                    #[codec(index = 10)]
                    InvalidTarget,
                    #[codec(index = 11)]
                    TooManyRegistrars,
                    #[codec(index = 12)]
                    AlreadyClaimed,
                    #[codec(index = 13)]
                    NotSub,
                    #[codec(index = 14)]
                    NotOwned,
                    #[codec(index = 15)]
                    JudgementForDifferentIdentity,
                    #[codec(index = 16)]
                    JudgementPaymentFailed,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum Event {
                    #[codec(index = 0)]
                    IdentitySet {
                        who: ::subxt::ext::subxt_core::utils::AccountId32,
                    },
                    #[codec(index = 1)]
                    IdentityCleared {
                        who: ::subxt::ext::subxt_core::utils::AccountId32,
                        deposit: ::core::primitive::u128,
                    },
                    #[codec(index = 2)]
                    IdentityKilled {
                        who: ::subxt::ext::subxt_core::utils::AccountId32,
                        deposit: ::core::primitive::u128,
                    },
                    #[codec(index = 3)]
                    JudgementRequested {
                        who: ::subxt::ext::subxt_core::utils::AccountId32,
                        registrar_index: ::core::primitive::u32,
                    },
                    #[codec(index = 4)]
                    JudgementUnrequested {
                        who: ::subxt::ext::subxt_core::utils::AccountId32,
                        registrar_index: ::core::primitive::u32,
                    },
                    #[codec(index = 5)]
                    JudgementGiven {
                        target: ::subxt::ext::subxt_core::utils::AccountId32,
                        registrar_index: ::core::primitive::u32,
                    },
                    #[codec(index = 6)]
                    RegistrarAdded {
                        registrar_index: ::core::primitive::u32,
                    },
                    #[codec(index = 7)]
                    SubIdentityAdded {
                        sub: ::subxt::ext::subxt_core::utils::AccountId32,
                        main: ::subxt::ext::subxt_core::utils::AccountId32,
                        deposit: ::core::primitive::u128,
                    },
                    #[codec(index = 8)]
                    SubIdentityRemoved {
                        sub: ::subxt::ext::subxt_core::utils::AccountId32,
                        main: ::subxt::ext::subxt_core::utils::AccountId32,
                        deposit: ::core::primitive::u128,
                    },
                    #[codec(index = 9)]
                    SubIdentityRevoked {
                        sub: ::subxt::ext::subxt_core::utils::AccountId32,
                        main: ::subxt::ext::subxt_core::utils::AccountId32,
                        deposit: ::core::primitive::u128,
                    },
                }
            }
            pub mod types {
                use super::runtime_types;
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
                pub enum Call {
                    #[codec(index = 0)]
                    heartbeat {
                        heartbeat:
                            runtime_types::pallet_im_online::Heartbeat<::core::primitive::u32>,
                        signature: runtime_types::pallet_im_online::sr25519::app_sr25519::Signature,
                    },
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum Error {
                    #[codec(index = 0)]
                    InvalidKey,
                    #[codec(index = 1)]
                    DuplicatedHeartbeat,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum Event {
                    #[codec(index = 0)]
                    HeartbeatReceived {
                        authority_id: runtime_types::pallet_im_online::sr25519::app_sr25519::Public,
                    },
                    #[codec(index = 1)]
                    AllGood,
                    #[codec(index = 2)]
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
                pub enum Call {
                    #[codec(index = 0)]
                    as_multi_threshold_1 {
                        other_signatories: ::subxt::ext::subxt_core::alloc::vec::Vec<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                        >,
                        call: ::subxt::ext::subxt_core::alloc::boxed::Box<
                            runtime_types::vara_runtime::RuntimeCall,
                        >,
                    },
                    #[codec(index = 1)]
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
                pub enum Error {
                    #[codec(index = 0)]
                    MinimumThreshold,
                    #[codec(index = 1)]
                    AlreadyApproved,
                    #[codec(index = 2)]
                    NoApprovalsNeeded,
                    #[codec(index = 3)]
                    TooFewSignatories,
                    #[codec(index = 4)]
                    TooManySignatories,
                    #[codec(index = 5)]
                    SignatoriesOutOfOrder,
                    #[codec(index = 6)]
                    SenderInSignatories,
                    #[codec(index = 7)]
                    NotFound,
                    #[codec(index = 8)]
                    NotOwner,
                    #[codec(index = 9)]
                    NoTimepoint,
                    #[codec(index = 10)]
                    WrongTimepoint,
                    #[codec(index = 11)]
                    UnexpectedTimepoint,
                    #[codec(index = 12)]
                    MaxWeightTooLow,
                    #[codec(index = 13)]
                    AlreadyStored,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum Event {
                    #[codec(index = 0)]
                    NewMultisig {
                        approving: ::subxt::ext::subxt_core::utils::AccountId32,
                        multisig: ::subxt::ext::subxt_core::utils::AccountId32,
                        call_hash: [::core::primitive::u8; 32usize],
                    },
                    #[codec(index = 1)]
                    MultisigApproval {
                        approving: ::subxt::ext::subxt_core::utils::AccountId32,
                        timepoint:
                            runtime_types::pallet_multisig::Timepoint<::core::primitive::u32>,
                        multisig: ::subxt::ext::subxt_core::utils::AccountId32,
                        call_hash: [::core::primitive::u8; 32usize],
                    },
                    #[codec(index = 2)]
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
                pub enum Call {
                    #[codec(index = 0)]
                    join {
                        #[codec(compact)]
                        amount: ::core::primitive::u128,
                        pool_id: ::core::primitive::u32,
                    },
                    #[codec(index = 1)]
                    bond_extra {
                        extra: runtime_types::pallet_nomination_pools::BondExtra<
                            ::core::primitive::u128,
                        >,
                    },
                    #[codec(index = 2)]
                    claim_payout,
                    #[codec(index = 3)]
                    unbond {
                        member_account: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                        #[codec(compact)]
                        unbonding_points: ::core::primitive::u128,
                    },
                    #[codec(index = 4)]
                    pool_withdraw_unbonded {
                        pool_id: ::core::primitive::u32,
                        num_slashing_spans: ::core::primitive::u32,
                    },
                    #[codec(index = 5)]
                    withdraw_unbonded {
                        member_account: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                        num_slashing_spans: ::core::primitive::u32,
                    },
                    #[codec(index = 6)]
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
                    nominate {
                        pool_id: ::core::primitive::u32,
                        validators: ::subxt::ext::subxt_core::alloc::vec::Vec<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                        >,
                    },
                    #[codec(index = 9)]
                    set_state {
                        pool_id: ::core::primitive::u32,
                        state: runtime_types::pallet_nomination_pools::PoolState,
                    },
                    #[codec(index = 10)]
                    set_metadata {
                        pool_id: ::core::primitive::u32,
                        metadata: ::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
                    },
                    #[codec(index = 11)]
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
                    chill { pool_id: ::core::primitive::u32 },
                    #[codec(index = 14)]
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
                    set_claim_permission {
                        permission: runtime_types::pallet_nomination_pools::ClaimPermission,
                    },
                    #[codec(index = 16)]
                    claim_payout_other {
                        other: ::subxt::ext::subxt_core::utils::AccountId32,
                    },
                    #[codec(index = 17)]
                    set_commission {
                        pool_id: ::core::primitive::u32,
                        new_commission: ::core::option::Option<(
                            runtime_types::sp_arithmetic::per_things::Perbill,
                            ::subxt::ext::subxt_core::utils::AccountId32,
                        )>,
                    },
                    #[codec(index = 18)]
                    set_commission_max {
                        pool_id: ::core::primitive::u32,
                        max_commission: runtime_types::sp_arithmetic::per_things::Perbill,
                    },
                    #[codec(index = 19)]
                    set_commission_change_rate {
                        pool_id: ::core::primitive::u32,
                        change_rate: runtime_types::pallet_nomination_pools::CommissionChangeRate<
                            ::core::primitive::u32,
                        >,
                    },
                    #[codec(index = 20)]
                    claim_commission { pool_id: ::core::primitive::u32 },
                    #[codec(index = 21)]
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
                pub enum Error {
                    #[codec(index = 0)]
                    PoolNotFound,
                    #[codec(index = 1)]
                    PoolMemberNotFound,
                    #[codec(index = 2)]
                    RewardPoolNotFound,
                    #[codec(index = 3)]
                    SubPoolsNotFound,
                    #[codec(index = 4)]
                    AccountBelongsToOtherPool,
                    #[codec(index = 5)]
                    FullyUnbonding,
                    #[codec(index = 6)]
                    MaxUnbondingLimit,
                    #[codec(index = 7)]
                    CannotWithdrawAny,
                    #[codec(index = 8)]
                    MinimumBondNotMet,
                    #[codec(index = 9)]
                    OverflowRisk,
                    #[codec(index = 10)]
                    NotDestroying,
                    #[codec(index = 11)]
                    NotNominator,
                    #[codec(index = 12)]
                    NotKickerOrDestroying,
                    #[codec(index = 13)]
                    NotOpen,
                    #[codec(index = 14)]
                    MaxPools,
                    #[codec(index = 15)]
                    MaxPoolMembers,
                    #[codec(index = 16)]
                    CanNotChangeState,
                    #[codec(index = 17)]
                    DoesNotHavePermission,
                    #[codec(index = 18)]
                    MetadataExceedsMaxLen,
                    #[codec(index = 19)]
                    Defensive(runtime_types::pallet_nomination_pools::pallet::DefensiveError),
                    #[codec(index = 20)]
                    PartialUnbondNotAllowedPermissionlessly,
                    #[codec(index = 21)]
                    MaxCommissionRestricted,
                    #[codec(index = 22)]
                    CommissionExceedsMaximum,
                    #[codec(index = 23)]
                    CommissionExceedsGlobalMaximum,
                    #[codec(index = 24)]
                    CommissionChangeThrottled,
                    #[codec(index = 25)]
                    CommissionChangeRateNotAllowed,
                    #[codec(index = 26)]
                    NoPendingCommission,
                    #[codec(index = 27)]
                    NoCommissionCurrentSet,
                    #[codec(index = 28)]
                    PoolIdInUse,
                    #[codec(index = 29)]
                    InvalidPoolId,
                    #[codec(index = 30)]
                    BondExtraRestricted,
                    #[codec(index = 31)]
                    NothingToAdjust,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum Event {
                    #[codec(index = 0)]
                    Created {
                        depositor: ::subxt::ext::subxt_core::utils::AccountId32,
                        pool_id: ::core::primitive::u32,
                    },
                    #[codec(index = 1)]
                    Bonded {
                        member: ::subxt::ext::subxt_core::utils::AccountId32,
                        pool_id: ::core::primitive::u32,
                        bonded: ::core::primitive::u128,
                        joined: ::core::primitive::bool,
                    },
                    #[codec(index = 2)]
                    PaidOut {
                        member: ::subxt::ext::subxt_core::utils::AccountId32,
                        pool_id: ::core::primitive::u32,
                        payout: ::core::primitive::u128,
                    },
                    #[codec(index = 3)]
                    Unbonded {
                        member: ::subxt::ext::subxt_core::utils::AccountId32,
                        pool_id: ::core::primitive::u32,
                        balance: ::core::primitive::u128,
                        points: ::core::primitive::u128,
                        era: ::core::primitive::u32,
                    },
                    #[codec(index = 4)]
                    Withdrawn {
                        member: ::subxt::ext::subxt_core::utils::AccountId32,
                        pool_id: ::core::primitive::u32,
                        balance: ::core::primitive::u128,
                        points: ::core::primitive::u128,
                    },
                    #[codec(index = 5)]
                    Destroyed { pool_id: ::core::primitive::u32 },
                    #[codec(index = 6)]
                    StateChanged {
                        pool_id: ::core::primitive::u32,
                        new_state: runtime_types::pallet_nomination_pools::PoolState,
                    },
                    #[codec(index = 7)]
                    MemberRemoved {
                        pool_id: ::core::primitive::u32,
                        member: ::subxt::ext::subxt_core::utils::AccountId32,
                    },
                    #[codec(index = 8)]
                    RolesUpdated {
                        root: ::core::option::Option<::subxt::ext::subxt_core::utils::AccountId32>,
                        bouncer:
                            ::core::option::Option<::subxt::ext::subxt_core::utils::AccountId32>,
                        nominator:
                            ::core::option::Option<::subxt::ext::subxt_core::utils::AccountId32>,
                    },
                    #[codec(index = 9)]
                    PoolSlashed {
                        pool_id: ::core::primitive::u32,
                        balance: ::core::primitive::u128,
                    },
                    #[codec(index = 10)]
                    UnbondingPoolSlashed {
                        pool_id: ::core::primitive::u32,
                        era: ::core::primitive::u32,
                        balance: ::core::primitive::u128,
                    },
                    #[codec(index = 11)]
                    PoolCommissionUpdated {
                        pool_id: ::core::primitive::u32,
                        current: ::core::option::Option<(
                            runtime_types::sp_arithmetic::per_things::Perbill,
                            ::subxt::ext::subxt_core::utils::AccountId32,
                        )>,
                    },
                    #[codec(index = 12)]
                    PoolMaxCommissionUpdated {
                        pool_id: ::core::primitive::u32,
                        max_commission: runtime_types::sp_arithmetic::per_things::Perbill,
                    },
                    #[codec(index = 13)]
                    PoolCommissionChangeRateUpdated {
                        pool_id: ::core::primitive::u32,
                        change_rate: runtime_types::pallet_nomination_pools::CommissionChangeRate<
                            ::core::primitive::u32,
                        >,
                    },
                    #[codec(index = 14)]
                    PoolCommissionClaimed {
                        pool_id: ::core::primitive::u32,
                        commission: ::core::primitive::u128,
                    },
                    #[codec(index = 15)]
                    MinBalanceDeficitAdjusted {
                        pool_id: ::core::primitive::u32,
                        amount: ::core::primitive::u128,
                    },
                    #[codec(index = 16)]
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
                pub enum Event {
                    #[codec(index = 0)]
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
                pub enum Call {
                    #[codec(index = 0)]
                    note_preimage {
                        bytes: ::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
                    },
                    #[codec(index = 1)]
                    unnote_preimage {
                        hash: ::subxt::ext::subxt_core::utils::H256,
                    },
                    #[codec(index = 2)]
                    request_preimage {
                        hash: ::subxt::ext::subxt_core::utils::H256,
                    },
                    #[codec(index = 3)]
                    unrequest_preimage {
                        hash: ::subxt::ext::subxt_core::utils::H256,
                    },
                    #[codec(index = 4)]
                    ensure_updated {
                        hashes: ::subxt::ext::subxt_core::alloc::vec::Vec<
                            ::subxt::ext::subxt_core::utils::H256,
                        >,
                    },
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum Error {
                    #[codec(index = 0)]
                    TooBig,
                    #[codec(index = 1)]
                    AlreadyNoted,
                    #[codec(index = 2)]
                    NotAuthorized,
                    #[codec(index = 3)]
                    NotNoted,
                    #[codec(index = 4)]
                    Requested,
                    #[codec(index = 5)]
                    NotRequested,
                    #[codec(index = 6)]
                    TooMany,
                    #[codec(index = 7)]
                    TooFew,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum Event {
                    #[codec(index = 0)]
                    Noted {
                        hash: ::subxt::ext::subxt_core::utils::H256,
                    },
                    #[codec(index = 1)]
                    Requested {
                        hash: ::subxt::ext::subxt_core::utils::H256,
                    },
                    #[codec(index = 2)]
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
                pub enum Call {
                    #[codec(index = 0)]
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
                    add_proxy {
                        delegate: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                        proxy_type: runtime_types::vara_runtime::ProxyType,
                        delay: ::core::primitive::u32,
                    },
                    #[codec(index = 2)]
                    remove_proxy {
                        delegate: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                        proxy_type: runtime_types::vara_runtime::ProxyType,
                        delay: ::core::primitive::u32,
                    },
                    #[codec(index = 3)]
                    remove_proxies,
                    #[codec(index = 4)]
                    create_pure {
                        proxy_type: runtime_types::vara_runtime::ProxyType,
                        delay: ::core::primitive::u32,
                        index: ::core::primitive::u16,
                    },
                    #[codec(index = 5)]
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
                    announce {
                        real: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                        call_hash: ::subxt::ext::subxt_core::utils::H256,
                    },
                    #[codec(index = 7)]
                    remove_announcement {
                        real: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                        call_hash: ::subxt::ext::subxt_core::utils::H256,
                    },
                    #[codec(index = 8)]
                    reject_announcement {
                        delegate: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                        call_hash: ::subxt::ext::subxt_core::utils::H256,
                    },
                    #[codec(index = 9)]
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
                pub enum Error {
                    #[codec(index = 0)]
                    TooMany,
                    #[codec(index = 1)]
                    NotFound,
                    #[codec(index = 2)]
                    NotProxy,
                    #[codec(index = 3)]
                    Unproxyable,
                    #[codec(index = 4)]
                    Duplicate,
                    #[codec(index = 5)]
                    NoPermission,
                    #[codec(index = 6)]
                    Unannounced,
                    #[codec(index = 7)]
                    NoSelfProxy,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum Event {
                    #[codec(index = 0)]
                    ProxyExecuted {
                        result:
                            ::core::result::Result<(), runtime_types::sp_runtime::DispatchError>,
                    },
                    #[codec(index = 1)]
                    PureCreated {
                        pure: ::subxt::ext::subxt_core::utils::AccountId32,
                        who: ::subxt::ext::subxt_core::utils::AccountId32,
                        proxy_type: runtime_types::vara_runtime::ProxyType,
                        disambiguation_index: ::core::primitive::u16,
                    },
                    #[codec(index = 2)]
                    Announced {
                        real: ::subxt::ext::subxt_core::utils::AccountId32,
                        proxy: ::subxt::ext::subxt_core::utils::AccountId32,
                        call_hash: ::subxt::ext::subxt_core::utils::H256,
                    },
                    #[codec(index = 3)]
                    ProxyAdded {
                        delegator: ::subxt::ext::subxt_core::utils::AccountId32,
                        delegatee: ::subxt::ext::subxt_core::utils::AccountId32,
                        proxy_type: runtime_types::vara_runtime::ProxyType,
                        delay: ::core::primitive::u32,
                    },
                    #[codec(index = 4)]
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
                pub enum Call {
                    #[codec(index = 0)]
                    add_member {
                        who: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                    },
                    #[codec(index = 1)]
                    promote_member {
                        who: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                    },
                    #[codec(index = 2)]
                    demote_member {
                        who: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                    },
                    #[codec(index = 3)]
                    remove_member {
                        who: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                        min_rank: ::core::primitive::u16,
                    },
                    #[codec(index = 4)]
                    vote {
                        poll: ::core::primitive::u32,
                        aye: ::core::primitive::bool,
                    },
                    #[codec(index = 5)]
                    cleanup_poll {
                        poll_index: ::core::primitive::u32,
                        max: ::core::primitive::u32,
                    },
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum Error {
                    #[codec(index = 0)]
                    AlreadyMember,
                    #[codec(index = 1)]
                    NotMember,
                    #[codec(index = 2)]
                    NotPolling,
                    #[codec(index = 3)]
                    Ongoing,
                    #[codec(index = 4)]
                    NoneRemaining,
                    #[codec(index = 5)]
                    Corruption,
                    #[codec(index = 6)]
                    RankTooLow,
                    #[codec(index = 7)]
                    InvalidWitness,
                    #[codec(index = 8)]
                    NoPermission,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum Event {
                    #[codec(index = 0)]
                    MemberAdded {
                        who: ::subxt::ext::subxt_core::utils::AccountId32,
                    },
                    #[codec(index = 1)]
                    RankChanged {
                        who: ::subxt::ext::subxt_core::utils::AccountId32,
                        rank: ::core::primitive::u16,
                    },
                    #[codec(index = 2)]
                    MemberRemoved {
                        who: ::subxt::ext::subxt_core::utils::AccountId32,
                        rank: ::core::primitive::u16,
                    },
                    #[codec(index = 3)]
                    Voted {
                        who: ::subxt::ext::subxt_core::utils::AccountId32,
                        poll: ::core::primitive::u32,
                        vote: runtime_types::pallet_ranked_collective::VoteRecord,
                        tally: runtime_types::pallet_ranked_collective::Tally,
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
                pub enum Call {
                    #[codec(index = 0)]
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
                    place_decision_deposit { index: ::core::primitive::u32 },
                    #[codec(index = 2)]
                    refund_decision_deposit { index: ::core::primitive::u32 },
                    #[codec(index = 3)]
                    cancel { index: ::core::primitive::u32 },
                    #[codec(index = 4)]
                    kill { index: ::core::primitive::u32 },
                    #[codec(index = 5)]
                    nudge_referendum { index: ::core::primitive::u32 },
                    #[codec(index = 6)]
                    one_fewer_deciding { track: ::core::primitive::u16 },
                    #[codec(index = 7)]
                    refund_submission_deposit { index: ::core::primitive::u32 },
                    #[codec(index = 8)]
                    set_metadata {
                        index: ::core::primitive::u32,
                        maybe_hash: ::core::option::Option<::subxt::ext::subxt_core::utils::H256>,
                    },
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum Error {
                    #[codec(index = 0)]
                    NotOngoing,
                    #[codec(index = 1)]
                    HasDeposit,
                    #[codec(index = 2)]
                    BadTrack,
                    #[codec(index = 3)]
                    Full,
                    #[codec(index = 4)]
                    QueueEmpty,
                    #[codec(index = 5)]
                    BadReferendum,
                    #[codec(index = 6)]
                    NothingToDo,
                    #[codec(index = 7)]
                    NoTrack,
                    #[codec(index = 8)]
                    Unfinished,
                    #[codec(index = 9)]
                    NoPermission,
                    #[codec(index = 10)]
                    NoDeposit,
                    #[codec(index = 11)]
                    BadStatus,
                    #[codec(index = 12)]
                    PreimageNotExist,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum Event1 {
                    #[codec(index = 0)]
                    Submitted {
                        index: ::core::primitive::u32,
                        track: ::core::primitive::u16,
                        proposal: runtime_types::frame_support::traits::preimages::Bounded<
                            runtime_types::vara_runtime::RuntimeCall,
                            runtime_types::sp_runtime::traits::BlakeTwo256,
                        >,
                    },
                    #[codec(index = 1)]
                    DecisionDepositPlaced {
                        index: ::core::primitive::u32,
                        who: ::subxt::ext::subxt_core::utils::AccountId32,
                        amount: ::core::primitive::u128,
                    },
                    #[codec(index = 2)]
                    DecisionDepositRefunded {
                        index: ::core::primitive::u32,
                        who: ::subxt::ext::subxt_core::utils::AccountId32,
                        amount: ::core::primitive::u128,
                    },
                    #[codec(index = 3)]
                    DepositSlashed {
                        who: ::subxt::ext::subxt_core::utils::AccountId32,
                        amount: ::core::primitive::u128,
                    },
                    #[codec(index = 4)]
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
                    Confirmed {
                        index: ::core::primitive::u32,
                        tally: runtime_types::pallet_conviction_voting::types::Tally<
                            ::core::primitive::u128,
                        >,
                    },
                    #[codec(index = 8)]
                    Approved { index: ::core::primitive::u32 },
                    #[codec(index = 9)]
                    Rejected {
                        index: ::core::primitive::u32,
                        tally: runtime_types::pallet_conviction_voting::types::Tally<
                            ::core::primitive::u128,
                        >,
                    },
                    #[codec(index = 10)]
                    TimedOut {
                        index: ::core::primitive::u32,
                        tally: runtime_types::pallet_conviction_voting::types::Tally<
                            ::core::primitive::u128,
                        >,
                    },
                    #[codec(index = 11)]
                    Cancelled {
                        index: ::core::primitive::u32,
                        tally: runtime_types::pallet_conviction_voting::types::Tally<
                            ::core::primitive::u128,
                        >,
                    },
                    #[codec(index = 12)]
                    Killed {
                        index: ::core::primitive::u32,
                        tally: runtime_types::pallet_conviction_voting::types::Tally<
                            ::core::primitive::u128,
                        >,
                    },
                    #[codec(index = 13)]
                    SubmissionDepositRefunded {
                        index: ::core::primitive::u32,
                        who: ::subxt::ext::subxt_core::utils::AccountId32,
                        amount: ::core::primitive::u128,
                    },
                    #[codec(index = 14)]
                    MetadataSet {
                        index: ::core::primitive::u32,
                        hash: ::subxt::ext::subxt_core::utils::H256,
                    },
                    #[codec(index = 15)]
                    MetadataCleared {
                        index: ::core::primitive::u32,
                        hash: ::subxt::ext::subxt_core::utils::H256,
                    },
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum Event2 {
                    #[codec(index = 0)]
                    Submitted {
                        index: ::core::primitive::u32,
                        track: ::core::primitive::u16,
                        proposal: runtime_types::frame_support::traits::preimages::Bounded<
                            runtime_types::vara_runtime::RuntimeCall,
                            runtime_types::sp_runtime::traits::BlakeTwo256,
                        >,
                    },
                    #[codec(index = 1)]
                    DecisionDepositPlaced {
                        index: ::core::primitive::u32,
                        who: ::subxt::ext::subxt_core::utils::AccountId32,
                        amount: ::core::primitive::u128,
                    },
                    #[codec(index = 2)]
                    DecisionDepositRefunded {
                        index: ::core::primitive::u32,
                        who: ::subxt::ext::subxt_core::utils::AccountId32,
                        amount: ::core::primitive::u128,
                    },
                    #[codec(index = 3)]
                    DepositSlashed {
                        who: ::subxt::ext::subxt_core::utils::AccountId32,
                        amount: ::core::primitive::u128,
                    },
                    #[codec(index = 4)]
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
                    Confirmed {
                        index: ::core::primitive::u32,
                        tally: runtime_types::pallet_ranked_collective::Tally,
                    },
                    #[codec(index = 8)]
                    Approved { index: ::core::primitive::u32 },
                    #[codec(index = 9)]
                    Rejected {
                        index: ::core::primitive::u32,
                        tally: runtime_types::pallet_ranked_collective::Tally,
                    },
                    #[codec(index = 10)]
                    TimedOut {
                        index: ::core::primitive::u32,
                        tally: runtime_types::pallet_ranked_collective::Tally,
                    },
                    #[codec(index = 11)]
                    Cancelled {
                        index: ::core::primitive::u32,
                        tally: runtime_types::pallet_ranked_collective::Tally,
                    },
                    #[codec(index = 12)]
                    Killed {
                        index: ::core::primitive::u32,
                        tally: runtime_types::pallet_ranked_collective::Tally,
                    },
                    #[codec(index = 13)]
                    SubmissionDepositRefunded {
                        index: ::core::primitive::u32,
                        who: ::subxt::ext::subxt_core::utils::AccountId32,
                        amount: ::core::primitive::u128,
                    },
                    #[codec(index = 14)]
                    MetadataSet {
                        index: ::core::primitive::u32,
                        hash: ::subxt::ext::subxt_core::utils::H256,
                    },
                    #[codec(index = 15)]
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
                pub enum Call {
                    #[codec(index = 0)]
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
                    cancel {
                        when: ::core::primitive::u32,
                        index: ::core::primitive::u32,
                    },
                    #[codec(index = 2)]
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
                    cancel_named {
                        id: [::core::primitive::u8; 32usize],
                    },
                    #[codec(index = 4)]
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
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum Error {
                    #[codec(index = 0)]
                    FailedToSchedule,
                    #[codec(index = 1)]
                    NotFound,
                    #[codec(index = 2)]
                    TargetBlockNumberInPast,
                    #[codec(index = 3)]
                    RescheduleNoChange,
                    #[codec(index = 4)]
                    Named,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum Event {
                    #[codec(index = 0)]
                    Scheduled {
                        when: ::core::primitive::u32,
                        index: ::core::primitive::u32,
                    },
                    #[codec(index = 1)]
                    Canceled {
                        when: ::core::primitive::u32,
                        index: ::core::primitive::u32,
                    },
                    #[codec(index = 2)]
                    Dispatched {
                        task: (::core::primitive::u32, ::core::primitive::u32),
                        id: ::core::option::Option<[::core::primitive::u8; 32usize]>,
                        result:
                            ::core::result::Result<(), runtime_types::sp_runtime::DispatchError>,
                    },
                    #[codec(index = 3)]
                    CallUnavailable {
                        task: (::core::primitive::u32, ::core::primitive::u32),
                        id: ::core::option::Option<[::core::primitive::u8; 32usize]>,
                    },
                    #[codec(index = 4)]
                    PeriodicFailed {
                        task: (::core::primitive::u32, ::core::primitive::u32),
                        id: ::core::option::Option<[::core::primitive::u8; 32usize]>,
                    },
                    #[codec(index = 5)]
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
                pub __ignore: ::core::marker::PhantomData<_4>,
            }
        }
        pub mod pallet_session {
            use super::runtime_types;
            pub mod pallet {
                use super::runtime_types;
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum Call {
                    #[codec(index = 0)]
                    set_keys {
                        keys: runtime_types::vara_runtime::SessionKeys,
                        proof: ::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
                    },
                    #[codec(index = 1)]
                    purge_keys,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum Error {
                    #[codec(index = 0)]
                    InvalidProof,
                    #[codec(index = 1)]
                    NoAssociatedValidatorId,
                    #[codec(index = 2)]
                    DuplicatedKey,
                    #[codec(index = 3)]
                    NoKeys,
                    #[codec(index = 4)]
                    NoAccount,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum Event {
                    #[codec(index = 0)]
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
                    pub enum Call {
                        #[codec(index = 0)]
                        bond {
                            #[codec(compact)]
                            value: ::core::primitive::u128,
                            payee: runtime_types::pallet_staking::RewardDestination<
                                ::subxt::ext::subxt_core::utils::AccountId32,
                            >,
                        },
                        #[codec(index = 1)]
                        bond_extra {
                            #[codec(compact)]
                            max_additional: ::core::primitive::u128,
                        },
                        #[codec(index = 2)]
                        unbond {
                            #[codec(compact)]
                            value: ::core::primitive::u128,
                        },
                        #[codec(index = 3)]
                        withdraw_unbonded {
                            num_slashing_spans: ::core::primitive::u32,
                        },
                        #[codec(index = 4)]
                        validate {
                            prefs: runtime_types::pallet_staking::ValidatorPrefs,
                        },
                        #[codec(index = 5)]
                        nominate {
                            targets: ::subxt::ext::subxt_core::alloc::vec::Vec<
                                ::subxt::ext::subxt_core::utils::MultiAddress<
                                    ::subxt::ext::subxt_core::utils::AccountId32,
                                    (),
                                >,
                            >,
                        },
                        #[codec(index = 6)]
                        chill,
                        #[codec(index = 7)]
                        set_payee {
                            payee: runtime_types::pallet_staking::RewardDestination<
                                ::subxt::ext::subxt_core::utils::AccountId32,
                            >,
                        },
                        #[codec(index = 8)]
                        set_controller,
                        #[codec(index = 9)]
                        set_validator_count {
                            #[codec(compact)]
                            new: ::core::primitive::u32,
                        },
                        #[codec(index = 10)]
                        increase_validator_count {
                            #[codec(compact)]
                            additional: ::core::primitive::u32,
                        },
                        #[codec(index = 11)]
                        scale_validator_count {
                            factor: runtime_types::sp_arithmetic::per_things::Percent,
                        },
                        #[codec(index = 12)]
                        force_no_eras,
                        #[codec(index = 13)]
                        force_new_era,
                        #[codec(index = 14)]
                        set_invulnerables {
                            invulnerables: ::subxt::ext::subxt_core::alloc::vec::Vec<
                                ::subxt::ext::subxt_core::utils::AccountId32,
                            >,
                        },
                        #[codec(index = 15)]
                        force_unstake {
                            stash: ::subxt::ext::subxt_core::utils::AccountId32,
                            num_slashing_spans: ::core::primitive::u32,
                        },
                        #[codec(index = 16)]
                        force_new_era_always,
                        #[codec(index = 17)]
                        cancel_deferred_slash {
                            era: ::core::primitive::u32,
                            slash_indices:
                                ::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u32>,
                        },
                        #[codec(index = 18)]
                        payout_stakers {
                            validator_stash: ::subxt::ext::subxt_core::utils::AccountId32,
                            era: ::core::primitive::u32,
                        },
                        #[codec(index = 19)]
                        rebond {
                            #[codec(compact)]
                            value: ::core::primitive::u128,
                        },
                        #[codec(index = 20)]
                        reap_stash {
                            stash: ::subxt::ext::subxt_core::utils::AccountId32,
                            num_slashing_spans: ::core::primitive::u32,
                        },
                        #[codec(index = 21)]
                        kick {
                            who: ::subxt::ext::subxt_core::alloc::vec::Vec<
                                ::subxt::ext::subxt_core::utils::MultiAddress<
                                    ::subxt::ext::subxt_core::utils::AccountId32,
                                    (),
                                >,
                            >,
                        },
                        #[codec(index = 22)]
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
                        chill_other {
                            controller: ::subxt::ext::subxt_core::utils::AccountId32,
                        },
                        #[codec(index = 24)]
                        force_apply_min_commission {
                            validator_stash: ::subxt::ext::subxt_core::utils::AccountId32,
                        },
                        #[codec(index = 25)]
                        set_min_commission {
                            new: runtime_types::sp_arithmetic::per_things::Perbill,
                        },
                        #[codec(index = 26)]
                        payout_stakers_by_page {
                            validator_stash: ::subxt::ext::subxt_core::utils::AccountId32,
                            era: ::core::primitive::u32,
                            page: ::core::primitive::u32,
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
                    pub enum Error {
                        #[codec(index = 0)]
                        NotController,
                        #[codec(index = 1)]
                        NotStash,
                        #[codec(index = 2)]
                        AlreadyBonded,
                        #[codec(index = 3)]
                        AlreadyPaired,
                        #[codec(index = 4)]
                        EmptyTargets,
                        #[codec(index = 5)]
                        DuplicateIndex,
                        #[codec(index = 6)]
                        InvalidSlashIndex,
                        #[codec(index = 7)]
                        InsufficientBond,
                        #[codec(index = 8)]
                        NoMoreChunks,
                        #[codec(index = 9)]
                        NoUnlockChunk,
                        #[codec(index = 10)]
                        FundedTarget,
                        #[codec(index = 11)]
                        InvalidEraToReward,
                        #[codec(index = 12)]
                        InvalidNumberOfNominations,
                        #[codec(index = 13)]
                        NotSortedAndUnique,
                        #[codec(index = 14)]
                        AlreadyClaimed,
                        #[codec(index = 15)]
                        InvalidPage,
                        #[codec(index = 16)]
                        IncorrectHistoryDepth,
                        #[codec(index = 17)]
                        IncorrectSlashingSpans,
                        #[codec(index = 18)]
                        BadState,
                        #[codec(index = 19)]
                        TooManyTargets,
                        #[codec(index = 20)]
                        BadTarget,
                        #[codec(index = 21)]
                        CannotChillOther,
                        #[codec(index = 22)]
                        TooManyNominators,
                        #[codec(index = 23)]
                        TooManyValidators,
                        #[codec(index = 24)]
                        CommissionTooLow,
                        #[codec(index = 25)]
                        BoundNotMet,
                    }
                    #[derive(
                        Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode,
                    )]
                    pub enum Event {
                        #[codec(index = 0)]
                        EraPaid {
                            era_index: ::core::primitive::u32,
                            validator_payout: ::core::primitive::u128,
                            remainder: ::core::primitive::u128,
                        },
                        #[codec(index = 1)]
                        Rewarded {
                            stash: ::subxt::ext::subxt_core::utils::AccountId32,
                            dest: runtime_types::pallet_staking::RewardDestination<
                                ::subxt::ext::subxt_core::utils::AccountId32,
                            >,
                            amount: ::core::primitive::u128,
                        },
                        #[codec(index = 2)]
                        Slashed {
                            staker: ::subxt::ext::subxt_core::utils::AccountId32,
                            amount: ::core::primitive::u128,
                        },
                        #[codec(index = 3)]
                        SlashReported {
                            validator: ::subxt::ext::subxt_core::utils::AccountId32,
                            fraction: runtime_types::sp_arithmetic::per_things::Perbill,
                            slash_era: ::core::primitive::u32,
                        },
                        #[codec(index = 4)]
                        OldSlashingReportDiscarded {
                            session_index: ::core::primitive::u32,
                        },
                        #[codec(index = 5)]
                        StakersElected,
                        #[codec(index = 6)]
                        Bonded {
                            stash: ::subxt::ext::subxt_core::utils::AccountId32,
                            amount: ::core::primitive::u128,
                        },
                        #[codec(index = 7)]
                        Unbonded {
                            stash: ::subxt::ext::subxt_core::utils::AccountId32,
                            amount: ::core::primitive::u128,
                        },
                        #[codec(index = 8)]
                        Withdrawn {
                            stash: ::subxt::ext::subxt_core::utils::AccountId32,
                            amount: ::core::primitive::u128,
                        },
                        #[codec(index = 9)]
                        Kicked {
                            nominator: ::subxt::ext::subxt_core::utils::AccountId32,
                            stash: ::subxt::ext::subxt_core::utils::AccountId32,
                        },
                        #[codec(index = 10)]
                        StakingElectionFailed,
                        #[codec(index = 11)]
                        Chilled {
                            stash: ::subxt::ext::subxt_core::utils::AccountId32,
                        },
                        #[codec(index = 12)]
                        PayoutStarted {
                            era_index: ::core::primitive::u32,
                            validator_stash: ::subxt::ext::subxt_core::utils::AccountId32,
                        },
                        #[codec(index = 13)]
                        ValidatorPrefsSet {
                            stash: ::subxt::ext::subxt_core::utils::AccountId32,
                            prefs: runtime_types::pallet_staking::ValidatorPrefs,
                        },
                        #[codec(index = 14)]
                        SnapshotVotersSizeExceeded { size: ::core::primitive::u32 },
                        #[codec(index = 15)]
                        SnapshotTargetsSizeExceeded { size: ::core::primitive::u32 },
                        #[codec(index = 16)]
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
                pub enum Call {
                    #[codec(index = 0)]
                    sudo {
                        call: ::subxt::ext::subxt_core::alloc::boxed::Box<
                            runtime_types::vara_runtime::RuntimeCall,
                        >,
                    },
                    #[codec(index = 1)]
                    sudo_unchecked_weight {
                        call: ::subxt::ext::subxt_core::alloc::boxed::Box<
                            runtime_types::vara_runtime::RuntimeCall,
                        >,
                        weight: runtime_types::sp_weights::weight_v2::Weight,
                    },
                    #[codec(index = 2)]
                    set_key {
                        new: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                    },
                    #[codec(index = 3)]
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
                    remove_key,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum Error {
                    #[codec(index = 0)]
                    RequireSudo,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum Event {
                    #[codec(index = 0)]
                    Sudid {
                        sudo_result:
                            ::core::result::Result<(), runtime_types::sp_runtime::DispatchError>,
                    },
                    #[codec(index = 1)]
                    KeyChanged {
                        old: ::core::option::Option<::subxt::ext::subxt_core::utils::AccountId32>,
                        new: ::subxt::ext::subxt_core::utils::AccountId32,
                    },
                    #[codec(index = 2)]
                    KeyRemoved,
                    #[codec(index = 3)]
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
                pub enum Call {
                    #[codec(index = 0)]
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
                pub enum Event {
                    #[codec(index = 0)]
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
                pub enum Call {
                    #[codec(index = 0)]
                    propose_spend {
                        #[codec(compact)]
                        value: ::core::primitive::u128,
                        beneficiary: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                    },
                    #[codec(index = 1)]
                    reject_proposal {
                        #[codec(compact)]
                        proposal_id: ::core::primitive::u32,
                    },
                    #[codec(index = 2)]
                    approve_proposal {
                        #[codec(compact)]
                        proposal_id: ::core::primitive::u32,
                    },
                    #[codec(index = 3)]
                    spend_local {
                        #[codec(compact)]
                        amount: ::core::primitive::u128,
                        beneficiary: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                    },
                    #[codec(index = 4)]
                    remove_approval {
                        #[codec(compact)]
                        proposal_id: ::core::primitive::u32,
                    },
                    #[codec(index = 5)]
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
                    payout { index: ::core::primitive::u32 },
                    #[codec(index = 7)]
                    check_status { index: ::core::primitive::u32 },
                    #[codec(index = 8)]
                    void_spend { index: ::core::primitive::u32 },
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum Error {
                    #[codec(index = 0)]
                    InsufficientProposersBalance,
                    #[codec(index = 1)]
                    InvalidIndex,
                    #[codec(index = 2)]
                    TooManyApprovals,
                    #[codec(index = 3)]
                    InsufficientPermission,
                    #[codec(index = 4)]
                    ProposalNotApproved,
                    #[codec(index = 5)]
                    FailedToConvertBalance,
                    #[codec(index = 6)]
                    SpendExpired,
                    #[codec(index = 7)]
                    EarlyPayout,
                    #[codec(index = 8)]
                    AlreadyAttempted,
                    #[codec(index = 9)]
                    PayoutError,
                    #[codec(index = 10)]
                    NotAttempted,
                    #[codec(index = 11)]
                    Inconclusive,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum Event {
                    #[codec(index = 0)]
                    Proposed {
                        proposal_index: ::core::primitive::u32,
                    },
                    #[codec(index = 1)]
                    Spending {
                        budget_remaining: ::core::primitive::u128,
                    },
                    #[codec(index = 2)]
                    Awarded {
                        proposal_index: ::core::primitive::u32,
                        award: ::core::primitive::u128,
                        account: ::subxt::ext::subxt_core::utils::AccountId32,
                    },
                    #[codec(index = 3)]
                    Rejected {
                        proposal_index: ::core::primitive::u32,
                        slashed: ::core::primitive::u128,
                    },
                    #[codec(index = 4)]
                    Burnt {
                        burnt_funds: ::core::primitive::u128,
                    },
                    #[codec(index = 5)]
                    Rollover {
                        rollover_balance: ::core::primitive::u128,
                    },
                    #[codec(index = 6)]
                    Deposit { value: ::core::primitive::u128 },
                    #[codec(index = 7)]
                    SpendApproved {
                        proposal_index: ::core::primitive::u32,
                        amount: ::core::primitive::u128,
                        beneficiary: ::subxt::ext::subxt_core::utils::AccountId32,
                    },
                    #[codec(index = 8)]
                    UpdatedInactive {
                        reactivated: ::core::primitive::u128,
                        deactivated: ::core::primitive::u128,
                    },
                    #[codec(index = 9)]
                    AssetSpendApproved {
                        index: ::core::primitive::u32,
                        asset_kind: (),
                        amount: ::core::primitive::u128,
                        beneficiary: ::subxt::ext::subxt_core::utils::AccountId32,
                        valid_from: ::core::primitive::u32,
                        expire_at: ::core::primitive::u32,
                    },
                    #[codec(index = 10)]
                    AssetSpendVoided { index: ::core::primitive::u32 },
                    #[codec(index = 11)]
                    Paid {
                        index: ::core::primitive::u32,
                        payment_id: (),
                    },
                    #[codec(index = 12)]
                    PaymentFailed {
                        index: ::core::primitive::u32,
                        payment_id: (),
                    },
                    #[codec(index = 13)]
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
                pub enum Call {
                    #[codec(index = 0)]
                    batch {
                        calls: ::subxt::ext::subxt_core::alloc::vec::Vec<
                            runtime_types::vara_runtime::RuntimeCall,
                        >,
                    },
                    #[codec(index = 1)]
                    as_derivative {
                        index: ::core::primitive::u16,
                        call: ::subxt::ext::subxt_core::alloc::boxed::Box<
                            runtime_types::vara_runtime::RuntimeCall,
                        >,
                    },
                    #[codec(index = 2)]
                    batch_all {
                        calls: ::subxt::ext::subxt_core::alloc::vec::Vec<
                            runtime_types::vara_runtime::RuntimeCall,
                        >,
                    },
                    #[codec(index = 3)]
                    dispatch_as {
                        as_origin: ::subxt::ext::subxt_core::alloc::boxed::Box<
                            runtime_types::vara_runtime::OriginCaller,
                        >,
                        call: ::subxt::ext::subxt_core::alloc::boxed::Box<
                            runtime_types::vara_runtime::RuntimeCall,
                        >,
                    },
                    #[codec(index = 4)]
                    force_batch {
                        calls: ::subxt::ext::subxt_core::alloc::vec::Vec<
                            runtime_types::vara_runtime::RuntimeCall,
                        >,
                    },
                    #[codec(index = 5)]
                    with_weight {
                        call: ::subxt::ext::subxt_core::alloc::boxed::Box<
                            runtime_types::vara_runtime::RuntimeCall,
                        >,
                        weight: runtime_types::sp_weights::weight_v2::Weight,
                    },
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum Error {
                    #[codec(index = 0)]
                    TooManyCalls,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum Event {
                    #[codec(index = 0)]
                    BatchInterrupted {
                        index: ::core::primitive::u32,
                        error: runtime_types::sp_runtime::DispatchError,
                    },
                    #[codec(index = 1)]
                    BatchCompleted,
                    #[codec(index = 2)]
                    BatchCompletedWithErrors,
                    #[codec(index = 3)]
                    ItemCompleted,
                    #[codec(index = 4)]
                    ItemFailed {
                        error: runtime_types::sp_runtime::DispatchError,
                    },
                    #[codec(index = 5)]
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
                pub enum Call {
                    #[codec(index = 0)]
                    vest,
                    #[codec(index = 1)]
                    vest_other {
                        target: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                    },
                    #[codec(index = 2)]
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
                    merge_schedules {
                        schedule1_index: ::core::primitive::u32,
                        schedule2_index: ::core::primitive::u32,
                    },
                    #[codec(index = 5)]
                    force_remove_vesting_schedule {
                        target: ::subxt::ext::subxt_core::utils::MultiAddress<
                            ::subxt::ext::subxt_core::utils::AccountId32,
                            (),
                        >,
                        schedule_index: ::core::primitive::u32,
                    },
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum Error {
                    #[codec(index = 0)]
                    NotVesting,
                    #[codec(index = 1)]
                    AtMaxVestingSchedules,
                    #[codec(index = 2)]
                    AmountLow,
                    #[codec(index = 3)]
                    ScheduleIndexOutOfBounds,
                    #[codec(index = 4)]
                    InvalidScheduleParams,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
                pub enum Event {
                    #[codec(index = 0)]
                    VestingUpdated {
                        account: ::subxt::ext::subxt_core::utils::AccountId32,
                        unvested: ::core::primitive::u128,
                    },
                    #[codec(index = 1)]
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
                pub enum Error {
                    #[codec(index = 0)]
                    UnavailablePreImage,
                    #[codec(index = 1)]
                    UndecodableCall,
                    #[codec(index = 2)]
                    InvalidCallWeightWitness,
                    #[codec(index = 3)]
                    CallIsNotWhitelisted,
                    #[codec(index = 4)]
                    CallAlreadyWhitelisted,
                }
                #[derive(Debug, crate::gp::Decode, crate::gp::DecodeAsType, crate::gp::Encode)]
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
                #[codec(index = 104)]
                Gear(runtime_types::pallet_gear::pallet::Call),
                #[codec(index = 106)]
                StakingRewards(runtime_types::pallet_gear_staking_rewards::pallet::Call),
                #[codec(index = 107)]
                GearVoucher(runtime_types::pallet_gear_voucher::pallet::Call),
                #[codec(index = 110)]
                GearEthBridge(runtime_types::pallet_gear_eth_bridge::pallet::Call),
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
                #[codec(index = 104)]
                Gear(runtime_types::pallet_gear::pallet::Event),
                #[codec(index = 106)]
                StakingRewards(runtime_types::pallet_gear_staking_rewards::pallet::Event),
                #[codec(index = 107)]
                GearVoucher(runtime_types::pallet_gear_voucher::pallet::Event),
                #[codec(index = 110)]
                GearEthBridge(runtime_types::pallet_gear_eth_bridge::pallet::Event),
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
        ClaimValueToInheritor,
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
    #[doc = "Calls of pallet `GearEthBridge`."]
    pub enum GearEthBridgeCall {
        Pause,
        Unpause,
        SendEthMessage,
    }
    impl CallInfo for GearEthBridgeCall {
        const PALLET: &'static str = "GearEthBridge";
        fn call_name(&self) -> &'static str {
            match self {
                Self::Pause => "pause",
                Self::Unpause => "unpause",
                Self::SendEthMessage => "send_eth_message",
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
        PayoutStakersByPage,
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
    #[doc = "Storage of pallet `GearEthBridge`."]
    pub enum GearEthBridgeStorage {
        Initialized,
        Paused,
        AuthoritySetHash,
        QueueMerkleRoot,
        Queue,
        SessionsTimer,
        ClearTimer,
        MessageNonce,
        QueueChanged,
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
                Self::SessionsTimer => "SessionsTimer",
                Self::ClearTimer => "ClearTimer",
                Self::MessageNonce => "MessageNonce",
                Self::QueueChanged => "QueueChanged",
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
        ErasStakersOverview,
        ErasStakersClipped,
        ErasStakersPaged,
        ClaimedRewards,
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
                Self::ErasStakersOverview => "ErasStakersOverview",
                Self::ErasStakersClipped => "ErasStakersClipped",
                Self::ErasStakersPaged => "ErasStakersPaged",
                Self::ClaimedRewards => "ClaimedRewards",
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
    pub mod sudo {
        pub use super::runtime_types::pallet_sudo::pallet::Event;
    }
    pub mod gear_debug {
        pub use super::runtime_types::pallet_gear_debug::pallet::Event;
    }
}
