use alloy::sol;

sol!(
    #[sol(rpc)]
    IProgram,
    "Program.json"
);

sol!(
    #[sol(rpc)]
    IRouter,
    "Router.json"
);

sol!(
    #[sol(rpc)]
    IWrappedVara,
    "WrappedVara.json"
);
