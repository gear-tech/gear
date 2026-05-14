// This file is part of Gear.
//
// Copyright (C) 2024-2025 Gear Technologies Inc.
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

#[path = "allocator.rs"]
pub(crate) mod allocator_ri;

#[path = "database.rs"]
pub(crate) mod database_ri;

#[path = "logging.rs"]
pub(crate) mod logging_ri;

#[path = "promise.rs"]
pub(crate) mod promise_ri;

pub(crate) mod utils {
    use ethexe_runtime_common::pack_u32_to_i64;

    pub fn repr_ri_slice(slice: impl AsRef<[u8]>) -> i64 {
        let slice = slice.as_ref();

        let ptr = slice.as_ptr() as u32;
        let len = slice.len() as u32;
        pack_u32_to_i64(ptr, len)
    }
}

macro_rules! declare {
    (
        $(
            $(#[$attrs:meta])*
            $vis:vis fn $symbol:ident(
                $($arg_name:ident: $arg_ty:ty),* $(,)?
            ) $(-> $ret_ty:ty)?;
        )*
    ) => {
        mod sys {
            #[allow(unused)]
            use super::*;

            #[allow(improper_ctypes)]
            unsafe extern "C" {
                $(
                    $(#[$attrs])*
                    $vis fn $symbol($($arg_name: $arg_ty),*) $(-> $ret_ty)?;
                )*
            }

            #[cfg(not(target_arch = "wasm32"))]
            mod sys_impl {
                #[allow(unused)]
                use super::*;

                $(
                    #[unsafe(no_mangle)]
                    $vis extern "C" fn $symbol($(_: $arg_ty),*) $(-> $ret_ty)? {
                        unimplemented!(concat!(
                            stringify!($symbol),
                            " syscall is only available for wasm32 architecture"
                        ))
                    }
                )*
            }
        }
    };
}

pub(crate) use declare;
