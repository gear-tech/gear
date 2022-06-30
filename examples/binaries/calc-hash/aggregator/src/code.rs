use gstd::{exec, msg, ActorId, Decode, MessageId, Vec};
use shared::{Method, Package};

#[no_mangle]
pub unsafe extern "C" fn init() {
    (state::THRESHOLD, state::CALCULATOR) =
        msg::load::<(u64, ActorId)>().expect("invalid calculator address");
}

#[gstd::async_main]
async fn main() {
    let method = msg::load::<Method>().expect("Invalid contract method");

    match method {
        Method::Start(mut pkg) => unsafe {
            state::STATUS = pkg;
            dispatch().await;
        },
        Method::Refuel => unsafe { dispatch().await },
    }
}

/// Dispatch calcuation
async unsafe fn dispatch() {
    loop {
        let gas_available = exec::gas_available();
        if gas_available < state::THRESHOLD {
            return;
        }

        let reply: Package = Package::decode(
            &mut msg::send_for_reply(state::CALCULATOR, state::STATUS.clone(), 0)
                .expect("send message failed")
                .await
                .expect("get reply failed")
                .as_ref(),
        )
        .expect("decode package failed");

        if reply.finished() {
            msg::reply(reply.paths, 0).expect("send reply failed");
            return;
        }

        state::STATUS = reply;
    }
}

mod state {
    use gstd::{ActorId, Vec};
    use shared::Package;

    pub static mut CALCULATOR: ActorId = ActorId::new([0; 32]);
    pub static mut THRESHOLD: u64 = 0;
    pub static mut STATUS: Package = Package {
        paths: Vec::new(),
        expected: [0; 32],
    };
}
