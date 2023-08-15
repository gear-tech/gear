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

//! Additional data injector module.

use crate::{
    generator::{
        CallIndexes, CallIndexesHandle, DisabledSysCallsImportsGenerator, ModuleWithCallIndexes,
        SysCallsImportsGenerationProof,
    },
    utils, EntryPointName, InvocableSysCall, MessageDestination, SysCallsConfig, WasmModule,
};
use arbitrary::Unstructured;
use gear_wasm_instrument::{
    parity_wasm::{builder, elements::Instruction},
    syscalls::SysCallName,
};
use std::{collections::BTreeMap, iter::Cycle, vec::IntoIter};

/// Cycled iterator over wasm module data offsets.
///
/// By data offsets we mean pointers to the beginning of
/// each data entry in wasm module's data section.
///
/// By implementation this type is not instantiated, when no
/// data is set to the wasm module. More precisely, if no
/// additional data was set to [`SysCallsConfig`].
pub struct AddressesOffsets(Cycle<IntoIter<u32>>);

impl AddressesOffsets {
    /// Get the next offset.
    pub fn next_offset(&mut self) -> u32 {
        self.0
            .next()
            .expect("offsets is created only from non empty vec")
    }
}

/// Additional data injector.
///
/// Injects some additional data from provided config to wasm module data section.
/// The config, which contains additional data types and values is received from [`DisabledSysCallsImportsGenerator`].
///
/// The generator is instantiated only with having [`SysCallsImportsGenerationProof`], which gives a guarantee. that
/// if log info should be injected, than `gr_debug` sys-call import is generated.
pub struct AdditionalDataInjector<'a, 'b> {
    unstructured: &'b mut Unstructured<'a>,
    call_indexes: CallIndexes,
    config: SysCallsConfig,
    last_offset: u32,
    module: WasmModule,
    addresses_offsets: Vec<u32>,
    sys_calls_imports: BTreeMap<InvocableSysCall, (u32, CallIndexesHandle)>,
}

impl<'a, 'b>
    From<(
        DisabledSysCallsImportsGenerator<'a, 'b>,
        SysCallsImportsGenerationProof,
    )> for AdditionalDataInjector<'a, 'b>
{
    fn from(
        (disabled_gen, _sys_calls_gen_proof): (
            DisabledSysCallsImportsGenerator<'a, 'b>,
            SysCallsImportsGenerationProof,
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
            addresses_offsets: Vec::new(),
            sys_calls_imports: disabled_gen.sys_calls_imports,
            call_indexes: disabled_gen.call_indexes,
        }
    }
}

impl<'a, 'b> AdditionalDataInjector<'a, 'b> {
    /// Injects additional data from config to the wasm module.
    ///
    /// Returns disabled additional data injector and injection outcome.
    pub fn inject(
        mut self,
    ) -> (
        DisabledAdditionalDataInjector<'a, 'b>,
        AddressesInjectionOutcome,
    ) {
        let offsets = self.inject_addresses();
        self.inject_log_info_printing();

        let disabled = DisabledAdditionalDataInjector {
            module: self.module,
            call_indexes: self.call_indexes,
            sys_calls_imports: self.sys_calls_imports,
            config: self.config,
            unstructured: self.unstructured,
        };
        let outcome = AddressesInjectionOutcome { offsets };

        (disabled, outcome)
    }

    /// Disable current generator.
    pub fn disable(self) -> DisabledAdditionalDataInjector<'a, 'b> {
        DisabledAdditionalDataInjector {
            module: self.module,
            call_indexes: self.call_indexes,
            sys_calls_imports: self.sys_calls_imports,
            config: self.config,
            unstructured: self.unstructured,
        }
    }

    /// Injects addresses from config, if they were defined, into the data section.
    ///
    /// Returns `Some` with pointers to each address entry in the data section.
    /// If no addresses were defined in the config, then returns `None`.
    pub fn inject_addresses(&mut self) -> Option<AddressesOffsets> {
        if !self.addresses_offsets.is_empty() {
            return Some(AddressesOffsets(
                self.addresses_offsets.clone().into_iter().cycle(),
            ));
        }

        let MessageDestination::ExistingAddresses(existing_addresses) = self.config.sending_message_destination() else {
            return None;
        };

        for address in existing_addresses {
            self.addresses_offsets.push(self.last_offset);

            let address_data_bytes = utils::hash_with_value_to_vec(address);
            let data_len = address_data_bytes.len();
            self.module.with(|module| {
                let module = builder::from_module(module)
                    .data()
                    .offset(Instruction::I32Const(self.last_offset as i32))
                    .value(address_data_bytes)
                    .build()
                    .build();

                (module, ())
            });

            self.last_offset += data_len as u32;
        }

        Some(AddressesOffsets(
            self.addresses_offsets.clone().into_iter().cycle(),
        ))
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
        let log_bytes = log_info.as_bytes().to_vec();

        let export_idx = self
            .module
            .gear_entry_point(EntryPointName::Init)
            .or_else(|| self.module.gear_entry_point(EntryPointName::Handle))
            .or_else(|| self.module.gear_entry_point(EntryPointName::HandleReply))
            // This generator is instantiated from SysCallsImportsGenerator, which can only be
            // generated if entry points and memory import were generated.
            .expect("impossible to have no gear export");

        let debug_call_indexes_handle = self
            .sys_calls_imports
            .get(&InvocableSysCall::Loose(SysCallName::Debug))
            .map(|&(_, handle)| handle as u32)
            .expect("impossible by configs generation to have log info printing without debug sys-call generated");

        self.module.with(|module| {
            let log_bytes_len = log_bytes.len() as u32;
            let log_info_offset = self.last_offset;

            self.last_offset = log_info_offset + log_bytes_len;

            let mut module = builder::from_module(module)
                .data()
                .offset(Instruction::I32Const(log_info_offset as i32))
                .value(log_bytes)
                .build()
                .build();

            module
                .code_section_mut()
                .expect("has at least one export")
                .bodies_mut()
                .get_mut(export_idx as usize)
                .expect("index of existing export")
                .code_mut()
                .elements_mut()
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

/// Data injection outcome.
///
/// Basically this type just carries inserted into data section
/// addresses offsets.
///
/// There's design point of having this type, which is described in [`super::SysCallsInvocator`] docs.
pub struct AddressesInjectionOutcome {
    pub(super) offsets: Option<AddressesOffsets>,
}

/// Disabled additional data injector.
///
/// Instance of this type signals that there was once active additional data injector,
/// but it ended up it's work.
pub struct DisabledAdditionalDataInjector<'a, 'b> {
    pub(super) unstructured: &'b mut Unstructured<'a>,
    pub(super) module: WasmModule,
    pub(super) call_indexes: CallIndexes,
    pub(super) sys_calls_imports: BTreeMap<InvocableSysCall, (u32, CallIndexesHandle)>,
    pub(super) config: SysCallsConfig,
}

impl<'a, 'b> From<DisabledAdditionalDataInjector<'a, 'b>> for ModuleWithCallIndexes {
    fn from(additional_data_inj: DisabledAdditionalDataInjector<'a, 'b>) -> Self {
        ModuleWithCallIndexes {
            module: additional_data_inj.module,
            call_indexes: additional_data_inj.call_indexes,
        }
    }
}
