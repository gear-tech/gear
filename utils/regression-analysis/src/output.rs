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

use crate::stats::{average, median, std_dev};
use std::borrow::Cow;
use thousands::Separable;

#[derive(Debug)]
pub struct Test {
    pub name: String,
    pub current_time: u64,
    pub median: u64,
    pub average: u64,
    pub std_dev: u64,
    pub quartile_lower: u64,
    pub quartile_upper: u64,
    pub min: u64,
    pub max: u64,
}

impl Test {
    pub fn new_for_stats(name: String, time: f64, times: &mut [u64]) -> Self {
        let mut this = Self::new_for_github(name, times);
        this.current_time = (1_000_000_000.0 * time) as u64;
        this
    }

    pub fn new_for_github(name: String, times: &mut [u64]) -> Self {
        // this is necessary as the order may be wrong after deserialization
        times.sort_unstable();

        let len = times.len();
        let len_remainder = len % 2;
        let quartile_lower = median(&times[..len / 2]);
        let quartile_upper = median(&times[len / 2 + len_remainder..]);
        let median = median(times);
        let average = average(times);
        let std_dev = std_dev(times);

        Self {
            name,
            current_time: average,
            median,
            average,
            std_dev,
            quartile_lower,
            quartile_upper,
            min: *times.first().unwrap(),
            max: *times.last().unwrap(),
        }
    }
}

impl tabled::Tabled for Test {
    const LENGTH: usize = 7;

    fn fields(&self) -> Vec<Cow<str>> {
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

        [
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
        .map(Into::into)
        .to_vec()
    }

    fn headers() -> Vec<Cow<'static, str>> {
        [
            "name",
            "current",
            "median",
            "average",
            "lower/upper quartile",
            "min",
            "max",
        ]
        .map(Into::into)
        .to_vec()
    }
}
