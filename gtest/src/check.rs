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

use crate::js::{MetaData, MetaType};
use crate::runner::{self, CollectState};
use crate::sample::{self, AllocationExpectationKind, AllocationFilter, PayloadVariant, Test};
use anyhow::anyhow;
use colored::{ColoredString, Colorize};
use derive_more::Display;
use env_logger::filter::{Builder, Filter};
use gear_core::storage::Storage;
use gear_core::{
    memory::PAGE_SIZE,
    message::Message,
    program::{Program, ProgramId},
    storage,
};
use rayon::prelude::*;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};
use std::thread::{self, ThreadId};
use std::{fmt, fs};

use log::{Log, Metadata, Record, SetLoggerError};

const FILTER_ENV: &str = "RUST_LOG";

pub struct FixtureLogger {
    inner: Filter,
    map: Arc<RwLock<HashMap<ThreadId, Vec<String>>>>,
}

impl FixtureLogger {
    fn new(map: Arc<RwLock<HashMap<ThreadId, Vec<String>>>>) -> FixtureLogger {
        let mut builder = Builder::from_env(FILTER_ENV);
        builder.parse(
            "gtest=debug,gear_core=debug,gear_core_backend=debug,gear_core_runner=debug,gwasm=debug",
        );

        FixtureLogger {
            inner: builder.build(),
            map,
        }
    }

    fn init(map: Arc<RwLock<HashMap<ThreadId, Vec<String>>>>) -> Result<(), SetLoggerError> {
        let logger = Self::new(map);

        let max_level = logger.inner.filter();
        let r = log::set_boxed_logger(Box::new(logger));

        if r.is_ok() {
            log::set_max_level(max_level);
        }

        r
    }
}

impl Log for FixtureLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        self.inner.enabled(metadata)
    }

    fn log(&self, record: &Record) {
        // Check if the record is matched by the logger before logging
        if self.inner.matches(record) {
            if let Ok(mut map) = self.map.try_write() {
                map.entry(thread::current().id())
                    .or_default()
                    .push(record.args().to_string());
            }
        }
    }

    fn flush(&self) {}
}

#[derive(Debug, derive_more::From)]
pub struct DisplayedPayload(Vec<u8>);

impl fmt::Display for DisplayedPayload {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if let Ok(utf8) = std::str::from_utf8(&self.0[..]) {
            write!(f, "utf-8 ({})", utf8)
        } else {
            write!(f, "bytes ({:?})", &self.0[..])
        }
    }
}

#[derive(Debug, Display)]
#[display(fmt = "expected: {}, actual: {}", expected, actual)]
pub struct ContentMismatch<T: std::fmt::Display + std::fmt::Debug> {
    expected: T,
    actual: T,
}

#[derive(Debug, Display)]
pub enum MessageContentMismatch {
    Destination(ContentMismatch<ProgramId>),
    Payload(ContentMismatch<DisplayedPayload>),
    GasLimit(ContentMismatch<u64>),
    Value(ContentMismatch<u128>),
    ExitCode(ContentMismatch<i32>),
}

#[derive(Debug, Display)]
pub enum MessagesError {
    Count(ContentMismatch<usize>),
    #[display(fmt = "at position: {}, mismatch {}", at, mismatch)]
    AtPosition {
        at: usize,
        mismatch: MessageContentMismatch,
    },
}

impl MessagesError {
    fn count(expected: usize, actual: usize) -> Self {
        Self::Count(ContentMismatch { expected, actual })
    }

    fn payload(at: usize, expected: Vec<u8>, actual: Vec<u8>) -> Self {
        Self::AtPosition {
            at,
            mismatch: MessageContentMismatch::Payload(ContentMismatch {
                expected: expected.into(),
                actual: actual.into(),
            }),
        }
    }

    fn destination(at: usize, expected: ProgramId, actual: ProgramId) -> Self {
        Self::AtPosition {
            at,
            mismatch: MessageContentMismatch::Destination(ContentMismatch { expected, actual }),
        }
    }

    fn gas_limit(at: usize, expected: u64, actual: u64) -> Self {
        Self::AtPosition {
            at,
            mismatch: MessageContentMismatch::GasLimit(ContentMismatch { expected, actual }),
        }
    }

    fn value(at: usize, expected: u128, actual: u128) -> Self {
        Self::AtPosition {
            at,
            mismatch: MessageContentMismatch::Value(ContentMismatch { expected, actual }),
        }
    }

    fn exit_code(at: usize, expected: i32, actual: i32) -> Self {
        Self::AtPosition {
            at,
            mismatch: MessageContentMismatch::ExitCode(ContentMismatch { expected, actual }),
        }
    }
}

fn match_or_else<T: PartialEq + Copy>(expectation: Option<T>, value: T, f: impl FnOnce(T, T)) {
    if let Some(expected) = expectation {
        if expected != value {
            f(expected, value);
        }
    }
}

fn check_messages(
    progs_n_paths: &[(&str, ProgramId)],
    messages: &[Message],
    expected_messages: &[sample::Message],
) -> Result<(), Vec<MessagesError>> {
    let mut errors = Vec::new();
    if expected_messages.len() != messages.len() {
        errors.push(MessagesError::count(
            expected_messages.len(),
            messages.len(),
        ))
    } else {
        let mut expected_messages: Vec<sample::Message> = expected_messages.into();
        let mut messages: Vec<Message> = messages.into();
        expected_messages
            .iter_mut()
            .zip(messages.iter_mut())
            .enumerate()
            .for_each(|(position, (exp, msg))| {
                let source_n_dest = [msg.source(), msg.dest()];
                let is_init = exp.init.unwrap_or(false);

                if exp
                    .payload
                    .as_mut()
                    .map(|payload| match payload {
                        PayloadVariant::Custom(_) => {
                            if let Some(&(path, prog_id)) = progs_n_paths
                                .iter()
                                .find(|(_, prog_id)| source_n_dest.contains(prog_id))
                            {
                                let is_outgoing = prog_id == source_n_dest[0];

                                let meta_type = match (is_init, is_outgoing) {
                                    (true, true) => MetaType::InitOutput,
                                    (true, false) => MetaType::InitInput,
                                    (false, true) => MetaType::HandleOutput,
                                    (false, false) => MetaType::HandleInput,
                                };

                                let path: String = path.replace(".wasm", ".meta.wasm");

                                let json =
                                    MetaData::Json(String::from_utf8(payload.to_bytes()).unwrap());

                                let bytes = json
                                    .convert(&path, &meta_type)
                                    .expect("Unable to get bytes");

                                *payload = PayloadVariant::Utf8(
                                    bytes
                                        .convert(&path, &meta_type)
                                        .expect("Unable to get json")
                                        .into_json(),
                                );

                                msg.payload = MetaData::CodecBytes(msg.payload.clone().into_raw())
                                    .convert(&path, &meta_type)
                                    .expect("Unable to get bytes")
                                    .into_bytes()
                                    .into();
                            };

                            !payload.equals(msg.payload.as_ref())
                        }
                        _ => !payload.equals(msg.payload.as_ref()),
                    })
                    .unwrap_or(false)
                {
                    errors.push(MessagesError::payload(
                        position,
                        exp.payload
                            .as_ref()
                            .expect("Checked above.")
                            .clone()
                            .into_raw(),
                        msg.payload.clone().into_raw(),
                    ))
                }

                match_or_else(
                    Some(exp.destination.to_program_id()),
                    msg.dest,
                    |expected, actual| {
                        errors.push(MessagesError::destination(position, expected, actual))
                    },
                );

                match_or_else(exp.gas_limit, msg.gas_limit, |expected, actual| {
                    errors.push(MessagesError::gas_limit(position, expected, actual))
                });

                match_or_else(exp.value, msg.value, |expected, actual| {
                    errors.push(MessagesError::value(position, expected, actual))
                });

                match_or_else(
                    exp.exit_code,
                    msg.reply.map(|(_, exit_code)| exit_code).unwrap_or(0),
                    |expected, actual| {
                        errors.push(MessagesError::exit_code(position, expected, actual))
                    },
                );
            });
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn check_allocations(
    programs: &[Program],
    expected_allocations: &[sample::Allocations],
) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();

    for exp in expected_allocations {
        let target_program_id = ProgramId::from(exp.program_id);
        if let Some(program) = programs.iter().find(|p| p.id() == target_program_id) {
            let actual_pages = program
                .get_pages()
                .iter()
                .filter(|(page, _buf)| match exp.filter {
                    Some(AllocationFilter::Static) => page.raw() < program.static_pages(),
                    Some(AllocationFilter::Dynamic) => page.raw() >= program.static_pages(),
                    None => true,
                })
                .collect::<Vec<_>>();

            match exp.kind {
                AllocationExpectationKind::PageCount(expected_page_count) => {
                    if actual_pages.len() != expected_page_count as usize {
                        errors.push(format!(
                            "Expectation error (Allocation page count does not match, expected: {}; actual: {}. Program id: {})",
                            expected_page_count,
                            actual_pages.len(),
                            exp.program_id,
                        ));
                    }
                }
                AllocationExpectationKind::ExactPages(ref expected_pages) => {
                    let mut actual_pages = actual_pages
                        .iter()
                        .map(|(page, _buf)| page.raw())
                        .collect::<Vec<_>>();
                    let mut expected_pages = expected_pages.clone();

                    actual_pages.sort_unstable();
                    expected_pages.sort_unstable();

                    if actual_pages != expected_pages {
                        errors.push(format!(
                            "Expectation error (Following allocation pages expected: {:?}; actual: {:?}. Program id: {})",
                            expected_pages,
                            actual_pages,
                            exp.program_id,
                        ))
                    }
                }
                AllocationExpectationKind::ContainsPages(ref expected_pages) => {
                    for &expected_page in expected_pages {
                        if !actual_pages
                            .iter()
                            .map(|(page, _buf)| page.raw())
                            .any(|actual_page| actual_page == expected_page)
                        {
                            errors.push(format!(
                                "Expectation error (Allocation page {} expected, but not found. Program id: {})",
                                expected_page,
                                exp.program_id,
                            ));
                        }
                    }
                }
            }
        } else {
            errors.push(format!(
                "Expectation error (Program id not found: {})",
                exp.program_id
            ))
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn check_memory(
    program_storage: &mut Vec<Program>,
    expected_memory: &[sample::BytesAt],
) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();
    for case in expected_memory {
        for p in &mut *program_storage {
            if p.id() == ProgramId::from(case.program_id) {
                let page = case.address / PAGE_SIZE;
                if let Some(page_buf) = p.get_page((page as u32).into()) {
                    if page_buf[case.address - page * PAGE_SIZE
                        ..(case.address - page * PAGE_SIZE) + case.bytes.len()]
                        != case.bytes
                    {
                        errors.push("Expectation error (Static memory doesn't match)".to_string());
                    }
                } else {
                    errors.push("Expectation error (Incorrect memory address)".to_string());
                }
            }
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn read_test_from_file<P: AsRef<std::path::Path>>(path: P) -> anyhow::Result<Test> {
    let file = fs::File::open(path.as_ref())
        .map_err(|e| anyhow::anyhow!("Error loading '{}': {}", path.as_ref().display(), e))?;
    let u = serde_yaml::from_reader(file)
        .map_err(|e| anyhow::anyhow!("Error decoding '{}': {}", path.as_ref().display(), e))?;

    Ok(u)
}

#[allow(clippy::too_many_arguments)]
fn run_fixture<SC>(
    storage: Storage<<SC as storage::StorageCarrier>::PS>,
    test: &Test,
    fixture_no: usize,
    progs_n_paths: &[(&str, ProgramId)],
    total_failed: &AtomicUsize,
    skip_messages: bool,
    skip_allocations: bool,
    skip_memory: bool,
) -> ColoredString
where
    SC: storage::StorageCarrier,
    storage::Storage<SC::PS>: CollectState,
{
    match runner::init_fixture::<SC>(storage, test, fixture_no) {
        Ok((runner, messages, log)) => {
            let last_exp_steps = test.fixtures[fixture_no].expected.last().unwrap().step;
            let results = runner::run(runner, messages.into(), log, last_exp_steps);

            let mut errors = Vec::new();
            for exp in &test.fixtures[fixture_no].expected {
                let mut final_state = results.last().unwrap().0.clone();
                if let Some(step) = exp.step {
                    final_state = results[step].0.clone();
                }
                if !skip_messages {
                    if let Some(messages) = &exp.messages {
                        if let Err(msg_errors) =
                            check_messages(progs_n_paths, &final_state.messages, messages)
                        {
                            errors.push(format!("step: {:?}", exp.step));
                            errors.extend(
                                msg_errors
                                    .into_iter()
                                    .map(|err| format!("Messages check [{}]", err)),
                            );
                        }
                    }
                }
                if let Some(log) = &exp.log {
                    for message in &final_state.log {
                        if let Ok(utf8) = std::str::from_utf8(message.payload()) {
                            log::info!("log({})", utf8)
                        }
                    }

                    if let Err(log_errors) = check_messages(progs_n_paths, &final_state.log, log) {
                        errors.push(format!("step: {:?}", exp.step));
                        errors.extend(
                            log_errors
                                .into_iter()
                                .map(|err| format!("Log check [{}]", err)),
                        );
                    }
                }
                if !skip_allocations {
                    if let Some(alloc) = &exp.allocations {
                        if let Err(alloc_errors) =
                            check_allocations(&final_state.program_storage, alloc)
                        {
                            errors.push(format!("step: {:?}", exp.step));
                            errors.extend(alloc_errors);
                        }
                    }
                }
                if !skip_memory {
                    if let Some(mem) = &exp.memory {
                        if let Err(mem_errors) = check_memory(&mut final_state.program_storage, mem)
                        {
                            errors.push(format!("step: {:?}", exp.step));
                            errors.extend(mem_errors);
                        }
                    }
                }
            }
            if !errors.is_empty() {
                errors.insert(0, "\n".to_string());
                total_failed.fetch_add(1, Ordering::SeqCst);
                errors.join("\n").bright_red()
            } else {
                "Ok".bright_green()
            }
        }
        Err(e) => {
            total_failed.fetch_add(1, Ordering::SeqCst);
            format!("Initialization error ({})", e).bright_red()
        }
    }
}

/// Runs tests defined in `files`.
///
/// To understand how tests are structured see [sample](../sample/index.html) module.
/// For each fixture in the test file from `files` the function setups (initializes) it and then performs all the checks
/// by first running messages defined in the fixture section and then checking (if required) message state, allocations and memory.
#[allow(clippy::too_many_arguments)]
pub fn check_main<SC, F>(
    files: Vec<std::path::PathBuf>,
    skip_messages: bool,
    skip_allocations: bool,
    skip_memory: bool,
    print_logs: bool,
    storage_factory: F,
    ext: Option<Box<dyn Fn() -> sp_io::TestExternalities + Send + Sync + 'static>>,
) -> anyhow::Result<()>
where
    SC: storage::StorageCarrier,
    F: Fn() -> storage::Storage<SC::PS> + std::marker::Sync + std::marker::Send,
    storage::Storage<SC::PS>: CollectState,
{
    let map = Arc::new(RwLock::new(HashMap::new()));
    if let Err(e) = FixtureLogger::init(Arc::clone(&map)) {
        println!("Logger err: {}", e);
    }
    let mut tests = Vec::new();

    for path in files {
        if path.is_dir() {
            for entry in path.read_dir().expect("read_dir call failed").flatten() {
                tests.push(read_test_from_file(&entry.path())?);
            }
        } else {
            tests.push(read_test_from_file(&path)?);
        }
    }

    let total_fixtures: usize = tests.iter().map(|t| t.fixtures.len()).sum();
    let total_failed = AtomicUsize::new(0);

    println!("Total fixtures: {}", total_fixtures);

    tests.par_iter().for_each(|test| {
        let progs_n_paths: Vec<(&str, ProgramId)> = test
            .programs
            .iter()
            .map(|prog| (prog.path.as_ref(), prog.id.to_program_id()))
            .collect();

        (0..test.fixtures.len())
            .into_par_iter()
            .for_each(|fixture_no| {
                map.write()
                    .unwrap()
                    .insert(thread::current().id(), Vec::new());

                let output = if let Some(test_ext) = &ext {
                    test_ext().execute_with(|| {
                        let storage = storage_factory();
                        run_fixture::<SC>(
                            storage,
                            test,
                            fixture_no,
                            &progs_n_paths,
                            &total_failed,
                            skip_messages,
                            skip_allocations,
                            skip_memory,
                        )
                    })
                } else {
                    let storage = storage_factory();
                    run_fixture::<SC>(
                        storage,
                        test,
                        fixture_no,
                        &progs_n_paths,
                        &total_failed,
                        skip_messages,
                        skip_allocations,
                        skip_memory,
                    )
                };
                if output != "Ok".bright_green() {
                    map.read()
                        .unwrap()
                        .get(&thread::current().id())
                        .unwrap()
                        .iter()
                        .for_each(|line| {
                            eprintln!("{}", line.bright_red());
                        });
                } else if print_logs {
                    map.read()
                        .unwrap()
                        .get(&thread::current().id())
                        .unwrap()
                        .iter()
                        .for_each(|line| {
                            println!("{}", line);
                        });
                }
                println!(
                    "Fixture {}: {}",
                    test.fixtures[fixture_no].title.bold(),
                    output
                );
            });
    });

    if total_failed.load(Ordering::SeqCst) == 0 {
        Ok(())
    } else {
        Err(anyhow!(
            "{} tests failed",
            total_failed.load(Ordering::SeqCst)
        ))
    }
}
