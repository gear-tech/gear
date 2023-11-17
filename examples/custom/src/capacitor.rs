// This file is part of Gear.

// Copyright (C) 2023 Gear Technologies Inc.
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

use crate::Program;
use gstd::{any::Any, debug, msg, prelude::*, str::FromStr, String, Vec};

#[derive(Default)]
pub(crate) struct Capacitor {
    charge: u32,
    limit: u32,
    discharge_history: Vec<u32>,
}

impl Program for Capacitor {
    fn init(a: Box<dyn Any>) -> Self {
        let limit =
            u32::from_str(a.downcast::<String>().unwrap().as_ref()).expect("Invalid number");

        debug!("Init capacitor with limit capacity {limit}");
        Self {
            charge: 0,
            limit,
            discharge_history: Vec::new(),
        }
    }

    fn handle(&mut self) {
        let new_msg = String::from_utf8(msg::load_bytes().expect("Failed to load payload bytes"))
            .expect("Invalid message: should be utf-8");
        let to_add = u32::from_str(new_msg.as_ref()).expect("Invalid number");

        self.charge += to_add;
        debug!("Charge capacitor with {to_add}, new charge {}", self.charge);
        if self.charge >= self.limit {
            debug!("Discharge #{} due to limit {}", self.charge, self.limit);
            msg::send_bytes(msg::source(), format!("Discharged: {}", self.charge), 0).unwrap();
            self.discharge_history.push(self.charge);
            self.charge = 0;
        }
    }
}
