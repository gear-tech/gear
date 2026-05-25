// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

/// Add a debug message to the log.
///
/// Same as [`gcore::debug`] but uses heap instead of stack for formatting.
#[cfg(any(feature = "debug", debug_assertions))]
#[macro_export]
macro_rules! heap_debug {
    ($fmt:expr) => {
        $crate::ext::debug(&$crate::format!($fmt))
    };
    ($fmt:expr, $($args:tt)*) => {
        $crate::ext::debug(&$crate::format!($fmt, $($args)*))
    };
}

#[cfg(not(any(feature = "debug", debug_assertions)))]
#[allow(missing_docs)]
#[macro_export]
macro_rules! heap_debug {
    ($fmt:expr) => {};
    ($fmt:expr, $($args:tt)*) => {};
}

/// Prints and returns the value of a given expression for quick and dirty
/// debugging.
///
/// Similar to the standard library's
/// [`dbg!`](https://doc.rust-lang.org/std/macro.dbg.html) macro.
#[macro_export]
macro_rules! dbg {
    () => {
        $crate::debug!("[{}:{}:{}]", $crate::prelude::file!(), $crate::prelude::line!(), $crate::prelude::column!())
    };
    ($val:expr $(,)?) => {
        match $val {
            tmp => {
                $crate::debug!("[{}:{}:{}] {} = {:#?}",
                    $crate::prelude::file!(),
                    $crate::prelude::line!(),
                    $crate::prelude::column!(),
                    $crate::prelude::stringify!($val),
                    &tmp,
                );
                tmp
            }
        }
    };
    ($($val:expr),+ $(,)?) => {
        ($($crate::dbg!($val)),+,)
    };
}
