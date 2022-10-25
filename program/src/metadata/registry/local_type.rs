use crate::metadata::{
    registry::local_registry::LocalRegistry,
    result::{Error, Result},
};
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
                write!(fmt, "{}", format!("{primitive:?}").to_lowercase())
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
