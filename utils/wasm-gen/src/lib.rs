// This file is part of Gear.

// Copyright (C) 2021-2023 Gear Technologies Inc.
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

#![allow(clippy::items_after_test_module)]

use std::{
    collections::{BTreeMap, HashSet},
    iter::Cycle,
    mem::size_of,
};

use arbitrary::Unstructured;
use gear_wasm_instrument::{
    parity_wasm::{
        self, builder,
        elements::{
            BlockType, External, FunctionType, ImportCountType, Instruction, Instructions,
            Internal, Module, Type, ValueType,
        },
    },
    syscalls::{ParamType, SysCallName, SysCallSignature},
};
pub use gsys;
use gsys::{ErrorWithHash, HashWithValue, Length};
use wasm_smith::{Module as ModuleSmith, SwarmConfig};

mod config;
pub use config::*;
mod generator;
pub use generator::*;

mod syscalls;
use syscalls::{sys_calls_table, CallInfo, CallSignature};

#[cfg(test)]
mod tests;

pub mod utils;
pub mod wasm;
use wasm::PageCount as WasmPageCount;
pub use wasm::*;

pub mod memory;
pub use memory::*;

const MEMORY_VALUE_SIZE: u32 = 100;
const MEMORY_FIELD_NAME: &str = "memory";

#[derive(Clone, Copy, Debug)]
pub struct Ratio {
    numerator: u32,
    denominator: u32,
}

impl Ratio {
    pub fn get(&self, u: &mut Unstructured) -> bool {
        if self.numerator == 0 {
            false
        } else {
            u.ratio(self.numerator, self.denominator).unwrap()
        }
    }
}

impl From<(u32, u32)> for Ratio {
    fn from((numerator, denominator): (u32, u32)) -> Self {
        Self {
            numerator,
            denominator,
        }
    }
}

impl Ratio {
    pub fn mult<T: Into<usize>>(&self, x: T) -> usize {
        (T::into(x) * self.numerator as usize) / self.denominator as usize
    }
}

// Module and an optional index of gr_debug syscall.
struct ModuleWithDebug {
    module: Module,
    debug_syscall_index: Option<u32>,
    last_offset: u32,
}

impl From<ModuleBuilderWithData> for ModuleWithDebug {
    fn from(data: ModuleBuilderWithData) -> Self {
        let module = data.module_builder.build();
        Self {
            module,
            debug_syscall_index: None,
            last_offset: data.last_offset,
        }
    }
}

impl From<(Module, Option<u32>, u32)> for ModuleWithDebug {
    fn from((module, debug_syscall_index, last_offset): (Module, Option<u32>, u32)) -> Self {
        Self {
            module,
            debug_syscall_index,
            last_offset,
        }
    }
}

pub fn gen_wasm_smith_module(u: &mut Unstructured, config: SwarmConfig) -> ModuleSmith {
    loop {
        if let Ok(module) = ModuleSmith::new(config.clone(), u) {
            return module;
        }
    }
}

fn build_checked_call(
    u: &mut Unstructured,
    results: &[ValueType],
    params_rules: &[ProcessedSysCallParams],
    func_no: u32,
    memory_pages: WasmPageCount,
    unchecked_memory: bool,
) -> Vec<Instruction> {
    let mut code = Vec::with_capacity(params_rules.len() * 2 + 1 + results.len());
    for parameter in params_rules {
        match parameter {
            ProcessedSysCallParams::Value {
                value_type,
                allowed_values,
            } => {
                let is_i32 = match value_type {
                    ValueType::I32 => true,
                    ValueType::I64 => false,
                    ValueType::F32 | ValueType::F64 => {
                        panic!("gear wasm must not have any floating nums")
                    }
                };
                let instr = if let Some(allowed_values) = allowed_values {
                    if is_i32 {
                        Instruction::I32Const(allowed_values.get_i32(u).unwrap())
                    } else {
                        Instruction::I64Const(allowed_values.get_i64(u).unwrap())
                    }
                } else if is_i32 {
                    Instruction::I32Const(u.arbitrary().unwrap())
                } else {
                    Instruction::I64Const(u.arbitrary().unwrap())
                };
                code.push(instr);
            }
            ProcessedSysCallParams::MemoryArray => {
                if unchecked_memory {
                    code.push(Instruction::I32Const(
                        u.arbitrary()
                            .expect("Unstructured::arbitrary failed for MemoryArray"),
                    ));
                    code.push(Instruction::I32Const(
                        u.arbitrary()
                            .expect("Unstructured::arbitrary failed for MemoryArray"),
                    ));
                } else {
                    let memory_size = memory_pages.memory_size();
                    let upper_limit = memory_size.saturating_sub(1);

                    let pointer_beyond = u
                        .int_in_range(0..=upper_limit)
                        .expect("Unstructured::int_in_range failed for MemoryArray");
                    let offset = u
                        .int_in_range(0..=pointer_beyond)
                        .expect("Unstructured::int_in_range failed for MemoryArray");

                    code.push(Instruction::I32Const(offset as i32));
                    code.push(Instruction::I32Const((pointer_beyond - offset) as i32));
                }
            }
            ProcessedSysCallParams::MemoryPtrValue => {
                if unchecked_memory {
                    code.push(Instruction::I32Const(
                        u.arbitrary()
                            .expect("Unstructured::arbitrary failed for MemoryValue"),
                    ));
                } else {
                    let memory_size = memory_pages.memory_size();
                    // Subtract a bit more so entities from gsys fit.
                    let upper_limit = memory_size.saturating_sub(MEMORY_VALUE_SIZE);
                    let offset = u
                        .int_in_range(0..=upper_limit)
                        .expect("Unstructured::int_in_range failed for MemoryValue");

                    code.push(Instruction::I32Const(offset as i32));
                }
            }
            ProcessedSysCallParams::Alloc => {
                if unchecked_memory {
                    code.push(Instruction::I32Const(
                        u.arbitrary()
                            .expect("Unstructured::arbitrary failed for Alloc"),
                    ));
                } else {
                    let pages_to_alloc = u
                        .int_in_range(0..=memory_pages.raw().saturating_sub(1))
                        .expect("Unstructured::int_in_range failed for Alloc");

                    code.push(Instruction::I32Const(pages_to_alloc as i32));
                }
            }
        }
    }

    code.push(Instruction::Call(func_no));
    code.extend(results.iter().map(|_| Instruction::Drop));
    code
}

fn make_call_instructions_vec(
    u: &mut Unstructured,
    params: &[ValueType],
    results: &[ValueType],
    func_no: u32,
) -> Vec<Instruction> {
    let mut code = Vec::with_capacity(params.len() + 1 + results.len());
    for val in params {
        let instr = match val {
            ValueType::I32 => Instruction::I32Const(
                u.arbitrary()
                    .expect("Unstructured::arbitrary failed for I32"),
            ),
            ValueType::I64 => Instruction::I64Const(
                u.arbitrary()
                    .expect("Unstructured::arbitrary failed for I64"),
            ),
            _ => panic!("Cannot handle f32/f64"),
        };
        code.push(instr);
    }
    code.push(Instruction::Call(func_no));
    code.extend(results.iter().map(|_| Instruction::Drop));

    code
}

#[derive(Debug, Clone, Copy)]
enum FuncIdx {
    Import(u32),
    Func(u32),
}

fn get_func_type(module: &Module, func_idx: FuncIdx) -> FunctionType {
    match func_idx {
        FuncIdx::Import(idx) => {
            let type_no = if let External::Function(type_no) =
                module.import_section().unwrap().entries()[idx as usize].external()
            {
                *type_no as usize
            } else {
                panic!("Import func index must be for import function");
            };
            let Type::Function(func_type) = &module.type_section().unwrap().types()[type_no];
            func_type.clone()
        }
        FuncIdx::Func(idx) => {
            let func = module.function_section().unwrap().entries()[idx as usize];
            let Type::Function(func_type) =
                &module.type_section().unwrap().types()[func.type_ref() as usize];
            func_type.clone()
        }
    }
}

struct WasmGen<'a> {
    u: &'a mut Unstructured<'a>,
    config: GearWasmGeneratorConfig,
    calls_indexes: Vec<FuncIdx>,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
enum CallName {
    System(SysCallName),
    SendFromReservation,
}

impl From<SysCallName> for CallName {
    fn from(value: SysCallName) -> Self {
        Self::System(value)
    }
}

#[derive(Debug)]
struct CallData {
    info: CallInfo,
    call_amount: usize,
    call_index: u32,              // Index in the `WasmGen::calls_indexes`
    call_body_index: Option<u32>, // Index of generated body in a code section
}

impl<'a> WasmGen<'a> {
    fn initial_calls_indexes(module: &Module) -> Vec<FuncIdx> {
        let mut calls_indexes = Vec::new();
        let import_funcs_num = module
            .import_section()
            .map(|imps| imps.functions() as u32)
            .unwrap_or(0);
        let code_funcs_num = module
            .function_section()
            .map(|funcs| funcs.entries().len() as u32)
            .unwrap_or(0);
        for i in 0..import_funcs_num {
            calls_indexes.push(FuncIdx::Import(i));
        }
        for i in 0..code_funcs_num {
            calls_indexes.push(FuncIdx::Func(i));
        }
        calls_indexes
    }

    pub fn new(
        module: &Module,
        u: &'a mut Unstructured<'a>,
        config: GearWasmGeneratorConfig,
    ) -> Self {
        let calls_indexes = Self::initial_calls_indexes(module);
        Self {
            u,
            config,
            calls_indexes,
        }
    }

    /// Inserts `instructions` into funcs satisfying the `insert_into_func_filter`
    /// predicate which takes function body index in the `module` code section.
    fn insert_instructions_into_functions(
        &mut self,
        mut module: Module,
        insert_into_func_filter: impl FnOnce(usize) -> bool,
        instructions: &[Instruction],
    ) -> Module {
        let funcs_num = module.code_section().unwrap().bodies().len();
        let insert_into_func_no = self.u.int_in_range(0..=funcs_num - 1).unwrap();
        if !insert_into_func_filter(insert_into_func_no) {
            return module;
        }
        let code = module.code_section_mut().unwrap().bodies_mut()[insert_into_func_no]
            .code_mut()
            .elements_mut();

        let pos = self.u.int_in_range(0..=code.len() - 1).unwrap();
        code.splice(pos..pos, instructions.iter().cloned());
        module
    }

    pub fn gen_export_func_which_call_func_no(
        &mut self,
        mut module: Module,
        name: &str,
        func_no: u32,
    ) -> Module {
        let funcs_len = module
            .function_section()
            .map(|funcs| funcs.entries().len() as u32)
            .expect("unreachable until functions section is not empty");
        let func_type = get_func_type(&module, FuncIdx::Func(func_no));

        let mut instructions =
            make_call_instructions_vec(self.u, func_type.params(), func_type.results(), func_no);
        instructions.push(Instruction::End);

        module = builder::from_module(module)
            .function()
            .body()
            .with_instructions(Instructions::new(instructions))
            .build()
            .signature()
            .build()
            .build()
            .export()
            .field(name)
            .internal()
            .func(funcs_len)
            .build()
            .build();

        let init_function_no = module.function_section().unwrap().entries().len() as u32 - 1;
        self.calls_indexes.push(FuncIdx::Func(init_function_no));

        module
    }

    pub fn gen_handle(&mut self, module: Module) -> (Module, bool) {
        if !self.config.entry_points_config.has_handle() {
            return (module, false);
        }

        let funcs_len = module
            .function_section()
            .map(|funcs| funcs.entries().len() as u32)
            .expect("unreachable until functions section is not empty");

        let func_no = self.u.int_in_range(0..=funcs_len - 1).unwrap();
        (
            self.gen_export_func_which_call_func_no(module, "handle", func_no),
            true,
        )
    }

    pub fn gen_handle_reply(&mut self, module: Module) -> (Module, bool) {
        if !self.config.entry_points_config.has_handle_reply() {
            return (module, false);
        }

        let funcs_len = module
            .function_section()
            .map(|funcs| funcs.entries().len() as u32)
            .expect("unreachable until functions section is not empty");

        let func_no = self.u.int_in_range(0..=funcs_len - 1).unwrap();
        (
            self.gen_export_func_which_call_func_no(module, "handle_reply", func_no),
            true,
        )
    }

    pub fn gen_init(&mut self, module: Module) -> (Module, bool) {
        if !self.config.entry_points_config.has_init() {
            return (module, false);
        }

        let funcs_len = module
            .function_section()
            .map(|funcs| funcs.entries().len() as u32)
            .expect("unreachable until functions section is not empty");

        let func_no = self.u.int_in_range(0..=funcs_len - 1).unwrap();
        (
            self.gen_export_func_which_call_func_no(module, "init", func_no),
            true,
        )
    }

    fn insert_calls(
        &mut self,
        builder: ModuleBuilderWithData,
        call_data: &BTreeMap<CallName, CallData>,
        memory_pages: WasmPageCount,
    ) -> ModuleWithDebug {
        let ModuleBuilderWithData {
            module_builder: builder,
            offsets,
            last_offset,
        } = builder;

        let mut module = builder.build();

        let source_call_index = call_data
            .get(&CallName::System(SysCallName::Source))
            .map(|value| value.call_index);
        let debug_call_index = call_data
            .get(&CallName::System(SysCallName::Debug))
            .map(|value| value.call_index);
        let generated_call_body_indexes: HashSet<Option<u32>> = call_data
            .values()
            .map(|call_data| call_data.call_body_index)
            .collect();

        let mut offsets = offsets.into_iter().cycle();

        // generate call instructions for syscalls and insert them somewhere into the code
        for (name, data) in call_data {
            let instructions = self.build_call_instructions(
                name,
                data,
                memory_pages,
                source_call_index,
                &mut offsets,
            );
            for _ in 0..data.call_amount {
                module = self.insert_instructions_into_functions(
                    module,
                    |func_body_idx| {
                        !generated_call_body_indexes.contains(&Some(func_body_idx as u32))
                    },
                    &instructions,
                );
            }
        }

        (module, debug_call_index, last_offset).into()
    }

    fn gen_syscall_imports(&mut self, module: Module) -> (Module, BTreeMap<CallName, CallData>) {
        let code_size = module.code_section().map_or(0, |code_section| {
            code_section
                .bodies()
                .iter()
                .map(|b| b.code().elements().len())
                .sum()
        });
        let mut syscall_data = BTreeMap::default();
        if code_size == 0 {
            return (module, syscall_data);
        }
        let import_count = module.import_count(ImportCountType::Function);
        let mut module_builder = builder::from_module(module);
        let sys_calls_table = sys_calls_table(self.config.sys_calls_config.params_config());
        for (i, (name, info, sys_call_amount)) in sys_calls_table
            .into_iter()
            .filter_map(|(name, info)| {
                let frequency = self.config.sys_calls_config.frequency(name);
                let sys_call_amount = self.u.int_in_range(frequency).unwrap() as usize;
                (sys_call_amount != 0).then_some((name, info, sys_call_amount))
            })
            .enumerate()
        {
            let signature_index = {
                let func_type = info.func_type();
                let mut signature_builder = builder::signature();
                for parameter in func_type.params() {
                    signature_builder = signature_builder.with_param(*parameter);
                }

                for result in func_type.results() {
                    signature_builder = signature_builder.with_result(*result);
                }

                module_builder.push_signature(signature_builder.build_sig())
            };

            // make import
            module_builder.push_import(
                builder::import()
                    .module("env")
                    .external()
                    .func(signature_index)
                    .field(name.to_str())
                    .build(),
            );

            let call_index = self.calls_indexes.len() as u32;

            self.calls_indexes
                .push(FuncIdx::Import((import_count + i) as u32));
            syscall_data.insert(
                name.into(),
                CallData {
                    info,
                    call_amount: sys_call_amount,
                    call_index,
                    call_body_index: None,
                },
            );
        }
        (module_builder.build(), syscall_data)
    }

    fn gen_send_from_reservation(
        &mut self,
        module: Module,
        call_data: &BTreeMap<CallName, CallData>,
        memory_pages: WasmPageCount,
    ) -> (Module, Option<CallData>) {
        let reserve_gas_call_data = call_data.get(&CallName::System(SysCallName::ReserveGas));
        let reservation_send_call_data =
            call_data.get(&CallName::System(SysCallName::ReservationSend));
        let (Some(reserve_gas_call_data), Some(reservation_send_call_data)) =
            (reserve_gas_call_data, reservation_send_call_data) else {
            return (module, None);
        };
        let send_from_reservation_signature = SysCallSignature {
            params: vec![
                ParamType::Ptr,      // Address of recipient and value (HashWithValue struct)
                ParamType::Ptr,      // Pointer to payload
                ParamType::Size,     // Size of the payload
                ParamType::Delay,    // Number of blocks to delay the sending for
                ParamType::Gas,      // Amount of gas to reserve
                ParamType::Duration, // Duration of the reservation
            ],
            results: Default::default(),
        };
        let send_from_reservation_call_info = CallInfo::new(
            CallSignature::Custom(send_from_reservation_signature),
            self.config.sys_calls_config.params_config(),
        );

        let func_type = send_from_reservation_call_info.func_type();

        let func_signature = builder::signature()
            .with_params(func_type.params().iter().copied())
            .with_results(func_type.results().iter().copied())
            .build_sig();

        let memory_size_in_bytes = memory_pages.memory_size();
        let reserve_gas_result_ptr = memory_size_in_bytes.saturating_sub(MEMORY_VALUE_SIZE) as i32; // Pointer to the LengthWithHash struct
        let rid_pid_value_ptr = reserve_gas_result_ptr + size_of::<Length>() as i32;
        let pid_value_ptr = reserve_gas_result_ptr + size_of::<ErrorWithHash>() as i32;
        let reservation_send_result_ptr = pid_value_ptr + size_of::<HashWithValue>() as i32;

        let func_instructions = Instructions::new(vec![
            Instruction::GetLocal(4),                      // Amount of gas to reserve
            Instruction::GetLocal(5),                      // Duration of the reservation
            Instruction::I32Const(reserve_gas_result_ptr), // Pointer to the LengthWithHash struct
            Instruction::Call(reserve_gas_call_data.call_index),
            Instruction::I32Const(reserve_gas_result_ptr), // Pointer to the LengthWithHash struct
            Instruction::I32Load(2, 0),                    // Load LengthWithHash.length
            Instruction::I32Eqz,                           // Check if LengthWithHash.length == 0
            Instruction::If(BlockType::NoResult),          // If LengthWithHash.length == 0
            Instruction::I32Const(pid_value_ptr), // Copy the HashWithValue struct (48 bytes) containing the recipient and value after the obtained reservation ID
            Instruction::GetLocal(0),
            Instruction::I64Load(3, 0),
            Instruction::I64Store(3, 0),
            Instruction::I32Const(pid_value_ptr),
            Instruction::GetLocal(0),
            Instruction::I64Load(3, 8),
            Instruction::I64Store(3, 8),
            Instruction::I32Const(pid_value_ptr),
            Instruction::GetLocal(0),
            Instruction::I64Load(3, 16),
            Instruction::I64Store(3, 16),
            Instruction::I32Const(pid_value_ptr),
            Instruction::GetLocal(0),
            Instruction::I64Load(3, 24),
            Instruction::I64Store(3, 24),
            Instruction::I32Const(pid_value_ptr),
            Instruction::GetLocal(0),
            Instruction::I64Load(3, 32),
            Instruction::I64Store(3, 32),
            Instruction::I32Const(pid_value_ptr),
            Instruction::GetLocal(0),
            Instruction::I64Load(3, 40),
            Instruction::I64Store(3, 40),
            Instruction::I32Const(rid_pid_value_ptr), // Pointer to reservation ID, recipient ID and value
            Instruction::GetLocal(1),                 // Pointer to payload
            Instruction::GetLocal(2),                 // Size of the payload
            Instruction::GetLocal(3),                 // Number of blocks to delay the sending for
            Instruction::I32Const(reservation_send_result_ptr), // Pointer to the result of the reservation send
            Instruction::Call(reservation_send_call_data.call_index),
            Instruction::End,
            Instruction::End,
        ]);

        let mut module_builder = builder::from_module(module);

        let func_location = module_builder.push_function(
            builder::function()
                .with_signature(func_signature)
                .body()
                .with_instructions(func_instructions)
                .build()
                .build(),
        );

        let call_idx = self.calls_indexes.len() as u32;

        self.calls_indexes
            .push(FuncIdx::Func(func_location.signature));

        (
            module_builder.build(),
            Some(CallData {
                info: send_from_reservation_call_info,
                // Could be generated from Unstructured based on code size,
                // but apparently this level os randomness suffices
                call_amount: reservation_send_call_data.call_amount,
                call_index: call_idx,
                call_body_index: Some(func_location.body),
            }),
        )
    }

    fn build_call_instructions<I: Clone + Iterator<Item = u32>>(
        &mut self,
        name: &CallName,
        data: &CallData,
        memory_pages: WasmPageCount,
        source_call_index: Option<u32>,
        offsets: &mut Cycle<I>,
    ) -> Vec<Instruction> {
        let info = &data.info;
        if ![
            CallName::System(SysCallName::Send),
            CallName::System(SysCallName::SendWGas),
            CallName::System(SysCallName::SendInput),
            CallName::System(SysCallName::SendInputWGas),
            CallName::SendFromReservation,
        ]
        .contains(name)
        {
            return build_checked_call(
                self.u,
                &info.results,
                &info.parameter_rules,
                data.call_index,
                memory_pages,
                self.config.sys_calls_config.random_mem_access(),
            );
        }

        let mut remaining_instructions = build_checked_call(
            self.u,
            &info.results,
            &info.parameter_rules[1..],
            data.call_index,
            memory_pages,
            self.config.sys_calls_config.random_mem_access(),
        );

        if let Some(source_call_index) = source_call_index {
            if self
                .config
                .sys_calls_config
                .message_destination()
                .is_source()
            {
                let mut instructions = Vec::with_capacity(3 + remaining_instructions.len());

                let memory_size = memory_pages.memory_size();
                let upper_limit = memory_size.saturating_sub(MEMORY_VALUE_SIZE);
                let offset = self
                    .u
                    .int_in_range(0..=upper_limit)
                    .expect("build_call_instructions: Unstructured::int_in_range failed");

                // call msg::source (gr_source) with a memory offset
                instructions.push(Instruction::I32Const(offset as i32));
                instructions.push(Instruction::Call(source_call_index));
                // pass the offset as the first argument to the send-call
                instructions.push(Instruction::I32Const(offset as i32));
                instructions.append(&mut remaining_instructions);

                return instructions;
            }
        }

        let address_offset = offsets.next().unwrap_or_else(|| {
            self.u
                .arbitrary()
                .expect("build_call_instructions: Unstructured::arbitrary failed")
        }) as i32;
        let mut instructions = Vec::with_capacity(1 + remaining_instructions.len());
        instructions.push(Instruction::I32Const(address_offset));
        instructions.append(&mut remaining_instructions);

        instructions
    }

    pub fn make_print_test_info(&mut self, result: ModuleWithDebug) -> Module {
        let Some(text) = self.config.sys_calls_config.log_info() else {
            return result.module;
        };

        let ModuleWithDebug {
            mut module,
            debug_syscall_index,
            last_offset,
        } = result;

        if let External::Memory(mem_type) = module
            .import_section()
            .unwrap()
            .entries()
            .iter()
            .find(|section| section.field() == MEMORY_FIELD_NAME)
            .unwrap()
            .external()
        {
            if mem_type.limits().initial() == 0 {
                return module;
            }
        }

        let mut init_func_no = None;
        if let Some(export_section) = module.export_section() {
            for export in export_section.entries().iter() {
                if export.field() == "init" {
                    init_func_no = if let Internal::Function(func_no) = export.internal() {
                        Some(*func_no)
                    } else {
                        panic!("init export is not a func, very strange -_-");
                    }
                }
            }
        }
        if init_func_no.is_none() {
            return module;
        }

        let bytes = text.as_bytes();
        module = builder::from_module(module)
            .data()
            .offset(Instruction::I32Const(last_offset as i32))
            .value(bytes.to_vec())
            .build()
            .build();

        let init_code = module.code_section_mut().unwrap().bodies_mut()
            [init_func_no.unwrap() as usize]
            .code_mut()
            .elements_mut();
        let print_code = [
            Instruction::I32Const(last_offset as i32),
            Instruction::I32Const(bytes.len() as i32),
            Instruction::Call(debug_syscall_index.expect("debug data specified so do the call")),
        ];

        init_code.splice(0..0, print_code);

        module
    }

    pub fn resolves_calls_indexes(self, mut module: Module) -> Module {
        if module.code_section().is_none() {
            return module;
        }

        let Self {
            calls_indexes,
            config,
            ..
        } = self;

        let import_funcs_num = module
            .import_section()
            .map(|imp| imp.functions() as u32)
            .unwrap_or(0);

        for instr in module
            .code_section_mut()
            .unwrap()
            .bodies_mut()
            .iter_mut()
            .flat_map(|body| body.code_mut().elements_mut().iter_mut())
        {
            if let Instruction::Call(func_no) = instr {
                let index = calls_indexes[*func_no as usize];
                match index {
                    FuncIdx::Func(no) => *func_no = no + import_funcs_num,
                    FuncIdx::Import(no) => *func_no = no,
                }
            }
        }

        let mut empty_export_section = Default::default();
        for func_no in module
            .export_section_mut()
            .unwrap_or(&mut empty_export_section)
            .entries_mut()
            .iter_mut()
            .filter_map(|export| {
                if let Internal::Function(func_no) = export.internal_mut() {
                    Some(func_no)
                } else {
                    None
                }
            })
        {
            if let FuncIdx::Func(code_func_no) = calls_indexes[*func_no as usize] {
                *func_no = import_funcs_num + code_func_no;
            } else {
                // TODO: export can be to the import function by WASM specification,
                // but we currently do not support this in wasm-gen.
                panic!("Export cannot be to the import function");
            }
        }

        match config.remove_recursions {
            true => utils::remove_recursion(module),
            false => module,
        }
    }
}

pub fn gen_gear_program_module<'a>(
    u: &'a mut Unstructured<'a>,
    config: WasmGenConfig,
    addresses: &[HashWithValue],
) -> Module {
    let WasmGenConfig {
        generator_config,
        selectable_params,
    } = config;

    let (module, memory_pages) = {
        // Create wasm module config.
        let arbitrary_params = u.arbitrary::<ArbitraryParams>().unwrap();
        let wasm_module =
            WasmModule::generate_with_config((selectable_params, arbitrary_params).into(), u)
                .unwrap();

        // Instantiate memory generator and generate memory import
        let mem_config = generator_config.memory_config;
        let (DisabledMemoryGenerator(wasm_module), _mem_import_gen_proof) =
            MemoryGenerator::new(wasm_module, mem_config).generate_memory();

        let memory_pages = wasm_module
            .initial_mem_size()
            .expect("proof of import memory generation exists")
            .into();
        (wasm_module.into_inner(), memory_pages)
    };

    let mut gen = WasmGen::new(&module, u, generator_config);
    let (module, has_init) = gen.gen_init(module);
    if !has_init {
        return gen.resolves_calls_indexes(module);
    }

    let (module, _has_handle) = gen.gen_handle(module);

    let (module, _has_handle_reply) = gen.gen_handle_reply(module);

    let (module, mut syscall_data) = gen.gen_syscall_imports(module);

    let (module, send_from_reservation_call_data) =
        gen.gen_send_from_reservation(module, &syscall_data, memory_pages);
    if let Some(send_from_reservation_call_data) = send_from_reservation_call_data {
        syscall_data.insert(
            CallName::SendFromReservation,
            send_from_reservation_call_data,
        );
    }

    let module = gen.insert_calls(
        ModuleBuilderWithData::new(addresses, module, memory_pages),
        &syscall_data,
        memory_pages,
    );

    let module = gen.make_print_test_info(module);

    gen.resolves_calls_indexes(module)
}

pub fn gen_gear_program_code<'a>(
    u: &'a mut Unstructured<'a>,
    config: WasmGenConfig,
    addresses: &[HashWithValue],
) -> Vec<u8> {
    let module = gen_gear_program_module(u, config, addresses);
    parity_wasm::serialize(module).unwrap()
}
