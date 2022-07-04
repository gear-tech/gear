use crate::Method;
use gstd::{exec, msg, ActorId, BTreeMap, Decode, Encode, MessageId, Vec};
use shared::Package;

mod state {
    use gstd::{ActorId, BTreeMap, Vec};
    use shared::{GasMeter, Package};

    pub static mut GAS_METER: GasMeter = GasMeter {
        last_gas_available: 0,
        max_gas_spent: 0,
    };
    pub static mut REGISTRY: BTreeMap<ActorId, Package> = BTreeMap::new();
}

#[gstd::async_main]
async fn main() {
    let method = msg::load::<Method>().expect("Invalid contract method");

    match method {
        Method::Start(pkg) => unsafe {
            state::REGISTRY.insert(msg::source(), pkg);

            // # NOTE
            //
            // // if we dispatch calculation here
            // {
            //     // don't have enough gas to do this
            //     dispatch().await;
            //
            //     // so this is unreachable forever
            //     msg::reply(
            //         state::REGISTRY
            //             .get_mut(&msg::source())
            //             .expect("Calculation not found"),
            //         0,
            //     )
            //     .expect("failed");
            // }
        },
        Method::Refuel => unsafe { dispatch().await },
        // TODO
        //
        // optimize this
        Method::Calculate(mut pkg) => unsafe {
            let _ = msg::reply(pkg.calc(), 0).expect("send reply failed");
        },
    }
}

/// Dispatch calcuation
async unsafe fn dispatch() {
    let mut pkg = state::REGISTRY
        .get_mut(&msg::source())
        .expect("Calculation not found, please run start first.");

    // first check here for saving gas and making `wake` operation standalone
    if pkg.finished() {
        return;
    }

    while state::GAS_METER.spin(exec::gas_available()) {
        let reply: Package = Package::decode(
            &mut msg::send_for_reply(exec::program_id(), Method::Calculate(pkg.clone()), 0)
                .expect("send message failed")
                .await
                .expect("get reply failed")
                .as_ref(),
        )
        .expect("decode package failed");

        *pkg = pkg.clone();

        // second checking finished in loop
        if pkg.finished() {
            // # NOTE
            //
            // if we want to reply on start message
            //
            // we need to pass this result to the start message
            //
            // but this `dispatch` may be executed in `Method::Refuel`.
            msg::reply(pkg.paths.clone(), 0).expect("send reply failed");
            break;
        }
    }
}
