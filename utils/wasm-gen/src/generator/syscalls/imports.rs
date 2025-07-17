// This file is part of Gear.

// Copyright (C) 2021-2025 Gear Technologies Inc.
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

//! Syscalls imports generator module.

use crate::{
    InvocableSyscall, MemoryLayout, SyscallInjectionType, SyscallsConfig,
    generator::{
        CallIndexes, CallIndexesHandle, FrozenGearWasmGenerator, GearEntryPointGenerationProof,
        GearWasmGenerator, MemoryImportGenerationProof, ModuleWithCallIndexes,
    },
    wasm::{PageCount as WasmPageCount, WasmModule},
};
use arbitrary::{Error as ArbitraryError, Result, Unstructured};
use gear_wasm_instrument::{
    Function, Import, Instruction, MemArg, ModuleBuilder, syscalls::SyscallName,
};
use gsys::{Handle, Hash, Length};
use std::{collections::BTreeMap, num::NonZero};
use wasmparser::BlockType;

/// Gear syscalls imports generator.
pub struct SyscallsImportsGenerator<'a, 'b> {
    unstructured: &'b mut Unstructured<'a>,
    call_indexes: CallIndexes,
    module: WasmModule,
    config: SyscallsConfig,
    syscalls_imports: BTreeMap<InvocableSyscall, (Option<NonZero<u32>>, CallIndexesHandle)>,
}

/// Syscalls imports generator instantiator.
///
/// Serves as a new type in order to create the generator from gear wasm generator and proofs.
pub struct SyscallsImportsGeneratorInstantiator<'a, 'b>(
    (GearWasmGenerator<'a, 'b>, GearEntryPointGenerationProof),
);

/// The set of syscalls that need to be imported to create precise syscall.
#[derive(thiserror::Error, Debug)]
#[error("The following syscalls must be imported: {0:?}")]
pub struct RequiredSyscalls(&'static [SyscallName]);

/// An error that occurs when generating precise syscall.
#[derive(thiserror::Error, Debug)]
pub enum PreciseSyscallError {
    #[error("{0}")]
    RequiredImports(#[from] RequiredSyscalls),
    #[error("{0}")]
    Arbitrary(#[from] ArbitraryError),
}

impl<'a, 'b> From<(GearWasmGenerator<'a, 'b>, GearEntryPointGenerationProof)>
    for SyscallsImportsGeneratorInstantiator<'a, 'b>
{
    fn from(inner: (GearWasmGenerator<'a, 'b>, GearEntryPointGenerationProof)) -> Self {
        Self(inner)
    }
}

impl<'a, 'b> From<SyscallsImportsGeneratorInstantiator<'a, 'b>>
    for (
        SyscallsImportsGenerator<'a, 'b>,
        FrozenGearWasmGenerator<'a, 'b>,
    )
{
    fn from(instantiator: SyscallsImportsGeneratorInstantiator<'a, 'b>) -> Self {
        let SyscallsImportsGeneratorInstantiator((generator, _gen_ep_gen_proof)) = instantiator;
        let syscall_gen = SyscallsImportsGenerator {
            unstructured: generator.unstructured,
            call_indexes: generator.call_indexes,
            module: generator.module,
            config: generator.config.syscalls_config.clone(),
            syscalls_imports: Default::default(),
        };
        let frozen = FrozenGearWasmGenerator {
            config: generator.config,
            call_indexes: None,
            unstructured: None,
        };

        (syscall_gen, frozen)
    }
}

impl<'a, 'b> SyscallsImportsGenerator<'a, 'b> {
    /// Instantiate a new gear syscalls imports generator.
    ///
    /// The generator instantiations requires having type-level proof that the wasm module has memory import in it.
    /// This proof could be gotten from memory generator.
    pub fn new(
        module_with_indexes: ModuleWithCallIndexes,
        config: SyscallsConfig,
        unstructured: &'b mut Unstructured<'a>,
        _mem_import_gen_proof: MemoryImportGenerationProof,
        _gen_ep_gen_proof: GearEntryPointGenerationProof,
    ) -> Self {
        let ModuleWithCallIndexes {
            module,
            call_indexes,
        } = module_with_indexes;

        Self {
            unstructured,
            call_indexes,
            module,
            config,
            syscalls_imports: Default::default(),
        }
    }

    /// Disable current generator.
    pub fn disable(self) -> DisabledSyscallsImportsGenerator<'a, 'b> {
        log::trace!(
            "Random data when disabling syscalls imports generator - {}",
            self.unstructured.len()
        );
        DisabledSyscallsImportsGenerator {
            unstructured: self.unstructured,
            call_indexes: self.call_indexes,
            module: self.module,
            config: self.config,
            syscalls_imports: self.syscalls_imports,
        }
    }

    /// Generates syscalls imports and precise syscalls, which are functions that call syscalls which have
    /// a precise version of them. For more info on precise syscalls see [`InvocableSyscall`].
    ///
    /// Returns disabled syscalls imports generator and a proof that imports from config were generated.
    pub fn generate(
        mut self,
    ) -> Result<(
        DisabledSyscallsImportsGenerator<'a, 'b>,
        SyscallsImportsGenerationProof,
    )> {
        log::trace!("Generating syscalls imports");

        let syscalls_proof = self.generate_syscalls_imports()?;
        self.generate_precise_syscalls()?;

        Ok((self.disable(), syscalls_proof))
    }

    /// Generates syscalls imports from config, used to instantiate the generator.
    pub fn generate_syscalls_imports(&mut self) -> Result<SyscallsImportsGenerationProof> {
        log::trace!(
            "Random data before syscalls imports - {}",
            self.unstructured.len()
        );

        for syscall in SyscallName::instrumentable() {
            let syscall_generation_data = self.generate_syscall_import(syscall)?;
            if let Some(syscall_generation_data) = syscall_generation_data {
                self.syscalls_imports
                    .insert(InvocableSyscall::Loose(syscall), syscall_generation_data);
            }
        }

        Ok(SyscallsImportsGenerationProof(()))
    }

    /// Generates precise syscalls and handles errors if any occurred during generation.
    fn generate_precise_syscalls(&mut self) -> Result<()> {
        use SyscallName::*;

        #[allow(clippy::type_complexity)]
        let precise_syscalls: [(
            SyscallName,
            fn(&mut Self, SyscallName) -> Result<CallIndexesHandle, PreciseSyscallError>,
        ); 5] = [
            (ReservationSend, Self::generate_send_from_reservation),
            (ReservationReply, Self::generate_reply_from_reservation),
            (SendCommit, Self::generate_send_commit),
            (SendCommitWGas, Self::generate_send_commit_with_gas),
            (ReplyDeposit, Self::generate_reply_deposit),
        ];

        for (precise_syscall, generate_method) in precise_syscalls {
            let syscall_injection_type = self
                .config
                .injection_type(InvocableSyscall::Precise(precise_syscall));
            if let SyscallInjectionType::Function(syscall_amount_range) = syscall_injection_type {
                let precise_syscall_amount =
                    NonZero::<u32>::new(self.unstructured.int_in_range(syscall_amount_range)?);
                log::trace!(
                    "Constructing `{name}` syscall...",
                    name = InvocableSyscall::Precise(precise_syscall).to_str()
                );

                if precise_syscall_amount.is_none() {
                    // Amount is zero.
                    continue;
                }

                match generate_method(self, precise_syscall) {
                    Ok(call_indexes_handle) => {
                        self.syscalls_imports.insert(
                            InvocableSyscall::Precise(precise_syscall),
                            (precise_syscall_amount, call_indexes_handle),
                        );
                    }
                    Err(PreciseSyscallError::RequiredImports(err)) => {
                        // By syscalls injection types config all required syscalls for
                        // precise syscalls are set.
                        // By generator's implementation, precise calls are generated after
                        // generating syscalls imports.
                        panic!(
                            "Invalid generators configuration or implementation: required syscalls aren't set: {err}"
                        )
                    }
                    Err(PreciseSyscallError::Arbitrary(err)) => return Err(err),
                }
            }
        }

        Ok(())
    }

    /// Generate import of the gear syscall defined by `syscall` param.
    ///
    /// Returns [`Option`] which wraps the tuple of maybe non-zero amount of syscall further injections
    /// and handle in the call indexes collection. The amount type is `NonZero<u32>` in order to distinguish
    /// between syscalls imports that must be generated without further invocation and ones,
    /// that must be invoked along with the import generation.
    /// If no import is required, `None` is returned.
    fn generate_syscall_import(
        &mut self,
        syscall: SyscallName,
    ) -> Result<Option<(Option<NonZero<u32>>, CallIndexesHandle)>> {
        let syscall_injection_type = self.config.injection_type(InvocableSyscall::Loose(syscall));

        let syscall_amount = match syscall_injection_type {
            SyscallInjectionType::Import => 0,
            SyscallInjectionType::Function(syscall_amount_range) => {
                self.unstructured.int_in_range(syscall_amount_range)?
            }
            _ => return Ok(None),
        };

        // Insert import either for case of `SyscallInjectionType::Import`, or
        // if `SyscallInjectionType::Function(syscall_amount_range)` yielded zero.
        let call_indexes_handle = self.insert_syscall_import(syscall);
        log::trace!(
            " -- Syscall `{}` will be invoked {} times",
            syscall.to_str(),
            syscall_amount,
        );

        Ok(Some((
            NonZero::<u32>::new(syscall_amount),
            call_indexes_handle,
        )))
    }

    /// Inserts gear syscall defined by the `syscall` param.
    fn insert_syscall_import(&mut self, syscall: SyscallName) -> CallIndexesHandle {
        let syscall_import_idx = self.module.count_import_funcs();
        let syscall_signature = syscall.signature().func_type();

        // Insert syscall import to the module
        self.module.with(|module| {
            let mut builder = ModuleBuilder::from_module(module);
            let signature_idx = builder.push_type(syscall_signature);
            // Create import entry with the built signature.
            builder.push_import(Import::func("env", syscall.to_str(), signature_idx));

            (builder.build(), ())
        });

        let call_indexes_handle = self.call_indexes.len();
        self.call_indexes.add_import(syscall_import_idx);

        call_indexes_handle
    }
}

impl SyscallsImportsGenerator<'_, '_> {
    /// The amount of reserved memory used to create a precise syscall.
    const PRECISE_SYSCALL_RESERVED_MEMORY_SIZE: u32 = 128;

    /// Generates a function which calls "properly" the `gr_reservation_send`.
    fn generate_send_from_reservation(
        &mut self,
        syscall: SyscallName,
    ) -> Result<CallIndexesHandle, PreciseSyscallError> {
        let [reserve_gas_idx, reservation_send_idx] =
            self.invocable_syscalls_indexes(InvocableSyscall::required_imports(syscall))?;

        // subtract to be sure we are in memory boundaries.
        let rid_pid_value_ptr = self.reserve_memory();
        let pid_value_ptr = rid_pid_value_ptr + size_of::<Hash>() as i32;

        let func_instructions = vec![
            // Copy the HashWithValue struct (48 bytes) containing
            // the recipient and value
            Instruction::I32Const(pid_value_ptr),
            Instruction::LocalGet(0),
            Instruction::I64Load(MemArg::i64()),
            Instruction::I64Store(MemArg::i64()),
            Instruction::I32Const(pid_value_ptr),
            Instruction::LocalGet(0),
            Instruction::I64Load(MemArg::i64_offset(8)),
            Instruction::I64Store(MemArg::i64_offset(8)),
            Instruction::I32Const(pid_value_ptr),
            Instruction::LocalGet(0),
            Instruction::I64Load(MemArg::i64_offset(16)),
            Instruction::I64Store(MemArg::i64_offset(16)),
            Instruction::I32Const(pid_value_ptr),
            Instruction::LocalGet(0),
            Instruction::I64Load(MemArg::i64_offset(24)),
            Instruction::I64Store(MemArg::i64_offset(24)),
            Instruction::I32Const(pid_value_ptr),
            Instruction::LocalGet(0),
            Instruction::I64Load(MemArg::i64_offset(32)),
            Instruction::I64Store(MemArg::i64_offset(32)),
            Instruction::I32Const(pid_value_ptr),
            Instruction::LocalGet(0),
            Instruction::I64Load(MemArg::i64_offset(40)),
            Instruction::I64Store(MemArg::i64_offset(40)),
            // Amount of gas to reserve
            Instruction::LocalGet(4),
            // Duration of the reservation
            Instruction::LocalGet(5),
            // Pointer to the ErrorWithHash struct
            Instruction::LocalGet(6),
            Instruction::Call(reserve_gas_idx as u32),
            // Pointer to the ErrorWithHash struct
            Instruction::LocalGet(6),
            // Load ErrorWithHash.error
            Instruction::I32Load(MemArg::i32()),
            // Check if ErrorWithHash.error == 0
            Instruction::I32Eqz,
            // If ErrorWithHash.error == 0
            Instruction::If(BlockType::Empty),
            // Copy the Hash struct (32 bytes) containing the reservation id.
            Instruction::I32Const(rid_pid_value_ptr),
            Instruction::LocalGet(6),
            Instruction::I64Load(MemArg::i64_offset(4)),
            Instruction::I64Store(MemArg::i64()),
            Instruction::I32Const(rid_pid_value_ptr),
            Instruction::LocalGet(6),
            Instruction::I64Load(MemArg::i64_offset(12)),
            Instruction::I64Store(MemArg::i64_offset(8)),
            Instruction::I32Const(rid_pid_value_ptr),
            Instruction::LocalGet(6),
            Instruction::I64Load(MemArg::i64_offset(20)),
            Instruction::I64Store(MemArg::i64_offset(16)),
            Instruction::I32Const(rid_pid_value_ptr),
            Instruction::LocalGet(6),
            Instruction::I64Load(MemArg::i64_offset(28)),
            Instruction::I64Store(MemArg::i64_offset(24)),
            // Pointer to reservation ID, recipient ID and value
            Instruction::I32Const(rid_pid_value_ptr),
            // Pointer to payload
            Instruction::LocalGet(1),
            // Size of the payload
            Instruction::LocalGet(2),
            // Number of blocks to delay the sending for
            Instruction::LocalGet(3),
            // Pointer to the result of the reservation send
            Instruction::LocalGet(6),
            Instruction::Call(reservation_send_idx as u32),
            Instruction::End,
            Instruction::End,
        ];
        let call_indexes_handle = self.generate_proper_syscall_function(syscall, func_instructions);

        Ok(call_indexes_handle)
    }

    /// Generates a function which calls "properly" the `gr_reservation_reply`.
    fn generate_reply_from_reservation(
        &mut self,
        syscall: SyscallName,
    ) -> Result<CallIndexesHandle, PreciseSyscallError> {
        let [reserve_gas_idx, reservation_reply_idx] =
            self.invocable_syscalls_indexes(InvocableSyscall::required_imports(syscall))?;

        // subtract to be sure we are in memory boundaries.
        let rid_value_ptr = self.reserve_memory();
        let value_ptr = rid_value_ptr + size_of::<Hash>() as i32;

        let func_instructions = vec![
            // Amount of gas to reserve
            Instruction::LocalGet(3),
            // Duration of the reservation
            Instruction::LocalGet(4),
            // Pointer to the ErrorWithHash struct
            Instruction::LocalGet(5),
            Instruction::Call(reserve_gas_idx as u32),
            // Pointer to the ErrorWithHash struct
            Instruction::LocalGet(5),
            // Load ErrorWithHash.error
            Instruction::I32Load(MemArg::i32()),
            // Check if ErrorWithHash.error == 0
            Instruction::I32Eqz,
            // If ErrorWithHash.error == 0
            Instruction::If(BlockType::Empty),
            // Copy the Hash struct (32 bytes) containing the reservation id.
            Instruction::I32Const(rid_value_ptr),
            Instruction::LocalGet(5),
            Instruction::I64Load(MemArg::i64_offset(4)),
            Instruction::I64Store(MemArg::i64()),
            Instruction::I32Const(rid_value_ptr),
            Instruction::LocalGet(5),
            Instruction::I64Load(MemArg::i64_offset(12)),
            Instruction::I64Store(MemArg::i64_offset(8)),
            Instruction::I32Const(rid_value_ptr),
            Instruction::LocalGet(5),
            Instruction::I64Load(MemArg::i64_offset(20)),
            Instruction::I64Store(MemArg::i64_offset(16)),
            Instruction::I32Const(rid_value_ptr),
            Instruction::LocalGet(5),
            Instruction::I64Load(MemArg::i64_offset(28)),
            Instruction::I64Store(MemArg::i64_offset(24)),
            // Copy the value (16 bytes).
            Instruction::I32Const(value_ptr),
            Instruction::LocalGet(0),
            Instruction::I64Load(MemArg::i64()),
            Instruction::I64Store(MemArg::i64()),
            Instruction::I32Const(value_ptr),
            Instruction::LocalGet(0),
            Instruction::I64Load(MemArg::i64_offset(8)),
            Instruction::I64Store(MemArg::i64_offset(8)),
            // Pointer to reservation ID and value
            Instruction::I32Const(rid_value_ptr),
            // Pointer to payload
            Instruction::LocalGet(1),
            // Size of the payload
            Instruction::LocalGet(2),
            // Pointer to the result of the reservation reply
            Instruction::LocalGet(5),
            Instruction::Call(reservation_reply_idx as u32),
            Instruction::End,
            Instruction::End,
        ];
        let call_indexes_handle = self.generate_proper_syscall_function(syscall, func_instructions);

        Ok(call_indexes_handle)
    }

    /// Generates a function which calls "properly" the `gr_send_commit`.
    fn generate_send_commit(
        &mut self,
        syscall: SyscallName,
    ) -> Result<CallIndexesHandle, PreciseSyscallError> {
        let [send_init_idx, send_push_idx, send_commit_idx] =
            self.invocable_syscalls_indexes(InvocableSyscall::required_imports(syscall))?;

        // subtract to be sure we are in memory boundaries.
        let handle_ptr = self.reserve_memory();
        let pid_value_ptr = handle_ptr + size_of::<Handle>() as i32;

        let mut elements = vec![
            // Pointer to the ErrorWithHandle struct
            Instruction::LocalGet(4),
            Instruction::Call(send_init_idx as u32),
            // Pointer to the ErrorWithHandle struct
            Instruction::LocalGet(4),
            // Load ErrorWithHandle.error
            Instruction::I32Load(MemArg::i32()),
            // Check if ErrorWithHandle.error == 0
            Instruction::I32Eqz,
            // If ErrorWithHandle.error == 0
            Instruction::If(BlockType::Empty),
            // Copy the Handle
            Instruction::I32Const(handle_ptr),
            Instruction::LocalGet(4),
            Instruction::I32Load(MemArg::i32_offset(4)),
            Instruction::I32Store(MemArg::i32()),
        ];

        let number_of_pushes = self.unstructured.int_in_range(
            self.config
                .precise_syscalls_config()
                .range_of_send_push_calls(),
        )?;

        for _ in 0..number_of_pushes {
            elements.extend_from_slice(&[
                // Handle of message
                Instruction::I32Const(handle_ptr),
                Instruction::I32Load(MemArg::i32()),
                // Pointer to payload
                Instruction::LocalGet(1),
                // Size of the payload
                Instruction::LocalGet(2),
                // Pointer to the result of the send push
                Instruction::LocalGet(4),
                Instruction::Call(send_push_idx as u32),
            ]);
        }

        elements.extend_from_slice(&[
            // Copy the HashWithValue struct (48 bytes) containing the recipient and value
            // TODO: extract into another method
            Instruction::I32Const(pid_value_ptr),
            Instruction::LocalGet(0),
            Instruction::I64Load(MemArg::i64()),
            Instruction::I64Store(MemArg::i64()),
            Instruction::I32Const(pid_value_ptr),
            Instruction::LocalGet(0),
            Instruction::I64Load(MemArg::i64_offset(8)),
            Instruction::I64Store(MemArg::i64_offset(8)),
            Instruction::I32Const(pid_value_ptr),
            Instruction::LocalGet(0),
            Instruction::I64Load(MemArg::i64_offset(16)),
            Instruction::I64Store(MemArg::i64_offset(16)),
            Instruction::I32Const(pid_value_ptr),
            Instruction::LocalGet(0),
            Instruction::I64Load(MemArg::i64_offset(24)),
            Instruction::I64Store(MemArg::i64_offset(24)),
            Instruction::I32Const(pid_value_ptr),
            Instruction::LocalGet(0),
            Instruction::I64Load(MemArg::i64_offset(32)),
            Instruction::I64Store(MemArg::i64_offset(32)),
            Instruction::I32Const(pid_value_ptr),
            Instruction::LocalGet(0),
            Instruction::I64Load(MemArg::i64_offset(40)),
            Instruction::I64Store(MemArg::i64_offset(40)),
            // Handle of message
            Instruction::I32Const(handle_ptr),
            Instruction::I32Load(MemArg::i32()),
            // Pointer to recipient ID and value
            Instruction::I32Const(pid_value_ptr),
            // Number of blocks to delay the sending for
            Instruction::LocalGet(3),
            // Pointer to the result of the send commit
            Instruction::LocalGet(4),
            Instruction::Call(send_commit_idx as u32),
            Instruction::End,
            Instruction::End,
        ]);
        let func_instructions = elements;
        let call_indexes_handle = self.generate_proper_syscall_function(syscall, func_instructions);

        Ok(call_indexes_handle)
    }

    /// Generates a function which calls "properly" the `gr_send_commit_wgas`.
    fn generate_send_commit_with_gas(
        &mut self,
        syscall: SyscallName,
    ) -> Result<CallIndexesHandle, PreciseSyscallError> {
        let [
            size_idx,
            send_init_idx,
            send_push_input_idx,
            send_commit_wgas_idx,
        ] = self.invocable_syscalls_indexes(InvocableSyscall::required_imports(syscall))?;

        // subtract to be sure we are in memory boundaries.
        let handle_ptr = self.reserve_memory();
        let pid_value_ptr = handle_ptr + size_of::<Handle>() as i32;
        let length_ptr = pid_value_ptr + size_of::<Length>() as i32;

        let mut elements = vec![
            // Pointer to the ErrorWithHandle struct
            Instruction::LocalGet(3),
            Instruction::Call(send_init_idx as u32),
            // Pointer to the ErrorWithHandle struct
            Instruction::LocalGet(3),
            // Load ErrorWithHandle.error
            Instruction::I32Load(MemArg::i32()),
            // Check if ErrorWithHandle.error == 0
            Instruction::I32Eqz,
            // If ErrorWithHandle.error == 0
            Instruction::If(BlockType::Empty),
            // Copy the Handle
            Instruction::I32Const(handle_ptr),
            Instruction::LocalGet(3),
            Instruction::I32Load(MemArg::i32_offset(4)),
            Instruction::I32Store(MemArg::i32()),
            // Pointer to message length
            Instruction::I32Const(length_ptr),
            Instruction::Call(size_idx as u32),
        ];

        let number_of_pushes = self.unstructured.int_in_range(
            self.config
                .precise_syscalls_config()
                .range_of_send_push_calls(),
        )?;

        for _ in 0..number_of_pushes {
            elements.extend_from_slice(&[
                // Handle of message
                Instruction::I32Const(handle_ptr),
                Instruction::I32Load(MemArg::i32()),
                // Offset of input
                Instruction::I32Const(0),
                // Length of input
                Instruction::I32Const(length_ptr),
                Instruction::I32Load(MemArg::i32()),
                // Pointer to the result of the send push input
                Instruction::LocalGet(3),
                Instruction::Call(send_push_input_idx as u32),
            ]);
        }

        elements.extend_from_slice(&[
            // Copy the HashWithValue struct (48 bytes) containing the recipient and value
            // TODO: extract into another method
            Instruction::I32Const(pid_value_ptr),
            Instruction::LocalGet(0),
            Instruction::I64Load(MemArg::i64()),
            Instruction::I64Store(MemArg::i64()),
            Instruction::I32Const(pid_value_ptr),
            Instruction::LocalGet(0),
            Instruction::I64Load(MemArg::i64_offset(8)),
            Instruction::I64Store(MemArg::i64_offset(8)),
            Instruction::I32Const(pid_value_ptr),
            Instruction::LocalGet(0),
            Instruction::I64Load(MemArg::i64_offset(16)),
            Instruction::I64Store(MemArg::i64_offset(16)),
            Instruction::I32Const(pid_value_ptr),
            Instruction::LocalGet(0),
            Instruction::I64Load(MemArg::i64_offset(24)),
            Instruction::I64Store(MemArg::i64_offset(24)),
            Instruction::I32Const(pid_value_ptr),
            Instruction::LocalGet(0),
            Instruction::I64Load(MemArg::i64_offset(32)),
            Instruction::I64Store(MemArg::i64_offset(32)),
            Instruction::I32Const(pid_value_ptr),
            Instruction::LocalGet(0),
            Instruction::I64Load(MemArg::i64_offset(40)),
            Instruction::I64Store(MemArg::i64_offset(40)),
            // Handle of message
            Instruction::I32Const(handle_ptr),
            Instruction::I32Load(MemArg::i32()),
            // Pointer to recipient ID and value
            Instruction::I32Const(pid_value_ptr),
            // Gas limit for message
            Instruction::LocalGet(2),
            // Number of blocks to delay the sending for
            Instruction::LocalGet(1),
            // Pointer to the result of the send commit
            Instruction::LocalGet(3),
            Instruction::Call(send_commit_wgas_idx as u32),
            Instruction::End,
            Instruction::End,
        ]);
        let func_instructions = elements;
        let call_indexes_handle = self.generate_proper_syscall_function(syscall, func_instructions);

        Ok(call_indexes_handle)
    }

    /// Generates a function which calls "properly" the `gr_reply_deposit`.
    fn generate_reply_deposit(
        &mut self,
        syscall: SyscallName,
    ) -> Result<CallIndexesHandle, PreciseSyscallError> {
        let [send_input_idx, reply_deposit_idx] =
            self.invocable_syscalls_indexes(InvocableSyscall::required_imports(syscall))?;

        let mid_ptr = self.reserve_memory();

        let precise_reply_deposit_invocation = [
            // Pointer to pid_value argument of HashWithValue type.
            Instruction::LocalGet(0),
            // Offset value defining starting index in the received message payload.
            Instruction::LocalGet(1),
            // Length of the slice of the received message payload.
            Instruction::LocalGet(2),
            // Delay.
            Instruction::LocalGet(3),
            // Pointer to the result of the `gr_send_input`, which is of type ErrorWithHash.
            Instruction::LocalGet(5),
            // Invocation of the `gr_send_input`.
            Instruction::Call(send_input_idx as u32),
            // Load ErrorWithHash.
            Instruction::LocalGet(5),
            // Take first 4 bytes from the data of ErrorWithHash type, which is error code, i.e.
            // ErrorWithHash.error.
            Instruction::I32Load(MemArg::i32()),
            // Check if ErrorWithHash.error == 0.
            Instruction::I32Eqz,
            // If ErrorWithHash.error == 0.
            Instruction::If(BlockType::Empty),
            // Copy Hash struct (32 bytes) containing message id.
            // Push on stack ptr to address where message id will be defined.
            Instruction::I32Const(mid_ptr),
            // Get the ErrorWithHash result of the `gr_send_input` call
            Instruction::LocalGet(5),
            // Load 8 bytes from the ErrorWithHash skipping first 4 bytes,
            // which are bytes of i32 error_code value.
            Instruction::I64Load(MemArg::i64_offset(4)),
            // Store these 8 bytes in the `mid_ptr` starting from the byte 0.
            Instruction::I64Store(MemArg::i64()),
            // Perform same procedure 3 times more to complete
            // 32 bytes message id value under `mid_ptr`.
            Instruction::I32Const(mid_ptr),
            Instruction::LocalGet(5),
            Instruction::I64Load(MemArg::i64_offset(12)),
            Instruction::I64Store(MemArg::i64_offset(8)),
            Instruction::I32Const(mid_ptr),
            Instruction::LocalGet(5),
            Instruction::I64Load(MemArg::i64_offset(20)),
            Instruction::I64Store(MemArg::i64_offset(16)),
            Instruction::I32Const(mid_ptr),
            Instruction::LocalGet(5),
            Instruction::I64Load(MemArg::i64_offset(28)),
            Instruction::I64Store(MemArg::i64_offset(24)),
            // Pointer to message id.
            Instruction::I32Const(mid_ptr),
            // Pointer to gas value for `gr_reply_deposit`.
            Instruction::LocalGet(4),
            // Pointer to the result of the `gr_reply_deposit`.
            Instruction::LocalGet(5),
            // Invocation of `gr_reply_deposit`.
            Instruction::Call(reply_deposit_idx as u32),
            Instruction::End,
        ];

        let invocations_amount = self.unstructured.int_in_range(
            self.config
                .precise_syscalls_config()
                .range_of_send_input_calls(),
        )?;

        // The capacity is amount of times `gr_reply_deposit` is invoked precisely + 1 for `End` instruction.
        let mut func_instructions =
            Vec::with_capacity(precise_reply_deposit_invocation.len() * invocations_amount + 1);
        for _ in 0..invocations_amount {
            func_instructions.extend_from_slice(&precise_reply_deposit_invocation);
        }
        func_instructions.push(Instruction::End);

        let call_indexes_handle = self.generate_proper_syscall_function(syscall, func_instructions);

        Ok(call_indexes_handle)
    }

    /// Returns the indexes of invocable syscalls.
    fn invocable_syscalls_indexes<const N: usize>(
        &mut self,
        syscalls: &'static [SyscallName; N],
    ) -> Result<[usize; N], RequiredSyscalls> {
        let mut indexes = [0; N];

        for (index, &syscall) in indexes.iter_mut().zip(syscalls.iter()) {
            *index = self
                .syscalls_imports
                .get(&InvocableSyscall::Loose(syscall))
                .map(|&(_, call_indexes_handle)| call_indexes_handle)
                .ok_or_else(|| RequiredSyscalls(&syscalls[..]))?;
        }

        Ok(indexes)
    }

    /// Reserves enough memory build precise syscall.
    fn reserve_memory(&self) -> i32 {
        self.memory_size_bytes()
            .saturating_sub(MemoryLayout::RESERVED_MEMORY_SIZE)
            .saturating_sub(Self::PRECISE_SYSCALL_RESERVED_MEMORY_SIZE) as i32
    }

    /// Returns the size of the memory in bytes that can be used to build precise syscall.
    fn memory_size_bytes(&self) -> u32 {
        let initial_mem_size: WasmPageCount = self
            .module
            .initial_mem_size()
            .expect("generator is instantiated with a mem import generation proof")
            .into();
        initial_mem_size.memory_size()
    }

    /// Generates a function which calls "properly" the given syscall.
    fn generate_proper_syscall_function(
        &mut self,
        syscall: SyscallName,
        func_instructions: Vec<Instruction>,
    ) -> CallIndexesHandle {
        let invocable_syscall = InvocableSyscall::Precise(syscall);
        let signature = invocable_syscall.into_signature();

        let func_ty = signature.func_type();

        let func_idx = self.module.with(|module| {
            let mut builder = ModuleBuilder::from_module(module);
            let idx = builder.add_func(func_ty, Function::from_instructions(func_instructions));

            (builder.build(), idx)
        });

        log::trace!(
            "Built proper call to {precise_syscall_name}",
            precise_syscall_name = invocable_syscall.to_str()
        );

        let call_indexes_handle = self.call_indexes.len();
        self.call_indexes.add_func(func_idx as usize);

        call_indexes_handle
    }
}

/// Proof that there was an instance of syscalls imports generator and `SyscallsImportsGenerator::generate_syscalls_imports` was called.
pub struct SyscallsImportsGenerationProof(());

/// Disabled gear wasm syscalls generator.
///
/// Instance of this types signals that there was once active syscalls generator,
/// but it ended up it's work.
pub struct DisabledSyscallsImportsGenerator<'a, 'b> {
    pub(super) unstructured: &'b mut Unstructured<'a>,
    pub(super) call_indexes: CallIndexes,
    pub(super) module: WasmModule,
    pub(super) config: SyscallsConfig,
    pub(super) syscalls_imports:
        BTreeMap<InvocableSyscall, (Option<NonZero<u32>>, CallIndexesHandle)>,
}

impl<'a, 'b> From<DisabledSyscallsImportsGenerator<'a, 'b>> for ModuleWithCallIndexes {
    fn from(disabled_syscall_gen: DisabledSyscallsImportsGenerator<'a, 'b>) -> Self {
        ModuleWithCallIndexes {
            module: disabled_syscall_gen.module,
            call_indexes: disabled_syscall_gen.call_indexes,
        }
    }
}
