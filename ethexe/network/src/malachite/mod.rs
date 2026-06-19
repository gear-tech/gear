pub(crate) mod adapter;
pub(crate) mod behaviour;
pub(crate) mod state;

#[cfg(test)]
mod tests;

pub use adapter::MalachiteNetworkParts;

pub type AppNetworkMsg<Ctx> = malachitebft_app_channel::NetworkMsg<Ctx>;
pub type EngineNetworkRef<Ctx> = malachitebft_engine::network::NetworkRef<Ctx>;
pub type EngineNetworkMsg<Ctx> = malachitebft_engine::network::NetworkMsg<Ctx>;
