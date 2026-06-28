use super::{CoreNetworkMsg, EngineNetworkMsg, EngineNetworkRef};
use async_trait::async_trait;
use ractor::{Actor, ActorProcessingErr, ActorRef};
use tokio::sync::mpsc;

pub type MalachiteNetworkParts = (EngineNetworkRef, mpsc::Sender<CoreNetworkMsg>);

pub(crate) struct Adapter {
    tx: mpsc::Sender<EngineNetworkMsg>,
}

#[async_trait]
impl Actor for Adapter {
    type Msg = EngineNetworkMsg;
    type State = ();
    type Arguments = ();

    async fn pre_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        _args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        Ok(())
    }

    async fn handle(
        &self,
        _myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        _state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        self.tx
            .send(message)
            .await
            .expect("channel must never be closed");
        Ok(())
    }
}

pub(crate) async fn spawn_adapter(
    tx: mpsc::Sender<EngineNetworkMsg>,
) -> anyhow::Result<MalachiteNetworkParts> {
    let (network_ref, _) = Actor::spawn(None, Adapter { tx }, ()).await?;

    let (tx_network, mut rx_network) = mpsc::channel::<CoreNetworkMsg>(128);
    tokio::spawn({
        let network_ref = network_ref.clone();
        async move {
            while let Some(message) = rx_network.recv().await {
                if let Err(error) = network_ref.cast(message.into()) {
                    log::error!("failed to send Malachite network message to adapter: {error}");
                    break;
                }
            }
        }
    });

    Ok((network_ref, tx_network))
}
