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

/// **The `bail!` macro**
///
/// Unwraps `Result<T, E: Debug>`.
/// In case of argument value Ok(T) returns T, else panics with custom message.
///
/// The macro has three implementations and its behavior depends on the build
/// type: is the `--features=debug` flag added.
///
/// - `bail!(res: Result<T, E: Debug>, msg: &str)`
///
/// Panics with the same `msg` in both modes. Contains error message in debug
/// mode.
///
/// - `bail!(res: Result<T, E: Debug>, static_release: &str, static_debug:
///   &str)`
///
/// Panics with the same `static_release` in release mode and with `static
/// debug` + error message in debug mode.
///
/// - `bail!(res: Result<T, E: Debug>, static_release: &str, formatter: &str,
///   args)`
///
/// Panics with the same `static_release` in release mode and with
/// `format!(formatter, args)` + error message in debug mode.
#[cfg(feature = "debug")]
#[macro_export]
macro_rules! bail {
    ($res:expr, $msg:literal) => {
        $res.expect($msg);
    };
    ($res:expr, $expl:literal, $fmtd:literal) => {
        $res.expect($fmtd);
    };
    ($res:expr, $expl:literal, $fmt:literal, $($args:tt)+) => {
        $res.expect(&crate::prelude::format!($fmt, $($args)+));
    };
}

#[cfg(not(feature = "debug"))]
#[macro_export]
macro_rules! bail {
    ($res:expr, $msg:literal) => {
        match $res {
            Ok(v) => v,
            Err(_) => crate::prelude::panic!($msg),
        }
    };
    ($res:expr, $expl:literal, $fmtd:literal) => {
        match $res {
            Ok(v) => v,
            Err(_) => crate::prelude::panic!($expl),
        }
    };
    ($res:expr, $expl:literal, $fmt:literal, $($args:tt)+) => {
        match $res {
            Ok(v) => v,
            Err(_) => crate::prelude::panic!($expl),
        }
    };
}

#[cfg(test)]
mod tests {
    struct SomeType(usize);

    #[derive(Debug)]
    struct SomeError;

    #[test]
    fn bail_ok() {
        let res: Result<SomeType, SomeError> = Ok(SomeType(0));
        let val = bail!(res, "Your static explanation for both features");
        assert_eq!(val.0, 0);

        let res: Result<SomeType, SomeError> = Ok(SomeType(1));
        let val = bail!(
            res,
            "Your static release explanation",
            "Your static debug explanation"
        );
        assert_eq!(val.0, 1);

        let res: Result<SomeType, SomeError> = Ok(SomeType(2));
        let val = bail!(
            res,
            "Your static release explanation",
            "It was formatted -> {}",
            0
        );
        assert_eq!(val.0, 2);

        let res: Result<SomeType, SomeError> = Ok(SomeType(3));
        let val = bail!(
            res,
            "Your static release explanation",
            "They were formatted -> {} {}",
            0,
            "SECOND_ARG"
        );
        assert_eq!(val.0, 3);
    }

    #[test]
    #[should_panic(expected = "Your static explanation for both features")]
    fn bail_err_general_message() {
        let res: Result<SomeType, SomeError> = Err(SomeError);

        bail!(res, "Your static explanation for both features");
    }

    #[test]
    #[cfg(not(feature = "debug"))]
    #[should_panic(expected = "Your static release explanation")]
    fn bail_err_no_format() {
        let res: Result<SomeType, SomeError> = Err(SomeError);

        bail!(
            res,
            "Your static release explanation",
            "Your static debug explanation"
        );
    }

    #[test]
    #[cfg(feature = "debug")]
    #[should_panic(expected = "Your static debug explanation")]
    fn bail_err_no_format() {
        let res: Result<SomeType, SomeError> = Err(SomeError);

        bail!(
            res,
            "Your static release explanation",
            "Your static debug explanation"
        );
    }

    #[test]
    #[cfg(not(feature = "debug"))]
    #[should_panic(expected = "Your static release explanation")]
    fn bail_err_single_arg_format() {
        let res: Result<SomeType, SomeError> = Err(SomeError);

        bail!(
            res,
            "Your static release explanation",
            "It was formatted -> {}",
            0
        );
    }

    #[test]
    #[cfg(feature = "debug")]
    #[should_panic(expected = "It was formatted -> 0: SomeError")]
    fn bail_err_single_arg_format() {
        let res: Result<SomeType, SomeError> = Err(SomeError);

        bail!(
            res,
            "Your static release explanation",
            "It was formatted -> {}",
            0
        );
    }

    #[test]
    #[cfg(not(feature = "debug"))]
    #[should_panic(expected = "Your static release explanation")]
    fn bail_err_multiple_args_format() {
        let res: Result<SomeType, SomeError> = Err(SomeError);

        bail!(
            res,
            "Your static release explanation",
            "They were formatted -> {} {}",
            0,
            "SECOND_ARG"
        );
    }

    #[test]
    #[cfg(feature = "debug")]
    #[should_panic(expected = "They were formatted -> 0 SECOND_ARG: SomeError")]
    fn bail_err_multiple_args_format() {
        let res: Result<SomeType, SomeError> = Err(SomeError);

        bail!(
            res,
            "Your static release explanation",
            "They were formatted -> {} {}",
            0,
            "SECOND_ARG"
        );
    }
}
