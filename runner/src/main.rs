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
    res: &mut String,
    messages: &Vec<Message>,
    expected_messages: &Vec<sample::Message>,
) {
    let mut err = 0;
    *res = format!("{} Messages:\n", res);
    if expected_messages.len() != messages.len() {
        *res = format!(
            "{}  Expectation error (messages count doesn't match)\n",
            res
        );
        err += 1;
    } else {
        &expected_messages
            .iter()
            .zip(messages.iter().rev())
            .for_each(|(exp, msg)| {
                if exp.destination != msg.dest.0 {
                    *res = format!("{}  Expectation error (destination doesn't match)\n", res);
                    err += 1;
                }
                if &exp.payload.clone().into_raw() != &msg.payload.clone().into_raw() {
                    *res = format!("{}Expectation error (payload doesn't match)\n", res);
                    err += 1;
                }
            });
    }
    if err == 0 {
        *res = format!("{}  Ok\n", res);
    }
}

fn check_allocation(
    res: &mut String,
    pages: &Vec<(PageNumber, ProgramId)>,
    expected_pages: &Vec<sample::AllocationStorage>,
) {
    let mut err = 0;
    *res = format!("{} Allocation:\n", res);
    if expected_pages.len() != pages.len() {
        *res = format!("{}  Expectation error (pages count doesn't match)\n", res);
        err += 1;
    } else {
        &expected_pages
            .iter()
            .zip(pages.iter())
            .for_each(|(exp, page)| {
                if exp.page_num != page.0.raw() {
                    *res = format!("{}  Expectation error (PageNumber doesn't match)\n", res);
                    err += 1;
                }
                if exp.program_id != page.1 .0 {
                    *res = format!("{}  Expectation error (ProgramId doesn't match)\n", res);
                    err += 1;
                }
            });
    }
    if err == 0 {
        *res = format!("{}  Ok\n", res);
    }
}

fn check_memory(
    res: &mut String,
    program_storage: &mut Vec<Program>,
    expected_memory: &Vec<sample::BytesAt>,
) {
    let mut err = 0;
    for case in expected_memory {
        for p in 0..program_storage.len() {
            if program_storage[p].id().0 == case.id {
                *res = format!(
                    "{} Memory (id: {}, address: 0x{:x})\n",
                    res, case.id, case.address
                );
                if &program_storage[p].static_pages()[case.address..case.address + case.bytes.len()]
                    != case.bytes
                {
                    dbg!(
                        &program_storage[p].static_pages()
                            [case.address..case.address + case.bytes.len()]
                    );
                    dbg!(&case.bytes);
                    *res = format!("{}  Expectation error (Memory doesn't match)\n", res);
                    err += 1;
                }
            }
        }
    }
    if err == 0 {
        *res = format!("{}  Ok\n", res);
    }
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
            for exp in &test.fixtures[fixture_no].expected {
                let output = match runner::init_fixture(&test, fixture_no) {
                    Ok(initialized_fixture) => match runner::run(initialized_fixture, exp.step) {
                        Ok(mut final_state) => {
                            let mut res = String::from("\n");
                            if let Some(messages) = &exp.messages {
                                check_messages(&mut res, &final_state.log, messages);
                            }
                            if let Some(alloc) = &exp.allocation {
                                check_allocation(&mut res, &final_state.allocation_storage, alloc);
                            }
                            if let Some(memory) = &exp.memory {
                                check_memory(&mut res, &mut final_state.program_storage, memory);
                            }

                            res
                        }
                        Err(e) => {
                            total_failed += 1;
                            format!("Running error ({})", e)
                        }
                    },
                    Err(e) => {
                        total_failed += 1;
                        format!("Initialization error ({})", e)
                    }
                };
                
                println!("Fixture {}: {}", test.fixtures[fixture_no].title, output);
            }
        }
    }

    if total_failed == 0 {
        Ok(())
    } else {
        Err(anyhow!("{} tests failed", total_failed))
    }
}
