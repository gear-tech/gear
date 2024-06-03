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
    use gtest::{Gas, Program, System};

    #[test]
    fn gas_burned() {
        let system = System::new();
        system.init_logger();

        let from = 42;

        let program = Program::current(&system);
        let res = program.send_bytes(from, "init");
        let init_gas_burned = res.main_gas_burned();
        log::debug!("Init gas burned: {init_gas_burned}");
        assert!(init_gas_burned > Gas::zero());

        let res = program.send_bytes(from, "handle");
        let handle_gas_burned = res.main_gas_burned();
        log::debug!("Handle gas burned: {handle_gas_burned}");
        assert!(handle_gas_burned > init_gas_burned);
    }
}
