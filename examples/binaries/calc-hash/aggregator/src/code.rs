use gstd::{exec, msg, ActorId, Decode};
use shared::{OverBlocksMethod, Package, PackageId};
use types::Method;

mod state {
    use gstd::{ActorId, BTreeMap};
    use shared::{Calculators, PackageId};

    pub static mut REGISTRY: BTreeMap<PackageId, ActorId> = BTreeMap::new();
    pub static mut CALCULATORS: Option<Calculators<ActorId>> = None;
}

mod types {
    use gstd::ActorId;

    pub type Method = shared::Method<ActorId>;
}

#[gstd::async_main]
async fn main() {
    let method = msg::load::<Method>().expect("Invalid contract method.");

    unsafe {
        match method {
            Method::Start(pkg) => start(pkg).await,
            Method::Refuel(id) => refuel(id),
            Method::ForceInOneBlock(_) => {}
            Method::SetCalculators(calculators) => state::CALCULATORS = Some(calculators),
        }
    }
}

async unsafe fn start(pkg: Package) {
    let calculators = state::CALCULATORS.clone().expect("Calculators not found.");
    state::REGISTRY.insert(pkg.id, msg::source());

    let pkg = Package::decode(
        &mut msg::send_for_reply(calculators.over_blocks, OverBlocksMethod::Start(pkg), 0)
            .expect("send package to calculator failed")
            .await
            .expect("get reply failed")
            .as_ref(),
    )
    .expect("Decode reply failed");

    let src = state::REGISTRY
        .remove(&pkg.id)
        .expect("Package has not been registered.");

    msg::send(src, pkg, 0).expect("send message failed");
}

unsafe fn refuel(id: PackageId) {
    let calculators = state::CALCULATORS.clone().expect("Calculators not found.");

    msg::send(calculators.over_blocks, OverBlocksMethod::Refuel(id), 0);
}
