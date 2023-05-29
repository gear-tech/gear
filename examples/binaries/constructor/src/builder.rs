use crate::{Arg, Call};
use alloc::{string::ToString, vec::Vec};
use core::{fmt::Debug, ops::Deref};
use parity_scale_codec::{WrapperTypeDecode, WrapperTypeEncode};

#[derive(Default, Debug, Clone)]
pub struct Calls(Vec<Call>);

impl From<Vec<Call>> for Calls {
    fn from(calls: Vec<Call>) -> Self {
        Self(calls)
    }
}

impl Deref for Calls {
    type Target = Vec<Call>;

    fn deref(&self) -> &Self::Target {
        self.0.as_ref()
    }
}

impl WrapperTypeEncode for Calls {}
impl WrapperTypeDecode for Calls {
    type Wrapped = Vec<Call>;
}

impl Calls {
    pub fn builder() -> Self {
        Default::default()
    }

    pub fn push(mut self, call: Call) -> Self {
        self.0.push(call);
        self
    }

    pub fn vec(self, value: impl AsRef<[u8]>) -> Self {
        self.push(Call::Vec(value.as_ref().to_vec()))
    }

    pub fn store(self, key: impl AsRef<str>) -> Self {
        self.push(Call::Store(key.as_ref().to_string()))
    }

    pub fn store_vec(self, key: impl AsRef<str>) -> Self {
        self.push(Call::StoreVec(key.as_ref().to_string()))
    }

    pub fn source(self) -> Self {
        self.push(Call::Source)
    }

    pub fn send(
        self,
        destination: impl Into<Arg<[u8; 32]>>,
        payload: impl Into<Arg<Vec<u8>>>,
    ) -> Self {
        self.send_value(destination, payload, 0)
    }

    pub fn send_value(
        self,
        destination: impl Into<Arg<[u8; 32]>>,
        payload: impl Into<Arg<Vec<u8>>>,
        value: impl Into<Arg<u128>>,
    ) -> Self {
        self.push(Call::Send(
            destination.into(),
            payload.into(),
            None,
            value.into(),
            0.into(),
        ))
    }

    pub fn send_wgas<T: TryInto<u64>>(
        self,
        destination: impl Into<Arg<[u8; 32]>>,
        payload: impl Into<Arg<Vec<u8>>>,
        gas_limit: T,
    ) -> Self
    where
        T::Error: Debug,
    {
        self.send_value_wgas(destination, payload, gas_limit, 0)
    }

    pub fn send_value_wgas<T: TryInto<u64>>(
        self,
        destination: impl Into<Arg<[u8; 32]>>,
        payload: impl Into<Arg<Vec<u8>>>,
        gas_limit: T,
        value: impl Into<Arg<u128>>,
    ) -> Self
    where
        T::Error: Debug,
    {
        let gas_limit = gas_limit
            .try_into()
            .expect("Cannot convert given gas limit into `u64`");
        self.push(Call::Send(
            destination.into(),
            payload.into(),
            Some(gas_limit),
            value.into(),
            0.into(),
        ))
    }

    pub fn reply(self, payload: impl Into<Arg<Vec<u8>>>) -> Self {
        self.push(Call::Reply(payload.into(), None, 0.into()))
    }

    pub fn reply_wgas<T: TryInto<u64>>(self, payload: impl Into<Arg<Vec<u8>>>, gas_limit: T) -> Self
    where
        T::Error: Debug,
    {
        let gas_limit = gas_limit
            .try_into()
            .expect("Cannot convert given gas limit into `u64`");
        self.push(Call::Reply(payload.into(), Some(gas_limit), 0.into()))
    }

    pub fn panic(self, message: impl Into<Option<&'static str>>) -> Self {
        self.push(Call::Panic(message.into().map(ToString::to_string)))
    }

    pub fn exit(self, inheritor: impl Into<Arg<[u8; 32]>>) -> Self {
        self.push(Call::Exit(inheritor.into()))
    }
}
