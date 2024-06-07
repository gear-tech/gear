use alloy::sol;

sol!(
    #[derive(Debug)]
    #[sol(rpc)]
    AlloyRouter,
    "router_abi.json"
);

sol!(
    #[derive(Debug)]
    #[sol(rpc)]
    AlloyProgram,
    "program_abi.json"
);
