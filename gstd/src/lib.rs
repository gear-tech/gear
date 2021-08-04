#![no_std]
#![cfg_attr(feature = "strict", deny(warnings))]

mod macros;
pub mod msg;
pub mod prelude;

mod general;
pub use general::*;

mod utils;
#[cfg(feature = "debug")]
pub use utils::ext;
