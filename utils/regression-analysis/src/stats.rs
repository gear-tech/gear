/*
 * This file is part of Gear.
 *
 * Copyright (C) 2022 Gear Technologies Inc.
 * SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program. If not, see <https://www.gnu.org/licenses/>.
 */

pub fn median(values: &[u64]) -> u64 {
    assert!(!values.is_empty());

    let len = values.len();
    if len % 2 == 0 {
        let i = len / 2;
        values[i - 1] / 2 + values[i] / 2 + values[i - 1] % 2 + values[i] % 2
    } else {
        values[len / 2]
    }
}

pub fn average(values: &[u64]) -> u64 {
    values.iter().sum::<u64>() / values.len() as u64
}

pub fn std_dev(values: &[u64]) -> u64 {
    let average = average(values);
    let sum = values
        .iter()
        .map(|x| (x.abs_diff(average) as u128).pow(2))
        .sum::<u128>();
    let div = sum / values.len() as u128;
    (div as f64).sqrt() as u64
}
