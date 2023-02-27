// This file is part of Gear.
//
// Copyright (C) 2021-2022 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

// TODO
//
// rename or remove this module when supporting both gear
// and vara in one build.
#[allow(clippy::all, missing_docs)]
pub mod api {
    include!(concat!(env!("OUT_DIR"), "/metadata.rs"));

    pub use metadata::*;

    #[cfg(any(
        all(feature = "gear", not(feature = "vara")),
        all(feature = "gear", feature = "vara")
    ))]
    pub use metadata::runtime_types::gear_runtime::RuntimeEvent;

    #[cfg(all(feature = "vara", not(feature = "gear")))]
    pub use metadata::runtime_types::vara_runtime::RuntimeEvent;
}

mod impls;
