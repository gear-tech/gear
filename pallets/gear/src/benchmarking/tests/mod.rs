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

//! This module contains pallet tests usually defined under "std" feature in the separate `tests` module.
//! The reason of moving them here is an ability to run these tests with different execution environments
//! (native or wasm, i.e. using wasmi or sandbox executors). When "std" is enabled we can run them on wasmi,
//! when it's not (only "runtime-benchmarks") - sandbox will be turned on.

use core::mem::size_of;

use ::alloc::collections::BTreeSet;
use gear_backend_common::lazy_pages::LazyPagesWeights;
use gear_core::memory::GranularityPage;
use rand::{Rng, SeedableRng};

use crate::HandleKind;

use super::{utils::prepare_exec, *};

pub mod syscalls_integrity;
mod utils;

pub fn check_lazy_pages_charging<T>()
where
    T: Config,
    T::AccountId: Origin,
{
    let test = |seed: u64| {
        const MAX_ACCESSES_NUMBER: u32 = 1000;
        const LOAD_PROB: f64 = 1.0 / 2.0;
        const MAX_COST: u64 = 1000;

        let gear_in_psg = GranularityPage::size() / PageNumber::size();
        let access_size = size_of::<u32>() as u32;
        let max_addr = ImportedMemory::max::<T>().min_pages * WasmPageNumber::size() - access_size;

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
            if rng.gen_bool(1.0 / 2.0) {
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
        let exec = prepare_exec::<T>(
            instance.caller.into_origin(),
            HandleKind::Handle(ProgramId::from_origin(instance.addr)),
            vec![],
            0,
            0..0,
            None,
        )
        .unwrap();

        let charged: Vec<(u64, u64)> = (0..2)
            .map(|_i| {
                let mut exec = exec.clone();
                let weights = LazyPagesWeights {
                    read: rng.gen_range(0..MAX_COST),
                    write: rng.gen_range(0..MAX_COST),
                    write_after_read: rng.gen_range(0..MAX_COST),
                    read_data_from_storage: rng.gen_range(0..MAX_COST),
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

    for seed in 0..100 {
        test(seed);
    }
}

// +_+_+
#[allow(unused)]
pub fn check_lazy_pages_charging_special() {
    // let psg = PAGE_STORAGE_GRANULARITY as i32;
    // let instrs = vec![
    //     Instruction::I32Const(0),
    //     Instruction::I32Load(2, 0),
    //     Instruction::Drop,
    //     Instruction::I32Const(psg - 1),
    //     Instruction::I32Load(2, 0),
    //     Instruction::Drop,
    //     Instruction::I32Const(psg * 10 - 1),
    //     Instruction::I32Load(2, 0),
    //     Instruction::Drop,
    // ];
    // let code = WasmModule::<T>::from(ModuleDefinition {
    //     memory: Some(ImportedMemory::max::<T>()),
    //     handle_body: Some(body::from_instructions(instrs)),
    //     ..Default::default()
    // });
    // let instance = Program::<T>::new(code, vec![]).unwrap();
    // let exec = prepare_exec::<T>(
    //     instance.caller.into_origin(),
    //     HandleKind::Handle(ProgramId::from_origin(instance.addr)),
    //     vec![],
    //     0,
    //     0..0,
    //     None,
    // )
    // .unwrap();

    // {
    //     let mut exec = exec.clone();
    //     exec.block_config.allocations_config.lazy_pages_weights = LazyPagesWeights {
    //         read: 1,
    //         write: 10,
    //         write_after_read: 100,
    //         read_data_from_storage: 100,
    //     };
    //     let res = core_processor::process::<Ext, ExecutionEnvironment>(
    //         &exec.block_config,
    //         exec.context,
    //         exec.random_data,
    //         exec.memory_pages,
    //     );
    //     log::trace!("lol = {:?}", res);
    // }
    // {
    //     let mut exec = exec.clone();
    //     exec.block_config.allocations_config.lazy_pages_weights = LazyPagesWeights {
    //         read: 0,
    //         write: 0,
    //         write_after_read: 0,
    //         read_data_from_storage: 0,
    //     };
    //     let res = core_processor::process::<Ext, ExecutionEnvironment>(
    //         &exec.block_config,
    //         exec.context,
    //         exec.random_data,
    //         exec.memory_pages,
    //     );
    //     log::trace!("kek = {:?}", res);
    // }
}
