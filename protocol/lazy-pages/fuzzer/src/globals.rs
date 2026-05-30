// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use std::collections::BTreeMap;

use crate::generate::GLOBAL_NAME_PREFIX;
use anyhow::Result;
use gear_wasm_instrument::Module;

pub trait InstanceAccessGlobal {
    fn set_global(&self, name: &str, value: i64) -> Result<()>;
    fn get_global(&self, name: &str) -> Result<i64>;

    fn increment_global(&self, name: &str, value: i64) -> Result<()> {
        let current_value = self.get_global(name)?;
        self.set_global(name, current_value.saturating_add(value))
    }
}

/// List of generated globals
pub fn globals_list(module: &Module) -> Vec<String> {
    module
        .export_section
        .as_ref()
        .map(|section| {
            section
                .iter()
                .filter_map(|entry| {
                    let export_name = &entry.name;
                    if export_name.starts_with(GLOBAL_NAME_PREFIX) {
                        Some(export_name.to_string())
                    } else {
                        None
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Get globals values from instance
pub fn get_globals(
    instance: &impl InstanceAccessGlobal,
    module: &Module,
) -> Result<BTreeMap<String, i64>> {
    let mut globals = BTreeMap::new();
    for global_name in globals_list(module) {
        let value = instance.get_global(&global_name)?;
        globals.insert(global_name, value);
    }
    Ok(globals)
}
