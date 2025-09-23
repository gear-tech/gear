#[derive(Debug, Clone)]
pub enum OperatorVaultType {
    Shared,
    Personal,
}

pub struct SymbioticEnv {}

pub struct RpcService<P: Provider + Clone> {
    rpc: String,
    inner: P,
}

