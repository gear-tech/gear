// This file is part of Gear.
//
// Copyright (C) 2021-2022 Gear Technologies Inc.
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

//! Static metadata.
#![allow(dead_code, unused_imports, non_camel_case_types)]
#![allow(clippy::all)]

mod impls;

#[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
pub enum Event {
    #[codec(index = 0)]
    System(system::Event),
    #[codec(index = 4)]
    Grandpa(grandpa::Event),
    #[codec(index = 5)]
    Balances(balances::Event),
    #[codec(index = 6)]
    TransactionPayment(transaction_payment::Event),
    #[codec(index = 7)]
    Session(session::Event),
    #[codec(index = 8)]
    Sudo(sudo::Event),
    #[codec(index = 9)]
    Utility(utility::Event),
    #[codec(index = 14)]
    Gear(gear::Event),
}

pub mod system {

    use super::runtime_types;

    #[doc = "Event for the System pallet."]
    pub type Event = runtime_types::frame_system::pallet::Event;
    pub mod events {
        use super::runtime_types;
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        #[doc = "An extrinsic completed successfully."]
        pub struct ExtrinsicSuccess {
            pub dispatch_info: runtime_types::frame_support::dispatch::DispatchInfo,
        }
        impl ::subxt::events::StaticEvent for ExtrinsicSuccess {
            const PALLET: &'static str = "System";
            const EVENT: &'static str = "ExtrinsicSuccess";
        }
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        #[doc = "An extrinsic failed."]
        pub struct ExtrinsicFailed {
            pub dispatch_error: runtime_types::sp_runtime::DispatchError,
            pub dispatch_info: runtime_types::frame_support::dispatch::DispatchInfo,
        }
        impl ::subxt::events::StaticEvent for ExtrinsicFailed {
            const PALLET: &'static str = "System";
            const EVENT: &'static str = "ExtrinsicFailed";
        }
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        #[doc = "`:code` was updated."]
        pub struct CodeUpdated;
        impl ::subxt::events::StaticEvent for CodeUpdated {
            const PALLET: &'static str = "System";
            const EVENT: &'static str = "CodeUpdated";
        }
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        #[doc = "A new account was created."]
        pub struct NewAccount {
            pub account: ::sp_core::crypto::AccountId32,
        }
        impl ::subxt::events::StaticEvent for NewAccount {
            const PALLET: &'static str = "System";
            const EVENT: &'static str = "NewAccount";
        }
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        #[doc = "An account was reaped."]
        pub struct KilledAccount {
            pub account: ::sp_core::crypto::AccountId32,
        }
        impl ::subxt::events::StaticEvent for KilledAccount {
            const PALLET: &'static str = "System";
            const EVENT: &'static str = "KilledAccount";
        }
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        #[doc = "On on-chain remark happened."]
        pub struct Remarked {
            pub sender: ::sp_core::crypto::AccountId32,
            pub hash: ::sp_core::H256,
        }
        impl ::subxt::events::StaticEvent for Remarked {
            const PALLET: &'static str = "System";
            const EVENT: &'static str = "Remarked";
        }
    }
}

pub mod grandpa {

    use super::runtime_types;
    #[doc = "
			The [event](https://docs.substrate.io/main-docs/build/events-errors/) emitted
			by this pallet.
			"]
    pub type Event = runtime_types::pallet_grandpa::pallet::Event;
    pub mod events {
        use super::runtime_types;
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        #[doc = "New authority set has been applied."]
        pub struct NewAuthorities {
            pub authority_set: ::std::vec::Vec<(
                runtime_types::sp_finality_grandpa::app::Public,
                ::core::primitive::u64,
            )>,
        }
        impl ::subxt::events::StaticEvent for NewAuthorities {
            const PALLET: &'static str = "Grandpa";
            const EVENT: &'static str = "NewAuthorities";
        }
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        #[doc = "Current authority set has been paused."]
        pub struct Paused;
        impl ::subxt::events::StaticEvent for Paused {
            const PALLET: &'static str = "Grandpa";
            const EVENT: &'static str = "Paused";
        }
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        #[doc = "Current authority set has been resumed."]
        pub struct Resumed;
        impl ::subxt::events::StaticEvent for Resumed {
            const PALLET: &'static str = "Grandpa";
            const EVENT: &'static str = "Resumed";
        }
    }
}

pub mod balances {

    use super::runtime_types;

    #[doc = "
			The [event](https://docs.substrate.io/main-docs/build/events-errors/) emitted
			by this pallet.
			"]
    pub type Event = runtime_types::pallet_balances::pallet::Event;
    pub mod events {
        use super::runtime_types;
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        #[doc = "An account was created with some free balance."]
        pub struct Endowed {
            pub account: ::sp_core::crypto::AccountId32,
            pub free_balance: ::core::primitive::u128,
        }
        impl ::subxt::events::StaticEvent for Endowed {
            const PALLET: &'static str = "Balances";
            const EVENT: &'static str = "Endowed";
        }
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        #[doc = "An account was removed whose balance was non-zero but below ExistentialDeposit,"]
        #[doc = "resulting in an outright loss."]
        pub struct DustLost {
            pub account: ::sp_core::crypto::AccountId32,
            pub amount: ::core::primitive::u128,
        }
        impl ::subxt::events::StaticEvent for DustLost {
            const PALLET: &'static str = "Balances";
            const EVENT: &'static str = "DustLost";
        }
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        #[doc = "Transfer succeeded."]
        pub struct Transfer {
            pub from: ::sp_core::crypto::AccountId32,
            pub to: ::sp_core::crypto::AccountId32,
            pub amount: ::core::primitive::u128,
        }
        impl ::subxt::events::StaticEvent for Transfer {
            const PALLET: &'static str = "Balances";
            const EVENT: &'static str = "Transfer";
        }
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        #[doc = "A balance was set by root."]
        pub struct BalanceSet {
            pub who: ::sp_core::crypto::AccountId32,
            pub free: ::core::primitive::u128,
            pub reserved: ::core::primitive::u128,
        }
        impl ::subxt::events::StaticEvent for BalanceSet {
            const PALLET: &'static str = "Balances";
            const EVENT: &'static str = "BalanceSet";
        }
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        #[doc = "Some balance was reserved (moved from free to reserved)."]
        pub struct Reserved {
            pub who: ::sp_core::crypto::AccountId32,
            pub amount: ::core::primitive::u128,
        }
        impl ::subxt::events::StaticEvent for Reserved {
            const PALLET: &'static str = "Balances";
            const EVENT: &'static str = "Reserved";
        }
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        #[doc = "Some balance was unreserved (moved from reserved to free)."]
        pub struct Unreserved {
            pub who: ::sp_core::crypto::AccountId32,
            pub amount: ::core::primitive::u128,
        }
        impl ::subxt::events::StaticEvent for Unreserved {
            const PALLET: &'static str = "Balances";
            const EVENT: &'static str = "Unreserved";
        }
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        #[doc = "Some balance was moved from the reserve of the first account to the second account."]
        #[doc = "Final argument indicates the destination balance type."]
        pub struct ReserveRepatriated {
            pub from: ::sp_core::crypto::AccountId32,
            pub to: ::sp_core::crypto::AccountId32,
            pub amount: ::core::primitive::u128,
            pub destination_status:
                runtime_types::frame_support::traits::tokens::misc::BalanceStatus,
        }
        impl ::subxt::events::StaticEvent for ReserveRepatriated {
            const PALLET: &'static str = "Balances";
            const EVENT: &'static str = "ReserveRepatriated";
        }
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        #[doc = "Some amount was deposited (e.g. for transaction fees)."]
        pub struct Deposit {
            pub who: ::sp_core::crypto::AccountId32,
            pub amount: ::core::primitive::u128,
        }
        impl ::subxt::events::StaticEvent for Deposit {
            const PALLET: &'static str = "Balances";
            const EVENT: &'static str = "Deposit";
        }
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        #[doc = "Some amount was withdrawn from the account (e.g. for transaction fees)."]
        pub struct Withdraw {
            pub who: ::sp_core::crypto::AccountId32,
            pub amount: ::core::primitive::u128,
        }
        impl ::subxt::events::StaticEvent for Withdraw {
            const PALLET: &'static str = "Balances";
            const EVENT: &'static str = "Withdraw";
        }
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        #[doc = "Some amount was removed from the account (e.g. for misbehavior)."]
        pub struct Slashed {
            pub who: ::sp_core::crypto::AccountId32,
            pub amount: ::core::primitive::u128,
        }
        impl ::subxt::events::StaticEvent for Slashed {
            const PALLET: &'static str = "Balances";
            const EVENT: &'static str = "Slashed";
        }
    }
}

pub mod transaction_payment {

    use super::runtime_types;
    #[doc = "
			The [event](https://docs.substrate.io/main-docs/build/events-errors/) emitted
			by this pallet.
			"]
    pub type Event = runtime_types::pallet_transaction_payment::pallet::Event;
    pub mod events {
        use super::runtime_types;
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        #[doc = "A transaction fee `actual_fee`, of which `tip` was added to the minimum inclusion fee,"]
        #[doc = "has been paid by `who`."]
        pub struct TransactionFeePaid {
            pub who: ::sp_core::crypto::AccountId32,
            pub actual_fee: ::core::primitive::u128,
            pub tip: ::core::primitive::u128,
        }
        impl ::subxt::events::StaticEvent for TransactionFeePaid {
            const PALLET: &'static str = "TransactionPayment";
            const EVENT: &'static str = "TransactionFeePaid";
        }
    }
}

pub mod session {

    use super::runtime_types;

    #[doc = "
			The [event](https://docs.substrate.io/main-docs/build/events-errors/) emitted
			by this pallet.
			"]
    pub type Event = runtime_types::pallet_session::pallet::Event;
    pub mod events {
        use super::runtime_types;
        #[derive(
            :: subxt :: ext :: codec :: CompactAs,
            :: subxt :: ext :: codec :: Decode,
            :: subxt :: ext :: codec :: Encode,
            Debug,
        )]
        #[doc = "New session has happened. Note that the argument is the session index, not the"]
        #[doc = "block number as the type might suggest."]
        pub struct NewSession {
            pub session_index: ::core::primitive::u32,
        }
        impl ::subxt::events::StaticEvent for NewSession {
            const PALLET: &'static str = "Session";
            const EVENT: &'static str = "NewSession";
        }
    }
}

pub mod sudo {

    use super::runtime_types;

    #[doc = "
			The [event](https://docs.substrate.io/main-docs/build/events-errors/) emitted
			by this pallet.
			"]
    pub type Event = runtime_types::pallet_sudo::pallet::Event;
    pub mod events {
        use super::runtime_types;
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        #[doc = "A sudo just took place. "]
        pub struct Sudid {
            pub sudo_result: ::core::result::Result<(), runtime_types::sp_runtime::DispatchError>,
        }
        impl ::subxt::events::StaticEvent for Sudid {
            const PALLET: &'static str = "Sudo";
            const EVENT: &'static str = "Sudid";
        }
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        #[doc = "The sudoer just switched identity; the old key is supplied if one existed."]
        pub struct KeyChanged {
            pub old_sudoer: ::core::option::Option<::sp_core::crypto::AccountId32>,
        }
        impl ::subxt::events::StaticEvent for KeyChanged {
            const PALLET: &'static str = "Sudo";
            const EVENT: &'static str = "KeyChanged";
        }
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        #[doc = "A sudo just took place. "]
        pub struct SudoAsDone {
            pub sudo_result: ::core::result::Result<(), runtime_types::sp_runtime::DispatchError>,
        }
        impl ::subxt::events::StaticEvent for SudoAsDone {
            const PALLET: &'static str = "Sudo";
            const EVENT: &'static str = "SudoAsDone";
        }
    }
}

pub mod utility {

    use super::runtime_types;

    #[doc = "
			The [event](https://docs.substrate.io/main-docs/build/events-errors/) emitted
			by this pallet.
			"]
    pub type Event = runtime_types::pallet_utility::pallet::Event;
    pub mod events {
        use super::runtime_types;
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        #[doc = "Batch of dispatches did not complete fully. Index of first failing dispatch given, as"]
        #[doc = "well as the error."]
        pub struct BatchInterrupted {
            pub index: ::core::primitive::u32,
            pub error: runtime_types::sp_runtime::DispatchError,
        }
        impl ::subxt::events::StaticEvent for BatchInterrupted {
            const PALLET: &'static str = "Utility";
            const EVENT: &'static str = "BatchInterrupted";
        }
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        #[doc = "Batch of dispatches completed fully with no error."]
        pub struct BatchCompleted;
        impl ::subxt::events::StaticEvent for BatchCompleted {
            const PALLET: &'static str = "Utility";
            const EVENT: &'static str = "BatchCompleted";
        }
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        #[doc = "Batch of dispatches completed but has errors."]
        pub struct BatchCompletedWithErrors;
        impl ::subxt::events::StaticEvent for BatchCompletedWithErrors {
            const PALLET: &'static str = "Utility";
            const EVENT: &'static str = "BatchCompletedWithErrors";
        }
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        #[doc = "A single item within a Batch of dispatches has completed with no error."]
        pub struct ItemCompleted;
        impl ::subxt::events::StaticEvent for ItemCompleted {
            const PALLET: &'static str = "Utility";
            const EVENT: &'static str = "ItemCompleted";
        }
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        #[doc = "A single item within a Batch of dispatches has completed with error."]
        pub struct ItemFailed {
            pub error: runtime_types::sp_runtime::DispatchError,
        }
        impl ::subxt::events::StaticEvent for ItemFailed {
            const PALLET: &'static str = "Utility";
            const EVENT: &'static str = "ItemFailed";
        }
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        #[doc = "A call was dispatched."]
        pub struct DispatchedAs {
            pub result: ::core::result::Result<(), runtime_types::sp_runtime::DispatchError>,
        }
        impl ::subxt::events::StaticEvent for DispatchedAs {
            const PALLET: &'static str = "Utility";
            const EVENT: &'static str = "DispatchedAs";
        }
    }
}

pub mod gear {

    use super::runtime_types;

    #[doc = "
			The [event](https://docs.substrate.io/main-docs/build/events-errors/) emitted
			by this pallet.
			"]
    pub type Event = runtime_types::pallet_gear::pallet::Event;
    pub mod events {
        use super::runtime_types;
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        #[doc = "User sends message to program, which was successfully"]
        #[doc = "added to the Gear message queue."]
        pub struct MessageQueued {
            pub id: runtime_types::gear_core::ids::MessageId,
            pub source: ::sp_core::crypto::AccountId32,
            pub destination: runtime_types::gear_core::ids::ProgramId,
            pub entry: runtime_types::gear_common::event::MessageEntry,
        }
        impl ::subxt::events::StaticEvent for MessageQueued {
            const PALLET: &'static str = "Gear";
            const EVENT: &'static str = "MessageQueued";
        }
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        #[doc = "Somebody sent a message to the user."]
        pub struct UserMessageSent {
            pub message: runtime_types::gear_core::message::stored::StoredMessage,
            pub expiration: ::core::option::Option<::core::primitive::u32>,
        }
        impl ::subxt::events::StaticEvent for UserMessageSent {
            const PALLET: &'static str = "Gear";
            const EVENT: &'static str = "UserMessageSent";
        }
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        #[doc = "Message marked as \"read\" and removes it from `Mailbox`."]
        #[doc = "This event only affects messages that were"]
        #[doc = "already inserted in `Mailbox`."]
        pub struct UserMessageRead {
            pub id: runtime_types::gear_core::ids::MessageId,
            pub reason: runtime_types::gear_common::event::Reason<
                runtime_types::gear_common::event::UserMessageReadRuntimeReason,
                runtime_types::gear_common::event::UserMessageReadSystemReason,
            >,
        }
        impl ::subxt::events::StaticEvent for UserMessageRead {
            const PALLET: &'static str = "Gear";
            const EVENT: &'static str = "UserMessageRead";
        }
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        #[doc = "The result of processing the messages within the block."]
        pub struct MessagesDispatched {
            pub total: ::core::primitive::u32,
            pub statuses: ::subxt::utils::KeyedVec<
                runtime_types::gear_core::ids::MessageId,
                runtime_types::gear_common::event::DispatchStatus,
            >,
            pub state_changes: ::std::vec::Vec<runtime_types::gear_core::ids::ProgramId>,
        }
        impl ::subxt::events::StaticEvent for MessagesDispatched {
            const PALLET: &'static str = "Gear";
            const EVENT: &'static str = "MessagesDispatched";
        }
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        #[doc = "Messages execution delayed (waited) and successfully"]
        #[doc = "added to gear waitlist."]
        pub struct MessageWaited {
            pub id: runtime_types::gear_core::ids::MessageId,
            pub origin: ::core::option::Option<
                runtime_types::gear_common::gas_provider::node::GasNodeId<
                    runtime_types::gear_core::ids::MessageId,
                    runtime_types::gear_core::ids::ReservationId,
                >,
            >,
            pub reason: runtime_types::gear_common::event::Reason<
                runtime_types::gear_common::event::MessageWaitedRuntimeReason,
                runtime_types::gear_common::event::MessageWaitedSystemReason,
            >,
            pub expiration: ::core::primitive::u32,
        }
        impl ::subxt::events::StaticEvent for MessageWaited {
            const PALLET: &'static str = "Gear";
            const EVENT: &'static str = "MessageWaited";
        }
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        #[doc = "Message is ready to continue its execution"]
        #[doc = "and was removed from `Waitlist`."]
        pub struct MessageWoken {
            pub id: runtime_types::gear_core::ids::MessageId,
            pub reason: runtime_types::gear_common::event::Reason<
                runtime_types::gear_common::event::MessageWokenRuntimeReason,
                runtime_types::gear_common::event::MessageWokenSystemReason,
            >,
        }
        impl ::subxt::events::StaticEvent for MessageWoken {
            const PALLET: &'static str = "Gear";
            const EVENT: &'static str = "MessageWoken";
        }
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        #[doc = "Any data related to program codes changed."]
        pub struct CodeChanged {
            pub id: runtime_types::gear_core::ids::CodeId,
            pub change: runtime_types::gear_common::event::CodeChangeKind<::core::primitive::u32>,
        }
        impl ::subxt::events::StaticEvent for CodeChanged {
            const PALLET: &'static str = "Gear";
            const EVENT: &'static str = "CodeChanged";
        }
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        #[doc = "Any data related to programs changed."]
        pub struct ProgramChanged {
            pub id: runtime_types::gear_core::ids::ProgramId,
            pub change:
                runtime_types::gear_common::event::ProgramChangeKind<::core::primitive::u32>,
        }
        impl ::subxt::events::StaticEvent for ProgramChanged {
            const PALLET: &'static str = "Gear";
            const EVENT: &'static str = "ProgramChanged";
        }
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        #[doc = "The pseudo-inherent extrinsic that runs queue processing rolled back or not executed."]
        pub struct QueueProcessingReverted;
        impl ::subxt::events::StaticEvent for QueueProcessingReverted {
            const PALLET: &'static str = "Gear";
            const EVENT: &'static str = "QueueProcessingReverted";
        }
    }
}

pub mod runtime_types {
    use super::runtime_types;
    pub mod finality_grandpa {
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
            #[doc = "Contains one variant per dispatchable that can be called by an extrinsic."]
            pub enum Call {
                #[codec(index = 0)]
                #[doc = "Make some on-chain remark."]
                #[doc = ""]
                #[doc = "# <weight>"]
                #[doc = "- `O(1)`"]
                #[doc = "# </weight>"]
                remark {
                    remark: ::std::vec::Vec<::core::primitive::u8>,
                },
                #[codec(index = 1)]
                #[doc = "Set the number of pages in the WebAssembly environment's heap."]
                set_heap_pages { pages: ::core::primitive::u64 },
                #[codec(index = 2)]
                #[doc = "Set the new runtime code."]
                #[doc = ""]
                #[doc = "# <weight>"]
                #[doc = "- `O(C + S)` where `C` length of `code` and `S` complexity of `can_set_code`"]
                #[doc = "- 1 call to `can_set_code`: `O(S)` (calls `sp_io::misc::runtime_version` which is"]
                #[doc = "  expensive)."]
                #[doc = "- 1 storage write (codec `O(C)`)."]
                #[doc = "- 1 digest item."]
                #[doc = "- 1 event."]
                #[doc = "The weight of this function is dependent on the runtime, but generally this is very"]
                #[doc = "expensive. We will treat this as a full block."]
                #[doc = "# </weight>"]
                set_code {
                    code: ::std::vec::Vec<::core::primitive::u8>,
                },
                #[codec(index = 3)]
                #[doc = "Set the new runtime code without doing any checks of the given `code`."]
                #[doc = ""]
                #[doc = "# <weight>"]
                #[doc = "- `O(C)` where `C` length of `code`"]
                #[doc = "- 1 storage write (codec `O(C)`)."]
                #[doc = "- 1 digest item."]
                #[doc = "- 1 event."]
                #[doc = "The weight of this function is dependent on the runtime. We will treat this as a full"]
                #[doc = "block. # </weight>"]
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
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
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
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
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
                    account: ::sp_core::crypto::AccountId32,
                },
                #[codec(index = 4)]
                #[doc = "An account was reaped."]
                KilledAccount {
                    account: ::sp_core::crypto::AccountId32,
                },
                #[codec(index = 5)]
                #[doc = "On on-chain remark happened."]
                Remarked {
                    sender: ::sp_core::crypto::AccountId32,
                    hash: ::sp_core::H256,
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
                    code_hash: ::sp_core::H256,
                    memory_hash: ::sp_core::H256,
                    waitlist_hash: ::sp_core::H256,
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
                    Cut { id: _0, value: _2 },
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
                    SendUserMessage(runtime_types::gear_core::ids::MessageId),
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
            pub code_hash: ::sp_core::H256,
            pub code_exports: ::std::vec::Vec<runtime_types::gear_core::message::DispatchKind>,
            pub static_pages: runtime_types::gear_core::memory::WasmPage,
            pub state: runtime_types::gear_common::ProgramState,
        }
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        pub struct CodeMetadata {
            pub author: ::sp_core::H256,
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
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
            pub struct CodeId(pub [::core::primitive::u8; 32usize]);
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
            pub struct MessageId(pub [::core::primitive::u8; 32usize]);
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
            pub struct ProgramId(pub [::core::primitive::u8; 32usize]);
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
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
    pub mod gear_runtime {
        use super::runtime_types;
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        pub enum OriginCaller {
            #[codec(index = 0)]
            system(
                runtime_types::frame_support::dispatch::RawOrigin<::sp_core::crypto::AccountId32>,
            ),
            #[codec(index = 1)]
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
            #[codec(index = 7)]
            Session(runtime_types::pallet_session::pallet::Call),
            #[codec(index = 8)]
            Sudo(runtime_types::pallet_sudo::pallet::Call),
            #[codec(index = 9)]
            Utility(runtime_types::pallet_utility::pallet::Call),
            #[codec(index = 14)]
            Gear(runtime_types::pallet_gear::pallet::Call),
        }
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
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
            Sudo(runtime_types::pallet_sudo::pallet::Event),
            #[codec(index = 9)]
            Utility(runtime_types::pallet_utility::pallet::Event),
            #[codec(index = 14)]
            Gear(runtime_types::pallet_gear::pallet::Event),
        }
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        pub struct SessionKeys {
            pub babe: runtime_types::sp_consensus_babe::app::Public,
            pub grandpa: runtime_types::sp_finality_grandpa::app::Public,
        }
    }
    pub mod pallet_babe {
        use super::runtime_types;
        pub mod pallet {
            use super::runtime_types;
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
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
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
            #[doc = "
			Custom [dispatch errors](https://docs.substrate.io/main-docs/build/events-errors/)
			of this pallet.
			"]
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
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
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
                #[doc = "# <weight>"]
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
                #[doc = "---------------------------------"]
                #[doc = "- Origin account is already in memory, so no DB operations for them."]
                #[doc = "# </weight>"]
                transfer {
                    dest: ::sp_runtime::MultiAddress<::sp_core::crypto::AccountId32, ()>,
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
                    who: ::sp_runtime::MultiAddress<::sp_core::crypto::AccountId32, ()>,
                    #[codec(compact)]
                    new_free: ::core::primitive::u128,
                    #[codec(compact)]
                    new_reserved: ::core::primitive::u128,
                },
                #[codec(index = 2)]
                #[doc = "Exactly as `transfer`, except the origin must be root and the source account may be"]
                #[doc = "specified."]
                #[doc = "# <weight>"]
                #[doc = "- Same as transfer, but additional read and write because the source account is not"]
                #[doc = "  assumed to be in the overlay."]
                #[doc = "# </weight>"]
                force_transfer {
                    source: ::sp_runtime::MultiAddress<::sp_core::crypto::AccountId32, ()>,
                    dest: ::sp_runtime::MultiAddress<::sp_core::crypto::AccountId32, ()>,
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
                    dest: ::sp_runtime::MultiAddress<::sp_core::crypto::AccountId32, ()>,
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
                #[doc = "  keep the sender account alive (true). # <weight>"]
                #[doc = "- O(1). Just like transfer, but reading the user's transferable balance first."]
                #[doc = "  #</weight>"]
                transfer_all {
                    dest: ::sp_runtime::MultiAddress<::sp_core::crypto::AccountId32, ()>,
                    keep_alive: ::core::primitive::bool,
                },
                #[codec(index = 5)]
                #[doc = "Unreserve some balance from a user by force."]
                #[doc = ""]
                #[doc = "Can only be called by ROOT."]
                force_unreserve {
                    who: ::sp_runtime::MultiAddress<::sp_core::crypto::AccountId32, ()>,
                    amount: ::core::primitive::u128,
                },
            }
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
            #[doc = "
			Custom [dispatch errors](https://docs.substrate.io/main-docs/build/events-errors/)
			of this pallet.
			"]
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
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
            #[doc = "
			The [event](https://docs.substrate.io/main-docs/build/events-errors/) emitted
			by this pallet.
			"]
            pub enum Event {
                #[codec(index = 0)]
                #[doc = "An account was created with some free balance."]
                Endowed {
                    account: ::sp_core::crypto::AccountId32,
                    free_balance: ::core::primitive::u128,
                },
                #[codec(index = 1)]
                #[doc = "An account was removed whose balance was non-zero but below ExistentialDeposit,"]
                #[doc = "resulting in an outright loss."]
                DustLost {
                    account: ::sp_core::crypto::AccountId32,
                    amount: ::core::primitive::u128,
                },
                #[codec(index = 2)]
                #[doc = "Transfer succeeded."]
                Transfer {
                    from: ::sp_core::crypto::AccountId32,
                    to: ::sp_core::crypto::AccountId32,
                    amount: ::core::primitive::u128,
                },
                #[codec(index = 3)]
                #[doc = "A balance was set by root."]
                BalanceSet {
                    who: ::sp_core::crypto::AccountId32,
                    free: ::core::primitive::u128,
                    reserved: ::core::primitive::u128,
                },
                #[codec(index = 4)]
                #[doc = "Some balance was reserved (moved from free to reserved)."]
                Reserved {
                    who: ::sp_core::crypto::AccountId32,
                    amount: ::core::primitive::u128,
                },
                #[codec(index = 5)]
                #[doc = "Some balance was unreserved (moved from reserved to free)."]
                Unreserved {
                    who: ::sp_core::crypto::AccountId32,
                    amount: ::core::primitive::u128,
                },
                #[codec(index = 6)]
                #[doc = "Some balance was moved from the reserve of the first account to the second account."]
                #[doc = "Final argument indicates the destination balance type."]
                ReserveRepatriated {
                    from: ::sp_core::crypto::AccountId32,
                    to: ::sp_core::crypto::AccountId32,
                    amount: ::core::primitive::u128,
                    destination_status:
                        runtime_types::frame_support::traits::tokens::misc::BalanceStatus,
                },
                #[codec(index = 7)]
                #[doc = "Some amount was deposited (e.g. for transaction fees)."]
                Deposit {
                    who: ::sp_core::crypto::AccountId32,
                    amount: ::core::primitive::u128,
                },
                #[codec(index = 8)]
                #[doc = "Some amount was withdrawn from the account (e.g. for transaction fees)."]
                Withdraw {
                    who: ::sp_core::crypto::AccountId32,
                    amount: ::core::primitive::u128,
                },
                #[codec(index = 9)]
                #[doc = "Some amount was removed from the account (e.g. for misbehavior)."]
                Slashed {
                    who: ::sp_core::crypto::AccountId32,
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
    pub mod pallet_gear {
        use super::runtime_types;
        pub mod pallet {
            use super::runtime_types;
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
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
            }
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
            #[doc = "
			Custom [dispatch errors](https://docs.substrate.io/main-docs/build/events-errors/)
			of this pallet.
			"]
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
            }
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
            #[doc = "
			The [event](https://docs.substrate.io/main-docs/build/events-errors/) emitted
			by this pallet.
			"]
            pub enum Event {
                #[codec(index = 0)]
                #[doc = "User sends message to program, which was successfully"]
                #[doc = "added to the Gear message queue."]
                MessageQueued {
                    id: runtime_types::gear_core::ids::MessageId,
                    source: ::sp_core::crypto::AccountId32,
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
                    change:
                        runtime_types::gear_common::event::CodeChangeKind<::core::primitive::u32>,
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
            #[doc = "
			Custom [dispatch errors](https://docs.substrate.io/main-docs/build/events-errors/)
			of this pallet.
			"]
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
            #[doc = "
			Custom [dispatch errors](https://docs.substrate.io/main-docs/build/events-errors/)
			of this pallet.
			"]
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
            #[doc = "
			Custom [dispatch errors](https://docs.substrate.io/main-docs/build/events-errors/)
			of this pallet.
			"]
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
            #[doc = "
			Custom [dispatch errors](https://docs.substrate.io/main-docs/build/events-errors/)
			of this pallet.
			"]
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
    pub mod pallet_grandpa {
        use super::runtime_types;
        pub mod pallet {
            use super::runtime_types;
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
            #[doc = "Contains one variant per dispatchable that can be called by an extrinsic."]
            pub enum Call {
                #[codec(index = 0)]
                #[doc = "Report voter equivocation/misbehavior. This method will verify the"]
                #[doc = "equivocation proof and validate the given key ownership proof"]
                #[doc = "against the extracted offender. If both are valid, the offence"]
                #[doc = "will be reported."]
                report_equivocation {
                    equivocation_proof: ::std::boxed::Box<
                        runtime_types::sp_finality_grandpa::EquivocationProof<
                            ::sp_core::H256,
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
                        runtime_types::sp_finality_grandpa::EquivocationProof<
                            ::sp_core::H256,
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
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
            #[doc = "
			Custom [dispatch errors](https://docs.substrate.io/main-docs/build/events-errors/)
			of this pallet.
			"]
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
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
            #[doc = "
			The [event](https://docs.substrate.io/main-docs/build/events-errors/) emitted
			by this pallet.
			"]
            pub enum Event {
                #[codec(index = 0)]
                #[doc = "New authority set has been applied."]
                NewAuthorities {
                    authority_set: ::std::vec::Vec<(
                        runtime_types::sp_finality_grandpa::app::Public,
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
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        pub struct StoredPendingChange<_0> {
            pub scheduled_at: _0,
            pub delay: _0,
            pub next_authorities:
                runtime_types::sp_core::bounded::weak_bounded_vec::WeakBoundedVec<(
                    runtime_types::sp_finality_grandpa::app::Public,
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
    pub mod pallet_session {
        use super::runtime_types;
        pub mod pallet {
            use super::runtime_types;
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
            #[doc = "Contains one variant per dispatchable that can be called by an extrinsic."]
            pub enum Call {
                #[codec(index = 0)]
                #[doc = "Sets the session key(s) of the function caller to `keys`."]
                #[doc = "Allows an account to set its session key prior to becoming a validator."]
                #[doc = "This doesn't take effect until the next session."]
                #[doc = ""]
                #[doc = "The dispatch origin of this function must be signed."]
                #[doc = ""]
                #[doc = "# <weight>"]
                #[doc = "- Complexity: `O(1)`. Actual cost depends on the number of length of"]
                #[doc = "  `T::Keys::key_ids()` which is fixed."]
                #[doc = "- DbReads: `origin account`, `T::ValidatorIdOf`, `NextKeys`"]
                #[doc = "- DbWrites: `origin account`, `NextKeys`"]
                #[doc = "- DbReads per key id: `KeyOwner`"]
                #[doc = "- DbWrites per key id: `KeyOwner`"]
                #[doc = "# </weight>"]
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
                #[doc = "# <weight>"]
                #[doc = "- Complexity: `O(1)` in number of key types. Actual cost depends on the number of length"]
                #[doc = "  of `T::Keys::key_ids()` which is fixed."]
                #[doc = "- DbReads: `T::ValidatorIdOf`, `NextKeys`, `origin account`"]
                #[doc = "- DbWrites: `NextKeys`, `origin account`"]
                #[doc = "- DbWrites per key id: `KeyOwner`"]
                #[doc = "# </weight>"]
                purge_keys,
            }
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
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
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
            #[doc = "
			The [event](https://docs.substrate.io/main-docs/build/events-errors/) emitted
			by this pallet.
			"]
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
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
            #[doc = "Contains one variant per dispatchable that can be called by an extrinsic."]
            pub enum Call {
                #[codec(index = 0)]
                #[doc = "Authenticates the sudo key and dispatches a function call with `Root` origin."]
                #[doc = ""]
                #[doc = "The dispatch origin for this call must be _Signed_."]
                #[doc = ""]
                #[doc = "# <weight>"]
                #[doc = "- O(1)."]
                #[doc = "- Limited storage reads."]
                #[doc = "- One DB write (event)."]
                #[doc = "- Weight of derivative `call` execution + 10,000."]
                #[doc = "# </weight>"]
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
                #[doc = "# <weight>"]
                #[doc = "- O(1)."]
                #[doc = "- The weight of this call is defined by the caller."]
                #[doc = "# </weight>"]
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
                #[doc = "# <weight>"]
                #[doc = "- O(1)."]
                #[doc = "- Limited storage reads."]
                #[doc = "- One DB change."]
                #[doc = "# </weight>"]
                set_key {
                    new: ::sp_runtime::MultiAddress<::sp_core::crypto::AccountId32, ()>,
                },
                #[codec(index = 3)]
                #[doc = "Authenticates the sudo key and dispatches a function call with `Signed` origin from"]
                #[doc = "a given account."]
                #[doc = ""]
                #[doc = "The dispatch origin for this call must be _Signed_."]
                #[doc = ""]
                #[doc = "# <weight>"]
                #[doc = "- O(1)."]
                #[doc = "- Limited storage reads."]
                #[doc = "- One DB write (event)."]
                #[doc = "- Weight of derivative `call` execution + 10,000."]
                #[doc = "# </weight>"]
                sudo_as {
                    who: ::sp_runtime::MultiAddress<::sp_core::crypto::AccountId32, ()>,
                    call: ::std::boxed::Box<runtime_types::gear_runtime::RuntimeCall>,
                },
            }
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
            #[doc = "Error for the Sudo pallet"]
            pub enum Error {
                #[codec(index = 0)]
                #[doc = "Sender must be the Sudo account"]
                RequireSudo,
            }
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
            #[doc = "
			The [event](https://docs.substrate.io/main-docs/build/events-errors/) emitted
			by this pallet.
			"]
            pub enum Event {
                #[codec(index = 0)]
                #[doc = "A sudo just took place. "]
                Sudid {
                    sudo_result:
                        ::core::result::Result<(), runtime_types::sp_runtime::DispatchError>,
                },
                #[codec(index = 1)]
                #[doc = "The sudoer just switched identity; the old key is supplied if one existed."]
                KeyChanged {
                    old_sudoer: ::core::option::Option<::sp_core::crypto::AccountId32>,
                },
                #[codec(index = 2)]
                #[doc = "A sudo just took place. "]
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
                #[doc = "# <weight>"]
                #[doc = "- `O(1)` (Note that implementations of `OnTimestampSet` must also be `O(1)`)"]
                #[doc = "- 1 storage read and 1 storage mutation (codec `O(1)`). (because of `DidUpdate::take` in"]
                #[doc = "  `on_finalize`)"]
                #[doc = "- 1 event handler `on_timestamp_set`. Must be `O(1)`."]
                #[doc = "# </weight>"]
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
            #[doc = "
			The [event](https://docs.substrate.io/main-docs/build/events-errors/) emitted
			by this pallet.
			"]
            pub enum Event {
                #[codec(index = 0)]
                #[doc = "A transaction fee `actual_fee`, of which `tip` was added to the minimum inclusion fee,"]
                #[doc = "has been paid by `who`."]
                TransactionFeePaid {
                    who: ::sp_core::crypto::AccountId32,
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
    pub mod pallet_utility {
        use super::runtime_types;
        pub mod pallet {
            use super::runtime_types;
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
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
                #[doc = "# <weight>"]
                #[doc = "- Complexity: O(C) where C is the number of calls to be batched."]
                #[doc = "# </weight>"]
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
                #[doc = "# <weight>"]
                #[doc = "- Complexity: O(C) where C is the number of calls to be batched."]
                #[doc = "# </weight>"]
                batch_all {
                    calls: ::std::vec::Vec<runtime_types::gear_runtime::RuntimeCall>,
                },
                #[codec(index = 3)]
                #[doc = "Dispatches a function call with a provided origin."]
                #[doc = ""]
                #[doc = "The dispatch origin for this call must be _Root_."]
                #[doc = ""]
                #[doc = "# <weight>"]
                #[doc = "- O(1)."]
                #[doc = "- Limited storage reads."]
                #[doc = "- One DB write (event)."]
                #[doc = "- Weight of derivative `call` execution + T::WeightInfo::dispatch_as()."]
                #[doc = "# </weight>"]
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
                #[doc = "# <weight>"]
                #[doc = "- Complexity: O(C) where C is the number of calls to be batched."]
                #[doc = "# </weight>"]
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
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
            #[doc = "
			Custom [dispatch errors](https://docs.substrate.io/main-docs/build/events-errors/)
			of this pallet.
			"]
            pub enum Error {
                #[codec(index = 0)]
                #[doc = "Too many calls batched."]
                TooManyCalls,
            }
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
            #[doc = "
			The [event](https://docs.substrate.io/main-docs/build/events-errors/) emitted
			by this pallet.
			"]
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
                    result: ::core::result::Result<(), runtime_types::sp_runtime::DispatchError>,
                },
            }
        }
    }
    pub mod primitive_types {
        use super::runtime_types;
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        pub struct H256(pub [::core::primitive::u8; 32usize]);
    }
    pub mod sp_arithmetic {
        use super::runtime_types;
        pub mod fixed_point {
            use super::runtime_types;
            #[derive(
                :: subxt :: ext :: codec :: CompactAs,
                :: subxt :: ext :: codec :: Decode,
                :: subxt :: ext :: codec :: Encode,
                Debug,
            )]
            pub struct FixedU128(pub ::core::primitive::u128);
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
            pub struct AccountId32(pub [::core::primitive::u8; 32usize]);
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
        pub enum Void {}
    }
    pub mod sp_finality_grandpa {
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
                runtime_types::finality_grandpa::Equivocation<
                    runtime_types::sp_finality_grandpa::app::Public,
                    runtime_types::finality_grandpa::Prevote<_0, _1>,
                    runtime_types::sp_finality_grandpa::app::Signature,
                >,
            ),
            #[codec(index = 1)]
            Precommit(
                runtime_types::finality_grandpa::Equivocation<
                    runtime_types::sp_finality_grandpa::app::Public,
                    runtime_types::finality_grandpa::Precommit<_0, _1>,
                    runtime_types::sp_finality_grandpa::app::Signature,
                >,
            ),
        }
        #[derive(:: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug)]
        pub struct EquivocationProof<_0, _1> {
            pub set_id: ::core::primitive::u64,
            pub equivocation: runtime_types::sp_finality_grandpa::Equivocation<_0, _1>,
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
                    pub parent_hash: ::sp_core::H256,
                    #[codec(compact)]
                    pub number: _0,
                    pub state_root: ::sp_core::H256,
                    pub extrinsics_root: ::sp_core::H256,
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
                    #[codec(skip)] pub ::core::marker::PhantomData<(_0, _1, _2, _3)>,
                );
            }
        }
        pub mod multiaddress {
            use super::runtime_types;
            #[derive(
                :: subxt :: ext :: codec :: Decode, :: subxt :: ext :: codec :: Encode, Debug,
            )]
            pub enum MultiAddress<_0, _1> {
                #[codec(index = 0)]
                Id(_0),
                #[codec(index = 1)]
                Index(#[codec(compact)] _1),
                #[codec(index = 2)]
                Raw(::std::vec::Vec<::core::primitive::u8>),
                #[codec(index = 3)]
                Address32([::core::primitive::u8; 32usize]),
                #[codec(index = 4)]
                Address20([::core::primitive::u8; 20usize]),
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
}
#[doc = r" The default error type returned when there is a runtime issue,"]
#[doc = r" exposed here for ease of use."]
pub type DispatchError = runtime_types::sp_runtime::DispatchError;
