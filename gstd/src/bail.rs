/// **The `bail!` macro**
///
/// Unwraps `Result<T, E: Debug>`.
/// In case of argument value Ok(T) returns T, else panics with custom message.
///
/// The macro has three implementations and its behavior depends
/// on the build type: is the `--features=debug` flag added.
///
/// - `bail!(res: Result<T, E: Debug>, msg: &str)`
///
/// Panics with the same `msg` in both modes.
/// Contains error message in debug mode.
///
/// - `bail!(res: Result<T, E: Debug>, static_release: &str, static_debug: &str)`
///
/// Panics with the same `static_release` in release mode
/// and with `static debug` + error message in debug mode.
///
/// - `bail!(res: Result<T, E: Debug>, static_release: &str, formatter: &str, args)`
///
/// Panics with the same `static_release` in release mode
/// and with `format!(formatter, args)` + error message in debug mode.
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
