mod cmd;
mod registry;
mod result;
mod template;

pub use self::{
    cmd::Opt,
    registry::Registry,
    result::{Error, Result},
};
