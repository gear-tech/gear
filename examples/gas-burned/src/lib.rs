#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "std")]
mod code {
    include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}

#[cfg(feature = "std")]
pub use code::WASM_BINARY_OPT as WASM_BINARY;

#[cfg(not(feature = "std"))]
mod wasm;

#[cfg(test)]
mod tests {
    use gtest::{constants::DEFAULT_USER_ALICE, Program, System};

    #[test]
    fn gas_burned() {
        let system = System::new();
        system.init_logger();

        let from = DEFAULT_USER_ALICE;

        let program = Program::current(&system);
        let init_msg_id = program.send_bytes(from, "init");
        let res = system.run_next_block();
        let init_gas_burned = res
            .gas_burned
            .get(&init_msg_id)
            .copied()
            .expect("internal error: init message isn't sent");
        log::debug!("Init gas burned: {init_gas_burned}");
        assert!(init_gas_burned > 0);

        let handle_msg_id = program.send_bytes(from, "handle");
        let res = system.run_next_block();
        let handle_gas_burned = res
            .gas_burned
            .get(&handle_msg_id)
            .copied()
            .expect("internal error: init message isn't sent");
        log::debug!("Handle gas burned: {handle_gas_burned}");
        assert!(handle_gas_burned > init_gas_burned);
    }
}
