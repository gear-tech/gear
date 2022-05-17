// This file is part of Gear.

// Copyright (C) 2021-2022 Gear Technologies Inc.
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

#[derive(Debug)]
pub struct Test {
    pub name: String,
    pub master_time: f64,
    pub current_time: f64,
}

impl tabled::Tabled for Test {
    const LENGTH: usize = 3;

    fn fields(&self) -> Vec<String> {
        let current = self.current_time;
        let master = self.master_time;

        let percent = 100.0 * (current - master) / master;

        vec![
            self.name.clone(),
            format!("{}; {:+.2}%", current, percent),
            master.to_string(),
        ]
    }

    fn headers() -> Vec<String> {
        vec!["name".to_owned(), "current".to_owned(), "master".to_owned()]
    }
}
