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

//! Lazy-pages wasm runtime tests.

use core::mem::size_of;

use ::alloc::collections::BTreeSet;
use common::ProgramStorage;
use gear_backend_common::lazy_pages::{LazyPagesWeights, Status};
use gear_core::memory::{GranularityPage, PageU32Size, PAGE_STORAGE_GRANULARITY};
use rand::{Rng, SeedableRng};

use gear_lazy_pages_common as lazy_pages;

use crate::{HandleKind, benchmarking::utils::PrepareConfig};
use super::*;
use crate::benchmarking::utils as common_utils;
// use super::utils::{self as common_utils, PrepareConfig};

pub fn lazy_pages_charging<T>()
where
    T: Config,
    T::AccountId: Origin,
{
    const MAX_ACCESSES_NUMBER: u32 = 1000;
    const LOAD_PROB: f64 = 1.0 / 2.0;
    const MAX_COST: u64 = 1000;
    const MAX_PAGES_WITH_DATA: u32 = 128;

    let memory = ImportedMemory::max::<T>();
    let size_wasm_pages = WasmPageNumber::new(memory.min_pages).unwrap();
    let size_psg = size_wasm_pages.to_page::<GranularityPage>();
    let gear_in_psg = GranularityPage::size() / PageNumber::size();
    let access_size = size_of::<u32>() as u32;
    let max_addr = size_wasm_pages.offset() - access_size;

    let test = |seed: u64| {
        let mut instrs = vec![];
        let mut read_pages = BTreeSet::new();
        let mut write_pages = BTreeSet::new();
        let mut write_after_read_pages = BTreeSet::new();

        let mut rng = rand_pcg::Pcg32::seed_from_u64(seed);

        let accesses_number = rng.gen_range(1..MAX_ACCESSES_NUMBER);
        for _ in 0..accesses_number {
            let addr = rng.gen_range(0..max_addr) as i32;
            let accessed_pages: BTreeSet<_> = vec![
                GranularityPage::from_offset(addr as u32),
                GranularityPage::from_offset(addr as u32 + access_size - 1),
            ]
            .into_iter()
            .collect();
            if rng.gen_bool(LOAD_PROB) {
                instrs.push(Instruction::I32Const(addr));
                instrs.push(Instruction::I32Load(2, 0));
                instrs.push(Instruction::Drop);

                for page in accessed_pages {
                    if !write_pages.contains(&page) && !write_after_read_pages.contains(&page) {
                        read_pages.insert(page);
                    }
                }
            } else {
                instrs.push(Instruction::I32Const(addr));
                instrs.push(Instruction::I32Const(u32::MAX as i32));
                instrs.push(Instruction::I32Store(2, 0));

                for page in accessed_pages {
                    if !write_pages.contains(&page) {
                        if read_pages.contains(&page) {
                            write_after_read_pages.insert(page);
                        } else if !write_after_read_pages.contains(&page) {
                            write_pages.insert(page);
                        }
                    }
                }
            }
        }

        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            handle_body: Some(body::from_instructions(instrs)),
            ..Default::default()
        });
        let instance = Program::<T>::new(code, vec![]).unwrap();
        let source = instance.caller.into_origin();
        let program_id = ProgramId::from_origin(instance.addr);

        // Append data in storage for some pages.
        for page in (0..rng.gen_range(0..MAX_PAGES_WITH_DATA))
            .map(|_| GranularityPage::new(rng.gen_range(0..size_psg.raw())).unwrap())
        {
            for page in page.to_pages_iter::<PageNumber>() {
                ProgramStorageOf::<T>::set_program_page_data(
                    program_id,
                    page,
                    PageBuf::new_zeroed(),
                );
            }
        }

        let exec = common_utils::prepare_exec::<T>(
            source,
            HandleKind::Handle(program_id),
            vec![],
            0..0,
            Default::default(),
        )
        .unwrap();

        let charged: Vec<(u64, u64)> = (0..2)
            .map(|_i| {
                let mut exec = exec.clone();
                let weights = LazyPagesWeights {
                    read: rng.gen_range(0..MAX_COST),
                    write: rng.gen_range(0..MAX_COST),
                    write_after_read: rng.gen_range(0..MAX_COST),
                };
                exec.block_config.allocations_config.lazy_pages_weights = weights.clone();

                let charged_for_pages = weights.read * gear_in_psg as u64 * read_pages.len() as u64
                    + weights.write * gear_in_psg as u64 * write_pages.len() as u64
                    + weights.write_after_read
                        * gear_in_psg as u64
                        * write_after_read_pages.len() as u64;

                let notes = core_processor::process::<Externalities, ExecutionEnvironment>(
                    &exec.block_config,
                    exec.context,
                    exec.random_data,
                    exec.memory_pages,
                );

                let mut gas_burned = 0;
                for note in notes.into_iter() {
                    match note {
                        JournalNote::GasBurned { amount, .. } => gas_burned = amount,
                        JournalNote::MessageDispatched {
                            outcome:
                                DispatchOutcome::InitFailure { .. }
                                | DispatchOutcome::MessageTrap { .. },
                            ..
                        } => {
                            panic!("Process was not successful")
                        }
                        _ => {}
                    }
                }

                (charged_for_pages, gas_burned)
            })
            .collect();

        assert_eq!(
            charged[0].0.abs_diff(charged[1].0),
            charged[0].1.abs_diff(charged[1].1)
        );
    };

    for seed in 0..300 {
        test(seed);
    }
}

pub fn lazy_pages_charging_special<T>()
where
    T: Config,
    T::AccountId: Origin,
{
    let psg = PAGE_STORAGE_GRANULARITY as i32;
    let read_cost = 1;
    let write_cost = 10;
    let write_after_read_cost = 100;

    let test = |instrs, expected| {
        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            handle_body: Some(body::from_instructions(instrs)),
            ..Default::default()
        });
        let instance = Program::<T>::new(code, vec![]).unwrap();
        let exec = common_utils::prepare_exec::<T>(
            instance.caller.into_origin(),
            HandleKind::Handle(ProgramId::from_origin(instance.addr)),
            vec![],
            0..0,
            Default::default(),
        )
        .unwrap();

        let charged: Vec<u64> = (0..2)
            .map(|i| {
                let mut exec = exec.clone();
                let weights = LazyPagesWeights {
                    read: i,
                    write: 10 * i,
                    write_after_read: 100 * i,
                };
                exec.block_config.allocations_config.lazy_pages_weights = weights;

                let notes = core_processor::process::<Externalities, ExecutionEnvironment>(
                    &exec.block_config,
                    exec.context,
                    exec.random_data,
                    exec.memory_pages,
                );

                let mut gas_burned = 0;
                for note in notes.into_iter() {
                    match note {
                        JournalNote::GasBurned { amount, .. } => gas_burned = amount,
                        JournalNote::MessageDispatched {
                            outcome:
                                DispatchOutcome::InitFailure { .. }
                                | DispatchOutcome::MessageTrap { .. },
                            ..
                        } => {
                            panic!("Process was not successful")
                        }
                        _ => {}
                    }
                }

                gas_burned
            })
            .collect();

        let k = GranularityPage::size() / PageNumber::size();
        assert_eq!(
            charged[1].checked_sub(charged[0]).unwrap(),
            expected * k as u64
        );
    };

    test(
        vec![
            // Read 0st and 1st psg pages
            Instruction::I32Const(psg - 1),
            Instruction::I32Load(2, 0),
            Instruction::Drop,
            // Write after read 1st psg page
            Instruction::I32Const(psg),
            Instruction::I32Const(42),
            Instruction::I32Store(2, 0),
        ],
        2 * read_cost + write_after_read_cost,
    );

    test(
        vec![
            // Read 0st and 1st psg pages
            Instruction::I32Const(psg - 1),
            Instruction::I32Load(2, 0),
            Instruction::Drop,
            // Write after read 0st and 1st psg page
            Instruction::I32Const(psg - 3),
            Instruction::I32Const(42),
            Instruction::I32Store(2, 0),
        ],
        2 * read_cost + 2 * write_after_read_cost,
    );

    test(
        vec![
            // Read 0st and 1st psg pages
            Instruction::I32Const(psg - 1),
            Instruction::I32Load(2, 0),
            Instruction::Drop,
            // Write after read 1st psg page and write 2st psg page
            Instruction::I32Const(2 * psg - 1),
            Instruction::I32Const(42),
            Instruction::I32Store(2, 0),
        ],
        2 * read_cost + write_after_read_cost + write_cost,
    );

    test(
        vec![
            // Read 1st psg page
            Instruction::I32Const(psg),
            Instruction::I32Load(2, 0),
            Instruction::Drop,
            // Write after read 1st psg page and write 0st psg page
            Instruction::I32Const(psg - 1),
            Instruction::I32Const(42),
            Instruction::I32Store(2, 0),
        ],
        read_cost + write_after_read_cost + write_cost,
    );

    test(
        vec![
            // Read 1st psg page
            Instruction::I32Const(psg),
            Instruction::I32Load(2, 0),
            Instruction::Drop,
            // Read 0st and 1st psg pages, but pay only for 0st.
            Instruction::I32Const(psg - 1),
            Instruction::I32Load(2, 0),
            Instruction::Drop,
        ],
        2 * read_cost,
    );

    test(
        vec![
            // Write 0st and 1st psg page
            Instruction::I32Const(psg - 1),
            Instruction::I32Const(42),
            Instruction::I32Store(2, 0),
            // Write 1st and 2st psg pages, but pay only for 2st page
            Instruction::I32Const(2 * psg - 1),
            Instruction::I32Const(42),
            Instruction::I32Store(2, 0),
        ],
        3 * write_cost,
    );

    test(
        vec![
            // Write 0st and 1st psg page
            Instruction::I32Const(psg - 1),
            Instruction::I32Const(42),
            Instruction::I32Store(2, 0),
            // Read 1st and 2st psg pages, but pay only for 2st page
            Instruction::I32Const(2 * psg - 1),
            Instruction::I32Load(2, 0),
            Instruction::Drop,
        ],
        read_cost + 2 * write_cost,
    );
}

pub fn lazy_pages_gas_exceed<T>()
where
    T: Config,
    T::AccountId: Origin,
{
    let instrs = vec![
        Instruction::I32Const(0),
        Instruction::I32Const(42),
        Instruction::I32Store(2, 0),
    ];
    let code = WasmModule::<T>::from(ModuleDefinition {
        memory: Some(ImportedMemory::max::<T>()),
        handle_body: Some(body::from_instructions(instrs)),
        ..Default::default()
    });
    let instance = Program::<T>::new(code, vec![]).unwrap();
    let source = instance.caller.into_origin();
    let origin = instance.addr;

    // Calculate how much gas burned, when lazy pages costs are zero.
    let gas_burned = {
        let mut exec = common_utils::prepare_exec::<T>(
            source,
            HandleKind::Handle(ProgramId::from_origin(origin)),
            vec![],
            0..0,
            Default::default(),
        )
        .unwrap();
        exec.block_config.allocations_config.lazy_pages_weights = LazyPagesWeights {
            read: 0,
            write: 0,
            write_after_read: 0,
        };

        let notes = core_processor::process::<Externalities, ExecutionEnvironment>(
            &exec.block_config,
            exec.context,
            exec.random_data,
            exec.memory_pages,
        );

        let mut gas_burned = None;
        for note in notes.into_iter() {
            match note {
                JournalNote::GasBurned { amount, .. } => gas_burned = Some(amount),
                JournalNote::MessageDispatched {
                    outcome:
                        DispatchOutcome::InitFailure { .. } | DispatchOutcome::MessageTrap { .. },
                    ..
                } => {
                    panic!("Process was not successful")
                }
                _ => {}
            }
        }

        gas_burned.unwrap()
    };

    // Check gas limit exceeded.
    {
        let mut exec = common_utils::prepare_exec::<T>(
            source,
            HandleKind::Handle(ProgramId::from_origin(origin)),
            vec![],
            0..0,
            PrepareConfig {
                gas_limit: gas_burned,
                ..Default::default()
            },
        )
        .unwrap();
        exec.block_config.allocations_config.lazy_pages_weights = LazyPagesWeights {
            read: 0,
            write: 1,
            write_after_read: 0,
        };

        let notes = core_processor::process::<Externalities, ExecutionEnvironment>(
            &exec.block_config,
            exec.context,
            exec.random_data,
            exec.memory_pages,
        );

        for note in notes.into_iter() {
            match note {
                JournalNote::MessageDispatched {
                    outcome: DispatchOutcome::MessageTrap { .. },
                    ..
                } => {}
                JournalNote::MessageDispatched { .. } => {
                    panic!("Gas limit exceeded must lead to message trap");
                }
                _ => {}
            }
        }

        assert_eq!(lazy_pages::get_status().unwrap(), Status::GasLimitExceeded);
    };

    // Check gas allowance exceeded.
    {
        let mut exec = common_utils::prepare_exec::<T>(
            source,
            HandleKind::Handle(ProgramId::from_origin(origin)),
            vec![],
            0..0,
            PrepareConfig {
                gas_allowance: gas_burned,
                ..Default::default()
            },
        )
        .unwrap();
        exec.block_config.allocations_config.lazy_pages_weights = LazyPagesWeights {
            read: 0,
            write: 1,
            write_after_read: 0,
        };

        let notes = core_processor::process::<Externalities, ExecutionEnvironment>(
            &exec.block_config,
            exec.context,
            exec.random_data,
            exec.memory_pages,
        );

        for note in notes.into_iter() {
            match note {
                JournalNote::StopProcessing { .. } => {}
                _ => {
                    panic!("Gas allowance exceeded must lead to stop processing");
                }
            }
        }

        assert_eq!(
            lazy_pages::get_status().unwrap(),
            Status::GasAllowanceExceeded
        );
    };
}

// TODO: add test which check lazy-pages charging and sys-calls interaction (issue +_+_+).
