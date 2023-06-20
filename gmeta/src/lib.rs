#![no_std]

extern crate alloc;

#[cfg(feature = "codegen")]
pub use gmeta_codegen::metawasm;

pub use alloc::{collections::BTreeMap, string::String};
pub use scale_info::{
    scale::{self, Decode, Encode},
    MetaType, PortableRegistry, Registry, TypeInfo,
};

use alloc::{vec, vec::Vec};
use blake2_rfc::blake2b;
use core::{any::TypeId, mem};

const METADATA_VERSION: u16 = 1;

#[repr(u8)]
pub enum LanguageId {
    Rust = 0,
    AssemblyScript,
}

#[derive(Encode, Debug, Decode, Eq, PartialEq)]
#[codec(crate = scale)]
pub struct TypesRepr {
    pub input: Option<u32>,
    pub output: Option<u32>,
}

#[derive(Encode, Debug, Decode, Eq, PartialEq)]
#[codec(crate = scale)]
pub struct MetadataRepr {
    pub init: TypesRepr,
    pub handle: TypesRepr,
    pub reply: Option<u32>,
    pub others: TypesRepr,
    pub signal: Option<u32>,
    pub state: Option<u32>,
    pub registry: Vec<u8>,
}

#[derive(Encode, Debug, Decode)]
#[codec(crate = scale)]
pub struct MetawasmData {
    pub funcs: BTreeMap<String, TypesRepr>,
    pub registry: Vec<u8>,
}

pub trait Type: TypeInfo + 'static {
    fn is_unit() -> bool {
        TypeId::of::<Self>().eq(&TypeId::of::<()>())
    }

    fn meta_type() -> MetaType {
        MetaType::new::<Self>()
    }

    fn register(registry: &mut Registry) -> Option<u32> {
        (!Self::is_unit()).then(|| registry.register_type(&Self::meta_type()).id)
    }
}

impl<T: TypeInfo + 'static> Type for T {}

pub trait Types {
    type Input: Type;
    type Output: Type;

    fn register(registry: &mut Registry) -> TypesRepr {
        let input = Self::Input::register(registry);
        let output = Self::Output::register(registry);

        TypesRepr { input, output }
    }
}

pub type InOut<I, O> = (I, O);
pub type In<I> = InOut<I, ()>;
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
    pub fn bytes(&self) -> Vec<u8> {
        // Append language ID and version as a preamble
        let version_bytes = METADATA_VERSION.to_le_bytes();
        let mut bytes = vec![LanguageId::Rust as u8, version_bytes[0], version_bytes[1]];

        bytes.extend(self.encode());
        bytes
    }

    pub fn from_bytes(data: impl AsRef<[u8]>) -> Result<Self, MetadataParseError> {
        let preamble_len = mem::size_of::<LanguageId>() | mem::size_of_val(&METADATA_VERSION);
        let data = data.as_ref();
        if data.len() < preamble_len {
            return Err(MetadataParseError::InvalidMetadata);
        }

        // Check language ID and version
        let lang_id = data[0];
        if lang_id != LanguageId::Rust as u8 {
            return Err(MetadataParseError::UnsupportedLanguageId(lang_id));
        }
        let version = u16::from_le_bytes([data[1], data[2]]);
        if version != METADATA_VERSION {
            return Err(MetadataParseError::UnsupportedVersion(version));
        }

        // Remove preamble before decoding
        let mut data = &data[preamble_len..];

        let this = Self::decode(&mut data)?;
        Ok(this)
    }

    pub fn from_hex<T: AsRef<[u8]>>(data: T) -> Result<Self, MetadataParseError> {
        Self::from_bytes(hex::decode(data)?)
    }

    pub fn hex(&self) -> String {
        hex::encode(self.bytes())
    }

    pub fn hash(&self) -> [u8; 32] {
        let mut arr = [0; 32];

        let blake2b_hash = blake2b::blake2b(arr.len(), &[], &self.bytes());
        arr[..].copy_from_slice(blake2b_hash.as_bytes());

        arr
    }

    pub fn hash_hex(&self) -> String {
        hex::encode(self.hash())
    }
}

#[derive(Debug, derive_more::From)]
pub enum MetadataParseError {
    Codec(scale_info::scale::Error),
    FromHex(hex::FromHexError),
    InvalidMetadata,
    UnsupportedLanguageId(u8),
    UnsupportedVersion(u16),
}

pub trait Metadata {
    type Init: Types;
    type Handle: Types;
    type Reply: Type;
    type Others: Types;
    type Signal: Type;
    type State: Type;

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
