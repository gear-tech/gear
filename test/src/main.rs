mod runner;
mod check;

use gear_core::storage;

pub fn main() -> anyhow::Result<()> {
    check::check_main(|| storage::new_in_memory_empty())
}
