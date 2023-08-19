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

//! Sys-calls invocator module.

use crate::{
    generator::{
        AddressesInjectionOutcome, AddressesOffsets, CallIndexes, CallIndexesHandle,
        DisabledAdditionalDataInjector, FunctionIndex, ModuleWithCallIndexes,
    },
    wasm::{PageCount as WasmPageCount, WasmModule},
    InvocableSysCall, SysCallInfo, SysCallParamAllowedValues, SysCallsConfig, SysCallsParamsConfig,
};
use arbitrary::{Result, Unstructured};
use gear_wasm_instrument::{
    parity_wasm::elements::{BlockType, Instruction, Internal, ValueType},
    syscalls::{ParamType, SysCallName},
};
use std::{collections::BTreeMap, iter};

#[derive(Debug)]
pub(crate) enum ProcessedSysCallParams {
    Alloc,
    Value {
        value_type: ValueType,
        allowed_values: Option<SysCallParamAllowedValues>,
    },
    MemoryArray,
    MemoryPtrValue,
}

pub(crate) fn process_sys_call_params(
    params: &[ParamType],
    params_config: &SysCallsParamsConfig,
) -> Vec<ProcessedSysCallParams> {
    let mut res = Vec::with_capacity(params.len());
    let mut skip_next_param = false;
    for &param in params {
        if skip_next_param {
            skip_next_param = false;
            continue;
        }
        let processed_param = match param {
            ParamType::Alloc => ProcessedSysCallParams::Alloc,
            ParamType::Ptr(maybe_idx) => maybe_idx
                .map(|_| {
                    // skipping next as we don't need the following `Size` param,
                    // because it will be chosen in accordance to the wasm module
                    // memory pages config.
                    skip_next_param = true;
                    ProcessedSysCallParams::MemoryArray
                })
                .unwrap_or(ProcessedSysCallParams::MemoryPtrValue),
            _ => ProcessedSysCallParams::Value {
                value_type: param.into(),
                allowed_values: params_config.get_rule(&param),
            },
        };

        res.push(processed_param);
    }

    res
}

/// Sys-calls invocator.
///
/// Inserts sys-calls invokes randomly into internal functions.
///
/// This type is instantiated from disable additional data injector and
/// data injection outcome ([`AddressesInjectionOutcome`]). The latter was introduced
/// to give additional guarantees for config and generators consistency. Otherwise,
/// if there wasn't any addresses injection outcome, which signals that there was a try to
/// inject addresses, sys-calls invocator could falsely set `gr_send*` call's destination param
/// to random value. For example, existing addresses could have been defined in the config, but
/// additional data injector was disabled, before injecting addresses from the config. As a result,
/// invocator would set un-intended by config values as messages destination. To avoid such
/// inconsistency the [`AddressesInjectionOutcome`] gives additional required guarantees.
pub struct SysCallsInvocator<'a, 'b> {
    unstructured: &'b mut Unstructured<'a>,
    call_indexes: CallIndexes,
    module: WasmModule,
    config: SysCallsConfig,
    offsets: Option<AddressesOffsets>,
    sys_call_imports: BTreeMap<InvocableSysCall, (u32, CallIndexesHandle)>,
}

impl<'a, 'b>
    From<(
        DisabledAdditionalDataInjector<'a, 'b>,
        AddressesInjectionOutcome,
    )> for SysCallsInvocator<'a, 'b>
{
    fn from(
        (disabled_gen, outcome): (
            DisabledAdditionalDataInjector<'a, 'b>,
            AddressesInjectionOutcome,
        ),
    ) -> Self {
        Self {
            unstructured: disabled_gen.unstructured,
            call_indexes: disabled_gen.call_indexes,
            module: disabled_gen.module,
            config: disabled_gen.config,
            offsets: outcome.offsets,
            sys_call_imports: disabled_gen.sys_calls_imports,
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum ParamSetter {
    I32(i32),
    I64(i64),
}

impl std::fmt::Display for ParamSetter {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Set parameter to ")?;
        match self {
            Self::I32(val) => {
                write!(f, "{}: i32", val)
            }
            Self::I64(val) => {
                write!(f, "{}: i64", val)
            }
        }
    }
}

impl ParamSetter {
    fn get_ix(&self) -> Instruction {
        match *self {
            Self::I32(val) => Instruction::I32Const(val),
            Self::I64(val) => Instruction::I64Const(val),
        }
    }
}

impl<'a, 'b> SysCallsInvocator<'a, 'b> {
    /// Insert sys-calls invokes.
    ///
    /// The method builds instructions, which describe how each sys-call is called, and then
    /// insert these instructions into any random function. In the end, all call indexes are resolved.
    pub fn insert_invokes(mut self) -> Result<DisabledSysCallsInvocator> {
        log::trace!(
            "Random data before inserting all sys-calls invocations - {}",
            self.unstructured.len()
        );

        for (invocable, (amount, call_indexes_handle)) in self.sys_call_imports.clone() {
            let instructions =
                self.build_sys_call_invoke_instructions(invocable, call_indexes_handle)?;

            log::trace!(
                "Inserting the {} sys_call {} times",
                invocable.to_str(),
                amount
            );

            for instructions in iter::repeat(&instructions).take(amount as usize) {
                self.insert_sys_call_instructions(instructions)?;
            }
        }

        log::trace!(
            "Random data after inserting all sys-calls invocations - {}",
            self.unstructured.len()
        );

        self.resolves_calls_indexes();

        Ok(DisabledSysCallsInvocator {
            module: self.module,
            call_indexes: self.call_indexes,
        })
    }

    fn build_sys_call_invoke_instructions(
        &mut self,
        invocable: InvocableSysCall,
        call_indexes_handle: CallIndexesHandle,
    ) -> Result<Vec<Instruction>> {
        log::trace!(
            "Random data before building {} sys-call invoke instructions - {}",
            invocable.to_str(),
            self.unstructured.len()
        );

        if self.is_not_send_sys_call(invocable) {
            log::trace!(
                " -- Generating build call for non-send sys-call {}",
                invocable.to_str()
            );
            return self.build_call(invocable.into_info(), call_indexes_handle);
        }

        log::trace!(
            " -- Generating build call for send sys-call {}",
            invocable.to_str()
        );

        let mut info = invocable.into_info();
        // The first ix will be I32Const(destination_ptr), so we replace it with non-random value.
        info.signature.params.remove(0);
        let mut call_without_destination_instrs = self.build_call(info, call_indexes_handle)?;

        let res = if self.config.sending_message_destination().is_source() {
            log::trace!(" -- Message destination is result of `gr_source`");

            let gr_source_call_indexes_handle = self
                .sys_call_imports
                .get(&InvocableSysCall::Loose(SysCallName::Source))
                .map(|&(_, call_indexes_handle)| call_indexes_handle as u32)
                .expect("by config if destination is source, then `gr_source` is generated");

            let mut instructions = Vec::with_capacity(3 + call_without_destination_instrs.len());

            let mem_size = self
                .module
                .initial_mem_size()
                .map(Into::<WasmPageCount>::into)
                // To instantiate this generator, we must instantiate SysCallImportsGenerator, which can be
                // instantiated only with memory import generation proof.
                .expect("generator is instantiated with a memory import generation proof")
                .memory_size();
            // Subtract a bit more so entities from `gsys` fit.
            let upper_limit = mem_size.saturating_sub(100);
            let offset = self.unstructured.int_in_range(0..=upper_limit)?;

            // call `gsys::gr_source` with a memory offset
            instructions.push(Instruction::I32Const(offset as i32));
            instructions.push(Instruction::Call(gr_source_call_indexes_handle));
            // pass the offset as the first argument to the send-call
            instructions.push(Instruction::I32Const(offset as i32));
            instructions.append(&mut call_without_destination_instrs);

            instructions
        } else {
            let mut instructions = Vec::with_capacity(1 + call_without_destination_instrs.len());

            let address_offset = match self.offsets.as_mut() {
                Some(offsets) => {
                    debug_assert!(self
                        .config
                        .sending_message_destination()
                        .is_existing_addresses());
                    log::trace!(" -- Message destination is an existing program address");

                    offsets.next_offset()
                }
                None => {
                    debug_assert!(self.config.sending_message_destination().is_random());
                    log::trace!(" -- Message destination is a random address");

                    self.unstructured.arbitrary()?
                }
            };

            instructions.push(Instruction::I32Const(address_offset as i32));
            instructions.append(&mut call_without_destination_instrs);

            instructions
        };

        println!("{} {:?}", invocable.to_str(), res);

        Ok(res)
    }

    fn is_not_send_sys_call(&self, sys_call: InvocableSysCall) -> bool {
        use InvocableSysCall::*;
        ![
            Loose(SysCallName::Send),
            Loose(SysCallName::SendWGas),
            Loose(SysCallName::SendInput),
            Loose(SysCallName::SendInputWGas),
            Precise(SysCallName::ReservationSend),
        ]
        .contains(&sys_call)
    }

    fn build_result_processing(
        &mut self,
        syscall: SysCallInfo,
        param_setters: &[ParamSetter],
    ) -> Vec<Instruction> {
        if syscall.fallible {
            // Assume here that:
            // 1. All the fallible syscalls write error to the pointer located in the last argument in syscall.
            // 2. All the errors contain `ErrorCode` in the start of memory where pointer points.
            // TODO: Assert these assumptions.

            let params = syscall.signature.params;
            assert!(
                matches!(
                    params
                    .last()
                    .expect("The last argument of fallible syscall must be pointer to error code (by assumption we made)"), 
                ParamType::Ptr(..)
            )
            );
            assert_eq!(params.len(), param_setters.len());

            if let ParamSetter::I32(ptr) = param_setters.last().unwrap() {
                return vec![
                    Instruction::I32Const(*ptr),
                    Instruction::I32Load(4, 0),
                    Instruction::I32Const(0),
                    Instruction::I32Ne,
                    Instruction::If(BlockType::NoResult),
                    Instruction::Unreachable,
                    Instruction::End,
                ];
            } else {
                panic!("Incorrect last parameter type: expected pointer");
            }
        }

        let results_len = syscall.signature.results.len();
        if results_len != 0 {
            assert_eq!(results_len, 1);
            // Assume here that return type of syscall means error when = -1.
            return vec![
                Instruction::I32Const(-1),
                Instruction::I32Eq,
                Instruction::If(BlockType::NoResult),
                Instruction::Unreachable,
                Instruction::End,
            ];
        }

        vec![]
    }

    fn build_param_setters(&mut self, params: &[ParamType]) -> Result<Vec<ParamSetter>> {
        log::trace!(
            "  ----  Random data before SysCallsInvocator::build_param_setters - {}",
            self.unstructured.len()
        );

        let mem_size_pages = self
            .module
            .initial_mem_size()
            // To instantiate this generator, we must instantiate SysCallImportsGenerator, which can be
            // instantiated only with memory import generation proof.
            .expect("generator is instantiated with a memory import generation proof");
        let mem_size = Into::<WasmPageCount>::into(mem_size_pages).memory_size();

        let mut setters = Vec::with_capacity(params.len());
        for processed_param in process_sys_call_params(params, self.config.params_config()) {
            match processed_param {
                ProcessedSysCallParams::Alloc => {
                    let pages_to_alloc = self
                        .unstructured
                        .int_in_range(0..=mem_size_pages.saturating_sub(1))?;
                    let setter = ParamSetter::I32(pages_to_alloc as i32);

                    log::trace!("  ----  Allocate memory - {setter}");

                    setters.push(setter);
                }
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
                    let setter = if let Some(allowed_values) = allowed_values {
                        if is_i32 {
                            ParamSetter::I32(allowed_values.get_i32(self.unstructured)?)
                        } else {
                            ParamSetter::I64(allowed_values.get_i64(self.unstructured)?)
                        }
                    } else if is_i32 {
                        ParamSetter::I32(self.unstructured.arbitrary()?)
                    } else {
                        ParamSetter::I64(self.unstructured.arbitrary()?)
                    };

                    log::trace!("  ----  Pointer value - {setter}");

                    setters.push(setter);
                }
                ProcessedSysCallParams::MemoryArray => {
                    let upper_limit = mem_size.saturating_sub(1);

                    let pointer_beyond = self.unstructured.int_in_range(0..=upper_limit)?;
                    let offset = self.unstructured.int_in_range(0..=pointer_beyond)?;

                    let first = ParamSetter::I32(offset as i32);
                    let second = ParamSetter::I32((pointer_beyond - offset) as i32);
                    log::trace!("  ----  Memory array {first}, {second}");

                    setters.push(first);
                    setters.push(second);
                }
                ProcessedSysCallParams::MemoryPtrValue => {
                    // Subtract a bit more so entities from `gsys` fit.
                    let upper_limit = mem_size.saturating_sub(100);
                    let offset = self.unstructured.int_in_range(0..=upper_limit)?;

                    let setter = ParamSetter::I32(offset as i32);
                    log::trace!("  ----  Memory pointer value - {setter}");

                    setters.push(setter);
                }
            }
        }

        log::trace!(
            "  ----  Random data after SysCallsInvocator::build_param_setters - {}",
            self.unstructured.len()
        );

        assert_eq!(setters.len(), params.len());

        Ok(setters)
    }

    fn build_call(
        &mut self,
        syscall: SysCallInfo,
        call_indexes_handle: CallIndexesHandle,
    ) -> Result<Vec<Instruction>> {
        let ignore_syscall_result = true;

        let mut instructions = vec![];

        let param_setters = self.build_param_setters(&syscall.signature.params)?;
        instructions.extend(param_setters.iter().map(ParamSetter::get_ix));

        instructions.push(Instruction::Call(call_indexes_handle as u32));

        let mut result_processing = if ignore_syscall_result {
            std::iter::repeat(Instruction::Drop)
                .take(syscall.signature.results.len())
                .collect()
        } else {
            self.build_result_processing(syscall, &param_setters)
        };
        instructions.append(&mut result_processing);

        Ok(instructions)
    }

    fn insert_sys_call_instructions(&mut self, instructions: &[Instruction]) -> Result<()> {
        log::trace!(
            "Random data before inserting sys-call's invoke instructions - {}",
            self.unstructured.len()
        );

        let last_funcs_idx = self.module.count_code_funcs() - 1;
        let mut insert_into_func_no = self.unstructured.int_in_range(0..=last_funcs_idx)?;

        // Do not insert into custom newly generated function, but only into pre-defined
        // internal functions.
        //
        // This loop will definitely end, because there are only 4 custom functions (3 for gear entry points
        // and one for precise reservation send) and minimal amount of internal functions is 15.
        while self.call_indexes.is_custom_func(insert_into_func_no) {
            insert_into_func_no = self.unstructured.int_in_range(0..=last_funcs_idx)?;
        }

        log::trace!(" -- Inserting sys-call into function with idx {insert_into_func_no}");

        self.module.with(|mut module| {
            let code = module
                .code_section_mut()
                .expect("has at least one function by config")
                .bodies_mut()[insert_into_func_no]
                .code_mut()
                .elements_mut();

            // The end of insertion range is second-to-last index, as the last
            // index is defined for `Instruction::End` of the function body.
            // But if there's only one instruction in the function, then `0`
            // index is used as an insertion point.
            let last = if code.len() > 1 { code.len() - 2 } else { 0 };

            let res = self.unstructured.int_in_range(0..=last).map(|pos| {
                log::trace!(" -- Inserting into position {pos}");
                code.splice(pos..pos, instructions.iter().cloned());
            });

            log::trace!(
                "Random data after inserting sys-call's invoke instructions - {}",
                self.unstructured.len()
            );

            (module, res)
        })
    }

    fn resolves_calls_indexes(&mut self) {
        log::trace!("Resolving calls indexes");

        let imports_num = self.module.count_import_funcs() as u32;

        self.module.with(|mut module| {
            let each_func_instructions = module
                .code_section_mut()
                .expect("has at least 1 function by config")
                .bodies_mut()
                .iter_mut()
                .flat_map(|body| body.code_mut().elements_mut().iter_mut());
            for instruction in each_func_instructions {
                if let Instruction::Call(call_indexes_handle) = instruction {
                    let index_ty = self
                        .call_indexes
                        .get(*call_indexes_handle as usize)
                        .expect("getting by handle of existing call");
                    match index_ty {
                        FunctionIndex::Func(idx) => {
                            log::trace!(" -- Old function index - {idx}");
                            *call_indexes_handle = idx + imports_num;
                            log::trace!(" -- New function index - {}", *call_indexes_handle);
                        }
                        FunctionIndex::Import(idx) => *call_indexes_handle = idx,
                    }
                }
            }

            let export_funcs_call_indexes_handles = module
                .export_section_mut()
                // This generator is instantiated from SysCallsImportsGenerator, which can only be
                // generated if entry points and memory import were generated.
                .expect("has at least 1 export")
                .entries_mut()
                .iter_mut()
                .filter_map(|export| match export.internal_mut() {
                    Internal::Function(call_indexes_handle) => Some(call_indexes_handle),
                    _ => None,
                });

            for export_call_indexes_handle in export_funcs_call_indexes_handles {
                let FunctionIndex::Func(idx) = self.call_indexes
                    .get(*export_call_indexes_handle as usize)
                    .expect("getting by handle of existing call") else {
                        // Export can be to the import function by WASM specification,
                        // but we currently do not support this in wasm-gen.
                        panic!("Export cannot be to the import function");
                    };

                log::trace!(" -- Old export function index - {idx}");
                *export_call_indexes_handle = idx + imports_num;
                log::trace!(
                    " -- New export function index - {}",
                    *export_call_indexes_handle
                );
            }

            (module, ())
        })
    }
}

/// Disabled sys-calls invocator.
///
/// This type signals that sys-calls imports generation, additional data injection and
/// sys-calls invocation (with further call indexes resolution) is done.
pub struct DisabledSysCallsInvocator {
    module: WasmModule,
    call_indexes: CallIndexes,
}

impl From<DisabledSysCallsInvocator> for ModuleWithCallIndexes {
    fn from(disabled_sys_calls_invocator: DisabledSysCallsInvocator) -> Self {
        ModuleWithCallIndexes {
            module: disabled_sys_calls_invocator.module,
            call_indexes: disabled_sys_calls_invocator.call_indexes,
        }
    }
}
