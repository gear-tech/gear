use crate::Method;
use gstd::{exec, msg, ActorId, BTreeMap, Decode, Encode, MessageId, Vec};
use shared::PackageId;
use types::Package;

#[no_mangle]
extern "C" fn init() {
    unsafe { state::THRESHOLD = Some(msg::load().expect("Invalid threshold.")) };
}

#[no_mangle]
extern "C" fn handle() {
    let threshold = unsafe { state::THRESHOLD.expect("Threshold has not been set.") };
    let method = msg::load::<Method>().expect("Invalid contract method.");
    let registry = unsafe { &mut state::REGISTRY };

    match method {
        Method::Start { expected, id, src } => {
            if !registry.contains_key(&id) {
                registry.insert(id, Package::new(expected, src));
            }

            let pkg = registry.get(&id).expect("Calculation not found.");

            if pkg.finished() {
                msg::reply(pkg.result(), 0).expect("send reply failed");
            } else {
                exec::wait();
            }
        }
        // Proxy the `Calculate` method for mocking aggregator && calculator.
        Method::Refuel(id) => {
            msg::send(exec::program_id(), Method::Calculate(id), 0).expect("Send message failed.");
        }
        Method::Calculate(id) => {
            if msg::source() != exec::program_id() {
                panic!("Invalid caller, this is a private method reserved for the program itself.");
            }

            let pkg = registry
                .get_mut(&id)
                .expect("Calculation not found, please run start first.");

            // First check here for saving gas and making `wake` operation standalone.
            if pkg.finished() {
                return;
            }

            while exec::gas_available() > threshold {
                pkg.calc();

                // Second checking if finished in `Method::Calculate`.
                if pkg.finished() {
                    pkg.wake();
                    return;
                }
            }
        }
    }
}

mod state {
    use super::types::Package;
    use gstd::{ActorId, BTreeMap, Vec};
    use shared::PackageId;

    pub static mut THRESHOLD: Option<u64> = None;
    pub static mut REGISTRY: BTreeMap<PackageId, Package> = BTreeMap::new();
}

mod types {
    use gstd::{exec, msg, MessageId};

    /// Package with counter
    pub struct Package {
        /// Expected calculation times.
        pub expected: u128,
        /// Id of the start message.
        pub message_id: MessageId,
        /// The calculation package.
        pub package: shared::Package,
    }

    impl Package {
        /// New package.
        pub fn new(expected: u128, src: [u8; 32]) -> Self {
            Self {
                expected,
                message_id: msg::id(),
                package: shared::Package::new(src),
            }
        }

        /// Deref `Package::calc`
        pub fn calc(&mut self) {
            self.package.calc();
        }

        /// Deref `Package::finished`
        ///
        /// Check if calculation is finished.
        pub fn finished(&self) -> bool {
            self.package.finished(self.expected)
        }

        /// Wake the start message.
        pub fn wake(&self) {
            exec::wake(self.message_id);
        }

        /// The result of calculation.
        pub fn result(&self) -> [u8; 32] {
            self.package.result
        }
    }
}
