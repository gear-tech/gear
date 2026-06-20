use super::*;
use crate::{NetworkEvent, tests::new_service, utils::tests::init_logger};
use futures::{Stream, StreamExt, future::poll_fn};
use libp2p::multiaddr::Protocol;
use libp2p_swarm_test::SwarmExt;
use malachitebft_engine::network::NetworkMsg;
use malachitebft_network::PersistentPeersOp;
use ractor::call_t;
use std::{task::Poll, time::Duration};

async fn poll_service_once(service: &mut crate::NetworkService) {
    poll_fn(|cx| match std::pin::Pin::new(&mut *service).poll_next(cx) {
        Poll::Ready(event) => panic!("unexpected network event: {event:?}"),
        Poll::Pending => Poll::Ready(()),
    })
    .await;
}

#[test]
fn adapter_parts_are_upstream_malachite_types() {
    #[allow(dead_code)]
    fn assert_parts(parts: MalachiteNetworkParts) {
        let (_network_ref, _tx_network) = parts.into_engine_parts();
    }
}

#[tokio::test]
async fn network_service_starts_with_malachite_lane() {
    let mut service = new_service().await;

    assert!(service.malachite_persistent_peers().is_empty());
    assert!(service.swarm.behaviour().malachite.as_ref().is_some());
    assert!(service.take_malachite_network_parts().is_some());
    assert!(service.take_malachite_network_parts().is_none());
}

#[tokio::test]
async fn take_malachite_network_parts_returns_engine_parts() {
    init_logger();

    let mut service = new_service().await;
    let parts = service
        .take_malachite_network_parts()
        .expect("malachite network parts are available");

    let (_network_ref, _tx_network) = parts.into_engine_parts();
}

#[tokio::test]
async fn persistent_peer_updates_reach_lane_state() {
    let mut service = new_service().await;
    let parts = service
        .take_malachite_network_parts()
        .expect("malachite network parts are available");
    let (network_ref, _tx_network) = parts.into_engine_parts();

    let peer: libp2p::Multiaddr =
        "/memory/7/p2p/12D3KooWKiS7cyTXeAaNR3q1i5DqsnVRKN34vTXBJG5VewmwoaV3"
            .parse()
            .expect("valid multiaddr");

    let command_peer = peer.clone();
    let update = tokio::spawn(async move {
        call_t!(
            network_ref,
            |reply| NetworkMsg::UpdatePersistentPeers(
                PersistentPeersOp::Add(command_peer.clone()),
                reply
            ),
            1000
        )
        .expect("actor replied")
    });

    tokio::time::timeout(Duration::from_secs(1), async {
        while !service.malachite_persistent_peers().contains(&peer) {
            poll_service_once(&mut service).await;
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("service polls lane command");

    let result = update.await.expect("command task completes");

    assert_eq!(result, Ok(()));
    assert!(service.malachite_persistent_peers().contains(&peer));
}

#[tokio::test]
async fn persistent_peer_updates_connect_shared_swarm() {
    init_logger();

    let mut service = new_service().await;
    let parts = service
        .take_malachite_network_parts()
        .expect("malachite network parts are available");
    let (network_ref, _tx_network) = parts.into_engine_parts();

    let mut peer_service = new_service().await;
    peer_service
        .swarm
        .listen()
        .with_memory_addr_external()
        .await;

    let peer_id = peer_service.local_peer_id();
    let mut peer_addr = peer_service
        .swarm
        .external_addresses()
        .next()
        .expect("peer service has an external memory address")
        .clone();
    peer_addr.push(Protocol::P2p(peer_id));

    let command_peer = peer_addr.clone();
    let update = tokio::spawn(async move {
        call_t!(
            network_ref,
            |reply| NetworkMsg::UpdatePersistentPeers(
                PersistentPeersOp::Add(command_peer.clone()),
                reply
            ),
            1000
        )
        .expect("actor replied")
    });

    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            tokio::select! {
                event = service.next() => {
                    if matches!(event, Some(NetworkEvent::PeerConnected(connected)) if connected == peer_id) {
                        break;
                    }
                }
                event = peer_service.next() => {
                    assert!(event.is_some(), "peer service stream ended");
                }
            }
        }
    })
    .await
    .expect("persistent peer should connect shared swarm");

    let result = update.await.expect("command task completes");
    assert_eq!(result, Ok(()));
    assert!(service.malachite_persistent_peers().contains(&peer_addr));
}

#[tokio::test]
async fn malachite_behaviour_does_not_own_gossipsub() {
    let service = new_service().await;

    let behaviour = service.swarm.behaviour();
    let _shared_gossipsub = &behaviour.gossipsub;
    let _malachite = &behaviour.malachite;
    assert!(behaviour.malachite.as_ref().is_some());
}
