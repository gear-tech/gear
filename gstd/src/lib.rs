#![no_std]
#![cfg_attr(feature = "strict", deny(warnings))]

mod macros;
pub mod msg;
pub mod prelude;
pub mod structs;
pub mod sys;

pub use structs::*;
#[cfg(feature = "debug")]
pub use sys::ext;
