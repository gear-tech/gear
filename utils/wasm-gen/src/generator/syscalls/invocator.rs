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

//! Syscalls invocator module.

use crate::{
    generator::{
        AddressesInjectionOutcome, AddressesOffsets, CallIndexes, CallIndexesHandle,
        DisabledAdditionalDataInjector, FunctionIndex, ModuleWithCallIndexes,
    },
    wasm::{PageCount as WasmPageCount, WasmModule},
    InvocableSyscall, SyscallParamAllowedValues, SyscallsConfig, SyscallsParamsConfig,
};
use arbitrary::{Result, Unstructured};
use gear_wasm_instrument::{
    parity_wasm::elements::{BlockType, Instruction, Internal, ValueType},
    syscalls::{
        FallibleSyscallSignature, ParamType, Ptr, RegularParamType, SyscallName, SyscallSignature,
        SystemSyscallSignature,
    },
};
use gsys::Hash;
use std::{
    collections::{btree_map::Entry, BTreeMap, BinaryHeap, HashSet},
    iter, mem,
    num::NonZeroU32,
};

#[derive(Debug)]
pub(crate) enum ProcessedSyscallParams {
    Alloc {
        allowed_values: Option<SyscallParamAllowedValues>,
    },
    FreeUpperBound {
        allowed_values: Option<SyscallParamAllowedValues>,
    },
    Value {
        value_type: ValueType,
        allowed_values: Option<SyscallParamAllowedValues>,
    },
    MemoryArrayLength,
    MemoryArrayPtr,
    MemoryPtrValue,
}

pub(crate) fn process_syscall_params(
    params: &[ParamType],
    params_config: &SyscallsParamsConfig,
) -> Vec<ProcessedSyscallParams> {
    use ParamType::*;
    use RegularParamType::*;

    let length_param_indexes = params
        .iter()
        .filter_map(|&param| match param {
            Regular(Pointer(
                Ptr::SizedBufferStart { length_param_idx }
                | Ptr::MutSizedBufferStart { length_param_idx },
            )) => Some(length_param_idx),
            _ => None,
        })
        .collect::<HashSet<_>>();

    let mut res = Vec::with_capacity(params.len());
    for (param_idx, &param) in params.iter().enumerate() {
        let processed_param = match param {
            Regular(Alloc) => ProcessedSyscallParams::Alloc {
                allowed_values: params_config.get_rule(&param),
            },
            Regular(Length) if length_param_indexes.contains(&param_idx) => {
                // Due to match guard `ParamType::Size` can be processed in two ways:
                // 1. The function will return `ProcessedSyscallParams::MemoryArraySize`
                //    if this parameter is associated with PtrType::BufferStart { .. }`.
                // 2. Otherwise, `ProcessedSyscallParams::Value` will be returned from the function.
                ProcessedSyscallParams::MemoryArrayLength
            }
            Regular(Pointer(Ptr::SizedBufferStart { .. })) => {
                ProcessedSyscallParams::MemoryArrayPtr
            }
            // It's guaranteed that fallible syscall has error pointer as a last param.
            Regular(Pointer(_)) | Error(_) => ProcessedSyscallParams::MemoryPtrValue,
            Regular(FreeUpperBound) => {
                let allowed_values = params_config.get_rule(&param);
                ProcessedSyscallParams::FreeUpperBound { allowed_values }
            }
            _ => ProcessedSyscallParams::Value {
                value_type: param.into(),
                allowed_values: params_config.get_rule(&param),
            },
        };

        res.push(processed_param);
    }

    res
}

/// Syscalls invocator.
///
/// Inserts syscalls invokes randomly into internal functions.
///
/// This type is instantiated from disable additional data injector and
/// data injection outcome ([`AddressesInjectionOutcome`]). The latter was introduced
/// to give additional guarantees for config and generators consistency. Otherwise,
/// if there wasn't any addresses injection outcome, which signals that there was a try to
/// inject addresses, syscalls invocator could falsely set `gr_send*` and `gr_exit` call's destination param
/// to random value. For example, existing addresses could have been defined in the config, but
/// additional data injector was disabled, before injecting addresses from the config. As a result,
/// invocator would set un-intended by config values as messages destination. To avoid such
/// inconsistency the [`AddressesInjectionOutcome`] gives additional required guarantees.
pub struct SyscallsInvocator<'a, 'b> {
    unstructured: &'b mut Unstructured<'a>,
    call_indexes: CallIndexes,
    module: WasmModule,
    config: SyscallsConfig,
    offsets: Option<AddressesOffsets>,
    syscalls_imports: BTreeMap<InvocableSyscall, (Option<NonZeroU32>, CallIndexesHandle)>,
}

impl<'a, 'b>
    From<(
        DisabledAdditionalDataInjector<'a, 'b>,
        AddressesInjectionOutcome,
    )> for SyscallsInvocator<'a, 'b>
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
            syscalls_imports: disabled_gen.syscalls_imports,
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

    /// Get value of the instruction.
    ///
    /// # Panics
    /// Panics if the instruction is not `I32Const` or `I64Const`.
    fn get_value(&self) -> i64 {
        match self.0 {
            Instruction::I32Const(value) => value as i64,
            Instruction::I64Const(value) => value,
            _ => unimplemented!("Incorrect instruction found"),
        }
    }
}

pub type SyscallInvokeInstructions = Vec<Instruction>;

impl<'a, 'b> SyscallsInvocator<'a, 'b> {
    /// Insert syscalls invokes.
    ///
    /// The method builds instructions, which describe how each syscall is called, and then
    /// insert these instructions into any random function. In the end, all call indexes are resolved.
    pub fn insert_invokes(mut self) -> Result<DisabledSyscallsInvocator> {
        log::trace!(
            "Random data before inserting all syscalls invocations - {}",
            self.unstructured.len()
        );

        self.insert_syscalls()?;

        log::trace!(
            "Random data after inserting all syscalls invocations - {}",
            self.unstructured.len()
        );

        self.resolves_calls_indexes();

        Ok(DisabledSyscallsInvocator {
            module: self.module,
            call_indexes: self.call_indexes,
        })
    }

    fn insert_syscalls(&mut self) -> Result<()> {
        let insertion_mapping = self.build_syscalls_insertion_mapping()?;
        for (insert_into_fn, syscalls) in insertion_mapping {
            self.insert_syscalls_into_fn(insert_into_fn, syscalls)?;
        }

        Ok(())
    }

    /// Distributes provided syscalls among provided function ids.
    ///
    /// Returns mapping `func_id` <-> `syscalls which should be inserted into func_id`.
    fn build_syscalls_insertion_mapping(
        &mut self,
    ) -> Result<BTreeMap<usize, Vec<InvocableSyscall>>> {
        let insert_into_funcs = self.call_indexes.predefined_funcs_indexes();
        let syscalls = self
            .syscalls_imports
            .clone()
            .into_iter()
            .filter_map(|(syscall, (amount, _))| amount.map(|a| (syscall, a)));

        let mut insertion_mapping: BTreeMap<_, Vec<_>> = BTreeMap::new();
        for (syscall, amount) in syscalls {
            for _ in 0..amount.get() {
                let insert_into = self.unstructured.int_in_range(insert_into_funcs.clone())?;

                match insertion_mapping.entry(insert_into) {
                    Entry::Occupied(mut entry) => {
                        entry.get_mut().push(syscall);
                    }
                    Entry::Vacant(entry) => {
                        entry.insert(vec![syscall]);
                    }
                }
            }
        }

        Ok(insertion_mapping)
    }

    fn insert_syscalls_into_fn(
        &mut self,
        insert_into_fn: usize,
        syscalls: Vec<InvocableSyscall>,
    ) -> Result<()> {
        log::trace!(
            "Random data before inserting syscalls invoke instructions into function with index {insert_into_fn} - {}",
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
                .syscalls_imports
                .get(&syscall)
                .map(|(_, call_indexes_handle)| *call_indexes_handle)
                .expect("Syscall presented in syscall_imports");
            let instructions =
                self.build_syscall_invoke_instructions(syscall, call_indexes_handle)?;

            log::trace!(
                " -- Inserting syscall `{}` into function with index {insert_into_fn} at position {pos}",
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
            "Random data after inserting syscalls invoke instructions into function {insert_into_fn} - {}",
            self.unstructured.len()
        );

        Ok(())
    }

    fn build_syscall_invoke_instructions(
        &mut self,
        invocable: InvocableSyscall,
        call_indexes_handle: CallIndexesHandle,
    ) -> Result<SyscallInvokeInstructions> {
        log::trace!(
            "Random data before building `{}` syscall invoke instructions - {}",
            invocable.to_str(),
            self.unstructured.len()
        );

        if let Some(argument_index) = invocable.destination_param_idx() {
            log::trace!(
                " -- Building call instructions for a `{}` syscall with destination",
                invocable.to_str()
            );

            self.build_call_with_destination(invocable, call_indexes_handle, argument_index)
        } else {
            log::trace!(
                " -- Building call for a common syscall `{}`",
                invocable.to_str()
            );

            self.build_call(invocable, call_indexes_handle)
        }
    }

    fn build_call_with_destination(
        &mut self,
        invocable: InvocableSyscall,
        call_indexes_handle: CallIndexesHandle,
        destination_arg_idx: usize,
    ) -> Result<Vec<Instruction>> {
        // The value for the destination param is chosen from config.
        // It's either the result of `gr_source`, some existing address (set in the data section) or a completely random value.
        let mut original_instructions = self.build_call(invocable, call_indexes_handle)?;

        let destination_instructions = if self.config.syscall_destination().is_source() {
            log::trace!("  ---  Syscall destination is result of `gr_source`");

            let gr_source_call_indexes_handle = self
                .syscalls_imports
                .get(&InvocableSyscall::Loose(SyscallName::Source))
                .map(|&(_, call_indexes_handle)| call_indexes_handle as u32)
                .expect("by config if destination is source, then `gr_source` is generated");

            let mem_size = self
                .module
                .initial_mem_size()
                .map(Into::<WasmPageCount>::into)
                // To instantiate this generator, we must instantiate SyscallImportsGenerator, which can be
                // instantiated only with memory import generation proof.
                .expect("generator is instantiated with a memory import generation proof")
                .memory_size();
            // Subtract a bit more so entities from `gsys` fit.
            let upper_limit = mem_size.saturating_sub(100);
            let offset = self.unstructured.int_in_range(0..=upper_limit)?;

            // 3 instructions for invoking `gsys::gr_source` and possibly 3 more
            // for defining value param so HashWithValue will be constructed.
            let mut ret = Vec::with_capacity(6);
            ret.extend_from_slice(&[
                // call `gsys::gr_source` storing actor id and some `offset` pointer.
                Instruction::I32Const(offset as i32),
                Instruction::Call(gr_source_call_indexes_handle),
                Instruction::I32Const(offset as i32),
            ]);

            if invocable.has_destination_param_with_value() {
                // We have to skip actor id bytes to define the following value param.
                let skip_bytes = mem::size_of::<Hash>();
                ret.extend_from_slice(&[
                    // Define 0 value for HashWithValue
                    Instruction::I32Const(0),
                    // Store value on the offset + skip_bytes. That will form HashWithValue.
                    Instruction::I32Store(2, skip_bytes as u32),
                    // Pass the offset as the first argument to the syscall with destination.
                    Instruction::I32Const(offset as i32),
                ]);
            }

            ret
        } else {
            let address_offset = match self.offsets.as_mut() {
                Some(offsets) => {
                    assert!(self.config.syscall_destination().is_existing_addresses());
                    log::trace!("  ----  Syscall destination is an existing program address");

                    offsets.next_offset()
                }
                None => {
                    assert!(self.config.syscall_destination().is_random());
                    log::trace!("  ----  Syscall destination is a random address");

                    self.unstructured.arbitrary()?
                }
            };

            vec![Instruction::I32Const(address_offset as i32)]
        };

        original_instructions.splice(
            destination_arg_idx..destination_arg_idx + 1,
            destination_instructions,
        );

        Ok(original_instructions)
    }

    fn build_call(
        &mut self,
        invocable: InvocableSyscall,
        call_indexes_handle: CallIndexesHandle,
    ) -> Result<Vec<Instruction>> {
        let signature = invocable.into_signature();
        let param_setters = self.build_param_setters(signature.params())?;
        let mut instructions: Vec<_> = param_setters
            .iter()
            .cloned()
            .map(ParamSetter::into_ix)
            .collect();

        instructions.push(Instruction::Call(call_indexes_handle as u32));

        let process_error = self
            .config
            .error_processing_config()
            .error_should_be_processed(invocable);

        let mut result_processing = match signature {
            SyscallSignature::Infallible(_) => {
                // It's guaranteed here that infallible has no errors to process
                // as it has not mut err pointers or error indicating values returned.
                Vec::new()
            }
            signature @ (SyscallSignature::Fallible(_) | SyscallSignature::System(_)) => {
                // It's guaranteed by definition that these variants return an error either by returning
                // error indicating value or by having err mut pointer in params.
                if process_error {
                    Self::build_error_processing(signature, param_setters)
                } else {
                    Self::build_error_processing_ignored(signature)
                }
            }
        };
        instructions.append(&mut result_processing);

        Ok(instructions)
    }

    fn build_param_setters(&mut self, params: &[ParamType]) -> Result<Vec<ParamSetter>> {
        log::trace!(
            "  -- Random data before building param setters - {}",
            self.unstructured.len()
        );

        let mem_size_pages = self
            .module
            .initial_mem_size()
            // To instantiate this generator, we must instantiate SyscallImportsGenerator, which can be
            // instantiated only with memory import generation proof.
            .expect("generator is instantiated with a memory import generation proof");
        let mem_size = Into::<WasmPageCount>::into(mem_size_pages).memory_size();

        let mut setters = Vec::with_capacity(params.len());
        let mut memory_array_definition: Option<(i32, Option<i32>)> = None;

        for processed_param in process_syscall_params(params, self.config.params_config()) {
            match processed_param {
                ProcessedSyscallParams::Alloc { allowed_values } => {
                    let pages_to_alloc = if let Some(allowed_values) = allowed_values {
                        allowed_values.get_i32(self.unstructured)?
                    } else {
                        let mem_size_pages = (mem_size_pages / 3).max(1);
                        self.unstructured.int_in_range(0..=mem_size_pages)? as i32
                    };

                    log::trace!("  ----  Allocate memory - {pages_to_alloc}");

                    setters.push(ParamSetter::new_i32(pages_to_alloc));
                }
                ProcessedSyscallParams::Value {
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

                    log::trace!("  ----  Value - {}", setter.get_value());

                    setters.push(setter);
                }
                ProcessedSyscallParams::MemoryArrayLength => {
                    let length;
                    let upper_limit = mem_size.saturating_sub(1) as i32;

                    (memory_array_definition, length) = if let Some((offset, _)) =
                        memory_array_definition
                    {
                        let length = self.unstructured.int_in_range(0..=(upper_limit - offset))?;
                        (None, length)
                    } else {
                        let offset = self.unstructured.int_in_range(0..=upper_limit)?;
                        let length = self.unstructured.int_in_range(0..=(upper_limit - offset))?;
                        (Some((offset, Some(length))), length)
                    };

                    log::trace!("  ----  Memory array length - {length}");
                    setters.push(ParamSetter::new_i32(length));
                }
                ProcessedSyscallParams::MemoryArrayPtr => {
                    let offset;
                    let upper_limit = mem_size.saturating_sub(1) as i32;

                    (memory_array_definition, offset) =
                        if let Some((offset, _)) = memory_array_definition {
                            (None, offset)
                        } else {
                            let offset = self.unstructured.int_in_range(0..=upper_limit)?;
                            (Some((offset, None)), offset)
                        };

                    log::trace!("  ----  Memory array offset - {offset}");
                    setters.push(ParamSetter::new_i32(offset));
                }
                ProcessedSyscallParams::MemoryPtrValue => {
                    // Subtract a bit more so entities from `gsys` fit.
                    let upper_limit = mem_size.saturating_sub(100);
                    let offset = self.unstructured.int_in_range(0..=upper_limit)? as i32;

                    let setter = ParamSetter::new_i32(offset);
                    log::trace!("  ----  Memory pointer value - {offset}");

                    setters.push(setter);
                }
                ProcessedSyscallParams::FreeUpperBound { allowed_values } => {
                    // This is the case only for `free_range` syscall.
                    let previous_param = setters
                        .last()
                        .expect("free_range syscall has at least 2 params")
                        .as_i32()
                        .expect("referenced param should evaluate to I32Const");

                    let delta = allowed_values
                        .expect("allowed_values should be set for FreeUpperBound")
                        .get_i32(self.unstructured)?;
                    let param = previous_param.saturating_add(delta);

                    log::trace!("  ----  Free upper bound - {param}, and delta - {delta}");

                    setters.push(ParamSetter::new_i32(param))
                }
            }
        }

        log::trace!(
            "  -- Random data after building param setters - {}",
            self.unstructured.len()
        );

        assert_eq!(setters.len(), params.len());

        Ok(setters)
    }

    fn build_error_processing(
        signature: SyscallSignature,
        param_setters: Vec<ParamSetter>,
    ) -> Vec<Instruction>
    where
        'a: 'b,
    {
        match signature {
            SyscallSignature::Fallible(fallible) => {
                Self::build_fallible_syscall_error_processing(fallible, param_setters)
            }
            SyscallSignature::System(system) => Self::build_system_syscall_error_processing(system),
            SyscallSignature::Infallible(_) => unreachable!(
                "Invalid implementation. This function is called only for returning errors syscall"
            ),
        }
    }

    fn build_fallible_syscall_error_processing(
        fallible_signature: FallibleSyscallSignature,
        param_setters: Vec<ParamSetter>,
    ) -> Vec<Instruction> {
        // TODO: #3129.
        // Assume here that:
        // 1. All the fallible syscalls write error to the pointer located in the last argument in syscall.
        // 2. All the errors contain `ErrorCode` in the start of memory where pointer points.

        static_assertions::assert_eq_size!(gsys::ErrorCode, u32);
        let no_error_val = gsys::ErrorCode::default() as i32;

        assert_eq!(
            fallible_signature.params().len(),
            param_setters.len(),
            "ParamsSetter is inconsistent with syscall params."
        );
        let res_ptr = param_setters
            .last()
            .expect("At least one argument in fallible syscall")
            .as_i32()
            .expect("Incorrect last parameter type: expected i32 pointer");

        vec![
            Instruction::I32Const(res_ptr),
            Instruction::I32Load(2, 0),
            Instruction::I32Const(no_error_val),
            Instruction::I32Ne,
            Instruction::If(BlockType::NoResult),
            Instruction::Unreachable,
            Instruction::End,
        ]
    }

    fn build_system_syscall_error_processing(
        system_signature: SystemSyscallSignature,
    ) -> Vec<Instruction> {
        // That's basically those syscalls, that doesn't have an error pointer,
        // but return value indicating error. These are currently `Alloc`, `Free` and `FreeRange`.
        assert_eq!(system_signature.results().len(), 1);

        let error_code = match system_signature.params()[0] {
            ParamType::Regular(RegularParamType::Alloc) => {
                // Alloc syscall: returns u32::MAX (= -1i32) in case of error.
                -1
            }
            ParamType::Regular(RegularParamType::Free | RegularParamType::FreeUpperBound) => {
                // Free/FreeRange syscall: returns 1 in case of error.
                1
            }
            _ => {
                unimplemented!("Only alloc, free and free_range are supported for now")
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

    fn build_error_processing_ignored(signature: SyscallSignature) -> Vec<Instruction> {
        match signature {
            SyscallSignature::System(system) => iter::repeat(Instruction::Drop)
                .take(system.results().len())
                .collect(),
            SyscallSignature::Fallible(_) => Vec::new(),
            SyscallSignature::Infallible(_) => unreachable!(
                "Invalid implementation. This function is called only for returning errors syscall"
            ),
        }
    }

    fn resolves_calls_indexes(&mut self) {
        log::trace!("Resolving calls indexes");

        let imports_num = self.module.count_import_funcs() as u32;
        let mut logged = HashSet::with_capacity(self.call_indexes.len());

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
                            let old_idx = *call_indexes_handle;
                            *call_indexes_handle = idx + imports_num;

                            // Log only not changed indexes, because loop can receive repeted
                            // call indexes.
                            if !logged.contains(&*call_indexes_handle) {
                                logged.insert(*call_indexes_handle);

                                log::trace!(
                                    " -- Old function index - {old_idx}, new index - {}",
                                    *call_indexes_handle
                                );
                            }
                        }
                        FunctionIndex::Import(idx) => *call_indexes_handle = idx,
                    }
                }
            }

            let export_funcs_call_indexes_handles = module
                .export_section_mut()
                // This generator is instantiated from SyscallsImportsGenerator, which can only be
                // generated if entry points and memory import were generated.
                .expect("has at least 1 export")
                .entries_mut()
                .iter_mut()
                .filter_map(|export| match export.internal_mut() {
                    Internal::Function(call_indexes_handle) => Some(call_indexes_handle),
                    _ => None,
                });

            for export_call_indexes_handle in export_funcs_call_indexes_handles {
                let FunctionIndex::Func(idx) = self
                    .call_indexes
                    .get(*export_call_indexes_handle as usize)
                    .expect("getting by handle of existing call")
                else {
                    // Export can be to the import function by WASM specification,
                    // but we currently do not support this in wasm-gen.
                    panic!("Export cannot be to the import function");
                };

                let old_idx = *export_call_indexes_handle;
                *export_call_indexes_handle = idx + imports_num;

                log::trace!(
                    " -- Old export function index - {old_idx}, new index - {}",
                    *export_call_indexes_handle
                );
            }

            (module, ())
        })
    }
}

/// Disabled syscalls invocator.
///
/// This type signals that syscalls imports generation, additional data injection and
/// syscalls invocation (with further call indexes resolution) is done.
pub struct DisabledSyscallsInvocator {
    module: WasmModule,
    call_indexes: CallIndexes,
}

impl From<DisabledSyscallsInvocator> for ModuleWithCallIndexes {
    fn from(disabled_syscalls_invocator: DisabledSyscallsInvocator) -> Self {
        ModuleWithCallIndexes {
            module: disabled_syscalls_invocator.module,
            call_indexes: disabled_syscalls_invocator.call_indexes,
        }
    }
}
