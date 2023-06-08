use alloc::{
    string::{String, ToString},
    vec::Vec,
};
use parity_scale_codec::{Codec, Decode, Encode};

#[derive(Clone, Debug, Decode, Encode)]
/// Represents argument type for `Call` to be executed in wasm: may be the
/// value itself or the key of variables storage inside program.
pub enum Arg<T: 'static + Clone + Codec> {
    New(T),
    Get(String),
}

impl<T: 'static + Clone + Codec> From<T> for Arg<T> {
    fn from(value: T) -> Self {
        Arg::new(value)
    }
}

impl<T: 'static + Clone + Codec> Arg<T> {
    pub fn new(value: T) -> Self {
        Arg::New(value)
    }

    pub fn new_from<R: Into<T>>(value: R) -> Self {
        value.into().into()
    }

    pub fn get(key: impl AsRef<str>) -> Self {
        Arg::Get(key.as_ref().to_string())
    }
}

impl Arg<Vec<u8>> {
    pub fn bytes(bytes: impl AsRef<[u8]>) -> Self {
        bytes.as_ref().to_vec().into()
    }

    pub fn encoded(encodable: impl Encode) -> Self {
        encodable.encode().into()
    }
}

impl From<[u8; 0]> for Arg<Vec<u8>> {
    fn from(_: [u8; 0]) -> Self {
        Arg::New(Default::default())
    }
}

impl From<[u8; 32]> for Arg<Vec<u8>> {
    fn from(hash: [u8; 32]) -> Self {
        Arg::New(hash.encode())
    }
}

impl From<&'static str> for Arg<Vec<u8>> {
    fn from(key: &'static str) -> Self {
        Self::get(key)
    }
}

impl From<&'static str> for Arg<[u8; 32]> {
    fn from(key: &'static str) -> Self {
        Self::get(key)
    }
}

impl From<&'static str> for Arg<u128> {
    fn from(key: &'static str) -> Self {
        Self::get(key)
    }
}

impl From<&'static str> for Arg<u32> {
    fn from(key: &'static str) -> Self {
        Self::get(key)
    }
}

impl From<&'static str> for Arg<bool> {
    fn from(key: &'static str) -> Self {
        Self::get(key)
    }
}

#[cfg(not(feature = "std"))]
mod wasm {
    use super::*;

    impl<T: 'static + Clone + Codec> Arg<T> {
        pub(crate) fn value(self) -> T {
            match self {
                Self::New(value) => value,
                Self::Get(key) => {
                    let value = unsafe { crate::DATA.get(&key) }
                        .unwrap_or_else(|| panic!("Value in key {key} doesn't exist"));
                    T::decode(&mut value.as_ref())
                        .unwrap_or_else(|_| panic!("Value in key {key} failed decode"))
                }
            }
        }
    }
}
