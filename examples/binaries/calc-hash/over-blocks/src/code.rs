use crate::Method;
use gstd::{exec, msg, ActorId, BTreeMap, Decode, Encode, MessageId, Vec};
use shared::{Package, PackageId};

mod state {
    use gstd::{ActorId, BTreeMap, Vec};
    use shared::{GasMeter, Package, PackageId};

    pub static mut GAS_METER: GasMeter = GasMeter {
        last_gas_available: 0,
        max_gas_spent: 0,
    };
    pub static mut REGISTRY: BTreeMap<PackageId, Package> = BTreeMap::new();
}

#[gstd::async_main]
async fn main() {
    let method = msg::load::<Method>().expect("Invalid contract method");

    match method {
        Method::Start(pkg_with_id) => unsafe {
            state::REGISTRY.insert(pkg_with_id.id, pkg_with_id.package);

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
        Method::Refuel(id) => unsafe { dispatch(id).await },
        Method::Calculate(mut pkg) => unsafe {
            while state::GAS_METER.spin(exec::gas_available()) {
                pkg = pkg.calc();

                if pkg.finished() {
                    break;
                }
            }

            let _ = msg::reply(pkg, 0).expect("send reply failed");
        },
    }
}

/// Dispatch calcuation
async unsafe fn dispatch(id: PackageId) {
    let mut pkg = state::REGISTRY
        .get_mut(&id)
        .expect("Calculation not found, please run start first.");

    // first check here for saving gas and making `wake` operation standalone
    if pkg.finished() {
        return;
    }

    let reply: Package = Package::decode(
        &mut msg::send_for_reply(exec::program_id(), Method::Calculate(pkg.clone()), 0)
            .expect("send message failed")
            .await
            .expect("get reply failed")
            .as_ref(),
    )
    .expect("decode package failed");

    *pkg = reply;

    // second checking finished in `Method::Calculate`
    if pkg.finished() {
        // # NOTE
        //
        // if we want to reply on start message
        //
        // we need to pass this result to the start message
        //
        // but this `dispatch` may be executed in `Method::Refuel`.
        msg::reply(pkg.paths.clone(), 0).expect("send reply failed");
    }
}
