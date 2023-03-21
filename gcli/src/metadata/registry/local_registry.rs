// This file is part of Gear.
//
// Copyright (C) 2021-2022 Gear Technologies Inc.
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

use crate::metadata::{
    registry::local_type::LocalType,
    result::{Error, Result},
};
use parity_scale_codec::Decode;
use scale_info::{
    form::{Form, MetaForm, PortableForm},
    interner::UntrackedSymbol,
    PortableRegistry, Type, TypeDef,
};
use std::{any::TypeId, collections::HashMap, convert::TryFrom, fmt, ops::Deref};

/// Local type registry
pub trait LocalRegistry: Sized + Clone {
    fn from_hex(hex: &str) -> Result<Self>;

    /// Get type from identity
    fn derive_id(&self, id: u32) -> Result<LocalType<'_, PortableForm>>;

    /// Get type from identity name
    ///
    /// # TODO
    ///
    /// Adding a indexer to register types for re-using,
    /// currently we don't have this requirements
    fn derive_name(&self, ident: &str) -> Result<LocalType<'_, PortableForm>>;
}

impl LocalRegistry for PortableRegistry {
    fn from_hex(encoded: &str) -> Result<Self> {
        Ok(PortableRegistry::decode(
            &mut hex::decode(encoded)?.as_ref(),
        )?)
    }

    fn derive_id(&self, id: u32) -> Result<LocalType<'_, PortableForm>> {
        Ok(LocalType {
            ty: self
                .resolve(id)
                .ok_or_else(|| Error::TypeNotFound(format!("{id:?}")))?,
            registry: self,
        })
    }

    fn derive_name(&self, ident: &str) -> Result<LocalType<'_, PortableForm>> {
        for ty in self.types() {
            let ty = ty.ty();
            if ty.path().ident() == Some(ident.into()) {
                return Ok(LocalType { ty, registry: self });
            }
        }

        Err(Error::TypeNotFound(ident.into()))
    }
}
