// This file is part of Gear.

// Copyright (C) 2021-2023 Gear Technologies Inc.
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

// This module simply exposes a set of types that contracts can use to interact
// with the so-called "built-in" actors - that is the actors that are defined
// for any Gear runtime and provide an API for the applications to build on top
// of some blockchain logic like staking, governance, etc.

/// For the built-in actor to process a message, it should be able to decode its
/// payload into one of the supported message types.
///
/// Currently supported message types include:
///
/// - [`StakingMessage::Bond { value }`] - bond up to the `value` to self as the
///   controller
/// - [`StakingMessage::BondExtra { value }`] - add more `value` to the sender's
///   bonded amount
/// - [`StakingMessage::Unbond { value }`] - unbond up to the `value` for future
///   withdrawal
/// - [`StakingMessage::Nominate { targets }`] - nominate `targets` as
///   validators

/// # Examples
///
/// The following example shows how a contract can send a message to the
/// built-in actor to bond some `value` to self as the controller so that the
/// contract can later use the staking API to nominate validators.
///
/// ```ignore
/// use gstd::ActorId;
/// use gstd::msg::{self, built_in::staking::StakingMessage};
/// use parity_scale_codec::Encode;
///
/// const BUILT_IN: ActorId = ActorId::new(hex_literal::hex!(
///     "9d765baea1938d17096421e4f881af7dc4ce5c15bb5022f409fc0d6265d97c3a"
/// ));
///
/// #[gstd::async_main]
/// async fn main() {
///     let value = msg::value();
///     let payload = StakingMessage::Bond { value }.encode();
///     let _ = msg::send_bytes_for_reply(BUILT_IN, &payload[..], 0, 0)
///         .expect("Error sending message")
///         .await;
/// }
/// # fn main() {}
/// ```
pub use gear_built_in_actor_common::*;
