use crate::Method;
use gstd::{exec, msg, ActorId, MessageId, Vec};
use traits::{ReplyGetter, ToReply};

#[no_mangle]
pub unsafe extern "C" fn init() {
    (state::THRESHOLD, state::CALCULATOR) =
        msg::load::<(u64, ActorId)>().expect("invalid calculator address");
}

#[gstd::async_main]
async fn main() {
    let method = msg::load::<types::PowMethod>().expect("Invalid contract method");

    match method {
        Method::Start(mut pkg) => unsafe {
            loop {
                let gas_available = exec::gas_available();
                if gas_available <= state::THRESHOLD {
                    exec::wait();
                }

                let reply = msg::send_with_gas_for_reply(
                    state::CALCULATOR,
                    pkg.to_vec(),
                    0,
                    (gas_available - state::THRESHOLD).into(),
                )
                .expect("send message failed")
                .await
                .expect("get reply failed")
                .to_reply();

                if reply.completed() {
                    msg::reply(reply.result(), 0).expect("send reply failed");
                    return;
                }

                pkg = reply.state();
            }
        },
        Method::Refuel(id) => exec::wake(id),
    }
}

mod state {
    use gstd::{ActorId, Vec};

    pub static mut CALCULATOR: ActorId = ActorId::new([0; 32]);
    pub static mut THRESHOLD: u64 = 0;
}

mod types {
    use crate::Method;
    use gstd::{MessageId, Vec};

    /// Pow reply
    ///
    /// (completed, result, state)
    pub type Reply = (bool, u128, Vec<u8>);

    pub type PowMethod = Method<Vec<u8>, MessageId>;
}

mod traits {
    use super::types::Reply;
    use codec::{Decode, Input};
    use gstd::Vec;

    /// Getter of pow reply
    pub trait ReplyGetter {
        fn completed(&self) -> bool;
        fn result(&self) -> u128;
        fn state(self) -> Vec<u8>;
    }

    impl ReplyGetter for Reply {
        fn completed(&self) -> bool {
            self.0
        }

        fn result(&self) -> u128 {
            self.1
        }

        fn state(self) -> Vec<u8> {
            self.2
        }
    }

    /// Convert `Vec<u8>` to `Reply`
    pub trait ToReply {
        fn to_reply(&self) -> Reply;
    }

    impl ToReply for Vec<u8> {
        fn to_reply(&self) -> Reply {
            Reply::decode(&mut self.as_ref()).expect("decode reply failed")
        }
    }
}
