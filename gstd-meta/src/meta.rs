// This file is part of Gear.

// Copyright (C) 2021 Gear Technologies Inc.
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

/// **The `meta!` macro**
#[macro_export]
macro_rules! meta {
    (
        title: $t:literal,
        input: $ti:ty,
        output: $to:ty,
        init_input: $ii:ty,
        init_output: $io:ty
    ) => {
        gstd_meta::declare!(meta_title, $t);
        gstd_meta::declare!(meta_input, $ti);
        gstd_meta::declare!(meta_output, $to);
        gstd_meta::declare!(meta_init_input, $ii);
        gstd_meta::declare!(meta_init_output, $io);
    };
    (
        title: $t:literal,
        input: $ti:ty,
        output: $to:ty,
        init_input: $ii:ty,
        init_output: $io:ty,
        extra: $($x:ty), +
    ) => {
        gstd_meta::declare!(meta_title, $t);
        gstd_meta::declare!(meta_input, $ti, $($x), +);
        gstd_meta::declare!(meta_output, $to, $($x), +);
        gstd_meta::declare!(meta_init_input, $ii, $($x), +);
        gstd_meta::declare!(meta_init_output, $io, $($x), +);
    };
}
