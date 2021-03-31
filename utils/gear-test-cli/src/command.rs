use rti::ext::{ExtAllocationStorage, ExtProgramStorage};
use sc_cli::{CliConfiguration, SharedParams};
use sc_service::Configuration;
use std::fs;

use gear_core::{
    memory::PageNumber,
    message::Message,
    program::{Program, ProgramId},
    storage::{AllocationStorage, ProgramStorage},
};

use crate::sample::Test;
use crate::test_runner;
use crate::GearTestCmd;

fn read_test_from_file<P: AsRef<std::path::Path>>(path: P) -> Result<Test, std::io::Error> {
    let file = fs::File::open(path)?;
    let u = serde_json::from_reader(file)?;
    Ok(u)
}

fn check_messages(
    messages: &[Message],
    expected_messages: &[crate::sample::Message],
) -> Result<(), Vec<String>> {
    dbg!(&messages);
    dbg!(&expected_messages);
    let mut errors = Vec::new();
    if expected_messages.len() != messages.len() {
        errors.push("Expectation error (messages count doesn't match)".to_string());
    } else {
        expected_messages
            .iter()
            .zip(messages.iter())
            .for_each(|(exp, msg)| {
                if ProgramId::from(exp.destination) != msg.dest {
                    errors.push(format!(
                        "Expectation error (destination doesn't match, expected: {}, found: {:?})",
                        exp.destination, msg.dest
                    ));
                }
                if exp.payload.clone().into_raw() != msg.payload.clone().into_raw() {
                    errors.push("Expectation error (payload doesn't match)".to_string());
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
    ext: &mut sp_io::TestExternalities,
    allocations: &ExtAllocationStorage,
    expected_pages: &[crate::sample::AllocationStorage],
) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();
    ext.execute_with(|| {

        for page in expected_pages {
            if let Some(program_id) = allocations.get(page.page_num.into()) {
                if program_id != page.program_id.into() {
                    errors.push(format!(
                        "Expectation error (ProgramId doesn't match, expected: {:?}, found: {:?})\n",
                        program_id, page.program_id
                    ));
                }
            }
        }
        // expected_pages
        //     .iter()
        //     .zip(pages.iter())
        //     .for_each(|(exp, page)| {
        //         if exp.page_num != page.0.raw() {
        //             errors.push(format!(
        //                 "Expectation error (PageNumber doesn't match, expected: {}, found: {})",
        //                 exp.page_num,
        //                 page.0.raw()
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
    })
}

fn check_memory(
    ext: &mut sp_io::TestExternalities,
    persistent_memory: &[u8],
    program_storage: &ExtProgramStorage,
    expected_memory: &[crate::sample::MemoryVariant],
) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();
    for case in expected_memory {
        match case {
            crate::sample::MemoryVariant::Static(case) => {
                ext.execute_with(|| {
                    if let Some(id) = case.program_id {
                        if let Some(program) = program_storage.get(ProgramId::from(id)) {
                            if program.id() == ProgramId::from(id)
                                && program.static_pages()[case.address..case.address + case.bytes.len()]
                                    != case.bytes
                            {
                                errors.push(
                                    "Expectation error (Static memory doesn't match)".to_string(),
                                );
                            }
                        }
                    }
                });
                
            }
            crate::sample::MemoryVariant::Shared(case) => {
                let offset = 256 * 65536;
                if persistent_memory
                    [case.address - offset..case.address - offset + case.bytes.len()]
                    != case.bytes
                {
                    errors.push("Expectation error (Shared memory doesn't match)".to_string());
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

impl GearTestCmd {
    /// Runs the command and benchmarks the chain.
    pub fn run(&self, config: Configuration) -> sc_cli::Result<()> {
        let mut total_fixtures: usize = 0;
        let mut total_failed = 0i32;
        let mut tests = Vec::new();
        if let Some(input) = &self.input {
            if input.is_dir() {
                for entry in input.read_dir().expect("read_dir call failed") {
                    if let Ok(entry) = entry {
                        tests.push(read_test_from_file(&entry.path())?);
                    }
                }
            } else {
                tests.push(read_test_from_file(&input)?);
            }

            total_fixtures = tests.iter().map(|t| t.fixtures.len()).sum();
            println!("Total fixtures: {}", total_fixtures);
        }

        for test in tests {
            for fixture_no in 0..test.fixtures.len() {
                let mut ext = crate::test_runner::new_test_ext();
                for exp in &test.fixtures[fixture_no].expected {
                    let output = match test_runner::init_fixture(&mut ext, &test, fixture_no) {
                        Ok(initialized_fixture) => {
                            match test_runner::run(&mut ext, initialized_fixture, exp.step) {
                                Ok((mut final_state, persistent_memory)) => {
                                    let mut errors = Vec::new();
                                    if let Some(messages) = &exp.messages {
                                        if let Err(msg_errors) =
                                            check_messages(&final_state.message_queue, messages)
                                        {
                                            errors.extend(msg_errors);
                                        }
                                    }
                                    if let Some(alloc) = &exp.allocations {
                                        if let Err(alloc_errors) = check_allocations(&mut ext,
                                            &final_state.allocation_storage,
                                            alloc,
                                        ) {
                                            errors.extend(alloc_errors);
                                        }
                                    }
                                    if let Some(mem) = &exp.memory {
                                        if let Err(mem_errors) = check_memory(&mut ext,
                                            &persistent_memory,
                                            &mut final_state.program_storage,
                                            mem,
                                        ) {
                                            errors.extend(mem_errors);
                                        }
                                    }

                                    if !errors.is_empty() {
                                        total_failed += 1;
                                        errors.join("\n")
                                    } else {
                                        format!("Ok")
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
                            format!("Initialization error ({})", e,)
                        }
                    };

                    println!("Fixture {}: {}", test.fixtures[fixture_no].title, output);
                }
            }
        }

        Ok(())
    }
}

impl CliConfiguration for GearTestCmd {
    fn shared_params(&self) -> &SharedParams {
        &self.shared_params
    }
}
