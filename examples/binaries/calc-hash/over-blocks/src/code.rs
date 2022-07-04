use gstd::{exec, msg, ActorId, BTreeMap, Decode, Encode, MessageId, Vec};
use shared::{OverBlocksMethod as Method, Package, PackageId};

mod state {
    use gstd::{ActorId, BTreeMap, Vec};
    use shared::{GasMeter, Package, PackageId};

    pub static mut AGGREGATOR: Option<ActorId> = None;
    pub static mut GAS_METER: GasMeter = GasMeter {
        last_gas_available: 0,
        max_gas_spent: 0,
    };
    pub static mut REGISTRY: BTreeMap<PackageId, Package> = BTreeMap::new();
}

#[no_mangle]
unsafe extern "C" fn init() {
    let aggregator = msg::load::<ActorId>().expect("Invalid aggregator Id");

    state::AGGREGATOR = Some(aggregator);
}

#[gstd::async_main]
async fn main() {
    unsafe {
        if state::AGGREGATOR != Some(msg::source()) {
            panic!("Invalid caller");
        }

        let method = msg::load::<Method>().expect("Invalid contract method");

        match method {
            Method::Start(pkg) => start(pkg),
            Method::Refuel(id) => refuel(id),
        }
    }
}

unsafe fn start(pkg: Package) {
    if state::REGISTRY.get(&pkg.id).is_some() {
        panic!("Package id is taken.");
    }

    state::REGISTRY.insert(pkg.id, pkg.into());
}

/// Dispatch calcuation
unsafe fn refuel(id: PackageId) {
    let mut pkg = state::REGISTRY
        .get_mut(&id)
        .expect("Calculation not found, please run start first.");

    // gstd::debug!("refuel pkg: {:?}", &pkg);

    // First check here for saving gas and making `wake` operation standalone.
    if pkg.finished() {
        return;
    }

    while state::GAS_METER.spin(exec::gas_available()) {
        *pkg = pkg.clone().calc();

        // second checking finished in loop
        if pkg.finished() {
            gstd::debug!("go reply");
            msg::send(pkg.paths.clone(), 0).expect("send reply failed");
            break;
        }
    }
}
