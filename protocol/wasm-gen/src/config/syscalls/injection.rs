// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Entities describing possible injection types for each syscall.
//! These entities allows to configure which syscalls to insert into
//! code section of wasm module and which ones to simply import.
//!
//! Types here are used to create [`crate::SyscallsConfig`].

use crate::InvocableSyscall;

use arbitrary::Unstructured;
use gear_wasm_instrument::syscalls::{SyscallKind, SyscallName};
use indexmap::IndexSet;
use std::{collections::HashMap, ops::RangeInclusive};

/// This enum defines how the syscall should be injected into wasm module.
#[derive(Debug, Clone)]
pub enum SyscallInjectionType {
    /// Don't modify wasm module at all.
    None,
    /// Syscall import will be injected into import section of wasm module,
    /// but the `wasm-gen` generators will not call that syscall.
    ///
    /// It should be used in cases where you don't need to invoke an actual syscall.
    /// For example, `precision_gr_reservation_send` syscall uses `gr_reserve_gas` under
    /// the hood. In this case, `gr_reserve_gas` will be imported but will not be called.
    Import,
    /// Syscall import will be injected into import section of wasm module,
    /// and the `wasm-gen` generators can insert invoke instructions for that syscall.
    ///
    /// It wraps syscall amount range `RangeInclusive<u32>` - the range from which
    /// amount of the syscall invocations will be generated.
    ///
    /// Setting range to `(0..=n)`, where `n >= 0` can imitate `SyscallInjectionType::Import`,
    /// as in case if syscall amount range is zero, then syscall import will be injected, but
    /// no invocations will be generated, which is pretty similar to the other variant.
    Function(RangeInclusive<u32>),
}

/// Possible injection types for each syscall.
#[derive(Debug, Clone)]
pub struct SyscallsInjectionTypes {
    syscall_kind: SyscallKind,
    inner: HashMap<InvocableSyscall, SyscallInjectionType>,
    order: IndexSet<InvocableSyscall>,
}

impl SyscallsInjectionTypes {
    /// Instantiate a syscalls map, where each gear syscall is injected into wasm-module only once.
    pub fn all_once() -> Self {
        Self::all_once_for(SyscallKind::Vara)
    }

    /// Instantiate a syscalls map for the given runtime, where each available syscall is injected once.
    pub fn all_once_for(syscall_kind: SyscallKind) -> Self {
        Self::all_with_range_for(syscall_kind, 1..=1)
    }

    /// Instantiate a syscalls map, where no gear syscall is ever injected into wasm-module.
    pub fn all_never() -> Self {
        Self::all_never_for(SyscallKind::Vara)
    }

    /// Instantiate a syscalls map for the given runtime, where no syscall is injected.
    pub fn all_never_for(syscall_kind: SyscallKind) -> Self {
        Self::new_with_injection_type_for(syscall_kind, SyscallInjectionType::None)
    }

    /// Instantiate a syscalls map, where each gear syscall is injected into wasm-module with given range.
    pub fn all_with_range(range: RangeInclusive<u32>) -> Self {
        Self::all_with_range_for(SyscallKind::Vara, range)
    }

    /// Instantiate a syscalls map for the given runtime, where each available syscall is injected with given range.
    pub fn all_with_range_for(syscall_kind: SyscallKind, range: RangeInclusive<u32>) -> Self {
        Self::new_with_injection_type_for(syscall_kind, SyscallInjectionType::Function(range))
    }

    pub fn all_from_unstructured(unstructured: &mut Unstructured) -> Self {
        Self::all_from_unstructured_for(SyscallKind::Vara, unstructured)
    }

    pub fn all_from_unstructured_for(
        syscall_kind: SyscallKind,
        unstructured: &mut Unstructured,
    ) -> Self {
        let instrumentable_syscalls = SyscallName::instrumentable(syscall_kind).collect::<Vec<_>>();
        Self {
            syscall_kind,
            inner: SyscallName::instrumentable(syscall_kind)
                .map(|name| {
                    let range = unstructured.int_in_range(1..=3).unwrap()
                        ..=unstructured.int_in_range(3..=20).unwrap();
                    let injection_type = if instrumentable_syscalls.contains(&name) {
                        SyscallInjectionType::Function(range)
                    } else {
                        SyscallInjectionType::None
                    };
                    (InvocableSyscall::Loose(name), injection_type)
                })
                .chain(
                    SyscallName::instrumentable(syscall_kind)
                        .filter(|&name| InvocableSyscall::has_precise_variant(name))
                        .map(|name| {
                            let injection_type = if instrumentable_syscalls.contains(&name) {
                                SyscallInjectionType::Function(
                                    /*unstructured.int_in_range(1..=3).unwrap()
                                    ..=unstructured.int_in_range(3..=20).unwrap(),*/
                                    1..=3,
                                )
                            } else {
                                SyscallInjectionType::None
                            };
                            (InvocableSyscall::Precise(name), injection_type.clone())
                        }),
                )
                .collect(),
            order: IndexSet::new(),
        }
    }

    /// Instantiate a syscalls map with given injection type.
    fn new_with_injection_type_for(
        syscall_kind: SyscallKind,
        injection_type: SyscallInjectionType,
    ) -> Self {
        let instrumentable_syscalls = SyscallName::instrumentable(syscall_kind).collect::<Vec<_>>();
        Self {
            syscall_kind,
            inner: SyscallName::instrumentable(syscall_kind)
                .map(|name| {
                    (
                        InvocableSyscall::Loose(name),
                        if instrumentable_syscalls.contains(&name) {
                            injection_type.clone()
                        } else {
                            SyscallInjectionType::None
                        },
                    )
                })
                .chain(
                    SyscallName::instrumentable(syscall_kind).filter_map(|name| {
                        InvocableSyscall::has_precise_variant(name).then_some((
                            InvocableSyscall::Precise(name),
                            if instrumentable_syscalls.contains(&name) {
                                injection_type.clone()
                            } else {
                                SyscallInjectionType::None
                            },
                        ))
                    }),
                )
                .collect(),
            order: IndexSet::new(),
        }
    }

    /// Gets runtime syscall set.
    pub fn syscall_kind(&self) -> SyscallKind {
        self.syscall_kind
    }

    /// Gets insertion order of injection types.
    pub fn order(&self) -> Vec<InvocableSyscall> {
        self.order.iter().cloned().collect()
    }

    /// Gets injection type for given syscall.
    pub fn get(&self, name: InvocableSyscall) -> SyscallInjectionType {
        self.inner
            .get(&name)
            .cloned()
            .expect("instantiated with all syscalls set")
    }

    /// Sets possible amount range for the the syscall.
    ///
    /// Sets injection type for `name` syscall to `SyscallInjectionType::Function`.
    pub fn set(&mut self, name: InvocableSyscall, min: u32, max: u32) {
        if !self.is_allowed(name) {
            self.disable(name);
            return;
        }

        self.inner
            .insert(name, SyscallInjectionType::Function(min..=max));
        self.order.insert(name);

        if let InvocableSyscall::Precise(syscall) = name {
            let Some(required_imports) = InvocableSyscall::required_imports_for_syscall(syscall)
            else {
                return;
            };

            for syscall_import in required_imports
                .iter()
                .map(|&syscall| InvocableSyscall::Loose(syscall))
            {
                self.enable_syscall_import(syscall_import);
                self.order.insert(syscall_import);
            }
        }
    }

    /// Disables import and invocation generation for the given syscall.
    pub fn disable(&mut self, name: InvocableSyscall) {
        self.inner.insert(name, SyscallInjectionType::None);
        self.order.shift_remove(&name);
    }

    /// Imports the given syscall, if possible.
    pub(crate) fn enable_syscall_import(&mut self, name: InvocableSyscall) {
        if !self.is_allowed(name) {
            return;
        }

        if let Some(injection_type @ SyscallInjectionType::None) = self.inner.get_mut(&name) {
            *injection_type = SyscallInjectionType::Import;
        }
    }

    /// Same as [`SyscallsInjectionTypes::set`], but sets amount ranges for multiple syscalls.
    pub fn set_multiple(
        &mut self,
        syscalls_freqs: impl Iterator<Item = (InvocableSyscall, RangeInclusive<u32>)>,
    ) {
        for (name, range) in syscalls_freqs {
            let (min, max) = range.into_inner();
            self.set(name, min, max);
        }
    }

    fn is_allowed(&self, name: InvocableSyscall) -> bool {
        let syscall = match name {
            InvocableSyscall::Loose(syscall) | InvocableSyscall::Precise(syscall) => syscall,
        };

        SyscallName::instrumentable(self.syscall_kind)
            .any(|allowed_syscall| allowed_syscall == syscall)
    }
}

impl Default for SyscallsInjectionTypes {
    fn default() -> Self {
        Self::all_once()
    }
}
