// This file is part of Gear.

// Copyright (C) 2021-2023 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! This program runs a hashing computation over several executions.
//!
//! `Init` method gets a u64 `threshold` in the payload, and saves it.
//!
//! `Handle` method gets a [`Method`] in the payload, and it executes some code based on the method.
//!
//! [`Start { expected, id, src }`] uses the given values to make a new [`Package`], saving it in
//! a static `registry`, mapping the `id` to a [`Package`]. We check if the [`Package`] is finished,
//! and if it is, we [`reply()`] with the result in the payload. Otherwise, we [`wait()`], halting execution.
//!
//! [`Refuel(id)`] creates a message which is sent to this program, with the payload being
//! [`Calculate(id)`].
//!
//! [`Calculate(id)`] checks that it has been called from this program, as it is a private method.
//! The [`Package`] is retrieved from the static `registry` based on the `id`. While we have more
//! gas than the `threshold`, we calculate the [`Package`] until it is finished. If out gas goes
//! below the `threshold`, the execution is halted. If the [`Package`] is finished, we [`wake()`] the
//! original message which sent the [`Start { expected, id, src }`].
//!
//! [`Start { expected, id, src }`]: Method::Start
//! [`Refuel(id)`]: Method::Refuel
//! [`Calculate(id)`]: Method::Calculate
//! [`wait()`]: exec::wait
//! [`wake()`]: exec::wake
//! [`reply()`]: msg::reply

use crate::Method;
use gstd::{exec, msg};
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
            registry
                .entry(id)
                .or_insert_with(|| Package::new(expected, src));

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
    use gstd::collections::BTreeMap;
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
            exec::wake(self.message_id).expect("Failed to wake message");
        }

        /// The result of calculation.
        pub fn result(&self) -> [u8; 32] {
            self.package.result
        }
    }
}
