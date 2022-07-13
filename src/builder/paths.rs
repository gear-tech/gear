//! Constant paths

/// Path of api module.
pub const API_MODULE: &str = "src/api";

/// Path of custom `Cargo.toml`
pub const CARGO_TOML: &str = "Cargo.toml";

/// Path of the gear binary.
#[cfg(test)]
pub const GEAR_BIN: &str = "target/release/gear";

/// Path of the gear submodule.
pub const GEAR_SUBMODULE: &str = "res/gear";

/// Path of generated gear api.
pub const GENERATED_RS: &str = "generated.rs";

/// Path of runtime library.
pub const RUNTIME_LIB_PATH: &str = "runtime/src/lib.rs";
