/// GWASM Libraries
///
/// - pub const LIBS: list of libraries.
/// - pub const LIBS_LEN: count of libraries.
mod libs {
    include!(concat!(env!("OUT_DIR"), "/libs.rs"));
}

/// Process wasm libraries to linker.
pub fn process<L, E>(mut link: L) -> Result<(), E>
where
    L: FnMut(String, Vec<u8>) -> Result<(), E>,
{
    libs::LIBS
        .iter()
        .map(|(lib, bin)| link(lib.to_string(), bin.to_vec()))
        .collect::<Result<_, _>>()
}
