#![no_std]

extern crate alloc;

use alloc::string::String;
use blake2_rfc::blake2b;
use codec::Encode;
use core::any;
use scale_info::{MetaType, PortableRegistry, Registry, TypeInfo};

#[derive(Encode, Debug)]
pub struct TypesRepr {
    input: Option<&'static str>,
    output: Option<&'static str>,
}

#[derive(Encode, Debug)]
pub struct MetadataRepr {
    init: TypesRepr,
    handle: TypesRepr,
    reply: TypesRepr,
    others: TypesRepr,
    state: Option<&'static str>,
    registry: PortableRegistry,
}

pub trait Type: TypeInfo + 'static {
    fn eq<T>() -> bool {
        any::type_name::<Self>().eq(any::type_name::<T>())
    }

    fn ne<T>() -> bool {
        !Self::eq::<T>()
    }

    fn repr() -> Option<&'static str> {
        Self::ne::<()>().then(any::type_name::<Self>)
    }

    fn meta_type() -> MetaType {
        MetaType::new::<Self>()
    }
}

impl<T: TypeInfo + 'static> Type for T {}

pub trait Types {
    type Input: Type;
    type Output: Type;

    fn repr() -> TypesRepr {
        let input = Self::Input::repr();
        let output = Self::Output::repr();

        TypesRepr { input, output }
    }

    fn meta_types() -> [MetaType; 2] {
        [
            Self::Input::meta_type(),
            Self::Output::meta_type(),
        ]
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
    pub fn hex(&self) -> String {
        hex::encode(self.encode())
    }

    pub fn hash(&self) -> [u8; 32] {
        let mut arr = [0; 32];

        let blake2b_hash = blake2b::blake2b(arr.len(), &[], &self.encode());
        arr[..].copy_from_slice(blake2b_hash.as_bytes());

        arr
    }

    pub fn hash_hex(&self) -> String {
        hex::encode(self.hash())
    }
}

// TODO: impl for primitives with unreachable call in to_repr
trait Metadata {
    type Init: Types;
    type Handle: Types;
    type Reply: Types;
    type Others: Types;
    type State: Type;

    fn repr() -> MetadataRepr {
        let mut registry = Registry::new();

        registry.register_types(Self::Init::meta_types());
        registry.register_types(Self::Handle::meta_types());
        registry.register_types(Self::Reply::meta_types());
        registry.register_types(Self::Others::meta_types());
        registry.register_type(&Self::State::meta_type());

        MetadataRepr {
            init: Self::Init::repr(),
            handle: Self::Handle::repr(),
            reply: Self::Reply::repr(),
            others: Self::Others::repr(),
            state: Self::State::repr(),
            registry: registry.into(),
        }
    }
}
