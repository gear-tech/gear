mod runner;

use anyhow::anyhow;
use derive_more::Display;
use gear_core::{
    message::Message,
    program::{Program, ProgramId},
};
use gear_test_sample::sample::{self, Test};
use std::{fmt, fs};
use termion::{color, style};

use clap::{AppSettings, Clap};

#[derive(Clap)]
#[clap(setting = AppSettings::ColoredHelp)]
struct Opts {
    /// Skip messages checks
    #[clap(long)]
    pub skip_messages: bool,
    /// Skip allocations checks
    #[clap(long)]
    pub skip_allocations: bool,
    /// Skip memory checks
    #[clap(long)]
    pub skip_memory: bool,
    /// JSON sample file(s) or dir
    pub input: Vec<std::path::PathBuf>,
    /// A level of verbosity, and can be used multiple times
    #[clap(short, long, parse(from_occurrences))]
    verbose: i32,
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
}

fn check_messages(
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
        expected_messages
            .iter()
            .zip(messages.iter())
            .enumerate()
            .for_each(|(position, (exp, msg))| {
                if ProgramId::from(exp.destination) != msg.dest {
                    errors.push(MessagesError::destination(
                        position,
                        exp.destination.into(),
                        msg.dest,
                    ))
                }
                if exp
                    .payload
                    .as_ref()
                    .map(|payload| !payload.equals(msg.payload.as_ref()))
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
                if let Some(expected_gas_limit) = exp.gas_limit {
                    if expected_gas_limit != msg.gas_limit {
                        errors.push(MessagesError::gas_limit(
                            position,
                            expected_gas_limit,
                            msg.gas_limit,
                        ))
                    }
                }
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
    expected_pages: &[sample::AllocationStorage],
) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();
    for exp in expected_pages {
        for program in programs {
            if ProgramId::from(exp.program_id) == program.id() {
                if !program.get_pages().contains_key(&exp.page_num.into()) {
                    errors.push(format!(
                        "Expectation error (PageNumber doesn't match, expected: {})",
                        exp.page_num
                    ));
                }
            }
        }
    }
    // expected_pages
    //     .iter()
    //     .zip(programs.iter())
    //     .for_each(|(exp, program)| {
    //         if program.get_pages().contains_key(&exp.page_num.into()) {
    //             errors.push(format!(
    //                 "Expectation error (PageNumber doesn't match, expected: {})",
    //                 exp.page_num
    //             ));
    //         }
    //         if ProgramId::from(exp.program_id) != page.1 {
    //             errors.push(format!(
    //                 "Expectation error (ProgramId doesn't match, expected: {}, found: {:?})\n",
    //                 exp.program_id, page.1
    //             ));
    //         }
    //     });

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
                let page = case.address / 65536;
                if let Some(page_buf) = p.get_page((page as u32).into()) {
                    if page_buf[case.address - page * 65536
                        ..(case.address - page * 65536) + case.bytes.len()]
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
    let u = serde_json::from_reader(file)
        .map_err(|e| anyhow::anyhow!("Error decoding '{}': {}", path.as_ref().display(), e))?;

    Ok(u)
}

pub fn main() -> anyhow::Result<()> {
    let opts: Opts = Opts::parse();
    let mut print_log = false;
    match opts.verbose {
        0 => env_logger::init(),
        1 => {
            print_log = true;
        }
        2 => {
            use env_logger::Env;

            print_log = true;
            env_logger::Builder::from_env(
                Env::default().default_filter_or("gear_core_backend=debug"),
            )
            .init();
        }
        _ => {
            use env_logger::Env;

            env_logger::Builder::from_env(Env::default().default_filter_or("debug")).init();
        }
    }

    let mut tests = Vec::new();

    for path in &opts.input {
        if path.is_dir() {
            for entry in path.read_dir().expect("read_dir call failed").flatten() {
                tests.push(read_test_from_file(&entry.path())?);
            }
        } else {
            tests.push(read_test_from_file(&path)?);
        }
    }

    let total_fixtures: usize = tests.iter().map(|t| t.fixtures.len()).sum();
    let mut total_failed = 0i32;

    println!("Total fixtures: {}", total_fixtures);

    for test in tests {
        for fixture_no in 0..test.fixtures.len() {
            for exp in &test.fixtures[fixture_no].expected {
                let output = match runner::init_fixture(&test, fixture_no) {
                    Ok(initialized_fixture) => match runner::run(initialized_fixture, exp.step) {
                        Ok(mut final_state) => {
                            let mut errors = Vec::new();
                            if !opts.skip_messages {
                                if let Some(messages) = &exp.messages {
                                    if let Err(msg_errors) =
                                        check_messages(&final_state.messages, messages)
                                    {
                                        errors.extend(
                                            msg_errors
                                                .into_iter()
                                                .map(|err| format!("Messages check [{}]", err)),
                                        );
                                    }
                                }
                            }
                            if let Some(log) = &exp.log {
                                if print_log {
                                    for message in &final_state.log {
                                        if let Ok(utf8) = std::str::from_utf8(message.payload()) {
                                            println!("log({})", utf8)
                                        }
                                    }
                                }
                                if let Err(log_errors) = check_messages(&final_state.log, log) {
                                    errors.extend(
                                        log_errors
                                            .into_iter()
                                            .map(|err| format!("Log check [{}]", err)),
                                    );
                                }
                            }
                            if !opts.skip_allocations {
                                if let Some(alloc) = &exp.allocations {
                                    if let Err(alloc_errors) =
                                        check_allocations(&final_state.program_storage, alloc)
                                    {
                                        errors.extend(alloc_errors);
                                    }
                                }
                            }
                            if !opts.skip_memory {
                                if let Some(mem) = &exp.memory {
                                    if let Err(mem_errors) =
                                        check_memory(&mut final_state.program_storage, mem)
                                    {
                                        errors.extend(mem_errors);
                                    }
                                }
                            }

                            if !errors.is_empty() {
                                total_failed += 1;
                                errors.insert(0, format!("{}", color::Fg(color::Red)));
                                errors.insert(errors.len(), format!("{}", style::Reset));
                                errors.join("\n")
                            } else {
                                format!("{}Ok{}", color::Fg(color::Green), style::Reset)
                            }
                        }
                        Err(e) => {
                            total_failed += 1;
                            format!(
                                "{}Running error ({}){}",
                                color::Fg(color::Red),
                                e,
                                style::Reset
                            )
                        }
                    },
                    Err(e) => {
                        total_failed += 1;
                        format!(
                            "{}Initialization error ({}){}",
                            color::Fg(color::Red),
                            e,
                            style::Reset
                        )
                    }
                };

                println!(
                    "Fixture {}{}{}: {}",
                    style::Bold,
                    test.fixtures[fixture_no].title,
                    style::Reset,
                    output
                );
            }
        }
    }

    if total_failed == 0 {
        Ok(())
    } else {
        Err(anyhow!("{} tests failed", total_failed))
    }
}
