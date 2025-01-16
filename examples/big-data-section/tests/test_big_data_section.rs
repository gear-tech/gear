#[cfg(test)]
mod tests {
    use demo_big_data_section::data_access::DataAccess;
    use gtest::{Log, Program, System};

    const USER_ID: u64 = gtest::constants::DEFAULT_USER_ALICE;

    // Testing random access to data section
    #[test]
    fn test_big_data_section() -> Result<(), &'static str> {
        let sys = System::new();
        sys.init_logger();

        let prog = Program::from_file(
            &sys,
            "../../target/wasm32-unknown-unknown/debug/demo_big_data_section.opt.wasm",
        );
        sys.mint_to(gtest::constants::DEFAULT_USER_ALICE, 100000000000);

        // Skipping program initialization
        let _ = prog.send_bytes(USER_ID, b"");
        let _ = sys.run_next_block();

        let random_payload: Vec<Vec<u8>> = vec![
            vec![1, 110, 115, 44],
            vec![2, 244, 215, 99],
            vec![4, 139, 72, 39],
            vec![6, 20, 61, 59],
            vec![4, 50, 249, 180],
            vec![3, 195, 161, 132],
            vec![10, 84, 39, 226],
            vec![5, 125, 136, 188],
            vec![3, 56, 246, 19],
            vec![1, 49, 142, 82],
            vec![4, 242, 66, 82],
            vec![1, 110, 254, 189],
        ];

        for payload in random_payload {
            let expected_value = DataAccess::from_payload(&payload)?.constant();

            let message_id = prog.send_bytes(USER_ID, payload);
            let block_run_result = sys.run_next_block();

            let log = Log::builder()
                .source(prog.id())
                .dest(USER_ID)
                .payload_bytes(expected_value.to_be_bytes());

            assert!(block_run_result.succeed.contains(&message_id));
            assert!(block_run_result.contains(&log));
        }

        Ok(())
    }
}
