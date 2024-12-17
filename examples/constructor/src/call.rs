use crate::Arg;
use alloc::{string::String, vec::Vec};
use gstd::prelude::*;
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
    ReplyDeposit(Arg<[u8; 32]>, Arg<u64>),
    Vec(Vec<u8>),
    Store(String),
    StoreVec(String),
    Source,
    ReplyCode,
    Value,
    ValueAvailable,
    ReservationSend(
        Arg<[u8; 32]>,
        Arg<[u8; 32]>,
        Arg<Vec<u8>>,
        Arg<u128>,
        Arg<u32>,
    ),
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
    IfElse(Arg<bool>, Vec<Self>, Vec<Self>),
    Load,
    LoadBytes,
    Wait,
    WaitFor(Arg<u32>),
    Wake(Arg<[u8; 32]>),
    MessageId,
    Loop,
    ReserveGas(Arg<u64>, Arg<u32>),
    UnreserveGas(Arg<[u8; 32]>),
    SystemReserveGas(Arg<u64>),
    WriteN(Arg<u64>),
}

#[cfg(not(feature = "wasm-wrapper"))]
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
            let Self::CreateProgram(code_id, salt, payload, gas_limit, value, delay) = self else {
                unreachable!()
            };

            let code_id = code_id.value().into();
            let salt = salt.value();
            let payload = payload.value();
            let value = value.value();
            let delay = delay.value();

            let res = if let Some(gas_limit) = gas_limit {
                prog::create_program_bytes_with_gas_delayed(
                    code_id,
                    salt,
                    payload,
                    gas_limit.value(),
                    value,
                    delay,
                )
            } else {
                prog::create_program_bytes_delayed(code_id, salt, payload, value, delay)
            };

            let (_message_id, program_id) = res.expect("Failed to create program");

            Some(program_id.encode())
        }

        fn reply_deposit(self) -> Option<Vec<u8>> {
            let Self::ReplyDeposit(message_id, gas_limit) = self else {
                unreachable!()
            };

            let message_id = message_id.value().into();
            let gas_limit = gas_limit.value();

            exec::reply_deposit(message_id, gas_limit).expect("Failed to deposit reply");

            None
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
                "\t[CONSTRUCTOR] >> Storing {key:?}: {:?}",
                &value[extra_encode as usize..]
            );

            unsafe { static_mut!(DATA).insert(key, value) };

            None
        }

        fn store(self, previous: Option<CallResult>) -> Option<Vec<u8>> {
            let Self::Store(key) = self.clone() else {
                unreachable!()
            };

            self.store_impl(key, previous, false)
        }

        fn store_vec(self, previous: Option<CallResult>) -> Option<Vec<u8>> {
            let Self::StoreVec(key) = self.clone() else {
                unreachable!()
            };

            self.store_impl(key, previous, true)
        }

        fn source(self) -> Option<Vec<u8>> {
            (!matches!(self, Self::Source)).then(|| unreachable!());

            Some(msg::source().encode())
        }

        fn reply_code(self) -> Option<Vec<u8>> {
            (!matches!(self, Self::ReplyCode)).then(|| unreachable!());

            Some(
                msg::reply_code()
                    .expect("Failed to get reply code")
                    .encode(),
            )
        }

        fn panic(self) -> ! {
            let Self::Panic(msg) = self else {
                unreachable!()
            };

            if let Some(msg) = msg {
                panic!("{msg}");
            } else {
                panic!();
            }
        }

        fn reservation_send(self) -> Option<Vec<u8>> {
            let Self::ReservationSend(reservation, destination, payload, value, delay) = self
            else {
                unreachable!()
            };

            let reservation = reservation.value().into();
            let destination = destination.value().into();
            let payload = payload.value();
            let value = value.value();
            let delay = delay.value();

            let message_id =
                msg::send_delayed_from_reservation(reservation, destination, payload, value, delay)
                    .expect("Failed to send message from reservation");

            Some(message_id.encode())
        }

        fn send(self) -> Option<Vec<u8>> {
            let Self::Send(destination, payload, gas_limit, value, delay) = self else {
                unreachable!()
            };

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
            let Self::Reply(payload, gas_limit, value) = self else {
                unreachable!()
            };

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
            let Self::Exit(inheritor) = self else {
                unreachable!()
            };

            let inheritor = inheritor.value().into();

            exec::exit(inheritor)
        }

        fn bytes_eq(self) -> Option<Vec<u8>> {
            let Self::BytesEq(left, right) = self else {
                unreachable!()
            };

            let left = left.value();
            let right = right.value();

            Some((left == right).encode())
        }

        fn if_else(self, mut previous: Option<CallResult>) -> Option<Vec<u8>> {
            let Self::IfElse(flag, true_calls, false_calls) = self else {
                unreachable!()
            };

            let flag = flag.value();

            let calls = if flag { true_calls } else { false_calls };

            for call in calls {
                previous = Some(call.process(previous));
            }

            previous.and_then(|res| res.1)
        }

        fn value(self) -> Option<Vec<u8>> {
            (!matches!(self, Self::Value)).then(|| unreachable!());

            Some(msg::value().encode())
        }

        fn value_available(self) -> Option<Vec<u8>> {
            (!matches!(self, Self::ValueAvailable)).then(|| unreachable!());

            Some(exec::value_available().encode())
        }

        fn load(self) -> Option<Vec<u8>> {
            (!matches!(self, Self::Load)).then(|| unreachable!());

            Some(msg::load_bytes().expect("Failed to load bytes").encode())
        }

        fn load_bytes(self) -> Option<Vec<u8>> {
            (!matches!(self, Self::LoadBytes)).then(|| unreachable!());

            Some(msg::load_bytes().expect("Failed to load bytes"))
        }

        fn wait(self) -> ! {
            (!matches!(self, Self::Wait)).then(|| unreachable!());

            exec::wait()
        }

        fn wait_for(self) -> ! {
            let Self::WaitFor(duration) = self else {
                unreachable!()
            };

            let duration = duration.value();

            exec::wait_for(duration)
        }

        fn wake(self) -> Option<Vec<u8>> {
            let Self::Wake(message_id) = self else {
                unreachable!()
            };

            let message_id = message_id.value().into();

            exec::wake(message_id).expect("Failed to wake message");

            None
        }

        fn message_id(self) -> Option<Vec<u8>> {
            (!matches!(self, Self::MessageId)).then(|| unreachable!());

            Some(msg::id().encode())
        }

        fn system_reserve_gas(self) -> Option<Vec<u8>> {
            let Self::SystemReserveGas(gas) = self else {
                unreachable!()
            };

            let gas = gas.value();
            exec::system_reserve_gas(gas).expect("Failed to reserve gas");

            None
        }

        fn reserve_gas(self) -> Option<Vec<u8>> {
            let Self::ReserveGas(amount, duration) = self else {
                unreachable!()
            };

            let amount = amount.value();
            let duration = duration.value();
            let reservation_id =
                exec::reserve_gas(amount, duration).expect("Failed to reserve gas");

            Some(reservation_id.encode())
        }

        fn unreserve_gas(self) -> Option<Vec<u8>> {
            let Self::UnreserveGas(reservation) = self else {
                unreachable!()
            };

            let reservation = reservation.value().into();
            let unreserved_value =
                exec::unreserve_gas(reservation).expect("Failed to unreserve gas");

            Some(unreserved_value.encode())
        }

        fn write_n(self) -> Option<Vec<u8>> {
            let Self::WriteN(count) = self else {
                unreachable!()
            };

            let end = count.value();
            for i in 0_u64..end {
                unsafe { static_mut!(DATA).insert("last_written_n".into(), i.encode()) };
            }

            None
        }

        pub(crate) fn process(self, previous: Option<CallResult>) -> CallResult {
            debug!("\t[CONSTRUCTOR] >> Processing {self:?}");
            let call = self.clone();

            let value = match self {
                Call::Bool(..) => self.bool(),
                Call::CreateProgram(..) => self.create_program(),
                Call::ReplyDeposit(..) => self.reply_deposit(),
                Call::Vec(..) => self.vec(),
                Call::Store(..) => self.store(previous),
                Call::StoreVec(..) => self.store_vec(previous),
                Call::Source => self.source(),
                Call::ReplyCode => self.reply_code(),
                Call::Panic(..) => self.panic(),
                Call::ReservationSend(..) => self.reservation_send(),
                Call::Send(..) => self.send(),
                Call::Reply(..) => self.reply(),
                Call::Exit(..) => self.exit(),
                Call::BytesEq(..) => self.bytes_eq(),
                Call::Noop => None,
                Call::IfElse(..) => self.if_else(previous),
                Call::Value => self.value(),
                Call::ValueAvailable => self.value_available(),
                Call::Load => self.load(),
                Call::LoadBytes => self.load_bytes(),
                Call::Wait => self.wait(),
                Call::WaitFor(..) => self.wait_for(),
                Call::Wake(..) => self.wake(),
                Call::MessageId => self.message_id(),
                #[allow(clippy::empty_loop)]
                Call::Loop => loop {},
                Call::SystemReserveGas(..) => self.system_reserve_gas(),
                Call::ReserveGas(..) => self.reserve_gas(),
                Call::UnreserveGas(..) => self.unreserve_gas(),
                Call::WriteN(..) => self.write_n(),
            };

            (call, value)
        }
    }
}
