/*
 * This file is part of Gear.
 *
 * Copyright (C) 2022 Gear Technologies Inc.
 * SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program. If not, see <https://www.gnu.org/licenses/>.
 */

use crate::parity_wasm::elements::Serialize;
use pwasm_utils::parity_wasm::{self, elements::Module};
use std::path::PathBuf;

#[derive(Debug)]
pub enum Error {
    OptimizerFailed,
    SerializationFailed(parity_wasm::elements::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OptimizerFailed => write!(f, "Optimizer failed"),
            Self::SerializationFailed(e) => write!(f, "Serialization failed {}", e),
        }
    }
}

impl std::error::Error for Error {}

pub struct Optimizer {
    module: Module,
    file: PathBuf,
}

impl Optimizer {
    pub fn new(file: PathBuf) -> Result<Self, Error> {
        let module = parity_wasm::deserialize_file(&file).map_err(Error::SerializationFailed)?;
        Ok(Self { module, file })
    }

    pub fn insert_stack_and_export(&mut self) {
        let _ = gear_wasm_builder::insert_stack_end_export(&mut self.module)
            .map_err(|s| log::debug!("{}", s));
    }

    pub fn optimized_file_name(&self) -> PathBuf {
        self.file.with_extension("opt.wasm")
    }

    pub fn metadata_file_name(&self) -> PathBuf {
        self.file.with_extension("meta.wasm")
    }

    /// Calls chain optimizer
    pub fn optimize(&mut self) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        log::debug!("*** Processing chain optimization: {}", self.file.display());

        let mut binary_module = self.module.clone();
        let binary_file_name = self.optimized_file_name();

        pwasm_utils::optimize(
            &mut binary_module,
            vec!["handle", "handle_reply", "init", "__gear_stack_end"],
        )
        .map_err(|_| Error::OptimizerFailed)?;

        gear_wasm_builder::check_exports(&binary_module, &binary_file_name)?;

        let mut code = vec![];
        binary_module
            .clone()
            .serialize(&mut code)
            .map_err(Error::SerializationFailed)?;

        log::debug!("Optimized wasm: {}", binary_file_name.to_string_lossy());
        Ok(code)
    }

    /// Calls metadata optimizer
    pub fn metadata(&mut self) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        log::debug!(
            "*** Processing metadata optimization: {}",
            self.file.display()
        );

        let mut metadata_module = self.module.clone();
        let metadata_file_name = self.metadata_file_name();

        pwasm_utils::optimize(
            &mut metadata_module,
            vec![
                "meta_init_input",
                "meta_init_output",
                "meta_async_init_input",
                "meta_async_init_output",
                "meta_handle_input",
                "meta_handle_output",
                "meta_async_handle_input",
                "meta_async_handle_output",
                "meta_registry",
                "meta_title",
                "meta_state",
                "meta_state_input",
                "meta_state_output",
            ],
        )
        .map_err(|_| Error::OptimizerFailed)?;

        let mut code = vec![];
        metadata_module
            .serialize(&mut code)
            .map_err(Error::SerializationFailed)?;

        log::debug!("Metadata wasm: {}", metadata_file_name.to_string_lossy());
        Ok(code)
    }
}
