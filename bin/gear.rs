//! gear command entry

fn main() {
    async_std::task::block_on(gear_program::Opt::run()).unwrap();
}
