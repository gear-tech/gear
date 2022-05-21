mod cmd;
mod registry;
mod result;

pub use self::{
    cmd::Opt,
    registry::Registry,
    result::{Error, Result},
};
