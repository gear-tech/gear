use crate::Arg;
use alloc::{
    string::{String, ToString},
    vec::Vec,
};
use core::fmt::Debug;
use parity_scale_codec::{Decode, Encode};

#[derive(Clone, Debug, Decode, Encode)]
pub enum Call {
    Vec(Vec<u8>),
    Store(String),
    StoreVec(String),
    Source,
    Send(Arg<[u8; 32]>, Arg<Vec<u8>>, Option<u64>, u128, u32),
    Reply(Arg<Vec<u8>>, Option<u64>, u128),
    Panic(Option<String>),
}

impl Call {
    pub fn vec(value: impl AsRef<[u8]>) -> Self {
        Self::Vec(value.as_ref().to_vec())
    }

    pub fn store(key: impl AsRef<str>) -> Self {
        Call::Store(key.as_ref().to_string())
    }

    pub fn store_vec(key: impl AsRef<str>) -> Self {
        Call::StoreVec(key.as_ref().to_string())
    }

    pub const fn source() -> Self {
        Call::Source
    }

    pub fn send(destination: Arg<[u8; 32]>, payload: Arg<Vec<u8>>) -> Self {
        Call::Send(destination, payload, None, 0, 0)
    }

    pub fn send_wgas<T: TryInto<u64>>(
        destination: Arg<[u8; 32]>,
        payload: Arg<Vec<u8>>,
        gas_limit: T,
    ) -> Self
    where
        T::Error: Debug,
    {
        let gas_limit = gas_limit
            .try_into()
            .expect("Cannot convert given gas limit into `u64`");
        Call::Send(destination, payload, Some(gas_limit), 0, 0)
    }

    pub fn reply(payload: Arg<Vec<u8>>) -> Self {
        Call::Reply(payload, None, 0)
    }

    pub fn reply_wgas<T: TryInto<u64>>(payload: Arg<Vec<u8>>, gas_limit: T) -> Self
    where
        T::Error: Debug,
    {
        let gas_limit = gas_limit
            .try_into()
            .expect("Cannot convert given gas limit into `u64`");
        Call::Reply(payload, Some(gas_limit), 0)
    }

    pub fn panic(message: impl Into<Option<&'static str>>) -> Self {
        Self::Panic(message.into().map(ToString::to_string))
    }
}

#[cfg(not(feature = "std"))]
mod wasm {
    use super::*;
    use crate::DATA;
    use gstd::{debug, msg, String, Vec};

    type CallResult = (Call, Option<Vec<u8>>);

    impl Call {
        fn vec_impl(self) -> Option<Vec<u8>> {
            let Self::Vec(v) = self else { unreachable!() };

            Some(v)
        }

        fn store_inner_impl(
            self,
            key: String,
            previous: Option<CallResult>,
            extra_encode: bool,
        ) -> Option<Vec<u8>> {
            let (call, value) = previous.unwrap_or_else(|| {
                panic!("Call <{self:?}> couldn't be called without previous call")
            });

            let value = value.unwrap_or_else(|| {
                panic!("Call <{self:?}> couldn't be called after no-output call <{call:?}>")
            });

            let value = extra_encode.then(|| value.encode()).unwrap_or(value);

            debug!(
                "\t[CONSTRUCTOR] >> Storing {:?}: {:?}",
                key,
                &value[extra_encode as usize..]
            );

            unsafe { DATA.insert(key, value) };

            None
        }

        fn store_impl(self, previous: Option<CallResult>) -> Option<Vec<u8>> {
            let Self::Store(key) = self.clone() else { unreachable!() };

            self.store_inner_impl(key, previous, false)
        }

        fn store_vec_impl(self, previous: Option<CallResult>) -> Option<Vec<u8>> {
            let Self::StoreVec(key) = self.clone() else { unreachable!() };

            self.store_inner_impl(key, previous, true)
        }

        fn source_impl(self) -> Option<Vec<u8>> {
            (!matches!(self, Self::Source)).then(|| unreachable!());

            Some(msg::source().encode())
        }

        fn panic_impl(self) -> ! {
            let Self::Panic(msg) = self else { unreachable!() };

            if let Some(msg) = msg {
                panic!("{msg}");
            } else {
                panic!();
            }
        }

        fn send_impl(self) -> Option<Vec<u8>> {
            let Self::Send(destination, payload, gas_limit, value, delay) = self else { unreachable!() };

            let destination = destination.value().into();
            let payload = payload.value();

            let res = if let Some(gas_limit) = gas_limit {
                msg::send_bytes_with_gas_delayed(destination, payload, gas_limit, value, delay)
            } else {
                msg::send_bytes_delayed(destination, payload, value, delay)
            };

            let message_id = res.expect("Failed to send message");

            Some(message_id.encode())
        }

        fn reply_impl(self) -> Option<Vec<u8>> {
            let Self::Reply(payload, gas_limit, value) = self else { unreachable!() };

            let payload = payload.value();

            let res = if let Some(gas_limit) = gas_limit {
                msg::reply_bytes_with_gas(payload, gas_limit, value)
            } else {
                msg::reply_bytes(payload, value)
            };

            let message_id = res.expect("Failed to send reply");

            Some(message_id.encode())
        }

        pub(crate) fn process(self, previous: Option<CallResult>) -> CallResult {
            debug!("\t[CONSTRUCTOR] >> Processing {:?}", self);
            let call = self.clone();

            let value = match self {
                Call::Vec(..) => self.vec_impl(),
                Call::Store(..) => self.store_impl(previous),
                Call::StoreVec(..) => self.store_vec_impl(previous),
                Call::Source => self.source_impl(),
                Call::Panic(..) => self.panic_impl(),
                Call::Send(..) => self.send_impl(),
                Call::Reply(..) => self.reply_impl(),
            };

            (call, value)
        }
    }
}
