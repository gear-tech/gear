//! Type registry

use crate::metadata::result::{Error, Result};
use parity_scale_codec::Decode;
use scale_info::{
    form::{Form, MetaForm, PortableForm},
    interner::UntrackedSymbol,
    PortableRegistry, Type, TypeDef,
};
use std::{any::TypeId, collections::HashMap, convert::TryFrom, fmt, ops::Deref};

/// Wrapper of `scale_info::Type` for rust formatting
pub struct LocalType<'t, T: Form> {
    pub ty: &'t Type<T>,
    pub registry: &'t PortableRegistry,
}

impl<'t, T: Form> LocalType<'t, T> {
    /// If this type is from the rust standard library.
    fn is_std(&self) -> bool {
        self.ty.path().namespace().len() == 1
    }

    /// Get the module of this type.
    fn module(&self) -> Option<&str> {
        self.ty.path().namespace().iter().next().map(|s| s.as_ref())
    }
}

impl<'t, T: Form<Type = UntrackedSymbol<TypeId>>> fmt::Debug for LocalType<'t, T> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self.ty.type_def() {
            TypeDef::Array(array) => fmt.write_str(&format!(
                "[{:?}; {}]",
                self.registry
                    .derive_id(array.type_param().id())
                    .map_err(|_| fmt::Error)?,
                array.len()
            )),
            TypeDef::BitSequence(bit_sequence) => {
                write!(
                    fmt,
                    "BitVec<{:?}, {:?}>",
                    self.registry
                        .derive_id(bit_sequence.bit_store_type().id())
                        .map_err(|_| fmt::Error)?,
                    self.registry
                        .derive_id(bit_sequence.bit_order_type().id())
                        .map_err(|_| fmt::Error)?,
                )
            }
            TypeDef::Compact(compact) => {
                write!(
                    fmt,
                    "{:?}",
                    self.registry
                        .derive_id(compact.type_param().id())
                        .map_err(|_| fmt::Error)?
                )
            }
            TypeDef::Composite(composite) => {
                let mut debug =
                    fmt.debug_struct(self.ty.path().ident().ok_or(fmt::Error)?.as_ref());
                for field in composite.fields() {
                    debug.field(
                        field.name().ok_or(fmt::Error)?.as_ref(),
                        &field.type_name().ok_or(fmt::Error)?.as_ref(),
                    );
                }

                debug.finish()
            }
            TypeDef::Primitive(primitive) => {
                write!(fmt, "{}", format!("{:?}", primitive).to_lowercase())
            }
            TypeDef::Sequence(sequence) => {
                write!(
                    fmt,
                    "[{:?}]",
                    self.registry
                        .derive_id(sequence.type_param().id())
                        .map_err(|_| fmt::Error)?,
                )
            }
            TypeDef::Tuple(tuple) => {
                let mut debug = fmt.debug_tuple(self.ty.path().ident().ok_or(fmt::Error)?.as_ref());
                for field in tuple.fields() {
                    debug.field(&format!(
                        "{:?}",
                        self.registry
                            .derive_id(field.id())
                            .map_err(|_| fmt::Error)?
                    ));
                }

                debug.finish()
            }
            TypeDef::Variant(_) => {
                write!(
                    fmt,
                    "{} ",
                    self.ty.path().ident().ok_or(fmt::Error)?.as_ref()
                )
            }
        }
    }
}

impl<'t, T: Form<Type = UntrackedSymbol<TypeId>>> fmt::Display for LocalType<'t, T> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt::Debug::fmt(&self, fmt)
    }
}

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
