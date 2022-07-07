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

use thousands::Separable;

#[derive(Debug)]
pub struct Test {
    pub name: String,
    pub current_time: u64,
    pub median: u64,
    pub average: u64,
    pub std: u64,
    pub quartile_lower: u64,
    pub quartile_upper: u64,
    pub min: u64,
    pub max: u64,
}

impl tabled::Tabled for Test {
    const LENGTH: usize = 7;

    fn fields(&self) -> Vec<String> {
        let current = self.current_time as f64;
        let median = self.median as f64;

        let percent = 100.0 * (current - median) / median;

        let symbol = if self.current_time < self.quartile_upper {
            ":heavy_check_mark:"
        } else if self.current_time < self.max {
            ":exclamation:"
        } else {
            ":bangbang:"
        };

        vec![
            self.name.clone(),
            format!(
                "{}; {:+.2}% {}",
                current.separate_with_spaces(),
                percent,
                symbol
            ),
            self.median.separate_with_spaces(),
            self.average.separate_with_spaces(),
            format!(
                "({}; {})",
                self.quartile_lower.separate_with_spaces(),
                self.quartile_upper.separate_with_spaces()
            ),
            self.min.separate_with_spaces(),
            self.max.separate_with_spaces(),
        ]
    }

    fn headers() -> Vec<String> {
        vec![
            "name".to_owned(),
            "current".to_owned(),
            "median".to_owned(),
            "average".to_owned(),
            "lower/upper quartile".to_owned(),
            "min".to_owned(),
            "max".to_owned(),
        ]
    }
}
