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

pub fn log(level: Level, target: &str, message: &[u8]) {
    let level = level as usize as i32;
    let target = utils::repr_ri_slice(target);
    let message = utils::repr_ri_slice(message);

    unsafe {
        sys::ext_logging_log_v1(level, target, message);
    }
}

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

pub struct RuntimeLogger;

impl RuntimeLogger {
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
