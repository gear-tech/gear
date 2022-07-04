use crate::Method;
use gstd::{exec, msg, ActorId, BTreeMap, Decode, Encode, MessageId, Vec};
use shared::{Package, PackageId};

mod state {
    use gstd::{ActorId, BTreeMap, Vec};
    use shared::{Package, PackageId};

    pub static mut THRESHOLD: Option<u64> = None;
    pub static mut REGISTRY: BTreeMap<PackageId, Package> = BTreeMap::new();
}

#[no_mangle]
unsafe extern "C" fn init() {
    state::THRESHOLD = Some(msg::load().expect("Invalid threshold."));
}

#[gstd::async_main]
async fn main() {
    let method = msg::load::<Method>().expect("Invalid contract method.");

    unsafe {
        let threshold = state::THRESHOLD.expect("Threshold has not been set.");

        match method {
            Method::Start(pkg_with_id) => {
                state::REGISTRY.insert(pkg_with_id.id, pkg_with_id.package);
            }
            Method::Refuel(id) => dispatch(id).await,
            Method::Calculate(mut pkg) => {
                while exec::gas_available() > threshold {
                    pkg = pkg.calc();

                    if pkg.finished() {
                        break;
                    }
                }

                let _ = msg::reply(pkg, 0).expect("Send reply failed.");
            }
        }
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
            .expect("Send message failed.")
            .await
            .expect("Get reply failed.")
            .as_ref(),
    )
    .expect("Decode package failed.");

    *pkg = reply;

    // second checking finished in `Method::Calculate`
    if pkg.finished() {
        msg::reply(pkg.paths.clone(), 0).expect("Send reply failed.");
    }
}
