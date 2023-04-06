//! Program metadata parser
mod registry;
#[cfg(test)]
mod tests;

use crate::result::Result;
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

/// Program metadata
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
        display.field("reply", &Io::new(&meta.reply, &registry));
        display.field("others", &Io::new(&meta.others, &registry));
        for (name, ty) in [("signal", meta.signal), ("state", meta.state)].into_iter() {
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

        let mut display = fmt.debug_struct("");
        for (name, io) in meta.funcs.iter() {
            display.field(&name, &Io::new(io, &registry));
        }

        display.finish()
    }

    /// Decode program meta.
    ///
    /// Either program metadata or state reading functions.
    pub fn decode(encoded: &[u8]) -> Result<Self> {
        MetadataRepr::decode(&mut encoded.as_ref())
            .map(Meta::Data)
            .or_else(|_| MetawasmData::decode(&mut encoded.as_ref()).map(Meta::Wasm))
            .map_err(Into::into)
    }
}

impl fmt::Debug for Meta {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Meta::Data(meta) => Self::format_metadata(&meta, fmt),
            Meta::Wasm(meta) => Self::format_metawasm(&meta, fmt),
        }
    }
}
impl fmt::Display for Meta {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt::Debug::fmt(&self, fmt)
    }
}
