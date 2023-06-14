// This file is part of Gear.

// Copyright (C) 2022-2023 Gear Technologies Inc.
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

//! Crate for providing metadata for smart contracts.
//!
//! Metadata is used to describe the interface of a smart contract. For example,
//! it can be used when uploading a contract using <https://idea.gear-tech.io>.
//! The metadata informs the user about the contract's interface and allows them
//! to interact with it using custom types on web applications UI.
//!
//! To generate a metadata output file for a contract, you need:
//!
//! - Add `gmeta` crate to your `Cargo.toml` file.
//! - Define an empty struct that will identify the contract metadata.
//! - Implement the [`Metadata`] trait for this struct by defining the
//!   associated types of the trait.
//! - **Option 1**: Call [`gear_wasm_builder::build_with_metadata`](https://docs.gear.rs/gear_wasm_builder/fn.build_with_metadata.html)
//!   function in `build.rs` file.
//! - **Option 2**: Convert metadata to hex string using [`MetadataRepr::hex`]
//!   function and write it to the text file.
//!
//! # Examples
//!
//! In this example we will create a simple ping-pong contract. Let's define
//! message types and metadata in a separate `ping-io` crate to be able to use
//! it in both smart contract and `build.rs` files.
//!
//! We will define message types for `handle()` and `state()` functions.
//!
//! - `ping-io` crate:
//!
//! ```
//! #[no_std]
//! use gmeta::{InOut, Metadata};
//! use gstd::prelude::*;
//!
//! // Message type for `handle()` function.
//! #[derive(Encode, Decode, TypeInfo)]
//! pub enum PingPong {
//!     Ping,
//!     Pong,
//! }
//!
//! // Metadata struct.
//! pub struct ContractMetadata;
//!
//! impl Metadata for ContractMetadata {
//!     // The unit tuple is used as neither incoming nor outgoing messages are
//!     // expected in the `init()` function.
//!     type Init = ();
//!     // We use the same `PingPong` type for both incoming and outgoing
//!     // messages.
//!     type Handle = InOut<PingPong, PingPong>;
//!     // The unit tuple is used as we don't use asynchronous interaction in this
//!     // contract.
//!     type Others = ();
//!     // The unit tuple is used as we don't process any replies in this contract.
//!     type Reply = ();
//!     // The unit tuple is used as we don't process any signals in this contract.
//!     type Signal = ();
//!     // We return a counter value (`i32`) in the `state()` function in this contract.
//!     type State = i32;
//! }
//! ```
//!
//! - `ping` smart contract crate:
//!
//! ```
//! #[no_std]
//! use gmeta::{InOut, Metadata};
//! use gstd::{msg, prelude::*};
//! # const IGNORE: &'static str = stringify! {
//! use ping_io::PingPong;
//! # };
//!
//! // Counter that will be incremented on each `Ping` message.
//! static mut COUNTER: i32 = 0;
//!
//! # #[derive(Encode, Decode, TypeInfo)]
//! # pub enum PingPong {
//! #     Ping,
//! #     Pong,
//! # }
//! #
//! #[no_mangle]
//! extern "C" fn handle() {
//!     // Load incoming message of `PingPong` type.
//!     let payload: PingPong = msg::load().expect("Unable to load");
//!
//!     if let PingPong::Ping = payload {
//!         unsafe { COUNTER += 1 };
//!         // Send a reply message of `PingPong` type back to the sender.
//!         msg::reply(PingPong::Pong, 0).expect("Unable to reply");
//!     }
//! }
//!
//! #[no_mangle]
//! extern "C" fn state() {
//!     msg::reply(unsafe { COUNTER }, 0).expect("Unable to reply");
//! }
//! ```
//!
//! - `build.rs` file:
//!
//! ```no_run
//! # const IGNORE: &'static str = stringify! {
//! use ping_io::ContractMetadata;
//! # };
//! #
//! # pub struct ContractMetadata;
//! # impl gmeta::Metadata for ContractMetadata {
//! #     type Init = ();
//! #     type Handle = ();
//! #     type Others = ();
//! #     type Reply = ();
//! #     type Signal = ();
//! #     type State = ();
//! # }
//!
//! fn main() {
//!     gear_wasm_builder::build_with_metadata::<ContractMetadata>();
//! }
//! ```
//!
//! You can also generate metadata manually and write it to the file without
//! using `build.rs`:
//!
//! ```
//! use gmeta::Metadata;
//! # const IGNORE: &'static str = stringify! {
//! use ping_io::ContractMetadata;
//! # };
//! use std::fs;
//!
//! # #[derive(gstd::Encode, gstd::Decode, gstd::TypeInfo)]
//! # pub enum PingPong {
//! #     Ping,
//! #     Pong,
//! # }
//! #
//! # pub struct ContractMetadata;
//! # impl gmeta::Metadata for ContractMetadata {
//! #     type Init = ();
//! #     type Handle = (PingPong, PingPong);
//! #     type Others = ();
//! #     type Reply = ();
//! #     type Signal = ();
//! #     type State = i32;
//! # }
//! #
//! let metadata_hex = ContractMetadata::repr().hex();
//! assert_eq!(metadata_hex.len(), 140);
//! fs::write("ping.meta.txt", metadata_hex).expect("Unable to write");
//! ```
//!
//! You can parse generated metadata file using `gear-js` API in JavaScript:
//!
//! ```javascript
//! import { getProgramMetadata } from '@gear-js/api';
//! import { readFileSync } from 'fs';
//!
//! const metadataHex = readFileSync('ping.meta.txt', 'utf-8');
//! const metadata = getProgramMetadata('0x' + metadataHex);
//!
//! console.log('Registry:', metadata.regTypes);
//! console.log('Types:', metadata.types);
//! ```
//!
//! This will print the following:
//!
//! ```text
//! Registry: Map(2) {
//!   0 => { name: 'RustOutPingPong', def: '{"_enum":["Ping","Pong"]}' },
//!   1 => { name: 'i32', def: null }
//! }
//! Types: {
//!   init: { input: null, output: null },
//!   handle: { input: 0, output: 0 },
//!   reply: { input: null, output: null },
//!   others: { input: null, output: null },
//!   signal: null,
//!   state: 1
//! }
//! ```

#![no_std]
#![warn(missing_docs)]
#![doc(html_logo_url = "https://docs.gear.rs/logo.svg")]

extern crate alloc;

#[cfg(feature = "codegen")]
pub use gmeta_codegen::metawasm;

pub use scale_info::{MetaType, PortableRegistry, Registry};

use alloc::{collections::BTreeMap, string::String, vec::Vec};
use blake2_rfc::blake2b;
use core::any::TypeId;
use scale_info::{
    scale::{self, Decode, Encode},
    TypeInfo,
};

/// Types representation used by metadata.
#[derive(Encode, Debug, Decode, Eq, PartialEq)]
#[codec(crate = scale)]
pub struct TypesRepr {
    /// Input types.
    pub input: Option<u32>,
    /// Output types.
    pub output: Option<u32>,
}

/// Metadata internal representation.
#[derive(Encode, Debug, Decode, Eq, PartialEq)]
#[codec(crate = scale)]
pub struct MetadataRepr {
    /// Internal representation for [`Metadata::Init`] type.
    pub init: TypesRepr,
    /// Internal representation for [`Metadata::Handle`] type.
    pub handle: TypesRepr,
    /// Internal representation for [`Metadata::Reply`] type.
    pub reply: TypesRepr,
    /// Internal representation for [`Metadata::Others`] type.
    pub others: TypesRepr,
    /// Internal representation for [`Metadata::Signal`] type.
    pub signal: Option<u32>,
    /// Internal representation for [`Metadata::State`] type.
    pub state: Option<u32>,
    /// Encoded registry of types.
    pub registry: Vec<u8>,
}

/// Metawasm data.
#[derive(Encode, Debug, Decode)]
#[codec(crate = scale)]
pub struct MetawasmData {
    /// Meta functions.
    pub funcs: BTreeMap<String, TypesRepr>,
    /// Registry.
    pub registry: Vec<u8>,
}

/// Trait used to get information about types.
pub trait Type: TypeInfo + 'static {
    /// Return `true` if type is unit.
    fn is_unit() -> bool {
        TypeId::of::<Self>().eq(&TypeId::of::<()>())
    }

    /// Create [`MetaType`] information about type.
    fn meta_type() -> MetaType {
        MetaType::new::<Self>()
    }

    /// Register type in the registry and return its identifier if it is not the
    /// unit type.
    fn register(registry: &mut Registry) -> Option<u32> {
        (!Self::is_unit()).then(|| registry.register_type(&Self::meta_type()).id)
    }
}

impl<T: TypeInfo + 'static> Type for T {}

/// Trait used for registering types in registry.
pub trait Types {
    /// Input type.
    type Input: Type;
    /// Output type.
    type Output: Type;

    /// Register input/output types in registry.
    fn register(registry: &mut Registry) -> TypesRepr {
        let input = Self::Input::register(registry);
        let output = Self::Output::register(registry);

        TypesRepr { input, output }
    }
}

/// Type alias for incoming/outgoing message types.
pub type InOut<I, O> = (I, O);
/// Type alias for incoming message type without any outgoing type.
pub type In<I> = InOut<I, ()>;
/// Type alias for outgoing message type without any incoming type.
pub type Out<O> = InOut<(), O>;

impl<I: Type, O: Type> Types for InOut<I, O> {
    type Input = I;
    type Output = O;
}

impl Types for () {
    type Input = ();
    type Output = ();
}

impl MetadataRepr {
    /// Encode metadata into bytes using codec.
    pub fn bytes(&self) -> Vec<u8> {
        self.encode()
    }

    /// Decode metadata from hex.
    pub fn from_hex<T: AsRef<[u8]>>(data: T) -> Result<Self, MetadataParseError> {
        let data = hex::decode(data)?;
        let this = Self::decode(&mut data.as_ref())?;
        Ok(this)
    }

    /// Encode metadata into hex string.
    pub fn hex(&self) -> String {
        hex::encode(self.bytes())
    }

    /// Calculate BLAKE2b hash of metadata bytes.
    pub fn hash(&self) -> [u8; 32] {
        let mut arr = [0; 32];

        let blake2b_hash = blake2b::blake2b(arr.len(), &[], &self.bytes());
        arr[..].copy_from_slice(blake2b_hash.as_bytes());

        arr
    }

    /// Calculate BLAKE2b hash of metadata and encode it into hex string.
    pub fn hash_hex(&self) -> String {
        hex::encode(self.hash())
    }
}

/// Error that can occur during metadata parsing.
#[derive(Debug, derive_more::From)]
pub enum MetadataParseError {
    /// Error that can occur during encoding/decoding.
    Codec(scale_info::scale::Error),
    /// Error that can occur during hex decoding.
    FromHex(hex::FromHexError),
}

/// Trait used for defining metadata.
pub trait Metadata {
    /// Init message type.
    ///
    /// Describes incoming/outgoing types for the `init()` function.
    ///
    /// - Use unit tuple `()` if neither incoming nor outgoing messages are
    ///   expected in the `init()` function.
    /// - Use [`In`] type alias if only incoming message is expected in the
    ///   `init()` function.
    /// - Use [`Out`] type alias if only outgoing message is expected in the
    ///   `init()` function.
    /// - Use [`InOut`] type alias if both incoming and outgoing messages are
    ///   expected in the `init()` function.
    type Init: Types;
    /// Handle message type.
    ///
    /// Describes incoming/outgoing types for the `handle()` function.
    type Handle: Types;
    /// Reply message type.
    ///
    /// Describes incoming/outgoing types for the `reply()` function.
    type Reply: Types;
    /// Asynchronous handle message type.
    ///
    /// Describes incoming/outgoing types for the `main()` function in case of
    /// asynchronous interaction.
    type Others: Types;
    /// Signal message type.
    ///
    /// Describes only the outgoing type from the program while processing the
    /// system signal.
    type Signal: Type;
    /// State type.
    ///
    /// Describes the type for the queried state returned by the `state()`
    /// function.
    ///
    /// Use the type that you pass to the `msg::reply` function in the `state()`
    /// function or unit tuple `()` if no `state()` function is defined.
    type State: Type;

    /// Create metadata representation and register types in registry.
    fn repr() -> MetadataRepr {
        let mut registry = Registry::new();

        MetadataRepr {
            init: Self::Init::register(&mut registry),
            handle: Self::Handle::register(&mut registry),
            reply: Self::Reply::register(&mut registry),
            others: Self::Others::register(&mut registry),
            signal: Self::Signal::register(&mut registry),
            state: Self::State::register(&mut registry),
            registry: PortableRegistry::from(registry).encode(),
        }
    }
}
