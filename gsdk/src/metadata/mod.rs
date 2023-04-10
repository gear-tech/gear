#![allow(dead_code, unused_imports, non_camel_case_types)]
#![allow(clippy::all)]
#![allow(unused)]

mod impls;

#[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
pub enum Event {
    #[codec(index = 0)]
    System(system::Event),
    #[codec(index = 4)]
    Grandpa(grandpa::Event),
    #[codec(index = 5)]
    Balances(balances::Event),
    #[codec(index = 10)]
    Vesting(vesting::Event),
    #[codec(index = 6)]
    TransactionPayment(transaction_payment::Event),
    #[codec(index = 11)]
    BagsList(bags_list::Event),
    #[codec(index = 12)]
    ImOnline(im_online::Event),
    #[codec(index = 13)]
    Staking(staking::Event),
    #[codec(index = 7)]
    Session(session::Event),
    #[codec(index = 14)]
    Treasury(treasury::Event),
    #[codec(index = 16)]
    ConvictionVoting(conviction_voting::Event),
    #[codec(index = 17)]
    Referenda(referenda::Event),
    #[codec(index = 18)]
    FellowshipCollective(fellowship_collective::Event),
    #[codec(index = 19)]
    FellowshipReferenda(fellowship_referenda::Event),
    #[codec(index = 21)]
    Whitelist(whitelist::Event),
    #[codec(index = 99)]
    Sudo(sudo::Event),
    #[codec(index = 22)]
    Scheduler(scheduler::Event),
    #[codec(index = 23)]
    Preimage(preimage::Event),
    #[codec(index = 24)]
    Identity(identity::Event),
    #[codec(index = 8)]
    Utility(utility::Event),
    #[codec(index = 104)]
    Gear(gear::Event),
    #[codec(index = 106)]
    StakingRewards(staking_rewards::Event),
    #[codec(index = 198)]
    Airdrop(airdrop::Event),
    #[codec(index = 199)]
    GearDebug(gear_debug::Event),
}

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
    pub use super::runtime_types::pallet_ranked_collective::pallet::Event;
}

pub mod whitelist {
    pub use super::runtime_types::pallet_whitelist::pallet::Event;
}

pub mod sudo {
    pub use super::runtime_types::pallet_sudo::pallet::Event;
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
pub mod utility {
    pub use super::runtime_types::pallet_utility::pallet::Event;
}

pub mod gear {
    pub use super::runtime_types::pallet_gear::pallet::Event;
}

pub mod staking_rewards {
    pub use super::runtime_types::pallet_gear_staking_rewards::pallet::Event;
}

pub mod airdrop {
    pub use super::runtime_types::pallet_airdrop::pallet::Event;
}

pub mod gear_debug {
    pub use super::runtime_types::pallet_gear_debug::pallet::Event;
}

pub mod runtime_types {
    use super::runtime_types;
    pub mod consensus_grandpa {
        use super::runtime_types;
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        pub struct Equivocation<_0, _1, _2> {
            pub round_number: ::core::primitive::u64,
            pub identity: _0,
            pub first: (_1, _2),
            pub second: (_1, _2),
        }
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        pub struct Precommit<_0, _1> {
            pub target_hash: _0,
            pub target_number: _1,
        }
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        pub struct Prevote<_0, _1> {
            pub target_hash: _0,
            pub target_number: _1,
        }
    }
    pub mod frame_support {
        use super::runtime_types;
        pub mod dispatch {
            use super::runtime_types;
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
            pub enum DispatchClass {
                #[codec(index = 0)]
                Normal,
                #[codec(index = 1)]
                Operational,
                #[codec(index = 2)]
                Mandatory,
            }
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
            pub struct DispatchInfo {
                pub weight: runtime_types::sp_weights::weight_v2::Weight,
                pub class: runtime_types::frame_support::dispatch::DispatchClass,
                pub pays_fee: runtime_types::frame_support::dispatch::Pays,
            }
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
            pub enum Pays {
                #[codec(index = 0)]
                Yes,
                #[codec(index = 1)]
                No,
            }
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
            pub struct PerDispatchClass<_0> {
                pub normal: _0,
                pub operational: _0,
                pub mandatory: _0,
            }
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
            pub struct PostDispatchInfo {
                pub actual_weight:
                    ::core::option::Option<runtime_types::sp_weights::weight_v2::Weight>,
                pub pays_fee: runtime_types::frame_support::dispatch::Pays,
            }
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
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
                #[derive(
                    :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
                )]
                pub struct WrapperOpaque<_0>(#[codec(compact)] pub ::core::primitive::u32, pub _0);
            }
            pub mod preimages {
                use super::runtime_types;
                #[derive(
                    :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
                )]
                pub enum Bounded<_0> {
                    #[codec(index = 0)]
                    Legacy {
                        hash: ::subxt::utils::H256,
                    },
                    #[codec(index = 1)]
                    Inline(
                        runtime_types::sp_core::bounded::bounded_vec::BoundedVec<
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
                #[derive(
                    :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
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
                pub mod misc {
                    use super::runtime_types;
                    #[derive(
                        :: subxt :: ext :: codec :: Decode,
                        :: subxt :: ext :: codec :: Encode,
                        Debug,
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
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        pub struct PalletId(pub [::core::primitive::u8; 8usize]);
    }
    pub mod frame_system {
        use super::runtime_types;
        pub mod extensions {
            use super::runtime_types;
            pub mod check_genesis {
                use super::runtime_types;
                #[derive(
                    :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
                )]
                pub struct CheckGenesis;
            }
            pub mod check_mortality {
                use super::runtime_types;
                #[derive(
                    :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
                )]
                pub struct CheckMortality(pub runtime_types::sp_runtime::generic::era::Era);
            }
            pub mod check_non_zero_sender {
                use super::runtime_types;
                #[derive(
                    :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
                )]
                pub struct CheckNonZeroSender;
            }
            pub mod check_nonce {
                use super::runtime_types;
                #[derive(
                    :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
                )]
                pub struct CheckNonce(#[codec(compact)] pub ::core::primitive::u32);
            }
            pub mod check_spec_version {
                use super::runtime_types;
                #[derive(
                    :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
                )]
                pub struct CheckSpecVersion;
            }
            pub mod check_tx_version {
                use super::runtime_types;
                #[derive(
                    :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
                )]
                pub struct CheckTxVersion;
            }
            pub mod check_weight {
                use super::runtime_types;
                #[derive(
                    :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
                )]
                pub struct CheckWeight;
            }
        }
        pub mod limits {
            use super::runtime_types;
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
            pub struct BlockLength {
                pub max: runtime_types::frame_support::dispatch::PerDispatchClass<
                    ::core::primitive::u32,
                >,
            }
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
            pub struct BlockWeights {
                pub base_block: runtime_types::sp_weights::weight_v2::Weight,
                pub max_block: runtime_types::sp_weights::weight_v2::Weight,
                pub per_class: runtime_types::frame_support::dispatch::PerDispatchClass<
                    runtime_types::frame_system::limits::WeightsPerClass,
                >,
            }
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
            pub struct WeightsPerClass {
                pub base_extrinsic: runtime_types::sp_weights::weight_v2::Weight,
                pub max_extrinsic:
                    ::core::option::Option<runtime_types::sp_weights::weight_v2::Weight>,
                pub max_total: ::core::option::Option<runtime_types::sp_weights::weight_v2::Weight>,
                pub reserved: ::core::option::Option<runtime_types::sp_weights::weight_v2::Weight>,
            }
        }
        pub mod pallet {
            use super::runtime_types;
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]

            pub enum Call {
                #[codec(index = 0)]
                remark {
                    remark: ::std::vec::Vec<::core::primitive::u8>,
                },
                #[codec(index = 1)]
                set_heap_pages { pages: ::core::primitive::u64 },
                #[codec(index = 2)]
                set_code {
                    code: ::std::vec::Vec<::core::primitive::u8>,
                },
                #[codec(index = 3)]
                set_code_without_checks {
                    code: ::std::vec::Vec<::core::primitive::u8>,
                },
                #[codec(index = 4)]
                set_storage {
                    items: ::std::vec::Vec<(
                        ::std::vec::Vec<::core::primitive::u8>,
                        ::std::vec::Vec<::core::primitive::u8>,
                    )>,
                },
                #[codec(index = 5)]
                kill_storage {
                    keys: ::std::vec::Vec<::std::vec::Vec<::core::primitive::u8>>,
                },
                #[codec(index = 6)]
                kill_prefix {
                    prefix: ::std::vec::Vec<::core::primitive::u8>,
                    subkeys: ::core::primitive::u32,
                },
                #[codec(index = 7)]
                remark_with_event {
                    remark: ::std::vec::Vec<::core::primitive::u8>,
                },
            }
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]

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
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]

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
                NewAccount { account: ::sp_runtime::AccountId32 },
                #[codec(index = 4)]
                KilledAccount { account: ::sp_runtime::AccountId32 },
                #[codec(index = 5)]
                Remarked {
                    sender: ::sp_runtime::AccountId32,
                    hash: ::subxt::utils::H256,
                },
            }
        }
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        pub struct AccountInfo<_0, _1> {
            pub nonce: _0,
            pub consumers: _0,
            pub providers: _0,
            pub sufficients: _0,
            pub data: _1,
        }
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        pub struct EventRecord<_0, _1> {
            pub phase: runtime_types::frame_system::Phase,
            pub event: _0,
            pub topics: ::std::vec::Vec<_1>,
        }
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        pub struct LastRuntimeUpgradeInfo {
            #[codec(compact)]
            pub spec_version: ::core::primitive::u32,
            pub spec_name: ::std::string::String,
        }
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
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
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
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
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
            pub enum DispatchStatus {
                #[codec(index = 0)]
                Success,
                #[codec(index = 1)]
                Failed,
                #[codec(index = 2)]
                NotExecuted,
            }
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
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
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
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
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
            pub enum MessageWaitedSystemReason {
                #[codec(index = 0)]
                ProgramIsNotInitialized,
            }
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
            pub enum MessageWokenRuntimeReason {
                #[codec(index = 0)]
                WakeCalled,
            }
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
            pub enum MessageWokenSystemReason {
                #[codec(index = 0)]
                ProgramGotInitialized,
                #[codec(index = 1)]
                TimeoutHasCome,
                #[codec(index = 2)]
                OutOfRent,
            }
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
            pub enum ProgramChangeKind<_0> {
                #[codec(index = 0)]
                Active { expiration: _0 },
                #[codec(index = 1)]
                Inactive,
                #[codec(index = 2)]
                Paused {
                    code_hash: ::subxt::utils::H256,
                    memory_hash: ::subxt::utils::H256,
                    waitlist_hash: ::subxt::utils::H256,
                },
            }
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
            pub enum Reason<_0, _1> {
                #[codec(index = 0)]
                Runtime(_0),
                #[codec(index = 1)]
                System(_1),
            }
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
            pub enum UserMessageReadRuntimeReason {
                #[codec(index = 0)]
                MessageReplied,
                #[codec(index = 1)]
                MessageClaimed,
            }
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
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
                    :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
                )]
                pub struct ChildrenRefs {
                    pub spec_refs: ::core::primitive::u32,
                    pub unspec_refs: ::core::primitive::u32,
                }
                #[derive(
                    :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
                )]
                pub enum GasNode<_0, _1, _2> {
                    #[codec(index = 0)]
                    External {
                        id: _0,
                        value: _2,
                        lock: _2,
                        system_reserve: _2,
                        refs: runtime_types::gear_common::gas_provider::node::ChildrenRefs,
                        consumed: ::core::primitive::bool,
                    },
                    #[codec(index = 1)]
                    Cut { id: _0, value: _2, lock: _2 },
                    #[codec(index = 2)]
                    Reserved {
                        id: _0,
                        value: _2,
                        lock: _2,
                        refs: runtime_types::gear_common::gas_provider::node::ChildrenRefs,
                        consumed: ::core::primitive::bool,
                    },
                    #[codec(index = 3)]
                    SpecifiedLocal {
                        parent: _1,
                        value: _2,
                        lock: _2,
                        system_reserve: _2,
                        refs: runtime_types::gear_common::gas_provider::node::ChildrenRefs,
                        consumed: ::core::primitive::bool,
                    },
                    #[codec(index = 4)]
                    UnspecifiedLocal {
                        parent: _1,
                        lock: _2,
                        system_reserve: _2,
                    },
                }
                #[derive(
                    :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
                )]
                pub enum GasNodeId<_0, _1> {
                    #[codec(index = 0)]
                    Node(_0),
                    #[codec(index = 1)]
                    Reservation(_1),
                }
            }
        }
        pub mod scheduler {
            use super::runtime_types;
            pub mod task {
                use super::runtime_types;
                #[derive(
                    :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
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
                        :: subxt :: ext :: codec :: Decode,
                        :: subxt :: ext :: codec :: Encode,
                        Debug,
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
                    :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
                )]
                pub struct Interval<_0> {
                    pub start: _0,
                    pub finish: _0,
                }
            }
        }
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        pub struct ActiveProgram {
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
        }
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        pub struct CodeMetadata {
            pub author: ::subxt::utils::H256,
            #[codec(compact)]
            pub block_number: ::core::primitive::u32,
        }
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        pub enum Program {
            #[codec(index = 0)]
            Active(runtime_types::gear_common::ActiveProgram),
            #[codec(index = 1)]
            Exited(runtime_types::gear_core::ids::ProgramId),
            #[codec(index = 2)]
            Terminated(runtime_types::gear_core::ids::ProgramId),
        }
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
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
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
            pub struct LimitedVec<_0, _1>(
                pub ::std::vec::Vec<_0>,
                #[codec(skip)] pub ::core::marker::PhantomData<_1>,
            );
        }
        pub mod code {
            use super::runtime_types;
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
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
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug, Copy,
            )]
            pub struct CodeId(pub [::core::primitive::u8; 32usize]);
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug, Copy,
            )]
            pub struct MessageId(pub [::core::primitive::u8; 32usize]);
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug, Copy,
            )]
            pub struct ProgramId(pub [::core::primitive::u8; 32usize]);
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug, Copy,
            )]
            pub struct ReservationId(pub [::core::primitive::u8; 32usize]);
        }
        pub mod memory {
            use super::runtime_types;
            #[derive(
                :: subxt :: ext :: codec :: CompactAs,
                :: subxt :: ext :: codec :: Decode,
                :: subxt :: ext :: codec :: Encode,
                Debug,
            )]
            pub struct GearPage(pub ::core::primitive::u32);
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
            pub struct PageBuf(
                pub runtime_types::gear_core::buffer::LimitedVec<::core::primitive::u8, ()>,
            );
            #[derive(
                :: subxt :: ext :: codec :: CompactAs,
                :: subxt :: ext :: codec :: Decode,
                :: subxt :: ext :: codec :: Encode,
                Debug,
            )]
            pub struct WasmPage(pub ::core::primitive::u32);
        }
        pub mod message {
            use super::runtime_types;
            pub mod common {
                use super::runtime_types;
                #[derive(
                    :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
                )]
                pub enum MessageDetails {
                    #[codec(index = 0)]
                    Reply(runtime_types::gear_core::message::common::ReplyDetails),
                    #[codec(index = 1)]
                    Signal(runtime_types::gear_core::message::common::SignalDetails),
                }
                #[derive(
                    :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
                )]
                pub struct ReplyDetails {
                    pub reply_to: runtime_types::gear_core::ids::MessageId,
                    pub status_code: ::core::primitive::i32,
                }
                #[derive(
                    :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
                )]
                pub struct SignalDetails {
                    pub from: runtime_types::gear_core::ids::MessageId,
                    pub status_code: ::core::primitive::i32,
                }
            }
            pub mod context {
                use super::runtime_types;
                #[derive(
                    :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
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
                    pub reservation_nonce: ::core::primitive::u64,
                    pub system_reservation: ::core::option::Option<::core::primitive::u64>,
                }
            }
            pub mod stored {
                use super::runtime_types;
                #[derive(
                    :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
                )]
                pub struct StoredDispatch {
                    pub kind: runtime_types::gear_core::message::DispatchKind,
                    pub message: runtime_types::gear_core::message::stored::StoredMessage,
                    pub context: ::core::option::Option<
                        runtime_types::gear_core::message::context::ContextStore,
                    >,
                }
                #[derive(
                    :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
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
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
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
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
            pub struct PayloadSizeError;
        }
        pub mod reservation {
            use super::runtime_types;
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
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
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]

            pub enum Call {
                #[codec(index = 0)]
                transfer {
                    source: ::sp_runtime::AccountId32,
                    dest: ::sp_runtime::AccountId32,
                    amount: ::core::primitive::u128,
                },
            }
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]

            pub enum Event {
                #[codec(index = 0)]
                TokensDeposited {
                    account: ::sp_runtime::AccountId32,
                    amount: ::core::primitive::u128,
                },
            }
        }
    }
    pub mod pallet_babe {
        use super::runtime_types;
        pub mod pallet {
            use super::runtime_types;
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]

            pub enum Call {
                #[codec(index = 0)]
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
                plan_config_change {
                    config: runtime_types::sp_consensus_babe::digests::NextConfigDescriptor,
                },
            }
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]

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
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
            pub struct Bag {
                pub head: ::core::option::Option<::sp_runtime::AccountId32>,
                pub tail: ::core::option::Option<::sp_runtime::AccountId32>,
            }
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
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
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
            pub struct Node {
                pub id: ::sp_runtime::AccountId32,
                pub prev: ::core::option::Option<::sp_runtime::AccountId32>,
                pub next: ::core::option::Option<::sp_runtime::AccountId32>,
                pub bag_upper: ::core::primitive::u64,
                pub score: ::core::primitive::u64,
            }
        }
        pub mod pallet {
            use super::runtime_types;
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]

            pub enum Call {
                #[codec(index = 0)]
                rebag {
                    dislocated: ::sp_runtime::MultiAddress<::sp_runtime::AccountId32, ()>,
                },
                #[codec(index = 1)]
                put_in_front_of {
                    lighter: ::sp_runtime::MultiAddress<::sp_runtime::AccountId32, ()>,
                },
            }
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]

            pub enum Error {
                #[codec(index = 0)]
                List(runtime_types::pallet_bags_list::list::ListError),
            }
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]

            pub enum Event {
                #[codec(index = 0)]
                Rebagged {
                    who: ::sp_runtime::AccountId32,
                    from: ::core::primitive::u64,
                    to: ::core::primitive::u64,
                },
                #[codec(index = 1)]
                ScoreUpdated {
                    who: ::sp_runtime::AccountId32,
                    new_score: ::core::primitive::u64,
                },
            }
        }
    }
    pub mod pallet_balances {
        use super::runtime_types;
        pub mod pallet {
            use super::runtime_types;
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]

            pub enum Call {
                #[codec(index = 0)]
                transfer {
                    dest: ::sp_runtime::MultiAddress<::sp_runtime::AccountId32, ()>,
                    #[codec(compact)]
                    value: ::core::primitive::u128,
                },
                #[codec(index = 1)]
                set_balance {
                    who: ::sp_runtime::MultiAddress<::sp_runtime::AccountId32, ()>,
                    #[codec(compact)]
                    new_free: ::core::primitive::u128,
                    #[codec(compact)]
                    new_reserved: ::core::primitive::u128,
                },
                #[codec(index = 2)]
                force_transfer {
                    source: ::sp_runtime::MultiAddress<::sp_runtime::AccountId32, ()>,
                    dest: ::sp_runtime::MultiAddress<::sp_runtime::AccountId32, ()>,
                    #[codec(compact)]
                    value: ::core::primitive::u128,
                },
                #[codec(index = 3)]
                transfer_keep_alive {
                    dest: ::sp_runtime::MultiAddress<::sp_runtime::AccountId32, ()>,
                    #[codec(compact)]
                    value: ::core::primitive::u128,
                },
                #[codec(index = 4)]
                transfer_all {
                    dest: ::sp_runtime::MultiAddress<::sp_runtime::AccountId32, ()>,
                    keep_alive: ::core::primitive::bool,
                },
                #[codec(index = 5)]
                force_unreserve {
                    who: ::sp_runtime::MultiAddress<::sp_runtime::AccountId32, ()>,
                    amount: ::core::primitive::u128,
                },
            }
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]

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
                KeepAlive,
                #[codec(index = 5)]
                ExistingVestingSchedule,
                #[codec(index = 6)]
                DeadAccount,
                #[codec(index = 7)]
                TooManyReserves,
            }
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]

            pub enum Event {
                #[codec(index = 0)]
                Endowed {
                    account: ::sp_runtime::AccountId32,
                    free_balance: ::core::primitive::u128,
                },
                #[codec(index = 1)]
                DustLost {
                    account: ::sp_runtime::AccountId32,
                    amount: ::core::primitive::u128,
                },
                #[codec(index = 2)]
                Transfer {
                    from: ::sp_runtime::AccountId32,
                    to: ::sp_runtime::AccountId32,
                    amount: ::core::primitive::u128,
                },
                #[codec(index = 3)]
                BalanceSet {
                    who: ::sp_runtime::AccountId32,
                    free: ::core::primitive::u128,
                    reserved: ::core::primitive::u128,
                },
                #[codec(index = 4)]
                Reserved {
                    who: ::sp_runtime::AccountId32,
                    amount: ::core::primitive::u128,
                },
                #[codec(index = 5)]
                Unreserved {
                    who: ::sp_runtime::AccountId32,
                    amount: ::core::primitive::u128,
                },
                #[codec(index = 6)]
                ReserveRepatriated {
                    from: ::sp_runtime::AccountId32,
                    to: ::sp_runtime::AccountId32,
                    amount: ::core::primitive::u128,
                    destination_status:
                        runtime_types::frame_support::traits::tokens::misc::BalanceStatus,
                },
                #[codec(index = 7)]
                Deposit {
                    who: ::sp_runtime::AccountId32,
                    amount: ::core::primitive::u128,
                },
                #[codec(index = 8)]
                Withdraw {
                    who: ::sp_runtime::AccountId32,
                    amount: ::core::primitive::u128,
                },
                #[codec(index = 9)]
                Slashed {
                    who: ::sp_runtime::AccountId32,
                    amount: ::core::primitive::u128,
                },
            }
        }
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        pub struct AccountData<_0> {
            pub free: _0,
            pub reserved: _0,
            pub misc_frozen: _0,
            pub fee_frozen: _0,
        }
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        pub struct BalanceLock<_0> {
            pub id: [::core::primitive::u8; 8usize],
            pub amount: _0,
            pub reasons: runtime_types::pallet_balances::Reasons,
        }
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        pub enum Reasons {
            #[codec(index = 0)]
            Fee,
            #[codec(index = 1)]
            Misc,
            #[codec(index = 2)]
            All,
        }
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        pub struct ReserveData<_0, _1> {
            pub id: _0,
            pub amount: _1,
        }
    }
    pub mod pallet_conviction_voting {
        use super::runtime_types;
        pub mod conviction {
            use super::runtime_types;
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
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
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]

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
                    to: ::sp_runtime::MultiAddress<::sp_runtime::AccountId32, ()>,
                    conviction: runtime_types::pallet_conviction_voting::conviction::Conviction,
                    balance: ::core::primitive::u128,
                },
                #[codec(index = 2)]
                undelegate { class: ::core::primitive::u16 },
                #[codec(index = 3)]
                unlock {
                    class: ::core::primitive::u16,
                    target: ::sp_runtime::MultiAddress<::sp_runtime::AccountId32, ()>,
                },
                #[codec(index = 4)]
                remove_vote {
                    class: ::core::option::Option<::core::primitive::u16>,
                    index: ::core::primitive::u32,
                },
                #[codec(index = 5)]
                remove_other_vote {
                    target: ::sp_runtime::MultiAddress<::sp_runtime::AccountId32, ()>,
                    class: ::core::primitive::u16,
                    index: ::core::primitive::u32,
                },
            }
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]

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
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]

            pub enum Event {
                #[codec(index = 0)]
                Delegated(::sp_runtime::AccountId32, ::sp_runtime::AccountId32),
                #[codec(index = 1)]
                Undelegated(::sp_runtime::AccountId32),
            }
        }
        pub mod types {
            use super::runtime_types;
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
            pub struct Delegations<_0> {
                pub votes: _0,
                pub capital: _0,
            }
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
            pub struct Tally<_0> {
                pub ayes: _0,
                pub nays: _0,
                pub support: _0,
            }
        }
        pub mod vote {
            use super::runtime_types;
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
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
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
            pub struct Casting<_0, _1, _2> {
                pub votes: runtime_types::sp_core::bounded::bounded_vec::BoundedVec<(
                    _1,
                    runtime_types::pallet_conviction_voting::vote::AccountVote<_0>,
                )>,
                pub delegations: runtime_types::pallet_conviction_voting::types::Delegations<_0>,
                pub prior: runtime_types::pallet_conviction_voting::vote::PriorLock<_1, _0>,
                #[codec(skip)]
                pub __subxt_unused_type_params: ::core::marker::PhantomData<_2>,
            }
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
            pub struct Delegating<_0, _1, _2> {
                pub balance: _0,
                pub target: _1,
                pub conviction: runtime_types::pallet_conviction_voting::conviction::Conviction,
                pub delegations: runtime_types::pallet_conviction_voting::types::Delegations<_0>,
                pub prior: runtime_types::pallet_conviction_voting::vote::PriorLock<_2, _0>,
            }
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
            pub struct PriorLock<_0, _1>(pub _0, pub _1);
            #[derive(
                :: subxt :: ext :: codec :: CompactAs,
                :: subxt :: ext :: codec :: Decode,
                :: subxt :: ext :: codec :: Encode,
                Debug,
            )]
            pub struct Vote(pub ::core::primitive::u8);
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
            pub enum Voting<_0, _1, _2, _3> {
                #[codec(index = 0)]
                Casting(runtime_types::pallet_conviction_voting::vote::Casting<_0, _2, _2>),
                #[codec(index = 1)]
                Delegating(runtime_types::pallet_conviction_voting::vote::Delegating<_0, _1, _2>),
                __Ignore(::core::marker::PhantomData<_3>),
            }
        }
    }
    pub mod pallet_gear {
        use super::runtime_types;
        pub mod pallet {
            use super::runtime_types;
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]

            pub enum Call {
                #[codec(index = 0)]
                upload_code {
                    code: ::std::vec::Vec<::core::primitive::u8>,
                },
                #[codec(index = 1)]
                upload_program {
                    code: ::std::vec::Vec<::core::primitive::u8>,
                    salt: ::std::vec::Vec<::core::primitive::u8>,
                    init_payload: ::std::vec::Vec<::core::primitive::u8>,
                    gas_limit: ::core::primitive::u64,
                    value: ::core::primitive::u128,
                },
                #[codec(index = 2)]
                create_program {
                    code_id: runtime_types::gear_core::ids::CodeId,
                    salt: ::std::vec::Vec<::core::primitive::u8>,
                    init_payload: ::std::vec::Vec<::core::primitive::u8>,
                    gas_limit: ::core::primitive::u64,
                    value: ::core::primitive::u128,
                },
                #[codec(index = 3)]
                send_message {
                    destination: runtime_types::gear_core::ids::ProgramId,
                    payload: ::std::vec::Vec<::core::primitive::u8>,
                    gas_limit: ::core::primitive::u64,
                    value: ::core::primitive::u128,
                },
                #[codec(index = 4)]
                send_reply {
                    reply_to_id: runtime_types::gear_core::ids::MessageId,
                    payload: ::std::vec::Vec<::core::primitive::u8>,
                    gas_limit: ::core::primitive::u64,
                    value: ::core::primitive::u128,
                },
                #[codec(index = 5)]
                claim_value {
                    message_id: runtime_types::gear_core::ids::MessageId,
                },
                #[codec(index = 6)]
                run,
                #[codec(index = 7)]
                set_execute_inherent { value: ::core::primitive::bool },
            }
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]

            pub enum Error {
                #[codec(index = 0)]
                MessageNotFound,
                #[codec(index = 1)]
                InsufficientBalanceForReserve,
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
                ValueLessThanMinimal,
                #[codec(index = 11)]
                MessagesStorageCorrupted,
                #[codec(index = 12)]
                MessageQueueProcessingDisabled,
            }
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]

            pub enum Event {
                #[codec(index = 0)]
                MessageQueued {
                    id: runtime_types::gear_core::ids::MessageId,
                    source: ::sp_runtime::AccountId32,
                    destination: runtime_types::gear_core::ids::ProgramId,
                    entry: runtime_types::gear_common::event::MessageEntry,
                },
                #[codec(index = 1)]
                UserMessageSent {
                    message: runtime_types::gear_core::message::stored::StoredMessage,
                    expiration: ::core::option::Option<::core::primitive::u32>,
                },
                #[codec(index = 2)]
                UserMessageRead {
                    id: runtime_types::gear_core::ids::MessageId,
                    reason: runtime_types::gear_common::event::Reason<
                        runtime_types::gear_common::event::UserMessageReadRuntimeReason,
                        runtime_types::gear_common::event::UserMessageReadSystemReason,
                    >,
                },
                #[codec(index = 3)]
                MessagesDispatched {
                    total: ::core::primitive::u32,
                    statuses: ::subxt::utils::KeyedVec<
                        runtime_types::gear_core::ids::MessageId,
                        runtime_types::gear_common::event::DispatchStatus,
                    >,
                    state_changes: ::std::vec::Vec<runtime_types::gear_core::ids::ProgramId>,
                },
                #[codec(index = 4)]
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
                MessageWoken {
                    id: runtime_types::gear_core::ids::MessageId,
                    reason: runtime_types::gear_common::event::Reason<
                        runtime_types::gear_common::event::MessageWokenRuntimeReason,
                        runtime_types::gear_common::event::MessageWokenSystemReason,
                    >,
                },
                #[codec(index = 6)]
                CodeChanged {
                    id: runtime_types::gear_core::ids::CodeId,
                    change:
                        runtime_types::gear_common::event::CodeChangeKind<::core::primitive::u32>,
                },
                #[codec(index = 7)]
                ProgramChanged {
                    id: runtime_types::gear_core::ids::ProgramId,
                    change: runtime_types::gear_common::event::ProgramChangeKind<
                        ::core::primitive::u32,
                    >,
                },
                #[codec(index = 8)]
                QueueProcessingReverted,
            }
        }
        pub mod schedule {
            use super::runtime_types;
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
            pub struct HostFnWeights {
                pub alloc: ::core::primitive::u64,
                pub free: ::core::primitive::u64,
                pub gr_reserve_gas: ::core::primitive::u64,
                pub gr_unreserve_gas: ::core::primitive::u64,
                pub gr_system_reserve_gas: ::core::primitive::u64,
                pub gr_gas_available: ::core::primitive::u64,
                pub gr_message_id: ::core::primitive::u64,
                pub gr_origin: ::core::primitive::u64,
                pub gr_program_id: ::core::primitive::u64,
                pub gr_source: ::core::primitive::u64,
                pub gr_value: ::core::primitive::u64,
                pub gr_value_available: ::core::primitive::u64,
                pub gr_size: ::core::primitive::u64,
                pub gr_read: ::core::primitive::u64,
                pub gr_read_per_byte: ::core::primitive::u64,
                pub gr_block_height: ::core::primitive::u64,
                pub gr_block_timestamp: ::core::primitive::u64,
                pub gr_random: ::core::primitive::u64,
                pub gr_send_init: ::core::primitive::u64,
                pub gr_send_push: ::core::primitive::u64,
                pub gr_send_push_per_byte: ::core::primitive::u64,
                pub gr_send_commit: ::core::primitive::u64,
                pub gr_send_commit_per_byte: ::core::primitive::u64,
                pub gr_reservation_send_commit: ::core::primitive::u64,
                pub gr_reservation_send_commit_per_byte: ::core::primitive::u64,
                pub gr_reply_commit: ::core::primitive::u64,
                pub gr_reservation_reply_commit: ::core::primitive::u64,
                pub gr_reply_push: ::core::primitive::u64,
                pub gr_reply_push_per_byte: ::core::primitive::u64,
                pub gr_reply_to: ::core::primitive::u64,
                pub gr_signal_from: ::core::primitive::u64,
                pub gr_reply_push_input: ::core::primitive::u64,
                pub gr_reply_push_input_per_byte: ::core::primitive::u64,
                pub gr_send_push_input: ::core::primitive::u64,
                pub gr_send_push_input_per_byte: ::core::primitive::u64,
                pub gr_debug: ::core::primitive::u64,
                pub gr_debug_per_byte: ::core::primitive::u64,
                pub gr_error: ::core::primitive::u64,
                pub gr_status_code: ::core::primitive::u64,
                pub gr_exit: ::core::primitive::u64,
                pub gr_leave: ::core::primitive::u64,
                pub gr_wait: ::core::primitive::u64,
                pub gr_wait_for: ::core::primitive::u64,
                pub gr_wait_up_to: ::core::primitive::u64,
                pub gr_wake: ::core::primitive::u64,
                pub gr_create_program_wgas: ::core::primitive::u64,
                pub gr_create_program_wgas_payload_per_byte: ::core::primitive::u64,
                pub gr_create_program_wgas_salt_per_byte: ::core::primitive::u64,
            }
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
            pub struct InstructionWeights {
                pub version: ::core::primitive::u32,
                pub i64const: ::core::primitive::u32,
                pub i64load: ::core::primitive::u32,
                pub i64store: ::core::primitive::u32,
                pub select: ::core::primitive::u32,
                pub r#if: ::core::primitive::u32,
                pub br: ::core::primitive::u32,
                pub br_if: ::core::primitive::u32,
                pub br_table: ::core::primitive::u32,
                pub br_table_per_entry: ::core::primitive::u32,
                pub call: ::core::primitive::u32,
                pub call_indirect: ::core::primitive::u32,
                pub call_indirect_per_param: ::core::primitive::u32,
                pub local_get: ::core::primitive::u32,
                pub local_set: ::core::primitive::u32,
                pub local_tee: ::core::primitive::u32,
                pub global_get: ::core::primitive::u32,
                pub global_set: ::core::primitive::u32,
                pub memory_current: ::core::primitive::u32,
                pub i64clz: ::core::primitive::u32,
                pub i64ctz: ::core::primitive::u32,
                pub i64popcnt: ::core::primitive::u32,
                pub i64eqz: ::core::primitive::u32,
                pub i64extendsi32: ::core::primitive::u32,
                pub i64extendui32: ::core::primitive::u32,
                pub i32wrapi64: ::core::primitive::u32,
                pub i64eq: ::core::primitive::u32,
                pub i64ne: ::core::primitive::u32,
                pub i64lts: ::core::primitive::u32,
                pub i64ltu: ::core::primitive::u32,
                pub i64gts: ::core::primitive::u32,
                pub i64gtu: ::core::primitive::u32,
                pub i64les: ::core::primitive::u32,
                pub i64leu: ::core::primitive::u32,
                pub i64ges: ::core::primitive::u32,
                pub i64geu: ::core::primitive::u32,
                pub i64add: ::core::primitive::u32,
                pub i64sub: ::core::primitive::u32,
                pub i64mul: ::core::primitive::u32,
                pub i64divs: ::core::primitive::u32,
                pub i64divu: ::core::primitive::u32,
                pub i64rems: ::core::primitive::u32,
                pub i64remu: ::core::primitive::u32,
                pub i64and: ::core::primitive::u32,
                pub i64or: ::core::primitive::u32,
                pub i64xor: ::core::primitive::u32,
                pub i64shl: ::core::primitive::u32,
                pub i64shrs: ::core::primitive::u32,
                pub i64shru: ::core::primitive::u32,
                pub i64rotl: ::core::primitive::u32,
                pub i64rotr: ::core::primitive::u32,
            }
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
            pub struct Limits {
                pub stack_height: ::core::option::Option<::core::primitive::u32>,
                pub globals: ::core::primitive::u32,
                pub parameters: ::core::primitive::u32,
                pub memory_pages: ::core::primitive::u16,
                pub table_size: ::core::primitive::u32,
                pub br_table_size: ::core::primitive::u32,
                pub subject_len: ::core::primitive::u32,
                pub call_depth: ::core::primitive::u32,
                pub payload_len: ::core::primitive::u32,
                pub code_len: ::core::primitive::u32,
            }
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
            pub struct MemoryWeights {
                pub lazy_pages_read: ::core::primitive::u64,
                pub lazy_pages_write: ::core::primitive::u64,
                pub lazy_pages_write_after_read: ::core::primitive::u64,
                pub initial_cost: ::core::primitive::u64,
                pub allocation_cost: ::core::primitive::u64,
                pub grow_cost: ::core::primitive::u64,
                pub load_cost: ::core::primitive::u64,
            }
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
            pub struct Schedule {
                pub limits: runtime_types::pallet_gear::schedule::Limits,
                pub instruction_weights: runtime_types::pallet_gear::schedule::InstructionWeights,
                pub host_fn_weights: runtime_types::pallet_gear::schedule::HostFnWeights,
                pub memory_weights: runtime_types::pallet_gear::schedule::MemoryWeights,
                pub module_instantiation_per_byte: ::core::primitive::u64,
                pub db_write_per_byte: ::core::primitive::u64,
                pub db_read_per_byte: ::core::primitive::u64,
                pub code_instrumentation_cost: ::core::primitive::u64,
                pub code_instrumentation_byte_cost: ::core::primitive::u64,
            }
        }
    }
    pub mod pallet_gear_gas {
        use super::runtime_types;
        pub mod pallet {
            use super::runtime_types;
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]

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
            }
        }
    }
    pub mod pallet_gear_messenger {
        use super::runtime_types;
        pub mod pallet {
            use super::runtime_types;
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]

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
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        pub struct CustomChargeTransactionPayment<_0>(
            pub runtime_types::pallet_transaction_payment::ChargeTransactionPayment,
            #[codec(skip)] pub ::core::marker::PhantomData<_0>,
        );
    }
    pub mod pallet_gear_program {
        use super::runtime_types;
        pub mod pallet {
            use super::runtime_types;
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]

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
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]

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
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
            pub struct StakingBlackList;
        }
        pub mod pallet {
            use super::runtime_types;
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]

            pub enum Call {
                #[codec(index = 0)]
                refill { value: ::core::primitive::u128 },
                #[codec(index = 1)]
                force_refill {
                    from: ::sp_runtime::MultiAddress<::sp_runtime::AccountId32, ()>,
                    value: ::core::primitive::u128,
                },
                #[codec(index = 2)]
                withdraw {
                    to: ::sp_runtime::MultiAddress<::sp_runtime::AccountId32, ()>,
                    value: ::core::primitive::u128,
                },
            }
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]

            pub enum Error {
                #[codec(index = 0)]
                FailureToRefillPool,
                #[codec(index = 1)]
                FailureToWithdrawFromPool,
            }
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]

            pub enum Event {
                #[codec(index = 0)]
                Refilled { amount: ::core::primitive::u128 },
                #[codec(index = 1)]
                Withdrawn { amount: ::core::primitive::u128 },
                #[codec(index = 2)]
                Burned { amount: ::core::primitive::u128 },
            }
        }
    }
    pub mod pallet_grandpa {
        use super::runtime_types;
        pub mod pallet {
            use super::runtime_types;
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]

            pub enum Call {
                #[codec(index = 0)]
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
                note_stalled {
                    delay: ::core::primitive::u32,
                    best_finalized_block_number: ::core::primitive::u32,
                },
            }
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]

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
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]

            pub enum Event {
                #[codec(index = 0)]
                NewAuthorities {
                    authority_set: ::std::vec::Vec<(
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
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        pub struct StoredPendingChange<_0> {
            pub scheduled_at: _0,
            pub delay: _0,
            pub next_authorities:
                runtime_types::sp_core::bounded::weak_bounded_vec::WeakBoundedVec<(
                    runtime_types::sp_consensus_grandpa::app::Public,
                    ::core::primitive::u64,
                )>,
            pub forced: ::core::option::Option<_0>,
        }
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
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
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]

            pub enum Call {
                #[codec(index = 0)]
                add_registrar {
                    account: ::sp_runtime::MultiAddress<::sp_runtime::AccountId32, ()>,
                },
                #[codec(index = 1)]
                set_identity {
                    info: ::std::boxed::Box<runtime_types::pallet_identity::types::IdentityInfo>,
                },
                #[codec(index = 2)]
                set_subs {
                    subs: ::std::vec::Vec<(
                        ::sp_runtime::AccountId32,
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
                    new: ::sp_runtime::MultiAddress<::sp_runtime::AccountId32, ()>,
                },
                #[codec(index = 8)]
                set_fields {
                    #[codec(compact)]
                    index: ::core::primitive::u32,
                    fields: runtime_types::pallet_identity::types::BitFlags<
                        runtime_types::pallet_identity::types::IdentityField,
                    >,
                },
                #[codec(index = 9)]
                provide_judgement {
                    #[codec(compact)]
                    reg_index: ::core::primitive::u32,
                    target: ::sp_runtime::MultiAddress<::sp_runtime::AccountId32, ()>,
                    judgement:
                        runtime_types::pallet_identity::types::Judgement<::core::primitive::u128>,
                    identity: ::subxt::utils::H256,
                },
                #[codec(index = 10)]
                kill_identity {
                    target: ::sp_runtime::MultiAddress<::sp_runtime::AccountId32, ()>,
                },
                #[codec(index = 11)]
                add_sub {
                    sub: ::sp_runtime::MultiAddress<::sp_runtime::AccountId32, ()>,
                    data: runtime_types::pallet_identity::types::Data,
                },
                #[codec(index = 12)]
                rename_sub {
                    sub: ::sp_runtime::MultiAddress<::sp_runtime::AccountId32, ()>,
                    data: runtime_types::pallet_identity::types::Data,
                },
                #[codec(index = 13)]
                remove_sub {
                    sub: ::sp_runtime::MultiAddress<::sp_runtime::AccountId32, ()>,
                },
                #[codec(index = 14)]
                quit_sub,
            }
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]

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
                TooManyFields,
                #[codec(index = 12)]
                TooManyRegistrars,
                #[codec(index = 13)]
                AlreadyClaimed,
                #[codec(index = 14)]
                NotSub,
                #[codec(index = 15)]
                NotOwned,
                #[codec(index = 16)]
                JudgementForDifferentIdentity,
                #[codec(index = 17)]
                JudgementPaymentFailed,
            }
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]

            pub enum Event {
                #[codec(index = 0)]
                IdentitySet { who: ::sp_runtime::AccountId32 },
                #[codec(index = 1)]
                IdentityCleared {
                    who: ::sp_runtime::AccountId32,
                    deposit: ::core::primitive::u128,
                },
                #[codec(index = 2)]
                IdentityKilled {
                    who: ::sp_runtime::AccountId32,
                    deposit: ::core::primitive::u128,
                },
                #[codec(index = 3)]
                JudgementRequested {
                    who: ::sp_runtime::AccountId32,
                    registrar_index: ::core::primitive::u32,
                },
                #[codec(index = 4)]
                JudgementUnrequested {
                    who: ::sp_runtime::AccountId32,
                    registrar_index: ::core::primitive::u32,
                },
                #[codec(index = 5)]
                JudgementGiven {
                    target: ::sp_runtime::AccountId32,
                    registrar_index: ::core::primitive::u32,
                },
                #[codec(index = 6)]
                RegistrarAdded {
                    registrar_index: ::core::primitive::u32,
                },
                #[codec(index = 7)]
                SubIdentityAdded {
                    sub: ::sp_runtime::AccountId32,
                    main: ::sp_runtime::AccountId32,
                    deposit: ::core::primitive::u128,
                },
                #[codec(index = 8)]
                SubIdentityRemoved {
                    sub: ::sp_runtime::AccountId32,
                    main: ::sp_runtime::AccountId32,
                    deposit: ::core::primitive::u128,
                },
                #[codec(index = 9)]
                SubIdentityRevoked {
                    sub: ::sp_runtime::AccountId32,
                    main: ::sp_runtime::AccountId32,
                    deposit: ::core::primitive::u128,
                },
            }
        }
        pub mod types {
            use super::runtime_types;
            #[derive(
                :: subxt :: ext :: codec :: CompactAs,
                :: subxt :: ext :: codec :: Decode,
                :: subxt :: ext :: codec :: Encode,
                Debug,
            )]
            pub struct BitFlags<_0>(
                pub ::core::primitive::u64,
                #[codec(skip)] pub ::core::marker::PhantomData<_0>,
            );
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
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
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
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
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
            pub struct IdentityInfo {
                pub additional: runtime_types::sp_core::bounded::bounded_vec::BoundedVec<(
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
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
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
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
            pub struct RegistrarInfo<_0, _1> {
                pub account: _1,
                pub fee: _0,
                pub fields: runtime_types::pallet_identity::types::BitFlags<
                    runtime_types::pallet_identity::types::IdentityField,
                >,
            }
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
            pub struct Registration<_0> {
                pub judgements: runtime_types::sp_core::bounded::bounded_vec::BoundedVec<(
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
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]

            pub enum Call {
                #[codec(index = 0)]
                heartbeat {
                    heartbeat: runtime_types::pallet_im_online::Heartbeat<::core::primitive::u32>,
                    signature: runtime_types::pallet_im_online::sr25519::app_sr25519::Signature,
                },
            }
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]

            pub enum Error {
                #[codec(index = 0)]
                InvalidKey,
                #[codec(index = 1)]
                DuplicatedHeartbeat,
            }
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]

            pub enum Event {
                #[codec(index = 0)]
                HeartbeatReceived {
                    authority_id: runtime_types::pallet_im_online::sr25519::app_sr25519::Public,
                },
                #[codec(index = 1)]
                AllGood,
                #[codec(index = 2)]
                SomeOffline {
                    offline: ::std::vec::Vec<(
                        ::sp_runtime::AccountId32,
                        runtime_types::pallet_staking::Exposure<
                            ::sp_runtime::AccountId32,
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
                    :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
                )]
                pub struct Public(pub runtime_types::sp_core::sr25519::Public);
                #[derive(
                    :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
                )]
                pub struct Signature(pub runtime_types::sp_core::sr25519::Signature);
            }
        }
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        pub struct BoundedOpaqueNetworkState {
            pub peer_id: runtime_types::sp_core::bounded::weak_bounded_vec::WeakBoundedVec<
                ::core::primitive::u8,
            >,
            pub external_addresses:
                runtime_types::sp_core::bounded::weak_bounded_vec::WeakBoundedVec<
                    runtime_types::sp_core::bounded::weak_bounded_vec::WeakBoundedVec<
                        ::core::primitive::u8,
                    >,
                >,
        }
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        pub struct Heartbeat<_0> {
            pub block_number: _0,
            pub network_state: runtime_types::sp_core::offchain::OpaqueNetworkState,
            pub session_index: _0,
            pub authority_index: _0,
            pub validators_len: _0,
        }
    }
    pub mod pallet_preimage {
        use super::runtime_types;
        pub mod pallet {
            use super::runtime_types;
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]

            pub enum Call {
                #[codec(index = 0)]
                note_preimage {
                    bytes: ::std::vec::Vec<::core::primitive::u8>,
                },
                #[codec(index = 1)]
                unnote_preimage { hash: ::subxt::utils::H256 },
                #[codec(index = 2)]
                request_preimage { hash: ::subxt::utils::H256 },
                #[codec(index = 3)]
                unrequest_preimage { hash: ::subxt::utils::H256 },
            }
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]

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
            }
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]

            pub enum Event {
                #[codec(index = 0)]
                Noted { hash: ::subxt::utils::H256 },
                #[codec(index = 1)]
                Requested { hash: ::subxt::utils::H256 },
                #[codec(index = 2)]
                Cleared { hash: ::subxt::utils::H256 },
            }
        }
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
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
    pub mod pallet_ranked_collective {
        use super::runtime_types;
        pub mod pallet {
            use super::runtime_types;
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]

            pub enum Call {
                #[codec(index = 0)]
                add_member {
                    who: ::sp_runtime::MultiAddress<::sp_runtime::AccountId32, ()>,
                },
                #[codec(index = 1)]
                promote_member {
                    who: ::sp_runtime::MultiAddress<::sp_runtime::AccountId32, ()>,
                },
                #[codec(index = 2)]
                demote_member {
                    who: ::sp_runtime::MultiAddress<::sp_runtime::AccountId32, ()>,
                },
                #[codec(index = 3)]
                remove_member {
                    who: ::sp_runtime::MultiAddress<::sp_runtime::AccountId32, ()>,
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
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]

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
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]

            pub enum Event {
                #[codec(index = 0)]
                MemberAdded { who: ::sp_runtime::AccountId32 },
                #[codec(index = 1)]
                RankChanged {
                    who: ::sp_runtime::AccountId32,
                    rank: ::core::primitive::u16,
                },
                #[codec(index = 2)]
                MemberRemoved {
                    who: ::sp_runtime::AccountId32,
                    rank: ::core::primitive::u16,
                },
                #[codec(index = 3)]
                Voted {
                    who: ::sp_runtime::AccountId32,
                    poll: ::core::primitive::u32,
                    vote: runtime_types::pallet_ranked_collective::VoteRecord,
                    tally: runtime_types::pallet_ranked_collective::Tally,
                },
            }
        }
        #[derive(
            :: subxt :: ext :: codec :: CompactAs,
            :: subxt :: ext :: codec :: Decode,
            :: subxt :: ext :: codec :: Encode,
            Debug,
        )]
        pub struct MemberRecord {
            pub rank: ::core::primitive::u16,
        }
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        pub struct Tally {
            pub bare_ayes: ::core::primitive::u32,
            pub ayes: ::core::primitive::u32,
            pub nays: ::core::primitive::u32,
        }
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
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
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]

            pub enum Call {
                #[codec(index = 0)]
                submit {
                    proposal_origin: ::std::boxed::Box<runtime_types::gear_runtime::OriginCaller>,
                    proposal: runtime_types::frame_support::traits::preimages::Bounded<
                        runtime_types::gear_runtime::RuntimeCall,
                    >,
                    enactment_moment: runtime_types::frame_support::traits::schedule::DispatchTime<
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
            }
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]

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
            }
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]

            pub enum Event {
                #[codec(index = 0)]
                Submitted {
                    index: ::core::primitive::u32,
                    track: ::core::primitive::u16,
                    proposal: runtime_types::frame_support::traits::preimages::Bounded<
                        runtime_types::gear_runtime::RuntimeCall,
                    >,
                },
                #[codec(index = 1)]
                DecisionDepositPlaced {
                    index: ::core::primitive::u32,
                    who: ::sp_runtime::AccountId32,
                    amount: ::core::primitive::u128,
                },
                #[codec(index = 2)]
                DecisionDepositRefunded {
                    index: ::core::primitive::u32,
                    who: ::sp_runtime::AccountId32,
                    amount: ::core::primitive::u128,
                },
                #[codec(index = 3)]
                DepositSlashed {
                    who: ::sp_runtime::AccountId32,
                    amount: ::core::primitive::u128,
                },
                #[codec(index = 4)]
                DecisionStarted {
                    index: ::core::primitive::u32,
                    track: ::core::primitive::u16,
                    proposal: runtime_types::frame_support::traits::preimages::Bounded<
                        runtime_types::gear_runtime::RuntimeCall,
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
                    who: ::sp_runtime::AccountId32,
                    amount: ::core::primitive::u128,
                },
            }
        }
        pub mod types {
            use super::runtime_types;
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
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
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
            pub struct DecidingStatus<_0> {
                pub since: _0,
                pub confirming: ::core::option::Option<_0>,
            }
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
            pub struct Deposit<_0, _1> {
                pub who: _0,
                pub amount: _1,
            }
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
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
                    ::core::option::Option<runtime_types::pallet_referenda::types::Deposit<_6, _4>>,
                    ::core::option::Option<runtime_types::pallet_referenda::types::Deposit<_6, _4>>,
                ),
                #[codec(index = 2)]
                Rejected(
                    _2,
                    ::core::option::Option<runtime_types::pallet_referenda::types::Deposit<_6, _4>>,
                    ::core::option::Option<runtime_types::pallet_referenda::types::Deposit<_6, _4>>,
                ),
                #[codec(index = 3)]
                Cancelled(
                    _2,
                    ::core::option::Option<runtime_types::pallet_referenda::types::Deposit<_6, _4>>,
                    ::core::option::Option<runtime_types::pallet_referenda::types::Deposit<_6, _4>>,
                ),
                #[codec(index = 4)]
                TimedOut(
                    _2,
                    ::core::option::Option<runtime_types::pallet_referenda::types::Deposit<_6, _4>>,
                    ::core::option::Option<runtime_types::pallet_referenda::types::Deposit<_6, _4>>,
                ),
                #[codec(index = 5)]
                Killed(_2),
            }
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
            pub struct ReferendumStatus<_0, _1, _2, _3, _4, _5, _6, _7> {
                pub track: _0,
                pub origin: _1,
                pub proposal: _3,
                pub enactment: runtime_types::frame_support::traits::schedule::DispatchTime<_2>,
                pub submitted: _2,
                pub submission_deposit: runtime_types::pallet_referenda::types::Deposit<_6, _4>,
                pub decision_deposit:
                    ::core::option::Option<runtime_types::pallet_referenda::types::Deposit<_6, _4>>,
                pub deciding: ::core::option::Option<
                    runtime_types::pallet_referenda::types::DecidingStatus<_2>,
                >,
                pub tally: _5,
                pub in_queue: ::core::primitive::bool,
                pub alarm: ::core::option::Option<(_2, _7)>,
            }
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
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
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]

            pub enum Call {
                #[codec(index = 0)]
                schedule {
                    when: ::core::primitive::u32,
                    maybe_periodic:
                        ::core::option::Option<(::core::primitive::u32, ::core::primitive::u32)>,
                    priority: ::core::primitive::u8,
                    call: ::std::boxed::Box<runtime_types::gear_runtime::RuntimeCall>,
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
                    maybe_periodic:
                        ::core::option::Option<(::core::primitive::u32, ::core::primitive::u32)>,
                    priority: ::core::primitive::u8,
                    call: ::std::boxed::Box<runtime_types::gear_runtime::RuntimeCall>,
                },
                #[codec(index = 3)]
                cancel_named {
                    id: [::core::primitive::u8; 32usize],
                },
                #[codec(index = 4)]
                schedule_after {
                    after: ::core::primitive::u32,
                    maybe_periodic:
                        ::core::option::Option<(::core::primitive::u32, ::core::primitive::u32)>,
                    priority: ::core::primitive::u8,
                    call: ::std::boxed::Box<runtime_types::gear_runtime::RuntimeCall>,
                },
                #[codec(index = 5)]
                schedule_named_after {
                    id: [::core::primitive::u8; 32usize],
                    after: ::core::primitive::u32,
                    maybe_periodic:
                        ::core::option::Option<(::core::primitive::u32, ::core::primitive::u32)>,
                    priority: ::core::primitive::u8,
                    call: ::std::boxed::Box<runtime_types::gear_runtime::RuntimeCall>,
                },
            }
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]

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
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]

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
                    result: ::core::result::Result<(), runtime_types::sp_runtime::DispatchError>,
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
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
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
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]

            pub enum Call {
                #[codec(index = 0)]
                set_keys {
                    keys: runtime_types::gear_runtime::SessionKeys,
                    proof: ::std::vec::Vec<::core::primitive::u8>,
                },
                #[codec(index = 1)]
                purge_keys,
            }
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]

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
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]

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
                    :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
                )]

                pub enum Call {
                    #[codec(index = 0)]
                    bond {
                        controller: ::sp_runtime::MultiAddress<::sp_runtime::AccountId32, ()>,
                        #[codec(compact)]
                        value: ::core::primitive::u128,
                        payee: runtime_types::pallet_staking::RewardDestination<
                            ::sp_runtime::AccountId32,
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
                        targets: ::std::vec::Vec<
                            ::sp_runtime::MultiAddress<::sp_runtime::AccountId32, ()>,
                        >,
                    },
                    #[codec(index = 6)]
                    chill,
                    #[codec(index = 7)]
                    set_payee {
                        payee: runtime_types::pallet_staking::RewardDestination<
                            ::sp_runtime::AccountId32,
                        >,
                    },
                    #[codec(index = 8)]
                    set_controller {
                        controller: ::sp_runtime::MultiAddress<::sp_runtime::AccountId32, ()>,
                    },
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
                        invulnerables: ::std::vec::Vec<::sp_runtime::AccountId32>,
                    },
                    #[codec(index = 15)]
                    force_unstake {
                        stash: ::sp_runtime::AccountId32,
                        num_slashing_spans: ::core::primitive::u32,
                    },
                    #[codec(index = 16)]
                    force_new_era_always,
                    #[codec(index = 17)]
                    cancel_deferred_slash {
                        era: ::core::primitive::u32,
                        slash_indices: ::std::vec::Vec<::core::primitive::u32>,
                    },
                    #[codec(index = 18)]
                    payout_stakers {
                        validator_stash: ::sp_runtime::AccountId32,
                        era: ::core::primitive::u32,
                    },
                    #[codec(index = 19)]
                    rebond {
                        #[codec(compact)]
                        value: ::core::primitive::u128,
                    },
                    #[codec(index = 20)]
                    reap_stash {
                        stash: ::sp_runtime::AccountId32,
                        num_slashing_spans: ::core::primitive::u32,
                    },
                    #[codec(index = 21)]
                    kick {
                        who: ::std::vec::Vec<
                            ::sp_runtime::MultiAddress<::sp_runtime::AccountId32, ()>,
                        >,
                    },
                    #[codec(index = 22)]
                    set_staking_configs {
                        min_nominator_bond: runtime_types::pallet_staking::pallet::pallet::ConfigOp<
                            ::core::primitive::u128,
                        >,
                        min_validator_bond: runtime_types::pallet_staking::pallet::pallet::ConfigOp<
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
                        chill_threshold: runtime_types::pallet_staking::pallet::pallet::ConfigOp<
                            runtime_types::sp_arithmetic::per_things::Percent,
                        >,
                        min_commission: runtime_types::pallet_staking::pallet::pallet::ConfigOp<
                            runtime_types::sp_arithmetic::per_things::Perbill,
                        >,
                    },
                    #[codec(index = 23)]
                    chill_other {
                        controller: ::sp_runtime::AccountId32,
                    },
                    #[codec(index = 24)]
                    force_apply_min_commission {
                        validator_stash: ::sp_runtime::AccountId32,
                    },
                    #[codec(index = 25)]
                    set_min_commission {
                        new: runtime_types::sp_arithmetic::per_things::Perbill,
                    },
                }
                #[derive(
                    :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
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
                    :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
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
                    IncorrectHistoryDepth,
                    #[codec(index = 16)]
                    IncorrectSlashingSpans,
                    #[codec(index = 17)]
                    BadState,
                    #[codec(index = 18)]
                    TooManyTargets,
                    #[codec(index = 19)]
                    BadTarget,
                    #[codec(index = 20)]
                    CannotChillOther,
                    #[codec(index = 21)]
                    TooManyNominators,
                    #[codec(index = 22)]
                    TooManyValidators,
                    #[codec(index = 23)]
                    CommissionTooLow,
                    #[codec(index = 24)]
                    BoundNotMet,
                }
                #[derive(
                    :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
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
                        stash: ::sp_runtime::AccountId32,
                        amount: ::core::primitive::u128,
                    },
                    #[codec(index = 2)]
                    Slashed {
                        staker: ::sp_runtime::AccountId32,
                        amount: ::core::primitive::u128,
                    },
                    #[codec(index = 3)]
                    SlashReported {
                        validator: ::sp_runtime::AccountId32,
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
                        stash: ::sp_runtime::AccountId32,
                        amount: ::core::primitive::u128,
                    },
                    #[codec(index = 7)]
                    Unbonded {
                        stash: ::sp_runtime::AccountId32,
                        amount: ::core::primitive::u128,
                    },
                    #[codec(index = 8)]
                    Withdrawn {
                        stash: ::sp_runtime::AccountId32,
                        amount: ::core::primitive::u128,
                    },
                    #[codec(index = 9)]
                    Kicked {
                        nominator: ::sp_runtime::AccountId32,
                        stash: ::sp_runtime::AccountId32,
                    },
                    #[codec(index = 10)]
                    StakingElectionFailed,
                    #[codec(index = 11)]
                    Chilled { stash: ::sp_runtime::AccountId32 },
                    #[codec(index = 12)]
                    PayoutStarted {
                        era_index: ::core::primitive::u32,
                        validator_stash: ::sp_runtime::AccountId32,
                    },
                    #[codec(index = 13)]
                    ValidatorPrefsSet {
                        stash: ::sp_runtime::AccountId32,
                        prefs: runtime_types::pallet_staking::ValidatorPrefs,
                    },
                    #[codec(index = 14)]
                    ForceEra {
                        mode: runtime_types::pallet_staking::Forcing,
                    },
                }
            }
        }
        pub mod slashing {
            use super::runtime_types;
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
            pub struct SlashingSpans {
                pub span_index: ::core::primitive::u32,
                pub last_start: ::core::primitive::u32,
                pub last_nonzero_slash: ::core::primitive::u32,
                pub prior: ::std::vec::Vec<::core::primitive::u32>,
            }
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
            pub struct SpanRecord<_0> {
                pub slashed: _0,
                pub paid_out: _0,
            }
        }
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        pub struct ActiveEraInfo {
            pub index: ::core::primitive::u32,
            pub start: ::core::option::Option<::core::primitive::u64>,
        }
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        pub struct EraRewardPoints<_0> {
            pub total: ::core::primitive::u32,
            pub individual: ::subxt::utils::KeyedVec<_0, ::core::primitive::u32>,
        }
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        pub struct Exposure<_0, _1> {
            #[codec(compact)]
            pub total: _1,
            #[codec(compact)]
            pub own: _1,
            pub others: ::std::vec::Vec<runtime_types::pallet_staking::IndividualExposure<_0, _1>>,
        }
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
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
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        pub struct IndividualExposure<_0, _1> {
            pub who: _0,
            #[codec(compact)]
            pub value: _1,
        }
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        pub struct Nominations {
            pub targets:
                runtime_types::sp_core::bounded::bounded_vec::BoundedVec<::sp_runtime::AccountId32>,
            pub submitted_in: ::core::primitive::u32,
            pub suppressed: ::core::primitive::bool,
        }
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
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
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        pub struct StakingLedger {
            pub stash: ::sp_runtime::AccountId32,
            #[codec(compact)]
            pub total: ::core::primitive::u128,
            #[codec(compact)]
            pub active: ::core::primitive::u128,
            pub unlocking: runtime_types::sp_core::bounded::bounded_vec::BoundedVec<
                runtime_types::pallet_staking::UnlockChunk<::core::primitive::u128>,
            >,
            pub claimed_rewards:
                runtime_types::sp_core::bounded::bounded_vec::BoundedVec<::core::primitive::u32>,
        }
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        pub struct UnappliedSlash<_0, _1> {
            pub validator: _0,
            pub own: _1,
            pub others: ::std::vec::Vec<(_0, _1)>,
            pub reporters: ::std::vec::Vec<_0>,
            pub payout: _1,
        }
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        pub struct UnlockChunk<_0> {
            #[codec(compact)]
            pub value: _0,
            #[codec(compact)]
            pub era: ::core::primitive::u32,
        }
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
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
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]

            pub enum Call {
                #[codec(index = 0)]
                sudo {
                    call: ::std::boxed::Box<runtime_types::gear_runtime::RuntimeCall>,
                },
                #[codec(index = 1)]
                sudo_unchecked_weight {
                    call: ::std::boxed::Box<runtime_types::gear_runtime::RuntimeCall>,
                    weight: runtime_types::sp_weights::weight_v2::Weight,
                },
                #[codec(index = 2)]
                set_key {
                    new: ::sp_runtime::MultiAddress<::sp_runtime::AccountId32, ()>,
                },
                #[codec(index = 3)]
                sudo_as {
                    who: ::sp_runtime::MultiAddress<::sp_runtime::AccountId32, ()>,
                    call: ::std::boxed::Box<runtime_types::gear_runtime::RuntimeCall>,
                },
            }
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]

            pub enum Error {
                #[codec(index = 0)]
                RequireSudo,
            }
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]

            pub enum Event {
                #[codec(index = 0)]
                Sudid {
                    sudo_result:
                        ::core::result::Result<(), runtime_types::sp_runtime::DispatchError>,
                },
                #[codec(index = 1)]
                KeyChanged {
                    old_sudoer: ::core::option::Option<::sp_runtime::AccountId32>,
                },
                #[codec(index = 2)]
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
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]

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
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]

            pub enum Event {
                #[codec(index = 0)]
                TransactionFeePaid {
                    who: ::sp_runtime::AccountId32,
                    actual_fee: ::core::primitive::u128,
                    tip: ::core::primitive::u128,
                },
            }
        }
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        pub struct ChargeTransactionPayment(#[codec(compact)] pub ::core::primitive::u128);
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
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
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]

            pub enum Call {
                #[codec(index = 0)]
                propose_spend {
                    #[codec(compact)]
                    value: ::core::primitive::u128,
                    beneficiary: ::sp_runtime::MultiAddress<::sp_runtime::AccountId32, ()>,
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
                spend {
                    #[codec(compact)]
                    amount: ::core::primitive::u128,
                    beneficiary: ::sp_runtime::MultiAddress<::sp_runtime::AccountId32, ()>,
                },
                #[codec(index = 4)]
                remove_approval {
                    #[codec(compact)]
                    proposal_id: ::core::primitive::u32,
                },
            }
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]

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
            }
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]

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
                    account: ::sp_runtime::AccountId32,
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
                    beneficiary: ::sp_runtime::AccountId32,
                },
                #[codec(index = 8)]
                UpdatedInactive {
                    reactivated: ::core::primitive::u128,
                    deactivated: ::core::primitive::u128,
                },
            }
        }
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
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
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]

            pub enum Call {
                #[codec(index = 0)]
                batch {
                    calls: ::std::vec::Vec<runtime_types::gear_runtime::RuntimeCall>,
                },
                #[codec(index = 1)]
                as_derivative {
                    index: ::core::primitive::u16,
                    call: ::std::boxed::Box<runtime_types::gear_runtime::RuntimeCall>,
                },
                #[codec(index = 2)]
                batch_all {
                    calls: ::std::vec::Vec<runtime_types::gear_runtime::RuntimeCall>,
                },
                #[codec(index = 3)]
                dispatch_as {
                    as_origin: ::std::boxed::Box<runtime_types::gear_runtime::OriginCaller>,
                    call: ::std::boxed::Box<runtime_types::gear_runtime::RuntimeCall>,
                },
                #[codec(index = 4)]
                force_batch {
                    calls: ::std::vec::Vec<runtime_types::gear_runtime::RuntimeCall>,
                },
                #[codec(index = 5)]
                with_weight {
                    call: ::std::boxed::Box<runtime_types::gear_runtime::RuntimeCall>,
                    weight: runtime_types::sp_weights::weight_v2::Weight,
                },
            }
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]

            pub enum Error {
                #[codec(index = 0)]
                TooManyCalls,
            }
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]

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
                    result: ::core::result::Result<(), runtime_types::sp_runtime::DispatchError>,
                },
            }
        }
    }
    pub mod pallet_vesting {
        use super::runtime_types;
        pub mod pallet {
            use super::runtime_types;
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]

            pub enum Call {
                #[codec(index = 0)]
                vest,
                #[codec(index = 1)]
                vest_other {
                    target: ::sp_runtime::MultiAddress<::sp_runtime::AccountId32, ()>,
                },
                #[codec(index = 2)]
                vested_transfer {
                    target: ::sp_runtime::MultiAddress<::sp_runtime::AccountId32, ()>,
                    schedule: runtime_types::pallet_vesting::vesting_info::VestingInfo<
                        ::core::primitive::u128,
                        ::core::primitive::u32,
                    >,
                },
                #[codec(index = 3)]
                force_vested_transfer {
                    source: ::sp_runtime::MultiAddress<::sp_runtime::AccountId32, ()>,
                    target: ::sp_runtime::MultiAddress<::sp_runtime::AccountId32, ()>,
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
            }
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]

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
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]

            pub enum Event {
                #[codec(index = 0)]
                VestingUpdated {
                    account: ::sp_runtime::AccountId32,
                    unvested: ::core::primitive::u128,
                },
                #[codec(index = 1)]
                VestingCompleted { account: ::sp_runtime::AccountId32 },
            }
        }
        pub mod vesting_info {
            use super::runtime_types;
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
            pub struct VestingInfo<_0, _1> {
                pub locked: _0,
                pub per_block: _0,
                pub starting_block: _1,
            }
        }
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
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
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]

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
                    call: ::std::boxed::Box<runtime_types::gear_runtime::RuntimeCall>,
                },
            }
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]

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
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]

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
    pub mod pallet_gear_debug {
        use super::runtime_types;
        pub mod pallet {
            use super::runtime_types;
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
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
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
            pub struct DebugData {
                pub dispatch_queue:
                    ::std::vec::Vec<runtime_types::gear_core::message::stored::StoredDispatch>,
                pub programs:
                    ::std::vec::Vec<runtime_types::pallet_gear_debug::pallet::ProgramDetails>,
            }
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
            #[doc = "\n\t\t\tCustom [dispatch errors](https://docs.substrate.io/main-docs/build/events-errors/)\n\t\t\tof this pallet.\n\t\t\t"]
            pub enum Error {}
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
            #[doc = "\n\t\t\tThe [event](https://docs.substrate.io/main-docs/build/events-errors/) emitted\n\t\t\tby this pallet.\n\t\t\t"]
            pub enum Event {
                #[codec(index = 0)]
                DebugMode(::core::primitive::bool),
                #[codec(index = 1)]
                #[doc = "A snapshot of the debug data: programs and message queue ('debug mode' only)"]
                DebugDataSnapshot(runtime_types::pallet_gear_debug::pallet::DebugData),
            }
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
            pub struct ProgramDetails {
                pub id: runtime_types::gear_core::ids::ProgramId,
                pub state: runtime_types::pallet_gear_debug::pallet::ProgramState,
            }
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
            pub struct ProgramInfo {
                pub static_pages: runtime_types::gear_core::memory::WasmPage,
                pub persistent_pages: ::subxt::utils::KeyedVec<
                    runtime_types::gear_core::memory::GearPage,
                    runtime_types::gear_core::memory::PageBuf,
                >,
                pub code_hash: ::subxt::utils::H256,
            }
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
            pub enum ProgramState {
                #[codec(index = 0)]
                Active(runtime_types::pallet_gear_debug::pallet::ProgramInfo),
                #[codec(index = 1)]
                Terminated,
            }
        }
    }
    pub mod sp_arithmetic {
        use super::runtime_types;
        pub mod fixed_point {
            use super::runtime_types;
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
            pub struct FixedI64(pub ::core::primitive::i64);
            #[derive(
                :: subxt :: ext :: codec :: CompactAs,
                :: subxt :: ext :: codec :: Decode,
                :: subxt :: ext :: codec :: Encode,
                Debug,
            )]
            pub struct FixedU128(pub ::core::primitive::u128);
        }
        pub mod per_things {
            use super::runtime_types;
            #[derive(
                :: subxt :: ext :: codec :: CompactAs,
                :: subxt :: ext :: codec :: Decode,
                :: subxt :: ext :: codec :: Encode,
                Debug,
            )]
            pub struct Perbill(pub ::core::primitive::u32);
            #[derive(
                :: subxt :: ext :: codec :: CompactAs,
                :: subxt :: ext :: codec :: Decode,
                :: subxt :: ext :: codec :: Encode,
                Debug,
            )]
            pub struct Percent(pub ::core::primitive::u8);
            #[derive(
                :: subxt :: ext :: codec :: CompactAs,
                :: subxt :: ext :: codec :: Decode,
                :: subxt :: ext :: codec :: Encode,
                Debug,
            )]
            pub struct Permill(pub ::core::primitive::u32);
            #[derive(
                :: subxt :: ext :: codec :: CompactAs,
                :: subxt :: ext :: codec :: Decode,
                :: subxt :: ext :: codec :: Encode,
                Debug,
            )]
            pub struct Perquintill(pub ::core::primitive::u64);
        }
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
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
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
            pub struct Public(pub runtime_types::sp_core::sr25519::Public);
        }
    }
    pub mod sp_consensus_babe {
        use super::runtime_types;
        pub mod app {
            use super::runtime_types;
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
            pub struct Public(pub runtime_types::sp_core::sr25519::Public);
        }
        pub mod digests {
            use super::runtime_types;
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
            pub enum NextConfigDescriptor {
                #[codec(index = 1)]
                V1 {
                    c: (::core::primitive::u64, ::core::primitive::u64),
                    allowed_slots: runtime_types::sp_consensus_babe::AllowedSlots,
                },
            }
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
            pub enum PreDigest {
                #[codec(index = 1)]
                Primary(runtime_types::sp_consensus_babe::digests::PrimaryPreDigest),
                #[codec(index = 2)]
                SecondaryPlain(runtime_types::sp_consensus_babe::digests::SecondaryPlainPreDigest),
                #[codec(index = 3)]
                SecondaryVRF(runtime_types::sp_consensus_babe::digests::SecondaryVRFPreDigest),
            }
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
            pub struct PrimaryPreDigest {
                pub authority_index: ::core::primitive::u32,
                pub slot: runtime_types::sp_consensus_slots::Slot,
                pub vrf_output: [::core::primitive::u8; 32usize],
                pub vrf_proof: [::core::primitive::u8; 64usize],
            }
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
            pub struct SecondaryPlainPreDigest {
                pub authority_index: ::core::primitive::u32,
                pub slot: runtime_types::sp_consensus_slots::Slot,
            }
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
            pub struct SecondaryVRFPreDigest {
                pub authority_index: ::core::primitive::u32,
                pub slot: runtime_types::sp_consensus_slots::Slot,
                pub vrf_output: [::core::primitive::u8; 32usize],
                pub vrf_proof: [::core::primitive::u8; 64usize],
            }
        }
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        pub enum AllowedSlots {
            #[codec(index = 0)]
            PrimarySlots,
            #[codec(index = 1)]
            PrimaryAndSecondaryPlainSlots,
            #[codec(index = 2)]
            PrimaryAndSecondaryVRFSlots,
        }
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        pub struct BabeEpochConfiguration {
            pub c: (::core::primitive::u64, ::core::primitive::u64),
            pub allowed_slots: runtime_types::sp_consensus_babe::AllowedSlots,
        }
    }
    pub mod sp_consensus_slots {
        use super::runtime_types;
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        pub struct EquivocationProof<_0, _1> {
            pub offender: _1,
            pub slot: runtime_types::sp_consensus_slots::Slot,
            pub first_header: _0,
            pub second_header: _0,
        }
        #[derive(
            :: subxt :: ext :: codec :: CompactAs,
            :: subxt :: ext :: codec :: Decode,
            :: subxt :: ext :: codec :: Encode,
            Debug,
        )]
        pub struct Slot(pub ::core::primitive::u64);
    }
    pub mod sp_core {
        use super::runtime_types;
        pub mod bounded {
            use super::runtime_types;
            pub mod bounded_vec {
                use super::runtime_types;
                #[derive(
                    :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
                )]
                pub struct BoundedVec<_0>(pub ::std::vec::Vec<_0>);
            }
            pub mod weak_bounded_vec {
                use super::runtime_types;
                #[derive(
                    :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
                )]
                pub struct WeakBoundedVec<_0>(pub ::std::vec::Vec<_0>);
            }
        }
        pub mod crypto {
            use super::runtime_types;
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
            pub struct KeyTypeId(pub [::core::primitive::u8; 4usize]);
        }
        pub mod ecdsa {
            use super::runtime_types;
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
            pub struct Signature(pub [::core::primitive::u8; 65usize]);
        }
        pub mod ed25519 {
            use super::runtime_types;
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
            pub struct Public(pub [::core::primitive::u8; 32usize]);
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
            pub struct Signature(pub [::core::primitive::u8; 64usize]);
        }
        pub mod offchain {
            use super::runtime_types;
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
            pub struct OpaqueMultiaddr(pub ::std::vec::Vec<::core::primitive::u8>);
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
            pub struct OpaqueNetworkState {
                pub peer_id: runtime_types::sp_core::OpaquePeerId,
                pub external_addresses:
                    ::std::vec::Vec<runtime_types::sp_core::offchain::OpaqueMultiaddr>,
            }
        }
        pub mod sr25519 {
            use super::runtime_types;
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
            pub struct Public(pub [::core::primitive::u8; 32usize]);
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
            pub struct Signature(pub [::core::primitive::u8; 64usize]);
        }
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        pub struct OpaquePeerId(pub ::std::vec::Vec<::core::primitive::u8>);
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        pub enum Void {}
    }
    pub mod sp_consensus_grandpa {
        use super::runtime_types;
        pub mod app {
            use super::runtime_types;
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
            pub struct Public(pub runtime_types::sp_core::ed25519::Public);
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
            pub struct Signature(pub runtime_types::sp_core::ed25519::Signature);
        }
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        pub enum Equivocation<_0, _1> {
            #[codec(index = 0)]
            Prevote(
                runtime_types::consensus_grandpa::Equivocation<
                    runtime_types::sp_consensus_grandpa::app::Public,
                    runtime_types::consensus_grandpa::Prevote<_0, _1>,
                    runtime_types::sp_consensus_grandpa::app::Signature,
                >,
            ),
            #[codec(index = 1)]
            Precommit(
                runtime_types::consensus_grandpa::Equivocation<
                    runtime_types::sp_consensus_grandpa::app::Public,
                    runtime_types::consensus_grandpa::Precommit<_0, _1>,
                    runtime_types::sp_consensus_grandpa::app::Signature,
                >,
            ),
        }
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        pub struct EquivocationProof<_0, _1> {
            pub set_id: ::core::primitive::u64,
            pub equivocation: runtime_types::sp_consensus_grandpa::Equivocation<_0, _1>,
        }
    }
    pub mod sp_runtime {
        use super::runtime_types;
        pub mod generic {
            use super::runtime_types;
            pub mod digest {
                use super::runtime_types;
                #[derive(
                    :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
                )]
                pub struct Digest {
                    pub logs:
                        ::std::vec::Vec<runtime_types::sp_runtime::generic::digest::DigestItem>,
                }
                #[derive(
                    :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
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
                    :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
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
                    :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
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
                    :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
                )]
                pub struct UncheckedExtrinsic<_0, _1, _2, _3>(
                    pub ::std::vec::Vec<::core::primitive::u8>,
                    #[codec(skip)] pub ::core::marker::PhantomData<(_1, _0, _2, _3)>,
                );
            }
        }
        pub mod traits {
            use super::runtime_types;
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
            pub struct BlakeTwo256;
        }
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
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
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        pub struct DispatchErrorWithPostInfo<_0> {
            pub post_info: _0,
            pub error: runtime_types::sp_runtime::DispatchError,
        }
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        pub struct ModuleError {
            pub index: ::core::primitive::u8,
            pub error: [::core::primitive::u8; 4usize],
        }
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        pub enum MultiSignature {
            #[codec(index = 0)]
            Ed25519(runtime_types::sp_core::ed25519::Signature),
            #[codec(index = 1)]
            Sr25519(runtime_types::sp_core::sr25519::Signature),
            #[codec(index = 2)]
            Ecdsa(runtime_types::sp_core::ecdsa::Signature),
        }
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
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
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        pub enum TransactionalError {
            #[codec(index = 0)]
            LimitReached,
            #[codec(index = 1)]
            NoLayer,
        }
    }
    pub mod sp_session {
        use super::runtime_types;
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        pub struct MembershipProof {
            pub session: ::core::primitive::u32,
            pub trie_nodes: ::std::vec::Vec<::std::vec::Vec<::core::primitive::u8>>,
            pub validator_count: ::core::primitive::u32,
        }
    }
    pub mod sp_version {
        use super::runtime_types;
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        pub struct RuntimeVersion {
            pub spec_name: ::std::string::String,
            pub impl_name: ::std::string::String,
            pub authoring_version: ::core::primitive::u32,
            pub spec_version: ::core::primitive::u32,
            pub impl_version: ::core::primitive::u32,
            pub apis: ::std::vec::Vec<([::core::primitive::u8; 8usize], ::core::primitive::u32)>,
            pub transaction_version: ::core::primitive::u32,
            pub state_version: ::core::primitive::u8,
        }
    }
    pub mod sp_weights {
        use super::runtime_types;
        pub mod weight_v2 {
            use super::runtime_types;
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
            pub struct Weight {
                #[codec(compact)]
                pub ref_time: ::core::primitive::u64,
                #[codec(compact)]
                pub proof_size: ::core::primitive::u64,
            }
        }
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        pub struct RuntimeDbWeight {
            pub read: ::core::primitive::u64,
            pub write: ::core::primitive::u64,
        }
    }
    pub mod gear_runtime {
        use super::runtime_types;
        pub mod extensions {
            use super::runtime_types;
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
            pub struct DisableValueTransfers;
        }
        pub mod governance {
            use super::runtime_types;
            pub mod origins {
                use super::runtime_types;
                pub mod pallet_custom_origins {
                    use super::runtime_types;
                    #[derive(
                        :: subxt :: ext :: codec :: Decode,
                        :: subxt :: ext :: codec :: Encode,
                        Debug,
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
                        WhitelistedCaller,
                        #[codec(index = 7)]
                        FellowshipInitiates,
                        #[codec(index = 8)]
                        Fellows,
                        #[codec(index = 9)]
                        FellowshipExperts,
                        #[codec(index = 10)]
                        FellowshipMasters,
                    }
                }
            }
        }
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        pub enum OriginCaller {
            #[codec(index = 0)]
            system(runtime_types::frame_support::dispatch::RawOrigin<::sp_runtime::AccountId32>),
            #[codec(index = 19)]
            Origins(
                runtime_types::gear_runtime::governance::origins::pallet_custom_origins::Origin,
            ),
            #[codec(index = 2)]
            Void(runtime_types::sp_core::Void),
        }
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        pub struct Runtime;
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
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
            #[codec(index = 99)]
            Sudo(runtime_types::pallet_sudo::pallet::Call),
            #[codec(index = 22)]
            Scheduler(runtime_types::pallet_scheduler::pallet::Call),
            #[codec(index = 23)]
            Preimage(runtime_types::pallet_preimage::pallet::Call),
            #[codec(index = 24)]
            Identity(runtime_types::pallet_identity::pallet::Call),
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
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
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
            #[codec(index = 99)]
            Sudo(runtime_types::pallet_sudo::pallet::Event),
            #[codec(index = 22)]
            Scheduler(runtime_types::pallet_scheduler::pallet::Event),
            #[codec(index = 23)]
            Preimage(runtime_types::pallet_preimage::pallet::Event),
            #[codec(index = 24)]
            Identity(runtime_types::pallet_identity::pallet::Event),
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
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        pub struct SessionKeys {
            pub babe: runtime_types::sp_consensus_babe::app::Public,
            pub grandpa: runtime_types::sp_consensus_grandpa::app::Public,
            pub im_online: runtime_types::pallet_im_online::sr25519::app_sr25519::Public,
            pub authority_discovery: runtime_types::sp_authority_discovery::app::Public,
        }
    }
}

pub type DispatchError = runtime_types::sp_runtime::DispatchError;
