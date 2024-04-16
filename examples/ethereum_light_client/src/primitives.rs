use alloc::vec::Vec;
use core::ops::Deref;
#[cfg(feature = "serde")]
use core::str::FromStr;

// use ssz_rs::{prelude::*, Deserialize, Sized};
use ssz_rs::prelude::*;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ByteVector<const N: usize> {
    inner: Vector<u8, N>,
}

impl<const N: usize> ByteVector<N> {
    pub fn as_slice(&self) -> &[u8] {
        self.inner.as_slice()
    }
}

impl<const N: usize> Deref for ByteVector<N> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.inner.as_slice()
    }
}

#[derive(Debug)]
pub struct ErrorDifferentLength;

impl<const N: usize> TryFrom<Vec<u8>> for ByteVector<N> {
    type Error = ErrorDifferentLength;

    fn try_from(value: Vec<u8>) -> core::result::Result<Self, Self::Error> {
        Ok(Self {
            inner: Vector::try_from(value).map_err(|_| ErrorDifferentLength)?,
        })
    }
}

impl<const N: usize> TryFrom<&[u8]> for ByteVector<N> {
    type Error = ErrorDifferentLength;

    fn try_from(value: &[u8]) -> core::result::Result<Self, Self::Error> {
        ByteVector::try_from(value.to_vec())
    }
}

impl<const N: usize> ssz_rs::Merkleized for ByteVector<N> {
    fn hash_tree_root(&mut self) -> core::result::Result<Node, MerkleizationError> {
        self.inner.hash_tree_root()
    }
}

impl<const N: usize> ssz_rs::Sized for ByteVector<N> {
    fn size_hint() -> usize {
        <Vector<u8, N> as ssz_rs::Sized>::size_hint()
    }

    fn is_variable_size() -> bool {
        <Vector<u8, N> as ssz_rs::Sized>::is_variable_size()
    }
}

impl<const N: usize> ssz_rs::Serialize for ByteVector<N> {
    fn serialize(&self, buffer: &mut Vec<u8>) -> core::result::Result<usize, SerializeError> {
        self.inner.serialize(buffer)
    }
}

impl<const N: usize> ssz_rs::Deserialize for ByteVector<N> {
    fn deserialize(encoding: &[u8]) -> core::result::Result<Self, DeserializeError>
    where
        Self: core::marker::Sized,
    {
        Ok(Self {
            inner: Vector::deserialize(encoding)?,
        })
    }
}

impl<const N: usize> ssz_rs::SimpleSerialize for ByteVector<N> {}

#[cfg(feature = "serde")]
impl<'de, const N: usize> serde::Deserialize<'de> for ByteVector<N> {
    fn deserialize<D>(deserializer: D) -> core::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let bytes: &str = serde::Deserialize::deserialize(deserializer)?;
        let bytes = match bytes.starts_with("0x") {
            true => &bytes[2..],
            false => bytes,
        };

        let bytes = hex::decode(bytes)
            .map_err(|e| <D::Error as serde::de::Error>::custom(e))?;
        Ok(Self {
            inner: bytes.to_vec().try_into().map_err(|_| <D::Error as serde::de::Error>::custom("Failed to convert to ByteVector"))?,
        })
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ByteList<const N: usize> {
    inner: List<u8, N>,
}

impl<const N: usize> ByteList<N> {
    pub fn as_slice(&self) -> &[u8] {
        self.inner.as_slice()
    }
}

impl<const N: usize> Deref for ByteList<N> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.inner.as_slice()
    }
}

#[derive(Debug)]
pub struct ErrorGreaterLength;

impl<const N: usize> TryFrom<Vec<u8>> for ByteList<N> {
    type Error = ErrorGreaterLength;

    fn try_from(value: Vec<u8>) -> core::result::Result<Self, Self::Error> {
        Ok(Self {
            inner: List::try_from(value).map_err(|_| ErrorGreaterLength)?,
        })
    }
}

impl<const N: usize> TryFrom<&[u8]> for ByteList<N> {
    type Error = ErrorGreaterLength;

    fn try_from(value: &[u8]) -> core::result::Result<Self, Self::Error> {
        ByteList::try_from(value.to_vec())
    }
}

impl<const N: usize> ssz_rs::Merkleized for ByteList<N> {
    fn hash_tree_root(&mut self) -> core::result::Result<Node, MerkleizationError> {
        self.inner.hash_tree_root()
    }
}

impl<const N: usize> ssz_rs::Sized for ByteList<N> {
    fn size_hint() -> usize {
        <List<u8, N> as ssz_rs::Sized>::size_hint()
    }

    fn is_variable_size() -> bool {
        <List<u8, N> as ssz_rs::Sized>::is_variable_size()
    }
}

impl<const N: usize> ssz_rs::Serialize for ByteList<N> {
    fn serialize(&self, buffer: &mut Vec<u8>) -> core::result::Result<usize, SerializeError> {
        self.inner.serialize(buffer)
    }
}

impl<const N: usize> ssz_rs::Deserialize for ByteList<N> {
    fn deserialize(encoding: &[u8]) -> core::result::Result<Self, DeserializeError>
    where
        Self: core::marker::Sized,
    {
        Ok(Self {
            inner: List::deserialize(encoding)?,
        })
    }
}

impl<const N: usize> ssz_rs::SimpleSerialize for ByteList<N> {}

#[cfg(feature = "serde")]
impl<'de, const N: usize> serde::Deserialize<'de> for ByteList<N> {
    fn deserialize<D>(deserializer: D) -> core::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let bytes: &str = serde::Deserialize::deserialize(deserializer)?;
        let bytes = match bytes.starts_with("0x") {
            true => &bytes[2..],
            false => bytes,
        };

        let bytes = hex::decode(bytes)
            .map_err(|e| <D::Error as serde::de::Error>::custom(e))?;
        Ok(Self {
            inner: bytes.to_vec().try_into().map_err(|_| <D::Error as serde::de::Error>::custom("Failed to convert to ByteList"))?,
        })
    }
}

#[derive(Debug, Copy, Clone, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct U64 {
    inner: u64,
}

impl U64 {
    pub fn as_u64(&self) -> u64 {
        self.inner
    }
}

impl From<U64> for u64 {
    fn from(value: U64) -> Self {
        value.inner
    }
}

impl From<u64> for U64 {
    fn from(value: u64) -> Self {
        Self { inner: value }
    }
}

impl ssz_rs::Merkleized for U64 {
    fn hash_tree_root(&mut self) -> core::result::Result<Node, MerkleizationError> {
        self.inner.hash_tree_root()
    }
}

impl ssz_rs::Sized for U64 {
    fn size_hint() -> usize {
        <u64 as ssz_rs::Sized>::size_hint()
    }

    fn is_variable_size() -> bool {
        <u64 as ssz_rs::Sized>::is_variable_size()
    }
}

impl ssz_rs::Serialize for U64 {
    fn serialize(&self, buffer: &mut Vec<u8>) -> core::result::Result<usize, SerializeError> {
        self.inner.serialize(buffer)
    }
}

impl ssz_rs::Deserialize for U64 {
    fn deserialize(encoding: &[u8]) -> core::result::Result<Self, DeserializeError>
    where
        Self: core::marker::Sized,
    {
        Ok(Self {
            inner: u64::deserialize(encoding)?,
        })
    }
}

impl ssz_rs::SimpleSerialize for U64 {}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for U64 {
    fn deserialize<D>(deserializer: D) -> core::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let val: &str = serde::Deserialize::deserialize(deserializer)?;
        let inner = u64::from_str(val)
            .map_err(|e| <D::Error as serde::de::Error>::custom(e))?;

        Ok(Self { inner })
    }
}
