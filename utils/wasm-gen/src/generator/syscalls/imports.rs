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
        CallIndexes, FrozenGearWasmGenerator, GearEntryPointGenerationProof, GearWasmGenerator,
        MemoryImportGenerationProof, ModuleWithCallIndexes, CallIndexesHandle,
    },
    wasm::{PageCount as WasmPageCount, WasmModule},
    InvocableSysCall, SysCallsConfig,
};
use arbitrary::{Result, Unstructured};
use gear_wasm_instrument::{
    parity_wasm::{
        builder,
        elements::{BlockType, Instruction, Instructions},
    },
    syscalls::{ParamType, SysCallName, SysCallSignature},
};
use gsys::{ErrorWithHash, HashWithValue, Length};
use std::{collections::HashMap, mem};

/// Gear sys-calls imports generator.
pub struct SysCallsImportsGenerator<'a> {
    unstructured: &'a mut Unstructured<'a>,
    call_indexes: CallIndexes,
    module: WasmModule,
    config: SysCallsConfig,
    // This collcetion of sys-calls imports data has an `Option` as a key.
    // That's done intentionally and has the following semantics: `Some` is
    // used for sys-calls which are invoked with semi-randomly generated params
    // from params config. `None` is for all sys-calls which have
    sys_calls_imports: HashMap<InvocableSysCall, (u32, CallIndexesHandle)>,
}

/// Sys-calls imports generator instantiator.
///
/// Serves as a new type in order to create the generator from gear wasm generator and proofs.
pub struct SysCallsImportsGeneratorInstantiator<'a>(
    (
        GearWasmGenerator<'a>,
        MemoryImportGenerationProof,
        GearEntryPointGenerationProof,
    ),
);

impl<'a>
    From<(
        GearWasmGenerator<'a>,
        MemoryImportGenerationProof,
        GearEntryPointGenerationProof,
    )> for SysCallsImportsGeneratorInstantiator<'a>
{
    fn from(
        inner: (
            GearWasmGenerator<'a>,
            MemoryImportGenerationProof,
            GearEntryPointGenerationProof,
        ),
    ) -> Self {
        Self(inner)
    }
}

impl<'a> From<SysCallsImportsGeneratorInstantiator<'a>>
    for (SysCallsImportsGenerator<'a>, FrozenGearWasmGenerator<'a>)
{
    fn from(instantiator: SysCallsImportsGeneratorInstantiator<'a>) -> Self {
        let SysCallsImportsGeneratorInstantiator((
            generator,
            _mem_import_gen_proof,
            _gen_ep_gen_proof,
        )) = instantiator;
        let sys_call_gen = SysCallsImportsGenerator {
            unstructured: generator.unstructured,
            call_indexes: generator.call_indexes,
            module: generator.module,
            config: generator.config.sys_calls_config.clone(),
            sys_calls_imports: Default::default(),
        };
        let frozen = FrozenGearWasmGenerator {
            config: generator.config,
            call_indexes: None,
            unstructured: None,
        };

        (sys_call_gen, frozen)
    }
}

impl<'a> SysCallsImportsGenerator<'a> {
    /// Instantiate a new gear sys-calls imports generator.
    ///
    /// The generator instantiations requires having type-level proof that the wasm module has memory import in it.
    /// This proof could be gotten from memory generator.
    pub fn new(
        module_with_indexes: ModuleWithCallIndexes,
        config: SysCallsConfig,
        unstructured: &'a mut Unstructured<'a>,
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
            sys_calls_imports: Default::default(),
        }
    }

    /// Disable current generator.
    pub fn disable(self) -> DisabledSysCallsImportsGenerator<'a> {
        DisabledSysCallsImportsGenerator {
            unstructured: self.unstructured,
            call_indexes: self.call_indexes,
            module: self.module,
            config: self.config,
            sys_calls_imports: self.sys_calls_imports,
        }
    }

    /// Generates sys-calls imports and a function, that calls `gr_reservation_send` from config,
    /// used to instantiate the generator.
    ///
    /// Returns disabled sys-calls imports generator and a proof that imports from config were generated.
    pub fn generate(
        mut self,
    ) -> Result<(
        DisabledSysCallsImportsGenerator<'a>,
        SysCallsImportsGenerationProof,
    )> {
        let sys_calls_proof = self.generate_sys_calls_imports()?;
        self.generate_send_from_reservation();

        let disabled = DisabledSysCallsImportsGenerator {
            unstructured: self.unstructured,
            call_indexes: self.call_indexes,
            module: self.module,
            config: self.config,
            sys_calls_imports: self.sys_calls_imports,
        };

        Ok((disabled, sys_calls_proof))
    }

    /// Generates sys-calls imports from config, used to instantiate the generator.
    pub fn generate_sys_calls_imports(&mut self) -> Result<SysCallsImportsGenerationProof> {
        for sys_call in SysCallName::instrumentable() {
            let sys_call_generation_data = self.generate_sys_call_import(sys_call)?;
            if let Some(sys_call_generation_data) = sys_call_generation_data {
                self.sys_calls_imports
                    .insert(InvocableSysCall::Loose(sys_call), sys_call_generation_data);
            }
        }

        Ok(SysCallsImportsGenerationProof(()))
    }

    /// Generate import of the gear sys-call defined by `sys_call` param.
    ///
    /// Returns [`Option`] which wraps the tuple of amount of sys-call further injections
    /// and handle in the call indexes collection, if amount is not zero. Otherwise returns
    /// None.
    fn generate_sys_call_import(&mut self, sys_call: SysCallName) -> Result<Option<(u32, CallIndexesHandle)>> {
        let sys_call_amount_range = self.config.injection_amounts(sys_call);
        let sys_call_amount = self.unstructured.int_in_range(sys_call_amount_range)?;

        Ok((sys_call_amount != 0).then(|| {
            let call_indexes_handle = self.insert_sys_call_import(sys_call);
            (sys_call_amount, call_indexes_handle)
        }))
    }

    /// Inserts gear sys-call defined by the `sys_call` param.
    fn insert_sys_call_import(&mut self, sys_call: SysCallName) -> CallIndexesHandle {
        let sys_call_import_idx = self.module.count_import_funcs();

        // Insert sys-call import to the module
        self.module.with(|module| {
            let mut module_builder = builder::from_module(module);

            // Build signature applicable for the parity-wasm for the sys call
            let sys_call_signature = sys_call.signature().func_type();
            let signature_idx = module_builder.push_signature(
                builder::signature()
                    .with_params(sys_call_signature.params().iter().copied())
                    .with_results(sys_call_signature.results().iter().copied())
                    .build_sig(),
            );

            // Create import entry with the built signature.
            module_builder.push_import(
                builder::import()
                    .module("env")
                    .external()
                    .func(signature_idx)
                    .field(sys_call.to_str())
                    .build(),
            );

            (module_builder.build(), ())
        });

        let call_indexes_handle = self.call_indexes.len();
        self.call_indexes.add_import(sys_call_import_idx);

        call_indexes_handle
    }
}

impl<'a> SysCallsImportsGenerator<'a> {
    /// Generates a function which calls "properly" the `gr_reservation_send`.
    fn generate_send_from_reservation(&mut self) {
        let (reserve_gas_idx, reservation_send_idx) = {
            let maybe_reserve_gas = self
                .sys_calls_imports
                .get(&InvocableSysCall::Loose(SysCallName::ReserveGas))
                .map(|&(_, call_indexes_handle)| call_indexes_handle);
            let maybe_reservation_send = self
                .sys_calls_imports
                .get(&InvocableSysCall::Loose(SysCallName::ReservationSend))
                .map(|&(_, call_indexes_handle)| call_indexes_handle);

            match maybe_reserve_gas.zip(maybe_reservation_send) {
                Some(indexes) => indexes,
                None => return,
            }
        };

        let send_from_reservation_signature = SysCallSignature {
            params: vec![
                ParamType::Ptr(None),    // Address of recipient and value (HashWithValue struct)
                ParamType::Ptr(Some(2)), // Pointer to payload
                ParamType::Size,         // Size of the payload
                ParamType::Delay,        // Number of blocks to delay the sending for
                ParamType::Gas,          // Amount of gas to reserve
                ParamType::Duration,     // Duration of the reservation
            ],
            results: Default::default(),
        };

        let send_from_reservation_func_ty = send_from_reservation_signature.func_type();
        let func_signature = builder::signature()
            .with_params(send_from_reservation_func_ty.params().iter().copied())
            .with_results(send_from_reservation_func_ty.results().iter().copied())
            .build_sig();

        let memory_size_in_bytes = {
            let initial_mem_size: WasmPageCount = self
                .module
                .initial_mem_size()
                .expect("generator is instantiated with a mem import generation proof")
                .into();
            initial_mem_size.memory_size()
        };
        // subtract to be sure we are in memory boundaries.
        let reserve_gas_result_ptr = memory_size_in_bytes.saturating_sub(100) as i32;
        let rid_pid_value_ptr = reserve_gas_result_ptr + mem::size_of::<Length>() as i32;
        let pid_value_ptr = reserve_gas_result_ptr + mem::size_of::<ErrorWithHash>() as i32;
        let reservation_send_result_ptr = pid_value_ptr + mem::size_of::<HashWithValue>() as i32;

        let func_instructions = Instructions::new(vec![
            // Amount of gas to reserve
            Instruction::GetLocal(4),
            // Duration of the reservation
            Instruction::GetLocal(5),
            // Pointer to the LengthWithHash struct
            Instruction::I32Const(reserve_gas_result_ptr),
            Instruction::Call(reserve_gas_idx as u32),
            // Pointer to the LengthWithHash struct
            Instruction::I32Const(reserve_gas_result_ptr),
            // Load LengthWithHash.length
            Instruction::I32Load(2, 0),
            // Check if LengthWithHash.length == 0
            Instruction::I32Eqz,
            // If LengthWithHash.length == 0
            Instruction::If(BlockType::NoResult),
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
            Instruction::I32Const(reservation_send_result_ptr),
            Instruction::Call(reservation_send_idx as u32),
            Instruction::End,
            Instruction::End,
        ]);

        let send_from_reservation_func_idx = self.module.with(|module| {
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

        let call_indexes_handle = self.call_indexes.len();
        self.call_indexes
            .add_func(send_from_reservation_func_idx.signature as usize);

        self.sys_calls_imports.insert(
            InvocableSysCall::Precise(send_from_reservation_signature),
            (1, call_indexes_handle),
        );
    }
}

/// Proof that there was an instance of sys-calls imports generator and `SysCallsImportsGenerator::generate_sys_calls_imports` was called.
pub struct SysCallsImportsGenerationProof(());

/// Disabled gear wasm sys-calls generator.
///
/// Instance of this types signals that there was once active sys-calls generator,
/// but it ended up it's work.
pub struct DisabledSysCallsImportsGenerator<'a> {
    pub(super) unstructured: &'a mut Unstructured<'a>,
    pub(super) call_indexes: CallIndexes,
    pub(super) module: WasmModule,
    pub(super) config: SysCallsConfig,
    pub(super) sys_calls_imports: HashMap<InvocableSysCall, (u32, CallIndexesHandle)>,
}

impl<'a> From<DisabledSysCallsImportsGenerator<'a>> for ModuleWithCallIndexes {
    fn from(disabled_sys_call_gen: DisabledSysCallsImportsGenerator<'a>) -> Self {
        ModuleWithCallIndexes {
            module: disabled_sys_call_gen.module,
            call_indexes: disabled_sys_call_gen.call_indexes,
        }
    }
}
