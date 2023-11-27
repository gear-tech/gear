//! mini-program for publishing packages to crates.io.

use anyhow::Result;
use crates_io_manager::Publisher;

fn main() -> Result<()> {
    Publisher::new()?.build()?.check()
}
