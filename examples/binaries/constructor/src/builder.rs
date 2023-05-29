use crate::{Arg, Call};
use alloc::{boxed::Box, string::ToString, vec::Vec};
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

    pub fn push_many<const N: usize>(mut self, calls: [Call; N]) -> Self {
        self.0.extend(calls.into_iter());
        self
    }

    pub fn vec(self, key: impl AsRef<str>, value: impl AsRef<[u8]>) -> Self {
        self.push(Call::Vec(value.as_ref().to_vec())).store_vec(key)
    }

    pub fn store(self, key: impl AsRef<str>) -> Self {
        self.push(Call::Store(key.as_ref().to_string()))
    }

    pub fn store_vec(self, key: impl AsRef<str>) -> Self {
        self.push(Call::StoreVec(key.as_ref().to_string()))
    }

    pub fn source(self, key: impl AsRef<str>) -> Self {
        self.push(Call::Source).store(key)
    }

    pub fn value(self, key: impl AsRef<str>) -> Self {
        self.push(Call::Value).store(key)
    }

    pub fn value_as_vec(self, key: impl AsRef<str>) -> Self {
        self.push(Call::Value).store_vec(key)
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

    pub fn bytes_eq(
        self,
        key: impl AsRef<str>,
        left: impl Into<Arg<Vec<u8>>>,
        right: impl Into<Arg<Vec<u8>>>,
    ) -> Self {
        self.push(Call::BytesEq(left.into(), right.into()))
            .store(key)
    }

    pub fn noop(self) -> Self {
        self.push(Call::Noop)
    }

    // TODO: support multiple calls for branches by passing mut ref instead of moving value in Call processing.
    pub fn if_else(self, key: impl AsRef<str>, mut true_call: Self, mut false_call: Self) -> Self {
        if true_call.len() != 1 || false_call.len() != 1 {
            unimplemented!()
        };

        let true_call = true_call.0.remove(0);
        let false_call = false_call.0.remove(0);

        self.push(Call::IfElse(
            Arg::get(key),
            Box::new(true_call),
            Box::new(false_call),
        ))
    }
}
