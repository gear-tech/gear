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

use ::alloc::{collections::BTreeSet, format};
use common::ProgramStorage;
use frame_support::codec::MaxEncodedLen;
use gear_backend_common::lazy_pages::Status;
use gear_core::memory::{MemoryInterval, PageU32Size};
use gear_lazy_pages_common as lazy_pages;
use rand::{Rng, SeedableRng};

use super::*;
use crate::{
    benchmarking::{utils as common_utils, utils::PrepareConfig},
    HandleKind,
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum SetNo {
    SignalRead,
    SignalWrite,
    SignalWriteAfterRead,
    HostFuncRead,
    HostFuncWrite,
    HostFuncWriteAfterRead,
    WithData,
    Amount,
}

#[derive(Default)]
struct PageSets<P: PageU32Size> {
    sets: [BTreeSet<P>; SetNo::Amount as usize],
}

impl<P: PageU32Size + core::fmt::Debug> core::fmt::Debug for PageSets<P> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        for set_no in SetNo::SignalRead as usize..=SetNo::WithData as usize {
            let set = if set_no == SetNo::WithData as usize {
                self.accessed_pages()
                    .intersection(&self.sets[set_no])
                    .copied()
                    .collect()
            } else {
                self.sets[set_no].clone()
            };
            f.write_str(&format!("{set_no:?} {:?}", set))?;
        }
        Ok(())
    }
}

impl<P: PageU32Size> PageSets<P> {
    fn get(&self, no: SetNo) -> &BTreeSet<P> {
        &self.sets[no as usize]
    }

    fn get_mut(&mut self, no: SetNo) -> &mut BTreeSet<P> {
        &mut self.sets[no as usize]
    }

    fn with_accessed(i: MemoryInterval, mut f: impl FnMut(P)) {
        let start = P::from_offset(i.offset);
        let end = P::from_offset(i.offset.checked_add(i.size.saturating_sub(1)).unwrap());
        for page in start.iter_end_inclusive(end).unwrap() {
            f(page);
        }
    }

    fn is_any_read(&self, page: P) -> bool {
        self.get(SetNo::SignalRead).contains(&page) || self.get(SetNo::HostFuncRead).contains(&page)
    }

    fn is_any_write(&self, page: P) -> bool {
        self.get(SetNo::SignalWrite).contains(&page)
            || self.get(SetNo::SignalWriteAfterRead).contains(&page)
            || self.get(SetNo::HostFuncWrite).contains(&page)
            || self.get(SetNo::HostFuncWriteAfterRead).contains(&page)
    }

    fn add_signal_read(&mut self, i: MemoryInterval) {
        Self::with_accessed(i, |page| {
            if !self.is_any_read(page) && !self.is_any_write(page) {
                self.get_mut(SetNo::SignalRead).insert(page);
            }
        });
    }

    fn add_signal_write(&mut self, i: MemoryInterval) {
        Self::with_accessed(i, |page| {
            if !self.is_any_write(page) {
                if self.is_any_read(page) {
                    self.get_mut(SetNo::SignalWriteAfterRead).insert(page);
                } else {
                    self.get_mut(SetNo::SignalWrite).insert(page);
                }
            }
        });
    }

    fn add_syscall_read(&mut self, i: MemoryInterval) {
        Self::with_accessed(i, |page| {
            if !self.is_any_read(page) && !self.is_any_write(page) {
                self.get_mut(SetNo::HostFuncRead).insert(page);
            }
        });
    }

    fn add_syscall_write(&mut self, i: MemoryInterval) {
        Self::with_accessed(i, |page| {
            if !self.is_any_write(page) {
                if self.is_any_read(page) {
                    self.get_mut(SetNo::HostFuncWriteAfterRead).insert(page);
                } else {
                    self.get_mut(SetNo::HostFuncWrite).insert(page);
                }
            }
        });
    }

    fn add_page_with_data(&mut self, p: P) {
        self.get_mut(SetNo::WithData).insert(p);
    }

    fn accessed_pages(&self) -> BTreeSet<P> {
        let mut accessed_pages = BTreeSet::new();
        for set in self.sets[..SetNo::WithData as usize].iter() {
            accessed_pages.extend(set.iter().copied());
        }
        accessed_pages
    }

    fn loaded_pages_count(&self) -> GearPage {
        (self
            .accessed_pages()
            .intersection(self.get(SetNo::WithData))
            .count() as u16)
            .into()
    }

    fn charged_for_pages(&self, costs: &PageCosts) -> u64 {
        let costs = costs.lazy_pages_weights();
        let costs = [
            costs.signal_read,
            costs.signal_write,
            costs.signal_write_after_read,
            costs.host_func_read,
            costs.host_func_write,
            costs.host_func_write_after_read,
            costs.load_page_storage_data,
        ];

        let mut amount = 0u64;
        #[allow(clippy::needless_range_loop)]
        for set_no in SetNo::SignalRead as usize..SetNo::WithData as usize {
            amount = amount
                .checked_add(costs[set_no].calc((self.sets[set_no].len() as u16).into()))
                .unwrap();
        }

        amount = amount
            .checked_add(costs[SetNo::WithData as usize].calc(self.loaded_pages_count()))
            .unwrap();

        amount
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
    let size_wasm_pages = memory.min_pages;
    let size_gear = size_wasm_pages.to_page::<GearPage>();
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
        let module = ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![SysCallName::Random],
            handle_body: Some(body::from_instructions(instrs)),
            stack_end: Some(0.into()),
            ..Default::default()
        };
        let instance = Program::<T>::new(module.into(), vec![]).unwrap();
        let source = instance.caller.into_origin();
        let program_id = ProgramId::from_origin(instance.addr);

        // Append data in storage for some pages.
        for page in (0..rng.gen_range(0..MAX_PAGES_WITH_DATA))
            .map(|_| GearPage::new(rng.gen_range(0..size_gear.raw())).unwrap())
        {
            page_sets.add_page_with_data(page);
            ProgramStorageOf::<T>::set_program_page_data(program_id, page, PageBuf::new_zeroed());
        }

        // execute program with random page costs
        let mut run = |_: u64| {
            let mut exec = common_utils::prepare_exec::<T>(
                source,
                HandleKind::Handle(program_id),
                vec![],
                Default::default(),
            )
            .unwrap();

            let mut rand_cost = || rng.gen_range(0..MAX_COST).into();
            let costs = &mut exec.block_config.page_costs;
            costs.signal_read = rand_cost();
            costs.signal_write = rand_cost();
            costs.lazy_pages_signal_write_after_read = rand_cost();
            costs.lazy_pages_host_func_read = rand_cost();
            costs.lazy_pages_host_func_write = rand_cost();
            costs.lazy_pages_host_func_write_after_read = rand_cost();
            costs.load_page_data = rand_cost();
            costs.upload_page_data = rand_cost();

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
    let size = GearPage::size() as i32;
    let read_cost = 1u64;
    let write_cost = 10u64;
    let write_after_read_cost = 100u64;

    let test = |instrs, expected| {
        let module = ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            handle_body: Some(body::from_instructions(instrs)),
            stack_end: Some(0.into()),
            ..Default::default()
        };
        let instance = Program::<T>::new(module.into(), vec![]).unwrap();

        let charged = |i: u64| {
            let instance = instance.clone();
            let mut exec = common_utils::prepare_exec::<T>(
                instance.caller.into_origin(),
                HandleKind::Handle(ProgramId::from_origin(instance.addr)),
                vec![],
                Default::default(),
            )
            .unwrap();

            exec.block_config.page_costs.signal_read = (read_cost * i).into();
            exec.block_config.page_costs.signal_write = (write_cost * i).into();
            exec.block_config
                .page_costs
                .lazy_pages_signal_write_after_read = (write_after_read_cost * i).into();

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
            // Read 0st and 1st gear pages
            Instruction::I32Const(size - 1),
            Instruction::I32Load(2, 0),
            Instruction::Drop,
            // Write after read 1st gear page
            Instruction::I32Const(size),
            Instruction::I32Const(42),
            Instruction::I32Store(2, 0),
        ],
        2 * read_cost + write_after_read_cost,
    );

    test(
        vec![
            // Read 0st and 1st gear pages
            Instruction::I32Const(size - 1),
            Instruction::I32Load(2, 0),
            Instruction::Drop,
            // Write after read 0st and 1st gear page
            Instruction::I32Const(size - 3),
            Instruction::I32Const(42),
            Instruction::I32Store(2, 0),
        ],
        2 * read_cost + 2 * write_after_read_cost,
    );

    test(
        vec![
            // Read 0st and 1st gear pages
            Instruction::I32Const(size - 1),
            Instruction::I32Load(2, 0),
            Instruction::Drop,
            // Write after read 1st gear page and write 2st gear page
            Instruction::I32Const(2 * size - 1),
            Instruction::I32Const(42),
            Instruction::I32Store(2, 0),
        ],
        2 * read_cost + write_after_read_cost + write_cost,
    );

    test(
        vec![
            // Read 1st gear page
            Instruction::I32Const(size),
            Instruction::I32Load(2, 0),
            Instruction::Drop,
            // Write after read 1st gear page and write 0st gear page
            Instruction::I32Const(size - 1),
            Instruction::I32Const(42),
            Instruction::I32Store(2, 0),
        ],
        read_cost + write_after_read_cost + write_cost,
    );

    test(
        vec![
            // Read 1st gear page
            Instruction::I32Const(size),
            Instruction::I32Load(2, 0),
            Instruction::Drop,
            // Read 0st and 1st gear pages, but pay only for 0st.
            Instruction::I32Const(size - 1),
            Instruction::I32Load(2, 0),
            Instruction::Drop,
        ],
        2 * read_cost,
    );

    test(
        vec![
            // Write 0st and 1st gear page
            Instruction::I32Const(size - 1),
            Instruction::I32Const(42),
            Instruction::I32Store(2, 0),
            // Write 1st and 2st gear pages, but pay only for 2st page
            Instruction::I32Const(2 * size - 1),
            Instruction::I32Const(42),
            Instruction::I32Store(2, 0),
        ],
        3 * write_cost,
    );

    test(
        vec![
            // Write 0st and 1st gear page
            Instruction::I32Const(size - 1),
            Instruction::I32Const(42),
            Instruction::I32Store(2, 0),
            // Read 1st and 2st gear pages, but pay only for 2st page
            Instruction::I32Const(2 * size - 1),
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
    let module = ModuleDefinition {
        memory: Some(ImportedMemory::max::<T>()),
        handle_body: Some(body::from_instructions(instrs)),
        stack_end: Some(0.into()),
        ..Default::default()
    };
    let instance = Program::<T>::new(module.into(), vec![]).unwrap();
    let source = instance.caller.into_origin();
    let origin = instance.addr;

    // Calculate how much gas burned, when lazy pages costs are zero.
    let gas_burned = {
        let mut exec = common_utils::prepare_exec::<T>(
            source,
            HandleKind::Handle(ProgramId::from_origin(origin)),
            vec![],
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

        assert_eq!(lazy_pages::get_status(), Status::GasLimitExceeded);
    };

    // Check gas allowance exceeded.
    {
        let mut exec = common_utils::prepare_exec::<T>(
            source,
            HandleKind::Handle(ProgramId::from_origin(origin)),
            vec![],
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

        assert_eq!(lazy_pages::get_status(), Status::GasAllowanceExceeded);
    };
}
