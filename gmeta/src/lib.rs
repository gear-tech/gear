#![no_std]

extern crate alloc;

#[cfg(feature = "codegen")]
pub use gmeta_codegen::metawasm;

use alloc::{string::String, vec::Vec};
use blake2_rfc::blake2b;
use codec::Encode;
use core::any::TypeId;
use scale_info::{MetaType, PortableRegistry, Registry, TypeInfo};

#[derive(Encode, Debug)]
pub struct TypesRepr {
    input: Option<u32>,
    output: Option<u32>,
}

#[derive(Encode, Debug)]
pub struct MetadataRepr {
    init: TypesRepr,
    handle: TypesRepr,
    reply: TypesRepr,
    others: TypesRepr,
    state: Option<u32>,
    registry: Vec<u8>,
}

pub trait Type: TypeInfo + 'static {
    fn is_unit() -> bool {
        TypeId::of::<Self>().eq(&TypeId::of::<()>())
    }

    fn meta_type() -> MetaType {
        MetaType::new::<Self>()
    }

    fn register(registry: &mut Registry) -> Option<u32> {
        (!Self::is_unit()).then(|| registry.register_type(&Self::meta_type()).id())
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
        self.encode()
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

pub trait Metadata {
    type Init: Types;
    type Handle: Types;
    type Reply: Types;
    type Others: Types;
    type State: Type;

    fn repr() -> MetadataRepr {
        let mut registry = Registry::new();

        MetadataRepr {
            init: Self::Init::register(&mut registry),
            handle: Self::Handle::register(&mut registry),
            reply: Self::Reply::register(&mut registry),
            others: Self::Others::register(&mut registry),
            state: Self::State::register(&mut registry),
            registry: PortableRegistry::from(registry).encode(),
        }
    }
}
