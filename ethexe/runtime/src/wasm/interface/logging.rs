// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use super::utils;
use crate::wasm::interface;
use alloc::vec::Vec;
use core::fmt::{self, Write};
use log::{Level, LevelFilter, Metadata, Record};

interface::declare! {
    pub(super) fn ext_logging_log_v1(level: i32, target: i64, message: i64);
    pub(super) fn ext_logging_max_level_v1() -> i32;
}

/// Emits a log record through the host logging interface.
///
/// Converts `level` and the target/message slices into the representation expected by the
/// `ext_logging_log_v1` host function and delegates to it.
pub fn log(level: Level, target: &str, message: &[u8]) {
    let level = level as usize as i32;
    let target = utils::repr_ri_slice(target);
    let message = utils::repr_ri_slice(message);

    unsafe {
        sys::ext_logging_log_v1(level, target, message);
    }
}

/// Returns the maximum log level accepted by the host, as a `LevelFilter`.
///
/// Calls `ext_logging_max_level_v1` and maps its integer return value to the corresponding
/// `log::LevelFilter` variant, treating any value above 4 as `Trace`.
pub fn max_level() -> LevelFilter {
    match unsafe { sys::ext_logging_max_level_v1() } {
        0 => LevelFilter::Off,
        1 => LevelFilter::Error,
        2 => LevelFilter::Warn,
        3 => LevelFilter::Info,
        4 => LevelFilter::Debug,
        _ => LevelFilter::Trace,
    }
}

/// A `log::Log` implementation that forwards records to the host via [`log`] and [`max_level`].
pub struct RuntimeLogger;

impl RuntimeLogger {
    /// Registers `RuntimeLogger` as the global logger and sets the max level from the host.
    ///
    /// Registering the logger is idempotent: `log::set_logger` returns an error on repeated calls,
    /// which is silently ignored. The max level is refreshed from the host on every call.
    pub fn init() {
        static LOGGER: RuntimeLogger = RuntimeLogger;
        let _ = log::set_logger(&LOGGER);

        log::set_max_level(max_level());
    }
}

impl log::Log for RuntimeLogger {
    fn enabled(&self, _: &Metadata) -> bool {
        true
    }

    fn log(&self, record: &Record) {
        let mut w = Writer::default();

        let _ = core::write!(&mut w, "{}", record.args());

        log(record.level(), record.target(), &w.0);
    }

    fn flush(&self) {}
}

#[derive(Default)]
struct Writer(Vec<u8>);

impl Write for Writer {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.0.extend(s.as_bytes());
        Ok(())
    }
}
