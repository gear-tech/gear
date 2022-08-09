// Copyright (C) 2021-2022 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Metadata parser
#![allow(dead_code)]
#![allow(unused_imports)]

mod env;
mod executor;
mod ext;
mod registry;
mod result;
mod tests;

use self::{registry::LocalRegistry, result::Result};
pub use result::Error;
use scale_info::{form::PortableForm, PortableRegistry};
use std::fmt;

/// Data used for the wasm exectuon.
pub type StoreData = ext::Ext;

macro_rules! construct_metadata {
    ($($meta:ident),+) => {
        /// Gear program metadata
        ///
        /// See https://github.com/gear-tech/gear/blob/master/gstd/src/macros/metadata.rs.
        #[derive(Debug, Eq)]
        pub struct Metadata {
            $(
                pub $meta: Option<String>,
            )+
        }

        impl PartialEq for Metadata {
            fn eq(&self, other: &Self) -> bool {
                $(
                    if self.$meta != other.$meta && stringify!($meta) != "meta_registry"{
                        return false;
                    }
                )+

                true
            }
        }

        impl Metadata {
            /// Get metadata of "*meta.wasm"
            pub fn of(bin: &[u8]) -> Result<Self> {
                executor::execute(bin, |mut reader| -> Result<Self> {
                    let memory = reader.memory()?;

                    unsafe {
                        Ok(Self {
                            $(
                                $meta: reader.meta(&memory, stringify!($meta)).ok(),
                            )+
                        })
                    }
                })
            }

            fn format(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
                let registry = self.registry().map_err(|_|fmt::Error)?;
                let mut display = fmt.debug_struct("Metadata");

                $(
                    if let Some(type_name) = &self.$meta {
                        if let Ok(ty) = registry.derive_name(&type_name) {
                            display.field(stringify!($meta), &ty);
                        }
                    }
                )+

                display.finish()
            }
        }

        impl fmt::Display for Metadata {
            fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
                self.format(fmt)
            }
        }
    };
}

construct_metadata![
    meta_title,
    meta_init_input,
    meta_init_output,
    meta_async_init_input,
    meta_async_init_output,
    meta_handle_input,
    meta_handle_output,
    meta_async_handle_input,
    meta_async_handle_output,
    meta_state_input,
    meta_state_output,
    meta_registry
];

impl Metadata {
    /// Get type registry
    pub fn registry(&self) -> Result<PortableRegistry> {
        PortableRegistry::from_hex(self.meta_registry.as_ref().ok_or(Error::RegistryNotFound)?)
    }
}
