// This file is part of Gear.
//
// Copyright (C) 2021-2023 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Program metadata parser
mod registry;
#[cfg(test)]
mod tests;

use crate::result::{Error, Result};
use core_processor::configs::BlockInfo;
use gear_core::code::{Code, CodeAndId, InstrumentedCode, InstrumentedCodeAndId};
use gmeta::{MetadataRepr, MetawasmData, TypesRepr};
use registry::LocalRegistry as _;
use scale_info::{scale::Decode, PortableRegistry};
use std::fmt;

struct Io<'d> {
    io: &'d TypesRepr,
    registry: &'d PortableRegistry,
}

impl<'d> Io<'d> {
    /// New instance of `Io` with given `io` and `registry`.
    pub fn new(io: &'d TypesRepr, registry: &'d PortableRegistry) -> Self {
        Self { io, registry }
    }
}

impl<'d> fmt::Debug for Io<'d> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        let mut display = fmt.debug_struct("");
        for (name, ty) in [("input", self.io.input), ("output", self.io.output)] {
            if let Some(id) = ty {
                display.field(name, &self.registry.derive_id(id).map_err(|_| fmt::Error)?);
            } else {
                display.field(name, &"()");
            }
        }

        display.finish()
    }
}

impl<'d> fmt::Display for Io<'d> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt::Debug::fmt(&self, fmt)
    }
}

/// Program metadata.
///
/// TODO: refactor this type with decoded registry.
/// doesn't necessary for now since everything in this crate
/// is just for a one-time call from the command line.
pub enum Meta {
    Data(MetadataRepr),
    Wasm(MetawasmData),
}

impl Meta {
    fn format_metadata(meta: &MetadataRepr, fmt: &mut fmt::Formatter) -> fmt::Result {
        let registry =
            PortableRegistry::decode(&mut meta.registry.as_ref()).map_err(|_| fmt::Error)?;
        let mut display = fmt.debug_struct("Metadata");
        display.field("init", &Io::new(&meta.init, &registry));
        display.field("handle", &Io::new(&meta.handle, &registry));
        display.field("others", &Io::new(&meta.others, &registry));
        let single_types = [
            ("reply", meta.reply),
            ("signal", meta.signal),
            ("state", meta.state),
        ];
        for (name, ty) in single_types {
            if let Some(id) = ty {
                display.field(name, &registry.derive_id(id).map_err(|_| fmt::Error)?);
            } else {
                display.field(name, &"()");
            }
        }

        display.finish()
    }

    fn format_metawasm(meta: &MetawasmData, fmt: &mut fmt::Formatter) -> fmt::Result {
        let registry =
            PortableRegistry::decode(&mut meta.registry.as_ref()).map_err(|_| fmt::Error)?;

        let mut display = fmt.debug_struct("Exports");
        for (name, io) in meta.funcs.iter() {
            display.field(name, &Io::new(io, &registry));
        }

        display.finish()
    }

    /// Execute meta method.
    fn execute(wasm: InstrumentedCode, method: &str) -> Result<Vec<u8>> {
        core_processor::informational::execute_for_reply::<
            gear_backend_wasmi::WasmiEnvironment<core_processor::Ext, String>,
            String,
        >(
            method.into(),
            wasm,
            None,
            None,
            None,
            Default::default(),
            u64::MAX,
            BlockInfo::default(),
        )
        .map_err(Error::WasmExecution)
    }

    /// Decode metawasm from wasm binary.
    pub fn decode_wasm(wasm: &[u8]) -> Result<Self> {
        let code = InstrumentedCodeAndId::from(CodeAndId::new(Code::new_raw(
            wasm.to_vec(),
            1,
            None,
            true,
            false,
        )?))
        .into_parts()
        .0;

        Ok(Self::Wasm(MetawasmData::decode(
            &mut Self::execute(code, "metadata")?.as_ref(),
        )?))
    }

    /// Decode metadata from hex bytes.
    pub fn decode_hex(hex: &[u8]) -> Result<Self> {
        let meta = MetadataRepr::from_hex(hex).map_err(Error::MetaParseError)?;
        Ok(Self::Data(meta))
    }

    /// Decode program meta.
    ///
    /// Either program metadata or state reading functions.
    pub fn decode(encoded: &[u8]) -> Result<Self> {
        MetadataRepr::from_bytes(encoded)
            .map(Meta::Data)
            .or_else(|_| -> Result<Meta> { Self::decode_wasm(encoded) })
            .map_err(Into::into)
    }

    /// Derive type by name.
    pub fn derive(&self, name: &str) -> Result<String> {
        let mut encoded_registry = match self {
            Meta::Data(meta) => meta.registry.as_ref(),
            Meta::Wasm(meta) => meta.registry.as_ref(),
        };
        let registry = PortableRegistry::decode(&mut encoded_registry)?;

        Ok(format!("{}", registry.derive_name(name)?))
    }
}

impl fmt::Debug for Meta {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Meta::Data(meta) => Self::format_metadata(meta, fmt),
            Meta::Wasm(meta) => Self::format_metawasm(meta, fmt),
        }
    }
}

impl fmt::Display for Meta {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt::Debug::fmt(&self, fmt)
    }
}
