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
        title: $title:literal,
        input: $type_in:ty,
        output: $type_out:ty,
        init_input: $type_init_in:ty,
        init_output: $type_init_out:ty
    ) => {
        declare!(meta_title, $title);
        declare!(meta_input, $type_in);
        declare!(meta_output, $type_out);
        declare!(meta_init_input, $type_init_in);
        declare!(meta_init_output, $type_init_out);
    };
    (
        title: $title:literal,
        input: $type_in:ty,
        output: $type_out:ty,
        init_input: $type_init_in:ty,
        init_output: $type_init_out:ty,
        extra_types: $($extra:ty), +
    ) => {
        declare!(meta_title, $title);
        declare!(meta_input, $type_in : $($extra), +);
        declare!(meta_output, $type_out: $($extra), +);
        declare!(meta_init_input, $type_init_in: $($extra), +);
        declare!(meta_init_output, $type_init_out: $($extra), +);
    };
}
