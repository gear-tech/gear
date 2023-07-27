/// GWASM Libraries
///
/// - pub const LIBS: list of libraries.
/// - pub const LIBS_LEN: count of libraries.
mod libs {
    include!(concat!(env!("OUT_DIR"), "/libs.rs"));
}

/// Process wasm libraries to linker.
pub fn process<L, E>(link: L) -> Result<(), E>
where
    L: Fn(&str, &[u8]) -> Result<(), E>,
{
    libs::LIBS
        .iter()
        .map(|(lib, bin)| link(lib, bin))
        .collect::<Result<_, _>>()
}
