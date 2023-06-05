use crate::Arg;
use alloc::{boxed::Box, string::String, vec::Vec};
use parity_scale_codec::{Decode, Encode};

#[derive(Clone, Debug, Decode, Encode)]
/// Represents wasm instruction the should be executed with given parameters.
pub enum Call {
    Bool(bool),
    CreateProgram(
        Arg<[u8; 32]>,
        Arg<Vec<u8>>,
        Arg<Vec<u8>>,
        Option<Arg<u64>>,
        Arg<u128>,
        Arg<u32>,
    ),
    Vec(Vec<u8>),
    Store(String),
    StoreVec(String),
    Source,
    StatusCode,
    Value,
    Send(
        Arg<[u8; 32]>,
        Arg<Vec<u8>>,
        Option<Arg<u64>>,
        Arg<u128>,
        Arg<u32>,
    ),
    Reply(Arg<Vec<u8>>, Option<Arg<u64>>, Arg<u128>),
    Panic(Option<String>),
    Exit(Arg<[u8; 32]>),
    BytesEq(Arg<Vec<u8>>, Arg<Vec<u8>>),
    Noop,
    IfElse(Arg<bool>, Box<Self>, Box<Self>),
    Load,
    LoadBytes,
}

#[cfg(not(feature = "std"))]
mod wasm {
    use super::*;
    use crate::DATA;
    use gstd::{debug, exec, msg, prog, String, Vec};

    type CallResult = (Call, Option<Vec<u8>>);

    impl Call {
        fn bool(self) -> Option<Vec<u8>> {
            let Self::Bool(b) = self else { unreachable!() };

            Some(b.encode())
        }

        // TODO: expand to be able store mid and pid separately.
        fn create_program(self) -> Option<Vec<u8>> {
            let Self::CreateProgram(code_id, salt, payload, gas_limit, value, delay) = self else { unreachable!() };

            let code_id = code_id.value().into();
            let salt = salt.value();
            let payload = payload.value();
            let value = value.value();
            let delay = delay.value();

            let res = if let Some(gas_limit) = gas_limit {
                prog::create_program_with_gas_delayed(
                    code_id,
                    salt,
                    payload,
                    gas_limit.value(),
                    value,
                    delay,
                )
            } else {
                prog::create_program_delayed(code_id, salt, payload, value, delay)
            };

            let (_message_id, program_id) = res.expect("Failed to create program");

            Some(program_id.encode())
        }

        fn vec(self) -> Option<Vec<u8>> {
            let Self::Vec(v) = self else { unreachable!() };

            Some(v)
        }

        fn store_impl(
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

        fn store(self, previous: Option<CallResult>) -> Option<Vec<u8>> {
            let Self::Store(key) = self.clone() else { unreachable!() };

            self.store_impl(key, previous, false)
        }

        fn store_vec(self, previous: Option<CallResult>) -> Option<Vec<u8>> {
            let Self::StoreVec(key) = self.clone() else { unreachable!() };

            self.store_impl(key, previous, true)
        }

        fn source(self) -> Option<Vec<u8>> {
            (!matches!(self, Self::Source)).then(|| unreachable!());

            Some(msg::source().encode())
        }

        fn status_code(self) -> Option<Vec<u8>> {
            (!matches!(self, Self::StatusCode)).then(|| unreachable!());

            Some(
                msg::status_code()
                    .expect("Failed to get status code")
                    .encode(),
            )
        }

        fn panic(self) -> ! {
            let Self::Panic(msg) = self else { unreachable!() };

            if let Some(msg) = msg {
                panic!("{msg}");
            } else {
                panic!();
            }
        }

        fn send(self) -> Option<Vec<u8>> {
            let Self::Send(destination, payload, gas_limit, value, delay) = self else { unreachable!() };

            let destination = destination.value().into();
            let payload = payload.value();
            let value = value.value();
            let delay = delay.value();

            let res = if let Some(gas_limit) = gas_limit {
                msg::send_bytes_with_gas_delayed(
                    destination,
                    payload,
                    gas_limit.value(),
                    value,
                    delay,
                )
            } else {
                msg::send_bytes_delayed(destination, payload, value, delay)
            };

            let message_id = res.expect("Failed to send message");

            Some(message_id.encode())
        }

        fn reply(self) -> Option<Vec<u8>> {
            let Self::Reply(payload, gas_limit, value) = self else { unreachable!() };

            let payload = payload.value();
            let value = value.value();

            let res = if let Some(gas_limit) = gas_limit {
                msg::reply_bytes_with_gas(payload, gas_limit.value(), value)
            } else {
                msg::reply_bytes(payload, value)
            };

            let message_id = res.expect("Failed to send reply");

            Some(message_id.encode())
        }

        fn exit(self) -> ! {
            let Self::Exit(inheritor) = self else { unreachable!() };

            let inheritor = inheritor.value().into();

            exec::exit(inheritor)
        }

        fn bytes_eq(self) -> Option<Vec<u8>> {
            let Self::BytesEq(left, right) = self else { unreachable!() };

            let left = left.value();
            let right = right.value();

            Some((left == right).encode())
        }

        fn if_else(self, previous: Option<CallResult>) -> Option<Vec<u8>> {
            let Self::IfElse(flag, true_call, false_call) = self else { unreachable!() };

            let flag = flag.value();

            let call = if flag { true_call } else { false_call };

            let (_call, value) = call.process(previous);

            value
        }

        fn value(self) -> Option<Vec<u8>> {
            (!matches!(self, Self::Value)).then(|| unreachable!());

            Some(msg::value().encode())
        }

        fn load(self) -> Option<Vec<u8>> {
            (!matches!(self, Self::Load)).then(|| unreachable!());

            Some(msg::load_bytes().expect("Failed to load bytes").encode())
        }

        fn load_bytes(self) -> Option<Vec<u8>> {
            (!matches!(self, Self::LoadBytes)).then(|| unreachable!());

            Some(msg::load_bytes().expect("Failed to load bytes"))
        }

        pub(crate) fn process(self, previous: Option<CallResult>) -> CallResult {
            debug!("\t[CONSTRUCTOR] >> Processing {:?}", self);
            let call = self.clone();

            let value = match self {
                Call::Bool(..) => self.bool(),
                Call::CreateProgram(..) => self.create_program(),
                Call::Vec(..) => self.vec(),
                Call::Store(..) => self.store(previous),
                Call::StoreVec(..) => self.store_vec(previous),
                Call::Source => self.source(),
                Call::StatusCode => self.status_code(),
                Call::Panic(..) => self.panic(),
                Call::Send(..) => self.send(),
                Call::Reply(..) => self.reply(),
                Call::Exit(..) => self.exit(),
                Call::BytesEq(..) => self.bytes_eq(),
                Call::Noop => None,
                Call::IfElse(..) => self.if_else(previous),
                Call::Value => self.value(),
                Call::Load => self.load(),
                Call::LoadBytes => self.load_bytes(),
            };

            (call, value)
        }
    }
}
