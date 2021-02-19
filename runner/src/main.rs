#![feature(num_as_ne_bytes)]
mod runner;
mod sample;

use anyhow::anyhow;
use gear_core::{memory::PageNumber, message::Message, program::ProgramId};
use sample::Test;
use std::collections::HashMap;
use std::fs;

fn check_messages(messages: &Vec<Message>, expected_messages: &Vec<sample::Message>) -> String {
    let mut res = format!("\nMessages: ");
    if expected_messages.len() != messages.len() {
        res = format!("{}Expectation error (messages count doesn't match)\n", res);
    } else {
        &expected_messages
            .iter()
            .zip(messages.iter().rev())
            .for_each(|(exp, msg)| {
                if exp.destination != msg.dest.0 {
                    res = format!("{}Expectation error (destination doesn't match)\n", res);
                }
                if &exp.payload.raw() != &msg.payload.clone().into_raw() {
                    res = format!("{}Expectation error (payload doesn't match)\n", res);
                }
            });
    }
    res
}

// fn check_allocation(
//     pages: &Vec<(PageNumber, ProgramId)>,
//     expected_messages: &Vec<sample::AllocationStorage>,
// ) -> String {
//     res
// }

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
                        Ok(final_state) => {
                            let res = check_messages(
                                &final_state.log,
                                &test.fixtures[fixture_no].expected.messages,
                            );
                            res
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
