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
    InvocableSysCall, SysCallParamAllowedValues, SysCallsConfig, SysCallsParamsConfig,
};
use arbitrary::{Result, Unstructured};
use gear_wasm_instrument::{
    parity_wasm::elements::{BlockType, Instruction, Internal, ValueType},
    syscalls::{ParamType, SysCallName, SysCallSignature},
};
use std::{
    collections::{btree_map::Entry, BTreeMap, BinaryHeap},
    iter,
};

#[derive(Debug)]
pub(crate) enum ProcessedSysCallParams {
    Alloc {
        allowed_values: Option<SysCallParamAllowedValues>,
    },
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
            ParamType::Alloc => ProcessedSysCallParams::Alloc {
                allowed_values: params_config.get_rule(&param),
            },
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
/// inject addresses, sys-calls invocator could falsely set `gr_send*` and `gr_exit` call's destination param
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

/// Newtype used to mark that some instruction is used to push values to stack before syscall execution.
#[derive(Clone)]
struct ParamSetter(Instruction);

impl ParamSetter {
    fn new_i32(value: i32) -> Self {
        Self(Instruction::I32Const(value))
    }

    fn new_i64(value: i64) -> Self {
        Self(Instruction::I64Const(value))
    }

    fn into_ix(self) -> Instruction {
        self.0
    }

    fn as_i32(&self) -> Option<i32> {
        if let Instruction::I32Const(value) = self.0 {
            Some(value)
        } else {
            None
        }
    }

    fn get_value(&self) -> i64 {
        match self.0 {
            Instruction::I32Const(value) => value as i64,
            Instruction::I64Const(value) => value,
            _ => unimplemented!("Incorrect instruction found"),
        }
    }
}

pub type SysCallInvokeInstructions = Vec<Instruction>;

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

        self.insert_sys_calls()?;

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

    fn insert_sys_calls(&mut self) -> Result<()> {
        log::trace!(
            "Random data before inserting sys-calls invoke instructions - {}",
            self.unstructured.len()
        );

        let code_funcs = self.module.count_code_funcs();
        let insert_into_funcs: Vec<_> = (0..code_funcs)
            .filter(|idx| !self.call_indexes.is_custom_func(*idx))
            .collect();

        let syscalls_to_insert =
            self.sys_call_imports
                .clone()
                .into_iter()
                .flat_map(|(syscall, (amount, _))| {
                    iter::repeat(syscall)
                        .take(amount as usize)
                        .collect::<Vec<_>>()
                });

        let insertion_mapping =
            self.build_syscalls_insertion_mapping(syscalls_to_insert, &insert_into_funcs)?;

        for (insert_into_fn, syscalls) in insertion_mapping {
            self.insert_sys_calls_into_fn(insert_into_fn, syscalls)?;
        }

        log::trace!(
            "Random data after inserting sys-calls invoke instructions - {}",
            self.unstructured.len()
        );

        Ok(())
    }

    /// Distributes provided syscalls among provided function ids.
    ///
    /// Returns mapping `func_id` <-> `syscalls which should be inserted into func_id`.
    fn build_syscalls_insertion_mapping<I>(
        &mut self,
        syscalls: I,
        insert_into_funcs: &[usize],
    ) -> Result<BTreeMap<usize, Vec<InvocableSysCall>>>
    where
        I: Iterator<Item = InvocableSysCall>,
    {
        let mut insertion_mapping: BTreeMap<usize, Vec<InvocableSysCall>> = BTreeMap::new();
        for syscall in syscalls {
            let insert_into = *self.unstructured.choose(insert_into_funcs)?;

            match insertion_mapping.entry(insert_into) {
                Entry::Occupied(mut entry) => {
                    entry.get_mut().push(syscall);
                }
                Entry::Vacant(entry) => {
                    entry.insert(vec![syscall]);
                }
            }
        }

        Ok(insertion_mapping)
    }

    fn insert_sys_calls_into_fn(
        &mut self,
        insert_into_fn: usize,
        syscalls: Vec<InvocableSysCall>,
    ) -> Result<()> {
        log::trace!(
            "Random data before inserting sys-calls invoke instructions into function {insert_into_fn} - {}",
            self.unstructured.len()
        );

        let fn_code_len = self.module.count_func_instructions(insert_into_fn);

        // The end of insertion range is second-to-last index, as the last
        // index is defined for `Instruction::End` of the function body.
        // But if there's only one instruction in the function, then `0`
        // index is used as an insertion point.
        let last = if fn_code_len > 1 { fn_code_len - 2 } else { 0 };

        // Sort in descending order. It's needed to guarantee that no syscall
        // invocation instructions will intersect with each other as we start
        // inserting syscalls from the last index.
        let insertion_positions = iter::from_fn(|| Some(self.unstructured.int_in_range(0..=last)))
            .take(syscalls.len())
            .collect::<Result<BinaryHeap<_>>>()?
            .into_sorted_vec()
            .into_iter()
            .rev();

        for (pos, syscall) in insertion_positions.zip(syscalls) {
            let call_indexes_handle = self
                .sys_call_imports
                .get(&syscall)
                .map(|(_, call_indexes_handle)| *call_indexes_handle)
                .expect("Syscall presented in sys_call_imports");
            let instructions = self.build_sys_call_invoke_instructions(
                syscall,
                syscall.into_signature(),
                call_indexes_handle,
            )?;

            log::trace!(
                " -- Inserting syscall {} into function {insert_into_fn} at position {pos}",
                syscall.to_str()
            );

            self.module.with(|mut module| {
                let code = module
                    .code_section_mut()
                    .expect("has at least one function by config")
                    .bodies_mut()[insert_into_fn]
                    .code_mut()
                    .elements_mut();
                code.splice(pos..pos, instructions);
                (module, ())
            });
        }

        log::trace!(
            "Random data after inserting sys-calls invoke instructions into function {insert_into_fn} - {}",
            self.unstructured.len()
        );

        Ok(())
    }

    fn build_sys_call_invoke_instructions(
        &mut self,
        invocable: InvocableSysCall,
        signature: SysCallSignature,
        call_indexes_handle: CallIndexesHandle,
    ) -> Result<SysCallInvokeInstructions> {
        log::trace!(
            "Random data before building {} sys-call invoke instructions - {}",
            invocable.to_str(),
            self.unstructured.len()
        );

        if self.is_sys_call_with_destination(invocable) {
            log::trace!(
                " -- Generating build call for {} sys-call with destination",
                invocable.to_str()
            );

            self.build_call_with_destination(invocable, signature, call_indexes_handle)
        } else {
            log::trace!(
                " -- Generating build call for common sys-call {}",
                invocable.to_str()
            );

            self.build_call(invocable, signature, call_indexes_handle)
        }
    }

    fn build_call_with_destination(
        &mut self,
        invocable: InvocableSysCall,
        mut signature: SysCallSignature,
        call_indexes_handle: CallIndexesHandle,
    ) -> Result<Vec<Instruction>> {
        // The value for the first param is chosen from config.
        // It's either the result of `gr_source`, some existing address (set in the data section) or a completely random value.
        signature.params.remove(0);
        let mut call_without_destination_instrs =
            self.build_call(invocable, signature, call_indexes_handle)?;

        let res = if self.config.sys_call_destination().is_source() {
            log::trace!(" -- Sys-call destination is result of `gr_source`");

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
                    assert!(self.config.sys_call_destination().is_existing_addresses());
                    log::trace!(" -- Sys-call destination is an existing program address");

                    offsets.next_offset()
                }
                None => {
                    assert!(self.config.sys_call_destination().is_random());
                    log::trace!(" -- Sys-call destination is a random address");

                    self.unstructured.arbitrary()?
                }
            };

            instructions.push(Instruction::I32Const(address_offset as i32));
            instructions.append(&mut call_without_destination_instrs);

            instructions
        };

        Ok(res)
    }

    fn is_sys_call_with_destination(&self, sys_call: InvocableSysCall) -> bool {
        self.is_send_sys_call(sys_call) || self.is_exit_sys_call(sys_call)
    }

    fn is_send_sys_call(&self, sys_call: InvocableSysCall) -> bool {
        use InvocableSysCall::*;
        [
            Loose(SysCallName::Send),
            Loose(SysCallName::SendWGas),
            Loose(SysCallName::SendInput),
            Loose(SysCallName::SendInputWGas),
            Precise(SysCallName::ReservationSend),
            Precise(SysCallName::SendCommit),
            Precise(SysCallName::SendCommitWGas),
        ]
        .contains(&sys_call)
    }

    fn is_exit_sys_call(&self, sys_call: InvocableSysCall) -> bool {
        matches!(sys_call, InvocableSysCall::Loose(SysCallName::Exit))
    }

    fn build_call(
        &mut self,
        invocable: InvocableSysCall,
        signature: SysCallSignature,
        call_indexes_handle: CallIndexesHandle,
    ) -> Result<Vec<Instruction>> {
        let param_setters = self.build_param_setters(&signature.params)?;
        let mut instructions: Vec<_> = param_setters
            .iter()
            .cloned()
            .map(ParamSetter::into_ix)
            .collect();

        instructions.push(Instruction::Call(call_indexes_handle as u32));

        let insert_error_processing = self
            .config
            .error_processing_config()
            .error_should_be_processed(&invocable);

        let mut result_processing = if !insert_error_processing {
            Self::build_result_processing_ignored(signature)
        } else if invocable.is_fallible() {
            Self::build_result_processing_fallible(signature, &param_setters)
        } else {
            Self::build_result_processing_infallible(signature)
        };
        instructions.append(&mut result_processing);

        Ok(instructions)
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
                ProcessedSysCallParams::Alloc { allowed_values } => {
                    let pages_to_alloc = if let Some(allowed_values) = allowed_values {
                        allowed_values.get_i32(self.unstructured)?
                    } else {
                        let mem_size_pages = (mem_size_pages / 3).max(1);
                        self.unstructured.int_in_range(0..=mem_size_pages)? as i32
                    };

                    log::trace!("  ----  Allocate memory - {pages_to_alloc}");

                    setters.push(ParamSetter::new_i32(pages_to_alloc));
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
                            ParamSetter::new_i32(allowed_values.get_i32(self.unstructured)?)
                        } else {
                            ParamSetter::new_i64(allowed_values.get_i64(self.unstructured)?)
                        }
                    } else if is_i32 {
                        ParamSetter::new_i32(self.unstructured.arbitrary()?)
                    } else {
                        ParamSetter::new_i64(self.unstructured.arbitrary()?)
                    };

                    log::trace!("  ----  Pointer value - {}", setter.get_value());

                    setters.push(setter);
                }
                ProcessedSysCallParams::MemoryArray => {
                    let upper_limit = mem_size.saturating_sub(1) as i32;

                    let offset = self.unstructured.int_in_range(0..=upper_limit)?;
                    let length = self.unstructured.int_in_range(0..=(upper_limit - offset))?;

                    log::trace!("  ----  Memory array {offset}, {length}");

                    setters.push(ParamSetter::new_i32(offset));
                    setters.push(ParamSetter::new_i32(length));
                }
                ProcessedSysCallParams::MemoryPtrValue => {
                    // Subtract a bit more so entities from `gsys` fit.
                    let upper_limit = mem_size.saturating_sub(100);
                    let offset = self.unstructured.int_in_range(0..=upper_limit)? as i32;

                    let setter = ParamSetter::new_i32(offset);
                    log::trace!("  ----  Memory pointer value - {offset}");

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

    fn build_result_processing_ignored(signature: SysCallSignature) -> Vec<Instruction> {
        iter::repeat(Instruction::Drop)
            .take(signature.results.len())
            .collect()
    }

    fn build_result_processing_fallible(
        signature: SysCallSignature,
        param_setters: &[ParamSetter],
    ) -> Vec<Instruction> {
        // TODO: #3129.
        // Assume here that:
        // 1. All the fallible syscalls write error to the pointer located in the last argument in syscall.
        // 2. All the errors contain `ErrorCode` in the start of memory where pointer points.

        static_assertions::assert_eq_size!(gsys::ErrorCode, u32);
        assert_eq!(gsys::ErrorCode::default(), 0);

        let params = signature.params;
        assert!(matches!(
            params
                .last()
                .expect("The last argument of fallible syscall must be pointer to error code"),
            ParamType::Ptr(None)
        ));
        assert_eq!(params.len(), param_setters.len());

        if let Some(ptr) = param_setters
            .last()
            .expect("At least one argument in fallible syscall")
            .as_i32()
        {
            vec![
                Instruction::I32Const(ptr),
                Instruction::I32Load(2, 0),
                Instruction::I32Const(0),
                Instruction::I32Ne,
                Instruction::If(BlockType::NoResult),
                Instruction::Unreachable,
                Instruction::End,
            ]
        } else {
            panic!("Incorrect last parameter type: expected pointer");
        }
    }

    fn build_result_processing_infallible(signature: SysCallSignature) -> Vec<Instruction> {
        // TODO: #3129
        // For now we don't check anywhere that `alloc` and `free` return
        // error codes as described here. Also we don't assert that only `alloc` and `free`
        // will have their first arguments equal to `ParamType::Alloc` and `ParamType::Free`.
        let results_len = signature.results.len();

        if results_len == 0 {
            return vec![];
        }

        assert_eq!(results_len, 1);

        let error_code = match signature.params[0] {
            ParamType::Alloc => {
                // Alloc syscall: returns u32::MAX (= -1i32) in case of error.
                -1
            }
            ParamType::Free => {
                // Free syscall: returns 1 in case of error.
                1
            }
            _ => {
                unimplemented!("Only alloc and free are supported for now")
            }
        };

        vec![
            Instruction::I32Const(error_code),
            Instruction::I32Eq,
            Instruction::If(BlockType::NoResult),
            Instruction::Unreachable,
            Instruction::End,
        ]
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
