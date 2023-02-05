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

use crate::{
    js::{MetaData, MetaType},
    manager::CollectState,
    proc,
    sample::{self, AllocationExpectationKind, AllocationFilter, PayloadVariant, Test},
};
use anyhow::anyhow;
use colored::{ColoredString, Colorize};
use core_processor::{
    common::{ExecutableActorData, JournalHandler},
    Ext,
};
use derive_more::Display;
use env_logger::filter::{Builder, Filter};
use gear_backend_common::Environment;
use gear_core::{
    code::Code,
    ids::{CodeId, MessageId, ProgramId},
    memory::{GearPage, PageBuf, PageU32Size, WasmPage},
    message::*,
};
use log::{Log, Metadata, Record, SetLoggerError};
use rayon::prelude::*;
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    convert::TryInto,
    fmt, fs,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, RwLock,
    },
    thread::{self, ThreadId},
};

const FILTER_ENV: &str = "RUST_LOG";

pub trait ExecutionContext {
    fn store_code(&mut self, code_id: CodeId, code: Code);
    fn load_code(&self, code_id: CodeId) -> Option<Code>;
    fn store_original_code(&mut self, code: &[u8]);
    fn store_program(
        &mut self,
        id: ProgramId,
        code: Code,
        init_message_id: MessageId,
    ) -> ExecutableActorData;
    fn write_gas(&mut self, message_id: MessageId, gas_limit: u64);
}

pub struct FixtureLogger {
    inner: Filter,
    map: Arc<RwLock<HashMap<ThreadId, Vec<String>>>>,
}

impl FixtureLogger {
    fn new(map: Arc<RwLock<HashMap<ThreadId, Vec<String>>>>) -> FixtureLogger {
        let mut builder = Builder::from_env(FILTER_ENV);

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
                map.entry(thread::current().id()).or_default().push(format!(
                    "[{}] {}",
                    record.target().green(),
                    record.args()
                ));
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
            write!(f, "utf-8 ({utf8}) bytes(0x{})", hex::encode(&self.0))
        } else {
            write!(f, "bytes (0x{})", hex::encode(&self.0))
        }
    }
}

#[derive(Debug, Display)]
#[display(fmt = "expected: {expected}, actual: {actual}")]
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
    StatusCode(ContentMismatch<i32>),
}

#[derive(Debug, Display)]
pub enum MessagesError {
    Count(ContentMismatch<usize>),
    #[display(fmt = "at position: {at}, mismatch {mismatch}")]
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

    fn status_code(at: usize, expected: i32, actual: i32) -> Self {
        Self::AtPosition {
            at,
            mismatch: MessageContentMismatch::StatusCode(ContentMismatch { expected, actual }),
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

pub fn check_messages(
    progs_n_paths: &[(&str, ProgramId)],
    messages: &[(StoredMessage, GasLimit)],
    expected_messages: &[sample::Message],
    skip_gas: bool,
) -> Result<(), Vec<MessagesError>> {
    let mut errors = Vec::new();
    if expected_messages.len() != messages.len() {
        errors.push(MessagesError::count(
            expected_messages.len(),
            messages.len(),
        ))
    } else {
        let mut expected_messages: Vec<sample::Message> = expected_messages.into();
        let mut messages: Vec<(StoredMessage, GasLimit)> = messages.into();
        expected_messages
            .iter_mut()
            .enumerate()
            .for_each(|(position, exp)| {
                let (msg, gas_limit) = messages
                    .get_mut(position)
                    .expect("Can't fail. Lengths checked above");
                let source_n_dest = [msg.source(), msg.destination()];
                let is_init = exp.init.unwrap_or(false);

                match_or_else(
                    exp.status_code,
                    msg.status_code().unwrap_or(0),
                    |expected, actual| {
                        errors.push(MessagesError::status_code(position, expected, actual))
                    },
                );

                if msg.status_code().unwrap_or(0) == 0
                    && exp
                        .payload
                        .as_mut()
                        .map(|payload| match payload {
                            PayloadVariant::Custom(v) => {
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

                                    let path: String =
                                        crate::sample::get_meta_wasm_path(String::from(path));

                                    let json = MetaData::Json(proc::parse_payload(
                                        serde_json::to_string(&v)
                                            .expect("Cannot convert to string"),
                                    ));

                                    let bytes = json
                                        .convert(&path, &meta_type)
                                        .expect("Unable to get bytes");

                                    *payload = PayloadVariant::Utf8(
                                        bytes
                                            .convert(&path, &meta_type)
                                            .expect("Unable to get json")
                                            .into_json(),
                                    );

                                    let new_payload =
                                        MetaData::CodecBytes((*msg.payload()).to_vec())
                                            .convert(&path, &meta_type)
                                            .expect("Unable to get bytes")
                                            .into_bytes();

                                    *msg = StoredMessage::new(
                                        msg.id(),
                                        msg.source(),
                                        msg.destination(),
                                        new_payload.try_into().unwrap(),
                                        msg.value(),
                                        msg.details(),
                                    );
                                };

                                !payload.equals(msg.payload())
                            }
                            _ => !payload.equals(msg.payload()),
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
                        (*msg.payload()).to_vec(),
                    ))
                }

                match_or_else(
                    Some(exp.destination.to_program_id()),
                    msg.destination(),
                    |expected, actual| {
                        errors.push(MessagesError::destination(position, expected, actual))
                    },
                );

                if !skip_gas && exp.gas_limit.is_some() {
                    match_or_else(exp.gas_limit, *gas_limit, |expected, actual| {
                        errors.push(MessagesError::gas_limit(position, expected, actual))
                    });
                }

                match_or_else(exp.value, msg.value(), |expected, actual| {
                    errors.push(MessagesError::value(position, expected, actual))
                });
            });
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

pub struct ProgramAllocations<'a> {
    pub id: ProgramId,
    pub static_pages: WasmPage,
    pub allocations: &'a BTreeSet<WasmPage>,
}

pub fn check_allocations(
    allocations: &[ProgramAllocations<'_>],
    expected_allocations: &[sample::Allocations],
) -> Result<(), Vec<String>> {
    let mut errors = Vec::with_capacity(3 * allocations.len());

    for exp in expected_allocations {
        let target_program_id = exp.id.to_program_id();
        let program_allocations = match allocations.iter().find(|&p| p.id == target_program_id) {
            None => {
                log::error!("Program not found");
                errors.push(format!(
                    "Expectation error (Program id not found: {target_program_id})",
                ));

                continue;
            }
            Some(a) => a,
        };

        let static_pages = program_allocations.static_pages;
        let actual_pages = program_allocations
            .allocations
            .iter()
            .filter(|&page| match exp.filter {
                Some(AllocationFilter::Static) => *page < static_pages,
                Some(AllocationFilter::Dynamic) => *page >= static_pages,
                None => true,
            })
            .collect::<BTreeSet<_>>();

        match exp.kind {
            AllocationExpectationKind::PageCount(expected_page_count) => {
                if actual_pages.len() != expected_page_count as usize {
                    errors.push(format!(
                        "Expectation error (Allocation page count does not match, expected: {expected_page_count}; actual: {}. Program id: {target_program_id})",
                        actual_pages.len(),
                    ));
                }
            }
            AllocationExpectationKind::ExactPages(ref expected_pages) => {
                let mut actual_pages = actual_pages
                    .iter()
                    .map(|page| page.raw())
                    .collect::<Vec<_>>();
                let mut expected_pages = expected_pages.clone();

                actual_pages.sort_unstable();
                expected_pages.sort_unstable();

                if actual_pages != expected_pages {
                    errors.push(format!(
                        "Expectation error (Following allocation pages expected: {expected_pages:?}; actual: {actual_pages:?}. Program id: {target_program_id})",
                    ))
                }
            }
            AllocationExpectationKind::ContainsPages(ref expected_pages) => {
                for &expected_page in expected_pages {
                    if !actual_pages
                        .iter()
                        .map(|page| page.raw())
                        .any(|actual_page| actual_page == expected_page)
                    {
                        errors.push(format!(
                            "Expectation error (Allocation page {expected_page} expected, but not found. Program id: {target_program_id})",
                        ));
                    }
                }
            }
        }
    }

    match errors.is_empty() {
        true => Ok(()),
        false => Err(errors),
    }
}

pub fn check_memory(
    actors_data: &Vec<(ProgramId, ExecutableActorData, BTreeMap<GearPage, PageBuf>)>,
    expected_memory: &[sample::BytesAt],
) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();
    for case in expected_memory {
        for (program_id, _data, memory) in actors_data {
            if *program_id == case.id.to_program_id() {
                let page = GearPage::from_offset(case.address as u32);
                if let Some(page_buf) = memory.get(&page) {
                    let begin_byte = case.address - page.offset() as usize;
                    let end_byte = begin_byte + case.bytes.len();
                    if page_buf[begin_byte..end_byte] != case.bytes {
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

pub fn check_programs_state(
    expected_programs: &BTreeMap<ProgramId, bool>,
    actual_programs: &BTreeMap<ProgramId, bool>,
    only: bool,
) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();

    if only {
        if actual_programs.len() != expected_programs.len() {
            errors.push(format!(
                "Different lens of actual and expected programs: actual length={}, expected length={}",
                actual_programs.len(), expected_programs.len(),
            ));
        }

        for id in actual_programs.keys() {
            if !expected_programs.contains_key(id) {
                errors.push(format!(
                    "Actual program {id:?} wasn't found in expectations",
                ));
            }
        }
    }

    for (id, terminated) in expected_programs {
        let actual_termination = actual_programs.get(id);
        if let Some(actual_termination) = actual_termination {
            if actual_termination != terminated {
                errors.push(format!(
                    "Wrong state of program: {id:?} expected to be active={terminated:?}, but it is active={actual_termination:?}",
                ));
            }
        } else {
            errors.push(format!("Invalid program id {id:?}."));
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

pub fn read_test_from_file<P: AsRef<std::path::Path>>(path: P) -> anyhow::Result<Test> {
    let file = fs::File::open(path.as_ref())
        .map_err(|e| anyhow::anyhow!("Error loading '{}': {}", path.as_ref().display(), e))?;
    let u = serde_yaml::from_reader(file)
        .map_err(|e| anyhow::anyhow!("Error decoding '{}': {}", path.as_ref().display(), e))?;

    Ok(u)
}

#[allow(clippy::too_many_arguments)]
fn run_fixture<JH, E>(
    mut journal_handler: JH,
    test: &Test,
    fixture_no: usize,
    progs_n_paths: &[(&str, ProgramId)],
    total_failed: &AtomicUsize,
    skip_messages: bool,
    skip_allocations: bool,
    skip_memory: bool,
) -> ColoredString
where
    JH: JournalHandler + CollectState + ExecutionContext,
    E: Environment<Ext = Ext>,
{
    if let Err(err) = proc::init_fixture::<E, JH>(test, fixture_no, &mut journal_handler) {
        total_failed.fetch_add(1, Ordering::SeqCst);
        return format!("Initialization error ({err})").bright_red();
    }

    let expected = match &test.fixtures[fixture_no].expected {
        Some(exp) => exp,
        None => return "Ok".bright_green(),
    };

    let last_exp_steps = expected.last().unwrap().step;
    let results = proc::run::<JH, E>(last_exp_steps, &mut journal_handler);

    let mut errors = Vec::new();
    for exp in expected {
        let mut final_state = results.last().unwrap().0.clone();
        if let Some(step) = exp.step {
            final_state = results[step].0.clone();
        }
        if !exp.allow_error.unwrap_or(false) && final_state.current_failed {
            errors.push(format!("step: {:?}", exp.step));
            errors.extend(["Failed, but wasn't allowed to".to_string()]);
        }

        if !skip_messages {
            if let Some(messages) = &exp.messages {
                let msgs: Vec<_> = final_state
                    .dispatch_queue
                    .into_iter()
                    .map(|(d, gas_limit)| (d.into_parts().1, gas_limit))
                    .collect();

                if let Err(msg_errors) = check_messages(progs_n_paths, &msgs, messages, false) {
                    errors.push(format!("step: {:?}", exp.step));
                    errors.extend(
                        msg_errors
                            .into_iter()
                            .map(|err| format!("Messages check [{err}]")),
                    );
                }
            }
        }
        if let Some(log) = &exp.log {
            for message in &final_state.log {
                if let Ok(utf8) = std::str::from_utf8(message.payload()) {
                    log::debug!("log(text: {})", utf8);
                } else {
                    log::debug!("log(<binary>)");
                }
            }

            let logs = final_state
                .log
                .into_iter()
                .map(|v| (v, 0u64))
                .collect::<Vec<(StoredMessage, GasLimit)>>();

            if let Err(log_errors) = check_messages(progs_n_paths, &logs, log, true) {
                errors.push(format!("step: {:?}", exp.step));
                errors.extend(
                    log_errors
                        .into_iter()
                        .map(|err| format!("Log check [{err}]")),
                );
            }
        }
        if let Some(programs) = &exp.programs {
            let expected_prog_ids = programs
                .ids
                .iter()
                .map(|program| {
                    (
                        program.address.to_program_id(),
                        program.terminated.unwrap_or_default(),
                    )
                })
                .collect();

            let actual_prog_ids = final_state
                .actors
                .iter()
                .map(|(id, actor)| (*id, actor.executable_data.is_none()))
                .collect();

            if let Err(prog_id_errors) = check_programs_state(
                &expected_prog_ids,
                &actual_prog_ids,
                programs.only.unwrap_or_default(),
            ) {
                errors.push(format!("step: {:?}", exp.step));
                errors.extend(
                    prog_id_errors
                        .into_iter()
                        .map(|err| format!("Program ids check: [{err}]")),
                );
            }
        }

        if !skip_allocations {
            if let Some(alloc) = &exp.allocations {
                let progs: Vec<ProgramAllocations<'_>> = final_state
                    .actors
                    .iter()
                    .filter_map(|(id, actor)| actor.executable_data.as_ref().map(|d| (*id, d)))
                    .map(|(id, data)| ProgramAllocations {
                        id,
                        static_pages: data.static_pages,
                        allocations: &data.allocations,
                    })
                    .collect();

                if let Err(alloc_errors) = check_allocations(&progs, alloc) {
                    errors.push(format!("step: {:?}", exp.step));
                    errors.extend(alloc_errors);
                }
            }
        }

        if !skip_memory {
            if let Some(mem) = &exp.memory {
                let data = final_state
                    .actors
                    .into_iter()
                    .filter_map(|(actor_id, actor)| match actor.executable_data {
                        None => None,
                        Some(d) => Some((actor_id, d, actor.memory_pages)),
                    })
                    .collect();
                if let Err(mem_errors) = check_memory(&data, mem) {
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

/// Runs tests defined in `files`.
///
/// To understand how tests are structured see [sample](../sample/index.html) module.
/// For each fixture in the test file from `files` the function setups (initializes) it and then performs all the checks
/// by first running messages defined in the fixture section and then checking (if required) message state, allocations and memory.
#[allow(clippy::too_many_arguments)]
pub fn check_main<JH, E, F>(
    files: Vec<std::path::PathBuf>,
    skip_messages: bool,
    skip_allocations: bool,
    skip_memory: bool,
    print_logs: bool,
    storage_factory: F,
) -> anyhow::Result<()>
where
    JH: JournalHandler + CollectState + ExecutionContext,
    E: Environment<Ext = Ext>,
    F: Fn() -> JH + Sync + Send,
{
    let map = Arc::new(RwLock::new(HashMap::new()));
    if let Err(e) = FixtureLogger::init(Arc::clone(&map)) {
        println!("Logger err: {e}");
    }
    let mut tests = Vec::new();

    for path in files {
        if path.is_dir() {
            for entry in path.read_dir().expect("read_dir call failed").flatten() {
                tests.push(read_test_from_file(entry.path())?);
            }
        } else {
            tests.push(read_test_from_file(&path)?);
        }
    }

    let total_fixtures: usize = tests.iter().map(|t| t.fixtures.len()).sum();
    let total_failed = AtomicUsize::new(0);

    println!("Total fixtures: {total_fixtures}");

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

                let storage = storage_factory();
                let output = run_fixture::<JH, E>(
                    storage,
                    test,
                    fixture_no,
                    &progs_n_paths,
                    &total_failed,
                    skip_messages,
                    skip_allocations,
                    skip_memory,
                );
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
                            println!("{line}");
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
