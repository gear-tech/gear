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

macro_rules! impl_futures {
    ($f:ident, $r:ty, |$fut:ident, $cx:ident| => { $p:expr }) => {
        impl_futures!($f, $r, ($fut, $cx), $p, );
    };
    ($f:ident, $g: tt, $r:ty, |$fut:ident, $cx:ident| => { $p:expr }) => {
        impl_futures!($f, $r, ($fut, $cx), $p, $g);
    };
    ($f:ident, $r:ty, ($fut:ident, $cx:ident), $p:expr, $($g:tt)?) => {
        impl $( <$g: Decode> )? FusedFuture for $f $( < $g > )? {
            fn is_terminated(&self) -> bool {
                !signals().waits_for(self.waiting_reply_to)
            }
        }

        impl $( <$g: Decode> )? Future for $f $( < $g > )? {
            type Output = Result<$r>;

            fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
                let $fut = &mut self;
                let $cx = cx;

                $p
            }
        }

        impl $( <$g: Decode> )? $f $( < $g > )? {
            /// Postpone handling for a maximum amount of blocks that could be paid, that
            /// doesn't exceed a given duration.
            pub fn up_to(self, duration: Option<u32>) -> Result<Self> {
                async_runtime::locks().lock(
                    crate::msg::id(),
                    self.waiting_reply_to,
                    Lock::up_to(duration.unwrap_or(Config::wait_up_to()))?,
                );

                Ok(self)
            }

            /// Postpone handling for a given specific amount of blocks.
            pub fn exactly(self, duration: Option<u32>) -> Result<Self> {
                async_runtime::locks().lock(
                    crate::msg::id(),
                    self.waiting_reply_to,
                    Lock::exactly(duration.unwrap_or(Config::wait_for()))?,
                );

                Ok(self)
            }
        }
    };
}

pub(super) use impl_futures;
