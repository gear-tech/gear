// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::vec::Vec;
use gprimitives::{H160, H256, U256};
use parity_scale_codec::{Decode, Encode};
use scale_info::TypeInfo;

/// pallet-gear-eth-bridge builtin actor request types.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Encode, Decode, TypeInfo)]
pub enum Request {
    /// Send an Ethereum message to the specified `destination` address with the given `payload`.
    #[codec(index = 0)]
    SendEthMessage { destination: H160, payload: Vec<u8> },
}

/// pallet-gear-eth-bridge builtin actor response types.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Encode, Decode, TypeInfo)]
pub enum Response {
    /// Returned when an Ethereum message is successfully enqueued.
    #[codec(index = 0)]
    EthMessageQueued {
        /// System block number when the message was enqueued.
        block_number: u32,
        /// Hash of the enqueued message.
        hash: H256,
        /// Nonce of the enqueued message.
        nonce: U256,
        /// ID of the queue where the message was enqueued.
        queue_id: u64,
    },
}
