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

//! Sys-calls imports generator module.

use crate::{
    generator::{
        CallIndexes, CallIndexesHandle, FrozenGearWasmGenerator, GearEntryPointGenerationProof,
        GearWasmGenerator, MemoryImportGenerationProof, ModuleWithCallIndexes,
    },
    wasm::{PageCount as WasmPageCount, WasmModule},
    InvocableSysCall, SysCallsConfig,
};
use arbitrary::{Error as ArbitraryError, Result, Unstructured};
use gear_wasm_instrument::{
    parity_wasm::{
        builder,
        elements::{BlockType, Instruction, Instructions},
    },
    syscalls::SysCallName,
};
use gsys::{Handle, Hash, Length};
use std::{collections::BTreeMap, mem};

/// Gear syscalls imports generator.
pub struct SysCallsImportsGenerator<'a, 'b> {
    unstructured: &'b mut Unstructured<'a>,
    call_indexes: CallIndexes,
    module: WasmModule,
    config: SysCallsConfig,
    syscalls_imports: BTreeMap<InvocableSysCall, (u32, CallIndexesHandle)>,
}

/// Sys-calls imports generator instantiator.
///
/// Serves as a new type in order to create the generator from gear wasm generator and proofs.
pub struct SysCallsImportsGeneratorInstantiator<'a, 'b>(
    (
        GearWasmGenerator<'a, 'b>,
        MemoryImportGenerationProof,
        GearEntryPointGenerationProof,
    ),
);

/// An error that occurs when generating precise syscall.
#[derive(thiserror::Error, Debug)]
pub enum PreciseSysCallError {
    #[error("{0}")]
    Arbitrary(#[from] ArbitraryError),
}

impl<'a, 'b>
    From<(
        GearWasmGenerator<'a, 'b>,
        MemoryImportGenerationProof,
        GearEntryPointGenerationProof,
    )> for SysCallsImportsGeneratorInstantiator<'a, 'b>
{
    fn from(
        inner: (
            GearWasmGenerator<'a, 'b>,
            MemoryImportGenerationProof,
            GearEntryPointGenerationProof,
        ),
    ) -> Self {
        Self(inner)
    }
}

impl<'a, 'b> From<SysCallsImportsGeneratorInstantiator<'a, 'b>>
    for (
        SysCallsImportsGenerator<'a, 'b>,
        FrozenGearWasmGenerator<'a, 'b>,
    )
{
    fn from(instantiator: SysCallsImportsGeneratorInstantiator<'a, 'b>) -> Self {
        let SysCallsImportsGeneratorInstantiator((
            generator,
            _mem_import_gen_proof,
            _gen_ep_gen_proof,
        )) = instantiator;
        let syscall_gen = SysCallsImportsGenerator {
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

impl<'a, 'b> SysCallsImportsGenerator<'a, 'b> {
    /// Instantiate a new gear syscalls imports generator.
    ///
    /// The generator instantiations requires having type-level proof that the wasm module has memory import in it.
    /// This proof could be gotten from memory generator.
    pub fn new(
        module_with_indexes: ModuleWithCallIndexes,
        config: SysCallsConfig,
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
    pub fn disable(self) -> DisabledSysCallsImportsGenerator<'a, 'b> {
        log::trace!(
            "Random data when disabling syscalls imports generator - {}",
            self.unstructured.len()
        );
        DisabledSysCallsImportsGenerator {
            unstructured: self.unstructured,
            call_indexes: self.call_indexes,
            module: self.module,
            config: self.config,
            syscalls_imports: self.syscalls_imports,
        }
    }

    /// Generates syscalls imports and a function, that calls `gr_reservation_send` from config,
    /// used to instantiate the generator.
    ///
    /// Returns disabled syscalls imports generator and a proof that imports from config were generated.
    pub fn generate(
        mut self,
    ) -> Result<(
        DisabledSysCallsImportsGenerator<'a, 'b>,
        SysCallsImportsGenerationProof,
    )> {
        log::trace!("Generating syscalls imports");

        let syscalls_proof = self.generate_syscalls_imports()?;
        self.generate_precise_syscalls()?;

        Ok((self.disable(), syscalls_proof))
    }

    /// Generates syscalls imports from config, used to instantiate the generator.
    pub fn generate_syscalls_imports(&mut self) -> Result<SysCallsImportsGenerationProof> {
        log::trace!(
            "Random data before syscalls imports - {}",
            self.unstructured.len()
        );

        for syscall in SysCallName::instrumentable() {
            let syscall_generation_data = self.generate_syscall_import(syscall)?;
            if let Some(syscall_generation_data) = syscall_generation_data {
                self.syscalls_imports
                    .insert(InvocableSysCall::Loose(syscall), syscall_generation_data);
            }
        }

        Ok(SysCallsImportsGenerationProof(()))
    }

    /// Generates precise syscalls and handles errors if any occurred during generation.
    fn generate_precise_syscalls(&mut self) -> Result<()> {
        use SysCallName::*;

        #[allow(clippy::type_complexity)]
        let syscalls: [(
            SysCallName,
            fn(&mut Self, SysCallName) -> Result<(), PreciseSysCallError>,
        ); 4] = [
            (ReservationSend, Self::generate_send_from_reservation),
            (ReservationReply, Self::generate_reply_from_reservation),
            (SendCommit, Self::generate_send_commit),
            (SendCommitWGas, Self::generate_send_commit_with_gas),
        ];

        for (syscall, generate_method) in syscalls {
            let syscall_amount_range = self
                .config
                .injection_amounts(InvocableSysCall::Precise(syscall));
            let syscall_amount = self.unstructured.int_in_range(syscall_amount_range)?;
            for _ in 0..syscall_amount {
                log::trace!(
                    "Constructing {name} syscall...",
                    name = InvocableSysCall::Precise(syscall).to_str()
                );

                if let Err(PreciseSysCallError::Arbitrary(err)) = generate_method(self, syscall) {
                    return Err(err);
                }
            }
        }

        Ok(())
    }

    /// Generate import of the gear syscall defined by `syscall` param.
    ///
    /// Returns [`Option`] which wraps the tuple of amount of syscall further injections
    /// and handle in the call indexes collection, if amount is not zero. Otherwise returns
    /// None.
    fn generate_syscall_import(
        &mut self,
        syscall: SysCallName,
    ) -> Result<Option<(u32, CallIndexesHandle)>> {
        let syscall_amount_range = self
            .config
            .injection_amounts(InvocableSysCall::Loose(syscall));
        let syscall_amount = self.unstructured.int_in_range(syscall_amount_range)?;
        Ok((syscall_amount != 0).then(|| {
            let call_indexes_handle = self.insert_syscall_import(syscall);
            log::trace!(
                " -- Generated {} amount of {} syscall",
                syscall_amount,
                syscall.to_str()
            );

            (syscall_amount, call_indexes_handle)
        }))
    }

    /// Inserts gear syscall defined by the `syscall` param.
    fn insert_syscall_import(&mut self, syscall: SysCallName) -> CallIndexesHandle {
        let syscall_import_idx = self.module.count_import_funcs();

        // Insert syscall import to the module
        self.module.with(|module| {
            let mut module_builder = builder::from_module(module);

            // Build signature applicable for the parity-wasm for the sys call
            let syscall_signature = syscall.signature().func_type();
            let signature_idx = module_builder.push_signature(
                builder::signature()
                    .with_params(syscall_signature.params().iter().copied())
                    .with_results(syscall_signature.results().iter().copied())
                    .build_sig(),
            );

            // Create import entry with the built signature.
            module_builder.push_import(
                builder::import()
                    .module("env")
                    .external()
                    .func(signature_idx)
                    .field(syscall.to_str())
                    .build(),
            );

            (module_builder.build(), ())
        });

        let call_indexes_handle = self.call_indexes.len();
        self.call_indexes.add_import(syscall_import_idx);

        call_indexes_handle
    }
}

impl<'a, 'b> SysCallsImportsGenerator<'a, 'b> {
    /// The amount of memory used to create a precise syscall.
    const PRECISE_SYS_CALL_MEMORY_SIZE: u32 = 100;

    /// Returns the indexes of invocable syscalls.
    fn invocable_syscalls_indexes<const N: usize>(
        &mut self,
        syscalls: &'static [SysCallName; N],
    ) -> [usize; N] {
        let mut indexes = [0; N];

        for (index, &syscall) in indexes.iter_mut().zip(syscalls.iter()) {
            *index = self
                .syscalls_imports
                .get(&InvocableSysCall::Loose(syscall))
                .map(|&(_, call_indexes_handle)| call_indexes_handle)
                .unwrap_or_else(|| {
                    // insert required import when we can't find it
                    let call_indexes_handle = self.insert_syscall_import(syscall);
                    self.syscalls_imports
                        .insert(InvocableSysCall::Loose(syscall), (0, call_indexes_handle));
                    call_indexes_handle
                })
        }

        indexes
    }

    /// Generates a function which calls "properly" the given syscall.
    fn generate_proper_syscall_invocation(
        &mut self,
        syscall: SysCallName,
        func_instructions: Instructions,
    ) {
        let invocable_syscall = InvocableSysCall::Precise(syscall);
        let signature = invocable_syscall.into_signature();

        let func_ty = signature.func_type();
        let func_signature = builder::signature()
            .with_params(func_ty.params().iter().copied())
            .with_results(func_ty.results().iter().copied())
            .build_sig();

        let func_idx = self.module.with(|module| {
            let mut module_builder = builder::from_module(module);
            let idx = module_builder.push_function(
                builder::function()
                    .with_signature(func_signature)
                    .body()
                    .with_instructions(func_instructions)
                    .build()
                    .build(),
            );

            (module_builder.build(), idx)
        });

        log::trace!(
            "Built proper call to {precise_syscall_name}",
            precise_syscall_name = invocable_syscall.to_str()
        );

        let call_indexes_handle = self.call_indexes.len();
        self.call_indexes.add_func(func_idx.signature as usize);

        self.syscalls_imports
            .insert(invocable_syscall, (1, call_indexes_handle));
    }

    /// Returns the size of the memory in bytes that can be used to build precise syscall.
    fn memory_size_in_bytes(&self) -> u32 {
        let initial_mem_size: WasmPageCount = self
            .module
            .initial_mem_size()
            .expect("generator is instantiated with a mem import generation proof")
            .into();
        initial_mem_size.memory_size()
    }

    /// Reserves enough memory build precise syscall.
    fn reserve_memory(&self) -> i32 {
        self.memory_size_in_bytes()
            .saturating_sub(Self::PRECISE_SYS_CALL_MEMORY_SIZE) as i32
    }

    /// Generates a function which calls "properly" the `gr_reservation_send`.
    fn generate_send_from_reservation(
        &mut self,
        syscall: SysCallName,
    ) -> Result<(), PreciseSysCallError> {
        let [reserve_gas_idx, reservation_send_idx] =
            self.invocable_syscalls_indexes(InvocableSysCall::required_imports(syscall));

        // subtract to be sure we are in memory boundaries.
        let rid_pid_value_ptr = self.reserve_memory();
        let pid_value_ptr = rid_pid_value_ptr + mem::size_of::<Hash>() as i32;

        let func_instructions = Instructions::new(vec![
            // Amount of gas to reserve
            Instruction::GetLocal(4),
            // Duration of the reservation
            Instruction::GetLocal(5),
            // Pointer to the ErrorWithHash struct
            Instruction::GetLocal(6),
            Instruction::Call(reserve_gas_idx as u32),
            // Pointer to the ErrorWithHash struct
            Instruction::GetLocal(6),
            // Load ErrorWithHash.error
            Instruction::I32Load(2, 0),
            // Check if ErrorWithHash.error == 0
            Instruction::I32Eqz,
            // If ErrorWithHash.error == 0
            Instruction::If(BlockType::NoResult),
            // Copy the Hash struct (32 bytes) containing the reservation id.
            Instruction::I32Const(rid_pid_value_ptr),
            Instruction::GetLocal(6),
            Instruction::I64Load(3, 4),
            Instruction::I64Store(3, 0),
            Instruction::I32Const(rid_pid_value_ptr),
            Instruction::GetLocal(6),
            Instruction::I64Load(3, 12),
            Instruction::I64Store(3, 8),
            Instruction::I32Const(rid_pid_value_ptr),
            Instruction::GetLocal(6),
            Instruction::I64Load(3, 20),
            Instruction::I64Store(3, 16),
            Instruction::I32Const(rid_pid_value_ptr),
            Instruction::GetLocal(6),
            Instruction::I64Load(3, 28),
            Instruction::I64Store(3, 24),
            // Copy the HashWithValue struct (48 bytes) containing
            // the recipient and value after the obtained reservation ID
            Instruction::I32Const(pid_value_ptr),
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
            // Pointer to reservation ID, recipient ID and value
            Instruction::I32Const(rid_pid_value_ptr),
            // Pointer to payload
            Instruction::GetLocal(1),
            // Size of the payload
            Instruction::GetLocal(2),
            // Number of blocks to delay the sending for
            Instruction::GetLocal(3),
            // Pointer to the result of the reservation send
            Instruction::GetLocal(6),
            Instruction::Call(reservation_send_idx as u32),
            Instruction::End,
            Instruction::End,
        ]);

        self.generate_proper_syscall_invocation(syscall, func_instructions);

        Ok(())
    }

    /// Generates a function which calls "properly" the `gr_reservation_reply`.
    fn generate_reply_from_reservation(
        &mut self,
        syscall: SysCallName,
    ) -> Result<(), PreciseSysCallError> {
        let [reserve_gas_idx, reservation_reply_idx] =
            self.invocable_syscalls_indexes(InvocableSysCall::required_imports(syscall));

        // subtract to be sure we are in memory boundaries.
        let rid_value_ptr = self.reserve_memory();
        let value_ptr = rid_value_ptr + mem::size_of::<Hash>() as i32;

        let func_instructions = Instructions::new(vec![
            // Amount of gas to reserve
            Instruction::GetLocal(3),
            // Duration of the reservation
            Instruction::GetLocal(4),
            // Pointer to the ErrorWithHash struct
            Instruction::GetLocal(5),
            Instruction::Call(reserve_gas_idx as u32),
            // Pointer to the ErrorWithHash struct
            Instruction::GetLocal(5),
            // Load ErrorWithHash.error
            Instruction::I32Load(2, 0),
            // Check if ErrorWithHash.error == 0
            Instruction::I32Eqz,
            // If ErrorWithHash.error == 0
            Instruction::If(BlockType::NoResult),
            // Copy the Hash struct (32 bytes) containing the reservation id.
            Instruction::I32Const(rid_value_ptr),
            Instruction::GetLocal(5),
            Instruction::I64Load(3, 4),
            Instruction::I64Store(3, 0),
            Instruction::I32Const(rid_value_ptr),
            Instruction::GetLocal(5),
            Instruction::I64Load(3, 12),
            Instruction::I64Store(3, 8),
            Instruction::I32Const(rid_value_ptr),
            Instruction::GetLocal(5),
            Instruction::I64Load(3, 20),
            Instruction::I64Store(3, 16),
            Instruction::I32Const(rid_value_ptr),
            Instruction::GetLocal(5),
            Instruction::I64Load(3, 28),
            Instruction::I64Store(3, 24),
            // Copy the value (16 bytes).
            Instruction::I32Const(value_ptr),
            Instruction::GetLocal(0),
            Instruction::I64Load(3, 0),
            Instruction::I64Store(3, 0),
            Instruction::I32Const(value_ptr),
            Instruction::GetLocal(0),
            Instruction::I64Load(3, 8),
            Instruction::I64Store(3, 8),
            // Pointer to reservation ID and value
            Instruction::I32Const(rid_value_ptr),
            // Pointer to payload
            Instruction::GetLocal(1),
            // Size of the payload
            Instruction::GetLocal(2),
            // Pointer to the result of the reservation reply
            Instruction::GetLocal(5),
            Instruction::Call(reservation_reply_idx as u32),
            Instruction::End,
            Instruction::End,
        ]);

        self.generate_proper_syscall_invocation(syscall, func_instructions);

        Ok(())
    }

    /// Generates a function which calls "properly" the `gr_send_commit`.
    fn generate_send_commit(&mut self, syscall: SysCallName) -> Result<(), PreciseSysCallError> {
        let [send_init_idx, send_push_idx, send_commit_idx] =
            self.invocable_syscalls_indexes(InvocableSysCall::required_imports(syscall));

        // subtract to be sure we are in memory boundaries.
        let handle_ptr = self.reserve_memory();
        let pid_value_ptr = handle_ptr + mem::size_of::<Handle>() as i32;

        let mut elements = vec![
            // Pointer to the ErrorWithHandle struct
            Instruction::GetLocal(4),
            Instruction::Call(send_init_idx as u32),
            // Pointer to the ErrorWithHandle struct
            Instruction::GetLocal(4),
            // Load ErrorWithHandle.error
            Instruction::I32Load(2, 0),
            // Check if ErrorWithHandle.error == 0
            Instruction::I32Eqz,
            // If ErrorWithHandle.error == 0
            Instruction::If(BlockType::NoResult),
            // Copy the Handle
            Instruction::I32Const(handle_ptr),
            Instruction::GetLocal(4),
            Instruction::I32Load(2, 4),
            Instruction::I32Store(2, 0),
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
                Instruction::I32Load(2, 0),
                // Pointer to payload
                Instruction::GetLocal(1),
                // Size of the payload
                Instruction::GetLocal(2),
                // Pointer to the result of the send push
                Instruction::GetLocal(4),
                Instruction::Call(send_push_idx as u32),
            ]);
        }

        elements.extend_from_slice(&[
            // Copy the HashWithValue struct (48 bytes) containing the recipient and value
            // TODO: extract into another method
            Instruction::I32Const(pid_value_ptr),
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
            // Handle of message
            Instruction::I32Const(handle_ptr),
            Instruction::I32Load(2, 0),
            // Pointer to recipient ID and value
            Instruction::I32Const(pid_value_ptr),
            // Number of blocks to delay the sending for
            Instruction::GetLocal(3),
            // Pointer to the result of the send commit
            Instruction::GetLocal(4),
            Instruction::Call(send_commit_idx as u32),
            Instruction::End,
            Instruction::End,
        ]);

        let func_instructions = Instructions::new(elements);

        self.generate_proper_syscall_invocation(syscall, func_instructions);

        Ok(())
    }

    /// Generates a function which calls "properly" the `gr_send_commit_wgas`.
    fn generate_send_commit_with_gas(
        &mut self,
        syscall: SysCallName,
    ) -> Result<(), PreciseSysCallError> {
        let [size_idx, send_init_idx, send_push_input_idx, send_commit_wgas_idx] =
            self.invocable_syscalls_indexes(InvocableSysCall::required_imports(syscall));

        // subtract to be sure we are in memory boundaries.
        let handle_ptr = self.reserve_memory();
        let pid_value_ptr = handle_ptr + mem::size_of::<Handle>() as i32;
        let length_ptr = pid_value_ptr + mem::size_of::<Length>() as i32;

        let mut elements = vec![
            // Pointer to the ErrorWithHandle struct
            Instruction::GetLocal(3),
            Instruction::Call(send_init_idx as u32),
            // Pointer to the ErrorWithHandle struct
            Instruction::GetLocal(3),
            // Load ErrorWithHandle.error
            Instruction::I32Load(2, 0),
            // Check if ErrorWithHandle.error == 0
            Instruction::I32Eqz,
            // If ErrorWithHandle.error == 0
            Instruction::If(BlockType::NoResult),
            // Copy the Handle
            Instruction::I32Const(handle_ptr),
            Instruction::GetLocal(3),
            Instruction::I32Load(2, 4),
            Instruction::I32Store(2, 0),
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
                Instruction::I32Load(2, 0),
                // Offset of input
                Instruction::I32Const(0),
                // Length of input
                Instruction::I32Const(length_ptr),
                Instruction::I32Load(2, 0),
                // Pointer to the result of the send push input
                Instruction::GetLocal(3),
                Instruction::Call(send_push_input_idx as u32),
            ]);
        }

        elements.extend_from_slice(&[
            // Copy the HashWithValue struct (48 bytes) containing the recipient and value
            // TODO: extract into another method
            Instruction::I32Const(pid_value_ptr),
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
            // Handle of message
            Instruction::I32Const(handle_ptr),
            Instruction::I32Load(2, 0),
            // Pointer to recipient ID and value
            Instruction::I32Const(pid_value_ptr),
            // Gas limit for message
            Instruction::GetLocal(2),
            // Number of blocks to delay the sending for
            Instruction::GetLocal(1),
            // Pointer to the result of the send commit
            Instruction::GetLocal(3),
            Instruction::Call(send_commit_wgas_idx as u32),
            Instruction::End,
            Instruction::End,
        ]);

        let func_instructions = Instructions::new(elements);

        self.generate_proper_syscall_invocation(syscall, func_instructions);

        Ok(())
    }
}

/// Proof that there was an instance of syscalls imports generator and `SysCallsImportsGenerator::generate_syscalls_imports` was called.
pub struct SysCallsImportsGenerationProof(());

/// Disabled gear wasm syscalls generator.
///
/// Instance of this types signals that there was once active syscalls generator,
/// but it ended up it's work.
pub struct DisabledSysCallsImportsGenerator<'a, 'b> {
    pub(super) unstructured: &'b mut Unstructured<'a>,
    pub(super) call_indexes: CallIndexes,
    pub(super) module: WasmModule,
    pub(super) config: SysCallsConfig,
    pub(super) syscalls_imports: BTreeMap<InvocableSysCall, (u32, CallIndexesHandle)>,
}

impl<'a, 'b> From<DisabledSysCallsImportsGenerator<'a, 'b>> for ModuleWithCallIndexes {
    fn from(disabled_syscall_gen: DisabledSysCallsImportsGenerator<'a, 'b>) -> Self {
        ModuleWithCallIndexes {
            module: disabled_syscall_gen.module,
            call_indexes: disabled_syscall_gen.call_indexes,
        }
    }
}
