mod runner;
mod sample;

use anyhow::anyhow;
use gear_core::{
    memory::PageNumber,
    message::Message,
    program::{Program, ProgramId},
};
use sample::Test;
use std::fs;

fn check_messages(
    messages: &[Message],
    expected_messages: &[sample::Message],
) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();
    if expected_messages.len() != messages.len() {
        errors.push(format!("Expectation error (messages count doesn't match)"));
    } else {
        &expected_messages
            .iter()
            .zip(messages.iter().rev())
            .for_each(|(exp, msg)| {
                if exp.destination != msg.dest.0 {
                    errors.push(
                        format!(
                            "Expectation error (destination doesn't match, expected: {}, found: {})", 
                            exp.destination, msg.dest.0
                        )
                    );
                }
                if &exp.payload.clone().into_raw() != &msg.payload.clone().into_raw() {
                    errors.push(format!("Expectation error (payload doesn't match)"));
                }
            });
    }

    if errors.is_empty() { Ok(()) }
    else { Err(errors) }
}

fn check_allocations(
    pages: &Vec<(PageNumber, ProgramId)>,
    expected_pages: &Vec<sample::AllocationStorage>,
) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();
    if expected_pages.len() != pages.len() {
        errors.push(format!("Expectation error (pages count doesn't match)\n"));
    } else {
        &expected_pages
            .iter()
            .zip(pages.iter())
            .for_each(|(exp, page)| {
                if exp.page_num != page.0.raw() {
                    errors.push(
                        format!(
                            "Expectation error (PageNumber doesn't match, expected: {}, found: {})", 
                            exp.page_num, page.0.raw()
                        )
                    );
                }
                if exp.program_id != page.1 .0 {
                    errors.push(
                        format!(
                            "Expectation error (ProgramId doesn't match, expected: {}, found: {})\n", 
                            exp.program_id, page.1.0
                        )
                    );
                }
            });
    }

    if errors.is_empty() { Ok(()) }
    else { Err(errors) }
}

fn check_memory(
    program_storage: &mut Vec<Program>,
    expected_memory: &Vec<sample::BytesAt>,
) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();
    for case in expected_memory {
        for p in 0..program_storage.len() {
            if program_storage[p].id().0 == case.id {
                if &program_storage[p].static_pages()[case.address..case.address + case.bytes.len()]
                    != case.bytes
                {
                    errors.push(format!("Expectation error (Memory doesn't match)"));
                }
            }
        }
    }
    if errors.is_empty() { Ok(()) }
    else { Err(errors) }
}

fn read_test_from_file<P: AsRef<std::path::Path>>(path: P) -> anyhow::Result<Test> {
    let file = fs::File::open(path)?;
    let u = serde_json::from_reader(file)?;
    Ok(u)
}

pub fn main() -> anyhow::Result<()> {
    let mut tests = Vec::new();

    for f in std::env::args().skip(1) {
        if fs::metadata(&f).map(|m| m.is_dir()).unwrap_or_else(|e| {
            println!("Error accessing {}: {}", f, e);
            false
        }) {
            continue;
        }

        tests.push(read_test_from_file(&f)?);
    }

    let total_fixtures: usize = tests.iter().map(|t| t.fixtures.len()).sum();
    let mut total_failed = 0i32;

    println!("Total fixtures: {}", total_fixtures);

    for test in tests {
        for fixture_no in 0..test.fixtures.len() {
            let output = match runner::init_fixture(&test, fixture_no) {
                Ok(initialized_fixture) => {
                    match runner::run(initialized_fixture, test.fixtures[fixture_no].expected.step)
                    {
                        Ok(mut final_state) => {
                            let mut errors = Vec::new();
                            if let Some(messages) = &test.fixtures[fixture_no].expected.messages {
                                if let Err(msg_errors) = check_messages(&final_state.log, messages) {
                                    errors.extend(msg_errors);
                                }
                            }
                            if let Some(alloc) = &test.fixtures[fixture_no].expected.allocations {
                                if let Err(alloc_errors) = check_allocations(&mut &final_state.allocation_storage, alloc) {
                                    errors.extend(alloc_errors);
                                }

                            }
                            if let Some(memory) = &test.fixtures[fixture_no].expected.memory {
                                if let Err(mem_errors) = check_memory(&mut final_state.program_storage, memory) {
                                    errors.extend(mem_errors);
                                }
                            }

                            if errors.len() > 0 {
                                total_failed += 1;
                                errors.join("\n")
                            } else { 
                                "Ok".to_string() 
                            }
                            
                        }
                        Err(e) => {
                            total_failed += 1;
                            format!("Running error ({})", e)
                        }
                    }
                }
                Err(e) => {
                    total_failed += 1;
                    format!("Initialization error ({})", e)
                }
            };

            println!("Fixture {}: {}", test.fixtures[fixture_no].title, output);
        }
    }

    if total_failed == 0 {
        Ok(())
    } else {
        Err(anyhow!("{} tests failed", total_failed))
    }
}
