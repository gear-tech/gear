mod manager;
pub mod program;
pub mod system;

#[cfg(test)]
mod tests {
    #[derive(Debug)]
    struct MyContract;

    use crate::program::{Program, WasmProgram};
    use crate::system::System;

    impl WasmProgram for MyContract {
        fn init(&mut self, _: Vec<u8>) -> Result<Vec<u8>, &'static str> {
            Ok(Vec::new())
        }
        fn handle(&mut self, payload: Vec<u8>) -> Result<Vec<u8>, &'static str> {
            if payload == b"PING".to_vec() {
                return Ok(b"PONG".to_vec());
            }

            Ok(Vec::new())
        }
        fn handle_reply(&mut self, _: Vec<u8>) -> Result<Vec<u8>, &'static str> {
            Ok(Vec::new())
        }
    }

    #[test]
    fn name() {
        let sys = System::new();
        sys.init_logger();

        let ping_pong = Program::from_file(
            &sys,
            "../target/wasm32-unknown-unknown/release/demo_ping.wasm",
        );

        ping_pong.send_bytes("INIT");
        sys.assert_log_empty();

        ping_pong.send_bytes("PING");
        sys.assert_log_bytes(1, "PONG");

        ping_pong.send_bytes("NOT PING");
        sys.assert_log_empty();
    }
}
