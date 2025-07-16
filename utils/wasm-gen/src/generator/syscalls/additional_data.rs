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

//! Additional data injector module.
//!
//! Currently it only injects data to be logged in some available
//! gear entry point (either `init`, `handle` or `handle_reply`).
//! The data is set in the data section, offset for this data is
//! chosen to be after stack end offset. Storing data after stack
//! end is a right thing, as stack end is kind of virtual stack,
//! data of which is stored on wasm pages, but not saved after
//! program execution. However, this data can be re-written by
//! other instructions of the wasm module, so one should not
//! rely on consistency of data from data section for all the wasm
//! executions.

use crate::{
    EntryPointName, InvocableSyscall, SyscallsConfig, WasmModule,
    generator::{
        CallIndexes, CallIndexesHandle, DisabledSyscallsImportsGenerator, ModuleWithCallIndexes,
        SyscallsImportsGenerationProof,
    },
};
use arbitrary::Unstructured;
use gear_wasm_instrument::{Data, Instruction, ModuleBuilder, syscalls::SyscallName};
use std::{collections::BTreeMap, num::NonZero};

/// Additional data injector.
///
/// Injects some additional data from provided config to wasm module data section.
/// The config, which contains additional data types and values is received from [`DisabledSyscallsImportsGenerator`].
///
/// The generator is instantiated only with having [`SyscallsImportsGenerationProof`], which gives a guarantee. that
/// if log info should be injected, than `gr_debug` syscall import is generated.
pub struct AdditionalDataInjector<'a, 'b> {
    unstructured: &'b mut Unstructured<'a>,
    call_indexes: CallIndexes,
    config: SyscallsConfig,
    last_offset: u32,
    module: WasmModule,
    syscalls_imports: BTreeMap<InvocableSyscall, (Option<NonZero<u32>>, CallIndexesHandle)>,
}

impl<'a, 'b>
    From<(
        DisabledSyscallsImportsGenerator<'a, 'b>,
        SyscallsImportsGenerationProof,
    )> for AdditionalDataInjector<'a, 'b>
{
    fn from(
        (disabled_gen, _syscalls_gen_proof): (
            DisabledSyscallsImportsGenerator<'a, 'b>,
            SyscallsImportsGenerationProof,
        ),
    ) -> Self {
        let data_offset = disabled_gen
            .module
            .get_stack_end_offset()
            .unwrap_or_default();
        Self {
            unstructured: disabled_gen.unstructured,
            config: disabled_gen.config,
            last_offset: data_offset as u32,
            module: disabled_gen.module,
            syscalls_imports: disabled_gen.syscalls_imports,
            call_indexes: disabled_gen.call_indexes,
        }
    }
}

impl<'a, 'b> AdditionalDataInjector<'a, 'b> {
    /// Injects additional data from config to the wasm module.
    ///
    /// Returns disabled additional data injector and injection outcome.
    pub fn inject(mut self) -> DisabledAdditionalDataInjector<'a, 'b> {
        log::trace!("Injecting additional data");

        self.inject_log_info_printing();

        self.disable()
    }

    /// Disable current generator.
    pub fn disable(self) -> DisabledAdditionalDataInjector<'a, 'b> {
        DisabledAdditionalDataInjector {
            module: self.module,
            call_indexes: self.call_indexes,
            syscalls_imports: self.syscalls_imports,
            config: self.config,
            unstructured: self.unstructured,
        }
    }

    /// Injects logging calls for the log info defined by the config.
    ///
    /// Basically, it inserts into any existing gear entry point `gr_debug` call with
    /// a pointer to data section entry, which stores the log info.
    ///
    /// If no log info is defined in the config, then just returns without mutating wasm.
    pub fn inject_log_info_printing(&mut self) {
        let Some(log_info) = self.config.log_info() else {
            return;
        };
        log::trace!("Inserting next logging info - {log_info}");

        let log_bytes = log_info.as_bytes().to_vec();

        let export_idx = self
            .module
            .gear_entry_point(EntryPointName::Init)
            .inspect(|_| {
                log::trace!("Info will be logged in init");
            })
            .or_else(|| {
                log::trace!("Info will be logged in handle");
                self.module.gear_entry_point(EntryPointName::Handle)
            })
            .or_else(|| {
                log::trace!("Info will be logged in handle_reply");
                self.module.gear_entry_point(EntryPointName::HandleReply)
            })
            // This generator is instantiated from SyscallsImportsGenerator, which can only be
            // generated if entry points and memory import were generated.
            .expect("impossible to have no gear export");

        let debug_call_indexes_handle = self
            .syscalls_imports
            .get(&InvocableSyscall::Loose(SyscallName::Debug))
            .map(|&(_, handle)| handle as u32)
            .expect("impossible by configs generation to have log info printing without debug syscall generated");

        self.module.with(|module| {
            let log_bytes_len = log_bytes.len() as u32;
            let log_info_offset = self.last_offset;

            self.last_offset = log_info_offset + log_bytes_len;

            let mut builder = ModuleBuilder::from_module(module);
            builder.push_data(Data::with_offset(log_bytes, log_info_offset));

            let mut module = builder.build();
            module
                .code_section
                .as_mut()
                .expect("has at least one export")
                .get_mut(export_idx as usize)
                .expect("index of existing export")
                .instructions
                .splice(
                    0..0,
                    [
                        Instruction::I32Const(log_info_offset as i32),
                        Instruction::I32Const(log_bytes_len as i32),
                        Instruction::Call(debug_call_indexes_handle),
                    ],
                );

            (module, ())
        });
    }
}

/// Disabled additional data injector.
///
/// Instance of this type signals that there was once active additional data injector,
/// but it ended up it's work.
pub struct DisabledAdditionalDataInjector<'a, 'b> {
    pub(super) unstructured: &'b mut Unstructured<'a>,
    pub(super) module: WasmModule,
    pub(super) call_indexes: CallIndexes,
    pub(super) syscalls_imports:
        BTreeMap<InvocableSyscall, (Option<NonZero<u32>>, CallIndexesHandle)>,
    pub(super) config: SyscallsConfig,
}

impl<'a, 'b> From<DisabledAdditionalDataInjector<'a, 'b>> for ModuleWithCallIndexes {
    fn from(additional_data_inj: DisabledAdditionalDataInjector<'a, 'b>) -> Self {
        ModuleWithCallIndexes {
            module: additional_data_inj.module,
            call_indexes: additional_data_inj.call_indexes,
        }
    }
}
