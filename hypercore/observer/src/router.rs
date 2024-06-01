use alloy::sol;

sol!(
    #[derive(Debug)]
    #[sol(rpc)]
    Router,
    "router_abi.json"
);
