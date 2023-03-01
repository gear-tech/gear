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
use codec::MaxEncodedLen;
use common::ProgramStorage;
use gear_backend_common::lazy_pages::Status;
use gear_core::memory::{GranularityPage, MemoryInterval, PageU32Size, PAGE_STORAGE_GRANULARITY};
use rand::{Rng, SeedableRng};

use gear_lazy_pages_common as lazy_pages;

use super::*;
use crate::{
    benchmarking::{utils as common_utils, utils::PrepareConfig},
    HandleKind,
};

#[derive(Debug, Default)]
struct PageSets<P: PageU32Size> {
    signal_read: BTreeSet<P>,
    signal_write: BTreeSet<P>,
    signal_write_after_read: BTreeSet<P>,
    syscall_read: BTreeSet<P>,
    syscall_write: BTreeSet<P>,
    with_data_pages: BTreeSet<P>,
}

impl<P: PageU32Size> PageSets<P> {
    fn with_accessed(i: MemoryInterval, mut f: impl FnMut(P)) {
        let start = P::from_offset(i.offset);
        let end = P::from_offset(i.offset.checked_add(i.size.saturating_sub(1)).unwrap());
        for page in start.iter_end_inclusive(end).unwrap() {
            f(page);
        }
    }

    fn is_any_read(&self, page: P) -> bool {
        self.signal_read.contains(&page) || self.syscall_read.contains(&page)
    }

    fn is_any_write(&self, page: P) -> bool {
        self.signal_write.contains(&page)
            || self.signal_write_after_read.contains(&page)
            || self.syscall_write.contains(&page)
    }

    fn add_signal_read(&mut self, i: MemoryInterval) {
        Self::with_accessed(i, |page| {
            if !self.is_any_read(page) && !self.is_any_write(page) {
                self.signal_read.insert(page);
            }
        });
    }

    fn add_signal_write(&mut self, i: MemoryInterval) {
        Self::with_accessed(i, |page| {
            if !self.is_any_write(page) {
                if self.is_any_read(page) {
                    self.signal_write_after_read.insert(page);
                } else {
                    self.signal_write.insert(page);
                }
            }
        });
    }

    fn add_syscall_read(&mut self, i: MemoryInterval) {
        Self::with_accessed(i, |page| {
            if !self.is_any_read(page) && !self.is_any_write(page) {
                self.syscall_read.insert(page);
            }
        });
    }

    fn add_syscall_write(&mut self, i: MemoryInterval) {
        Self::with_accessed(i, |page| {
            if !self.is_any_write(page) {
                self.syscall_write.insert(page);
            }
        });
    }

    fn accessed_pages(&self) -> BTreeSet<P> {
        let mut accessed_pages = self.signal_read.clone();
        accessed_pages.extend(self.signal_write.iter().copied());
        accessed_pages.extend(self.signal_write_after_read.iter().copied());
        accessed_pages.extend(self.syscall_read.iter().copied());
        accessed_pages.extend(self.syscall_write.iter().copied());
        accessed_pages
    }

    fn loaded_pages_count(&self) -> GranularityPage {
        (self
            .accessed_pages()
            .intersection(&self.with_data_pages)
            .count() as u16)
            .into()
    }

    fn charged_for_pages(&self, costs: &PageCosts) -> u64 {
        let costs = costs.lazy_pages_weights();

        let signal_read_amount = (self.signal_read.len() as u16).into();
        let signal_write_amount = (self.signal_write.len() as u16).into();
        let signal_write_after_read_amount = (self.signal_write_after_read.len() as u16).into();
        let syscall_read_amount = (self.syscall_read.len() as u16).into();
        let syscall_write_amount = (self.syscall_write.len() as u16).into();

        let read_signal_charged = costs.signal_read.calc(signal_read_amount);
        let write_signal_charged = costs.signal_write.calc(signal_write_amount);
        let write_after_read_signal_charged = costs
            .signal_write_after_read
            .calc(signal_write_after_read_amount);
        let syscall_read_charged = costs.host_func_read.calc(syscall_read_amount);
        let syscall_write_charged = costs.host_func_write.calc(syscall_write_amount);

        let charged_for_data_load = costs.load_page_storage_data.calc(self.loaded_pages_count());

        read_signal_charged
            .checked_add(write_signal_charged)
            .unwrap()
            .checked_add(write_after_read_signal_charged)
            .unwrap()
            .checked_add(syscall_read_charged)
            .unwrap()
            .checked_add(syscall_write_charged)
            .unwrap()
            .checked_add(charged_for_data_load)
            .unwrap()
    }
}

pub fn lazy_pages_charging<T>()
where
    T: Config,
    T::AccountId: Origin,
{
    const MAX_ACCESSES_NUMBER: u32 = 1000;
    const MAX_COST: u64 = 1000;
    const MAX_PAGES_WITH_DATA: u32 = 128;

    let (load_prob, store_prob, syscall_prob) = (4, 4, 2);
    let prob_max = load_prob + store_prob + syscall_prob;

    let memory = ImportedMemory::max::<T>();
    let size_wasm_pages = WasmPage::new(memory.min_pages).unwrap();
    let size_psg = size_wasm_pages.to_page::<GranularityPage>();
    let access_size = size_of::<u32>() as u32;
    let max_addr = size_wasm_pages.offset();

    let test = |seed: u64| {
        let mut rng = rand_pcg::Pcg32::seed_from_u64(seed);
        let mut instrs = vec![];
        let mut page_sets = PageSets::default();

        // Generate different read and write accesses.
        for _ in 0..rng.gen_range(1..MAX_ACCESSES_NUMBER) {
            let prob_number = rng.gen_range(0..prob_max);
            if prob_number < load_prob {
                // Generate load
                let addr = rng.gen_range(0..max_addr - access_size) as i32;
                instrs.push(Instruction::I32Const(addr));
                instrs.push(Instruction::I32Load(2, 0));
                instrs.push(Instruction::Drop);

                page_sets.add_signal_read(MemoryInterval {
                    offset: addr as u32,
                    size: access_size,
                })
            } else if prob_number >= load_prob + store_prob {
                // Generate syscall
                // We use syscall random here, because it has read and write access,
                // and cannot cause errors because of input params
                let subject_size = gsys::Hash::max_encoded_len() as u32;
                let bn_random_size = core::mem::size_of::<gsys::BlockNumberWithHash>() as u32;

                let subject_ptr = rng.gen_range(0..max_addr - subject_size) as i32;
                let bn_random_ptr = rng.gen_range(0..max_addr - bn_random_size) as i32;

                instrs.push(Instruction::I32Const(subject_ptr));
                instrs.push(Instruction::I32Const(bn_random_ptr));
                instrs.push(Instruction::Call(0));

                page_sets.add_syscall_read(MemoryInterval {
                    offset: subject_ptr as u32,
                    size: subject_size,
                });
                page_sets.add_syscall_write(MemoryInterval {
                    offset: bn_random_ptr as u32,
                    size: bn_random_size,
                });
            } else {
                // Generate store
                let addr = rng.gen_range(0..max_addr - access_size) as i32;
                instrs.push(Instruction::I32Const(addr));
                instrs.push(Instruction::I32Const(u32::MAX as i32));
                instrs.push(Instruction::I32Store(2, 0));

                page_sets.add_signal_write(MemoryInterval {
                    offset: addr as u32,
                    size: access_size,
                })
            }
        }

        // Upload program with code
        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![SysCallName::Random],
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
            page_sets.with_data_pages.insert(page);
            for page in page.to_pages_iter::<GearPage>() {
                ProgramStorageOf::<T>::set_program_page_data(
                    program_id,
                    page,
                    PageBuf::new_zeroed(),
                );
            }
        }

        // execute program with random page costs
        let mut run = |_| {
            let mut exec = common_utils::prepare_exec::<T>(
                source,
                HandleKind::Handle(program_id),
                vec![],
                0..0,
                Default::default(),
            )
            .unwrap();

            exec.block_config.page_costs.signal_read = rng.gen_range(0..MAX_COST).into();
            exec.block_config.page_costs.signal_write = rng.gen_range(0..MAX_COST).into();
            exec.block_config.page_costs.signal_write_after_read =
                rng.gen_range(0..MAX_COST).into();
            exec.block_config.page_costs.host_func_read = rng.gen_range(0..MAX_COST).into();
            exec.block_config.page_costs.host_func_write = rng.gen_range(0..MAX_COST).into();
            exec.block_config.page_costs.host_func_write_after_read =
                rng.gen_range(0..MAX_COST).into();
            exec.block_config.page_costs.load_page_data = rng.gen_range(0..MAX_COST).into();
            exec.block_config.page_costs.upload_page_data = rng.gen_range(0..MAX_COST).into();

            let charged_for_pages = page_sets.charged_for_pages(&exec.block_config.page_costs);

            let notes = core_processor::process::<ExecutionEnvironment>(
                &exec.block_config,
                exec.context,
                exec.random_data,
                exec.memory_pages,
            )
            .unwrap_or_else(|e| unreachable!("core-processor logic invalidated: {}", e));

            let mut gas_burned = 0;
            for note in notes.into_iter() {
                match note {
                    JournalNote::GasBurned { amount, .. } => gas_burned = amount,
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

            (charged_for_pages, gas_burned)
        };

        // Difference between gas burned in two runs must be equal to difference,
        // between gas burned for pages accesses and data loading, because in `run`
        // only `page_costs` is different.
        let (charged_for_pages1, gas_burned1) = run(0);
        let (charged_for_pages2, gas_burned2) = run(1);
        assert_eq!(
            charged_for_pages1.abs_diff(charged_for_pages2),
            gas_burned1.abs_diff(gas_burned2)
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
    let read_cost = 1u64;
    let write_cost = 10u64;
    let write_after_read_cost = 100u64;

    let test = |instrs, expected| {
        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            handle_body: Some(body::from_instructions(instrs)),
            ..Default::default()
        });
        let instance = Program::<T>::new(code, vec![]).unwrap();

        let charged = |i: u64| {
            let instance = instance.clone();
            let mut exec = common_utils::prepare_exec::<T>(
                instance.caller.into_origin(),
                HandleKind::Handle(ProgramId::from_origin(instance.addr)),
                vec![],
                0..0,
                Default::default(),
            )
            .unwrap();

            exec.block_config.page_costs.signal_read = (read_cost * i).into();
            exec.block_config.page_costs.signal_write = (write_cost * i).into();
            exec.block_config.page_costs.signal_write_after_read =
                (write_after_read_cost * i).into();

            let notes = core_processor::process::<ExecutionEnvironment>(
                &exec.block_config,
                exec.context,
                exec.random_data,
                exec.memory_pages,
            )
            .unwrap_or_else(|e| unreachable!("core-processor logic invalidated: {}", e));

            let mut gas_burned = 0;
            for note in notes.into_iter() {
                match note {
                    JournalNote::GasBurned { amount, .. } => gas_burned = amount,
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

            gas_burned
        };

        assert_eq!(charged(1).checked_sub(charged(0)).unwrap(), expected);
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

        exec.block_config.page_costs = Default::default();

        let notes = core_processor::process::<ExecutionEnvironment>(
            &exec.block_config,
            exec.context,
            exec.random_data,
            exec.memory_pages,
        )
        .unwrap_or_else(|e| unreachable!("core-processor logic invalidated: {}", e));

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

        exec.block_config.page_costs = PageCosts {
            signal_write: 1.into(),
            ..Default::default()
        };

        let notes = core_processor::process::<ExecutionEnvironment>(
            &exec.block_config,
            exec.context,
            exec.random_data,
            exec.memory_pages,
        )
        .unwrap_or_else(|e| unreachable!("core-processor logic invalidated: {}", e));

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

        exec.block_config.page_costs = PageCosts {
            signal_write: 1.into(),
            ..Default::default()
        };

        let notes = core_processor::process::<ExecutionEnvironment>(
            &exec.block_config,
            exec.context,
            exec.random_data,
            exec.memory_pages,
        )
        .unwrap_or_else(|e| unreachable!("core-processor logic invalidated: {}", e));

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
