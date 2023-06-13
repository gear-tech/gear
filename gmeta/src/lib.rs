//! Crate for providing metadata for smart contracts.
//!
//! Metadata is used to describe the interface of a smart contract. For example,
//! it can be used when uploading a contract using <https://idea.gear-tech.io>.
//! The metadata informs the user about the contract's interface and allows them
//! to interact with it using custom types.
//!
//! To generate a metadata output file `contract_name.meta.txt` for a contract,
//! you need:
//!
//! - Add `gmeta` crate to your `Cargo.toml` file.
//! - Define an empty struct that will identify the contract metadata.
//! - Implement the [`Metadata`] trait for this struct by defining the
//!   associated types of the trait.
//! - Call [`gear_wasm_builder::build_with_metadata`](https://docs.gear.rs/gear_wasm_builder/fn.build_with_metadata.html)
//!   function instead of [`gear_wasm_builder::build`](https://docs.gear.rs/gear_wasm_builder/fn.build.html)
//!   function in `build.rs` file.
//!
//! # Example
//!
//! In this example we will create a simple ping-pong contract.
//!
//! We will define message types for `handle()` and `state()` functions.
//!
//! ```
//! #[no_std]
//! use gmeta::{InOut, Metadata};
//! use gstd::{msg, prelude::*, ActorId};
//!
//! // Counter that will be incremented on each `Ping` message.
//! static mut COUNTER: i32 = 0;
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
//!
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
//! `build.rs` file:
//!
//! ```no_run
//! # const IGNORE: &'static str = stringify! {
//! use ping::ContractMetadata;
//! # };
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

/// Metadata representation.
#[derive(Encode, Debug, Decode, Eq, PartialEq)]
#[codec(crate = scale)]
pub struct MetadataRepr {
    /// Init types representation.
    pub init: TypesRepr,
    /// Handle types representation.
    pub handle: TypesRepr,
    /// Reply types representation.
    pub reply: TypesRepr,
    /// Async main types representation.
    pub others: TypesRepr,
    /// Signal output type representation.
    pub signal: Option<u32>,
    /// State type representation.
    pub state: Option<u32>,
    /// Registry.
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

    /// Return [`MetaType`](scale_info::MetaType) information about type.
    fn meta_type() -> MetaType {
        MetaType::new::<Self>()
    }

    /// Register type in registry.
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
/// Type alias for incoming message type.
pub type In<I> = InOut<I, ()>;
/// Type alias for outgoing message type.
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
    /// Encode metadata representation into bytes.
    pub fn bytes(&self) -> Vec<u8> {
        self.encode()
    }

    /// Decode metadata representation from hex.
    pub fn from_hex<T: AsRef<[u8]>>(data: T) -> Result<Self, MetadataParseError> {
        let data = hex::decode(data)?;
        let this = Self::decode(&mut data.as_ref())?;
        Ok(this)
    }

    /// Encode metadata representation into hex string.
    pub fn hex(&self) -> String {
        hex::encode(self.bytes())
    }

    /// Calculate BLAKE2b hash of metadata representation.
    pub fn hash(&self) -> [u8; 32] {
        let mut arr = [0; 32];

        let blake2b_hash = blake2b::blake2b(arr.len(), &[], &self.bytes());
        arr[..].copy_from_slice(blake2b_hash.as_bytes());

        arr
    }

    /// Calculate BLAKE2b hash of metadata representation and encode it into hex
    /// string.
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
    ///
    /// - Use unit tuple `()` if neither incoming nor outgoing messages are
    ///   expected in the `handle()` function.
    /// - Use [`In`] type alias if only incoming message is expected in the
    ///   `handle()` function.
    /// - Use [`Out`] type alias if only outgoing message is expected in the
    ///   `handle()` function.
    /// - Use [`InOut`] type alias if both incoming and outgoing messages are
    ///   expected in the `handle()` function.
    type Handle: Types;
    /// Reply message type.
    ///
    /// Describes incoming/outgoing types for the `reply()` function.
    type Reply: Types;
    /// Asynchronous handle message type.
    ///
    /// Describes incoming/outgoing types for the `main()` function in case of
    /// asynchronous interaction.
    ///
    /// - Use unit tuple `()` if neither incoming nor outgoing messages are
    ///   expected in the `main()` function.
    /// - Use [`In`] type alias if only incoming message is expected in the
    ///   `main()` function.
    /// - Use [`Out`] type alias if only outgoing message is expected in the
    ///   `main()` function.
    /// - Use [`InOut`] type alias if both incoming and outgoing messages are
    ///   expected in the `main()` function.
    type Others: Types;
    /// Signal message type.
    ///
    /// - Use unit tuple `()` if neither incoming nor outgoing messages are
    ///   expected in the `handle_signal()` function.
    /// - Use [`Out`] type alias if only outgoing message is expected in the
    ///   `handle_signal()` function.
    type Signal: Type;
    /// State message type.
    ///
    /// Describes the type for the queried state returned by the state()
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
