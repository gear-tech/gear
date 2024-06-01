use alloy::sol;

sol!(
    #[derive(Debug)]
    #[sol(rpc)]
    Program,
    "program_abi.json"
);
