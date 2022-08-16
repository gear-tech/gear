//! Logs from binaries
use std::{
    io::{BufReader, Lines, Result as IoResult},
    iter::FilterMap,
    process::ChildStderr,
};

pub type Logs = FilterMap<Lines<BufReader<ChildStderr>>, fn(IoResult<String>) -> Option<String>>;

pub mod gear_node {
    pub const IMPORTING_BLOCKS: &str = "Imported #1 ";
}

pub mod gear_program {
    pub const EX_UPLOAD_PROGRAM: &str = "Successfully submited call Gear::upload_program";
}
