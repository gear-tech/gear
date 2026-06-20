pub(crate) mod adapter;
pub(crate) mod behaviour;
pub(crate) mod state;

#[cfg(test)]
mod tests;

pub use adapter::MalachiteNetworkParts;

pub type AppNetworkMsg = ethexe_malachite_core::NetworkMsg<ethexe_malachite_core::MalachiteCtx>;
pub type EngineNetworkRef = ethexe_malachite_core::NetworkRef<ethexe_malachite_core::MalachiteCtx>;
pub type EngineNetworkMsg =
    ethexe_malachite_core::EngineNetworkMsg<ethexe_malachite_core::MalachiteCtx>;
