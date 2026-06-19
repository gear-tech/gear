use super::*;
use crate::{NetworkEvent, tests::new_service, utils::tests::init_logger};
use futures::{Stream, StreamExt, future::poll_fn};
use libp2p::multiaddr::Protocol;
use libp2p_swarm_test::SwarmExt;
use malachitebft_engine::network::NetworkMsg;
use malachitebft_network::PersistentPeersOp;
use malachitebft_test::{TestContext, codec::json::JsonCodec};
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
    fn assert_parts(parts: MalachiteNetworkParts<TestContext>) {
        let (_network_ref, _tx_network) = parts.into_engine_parts();
    }
}

#[tokio::test]
async fn network_service_runs_without_malachite_lane() {
    let service = new_service();
    assert!(service.malachite_lane_status().is_none());
}

#[tokio::test]
async fn register_malachite_lane_with_persistent_peers_returns_engine_parts() {
    init_logger();

    let mut service = new_service();
    let parts = service
        .register_malachite_lane_with_persistent_peers::<TestContext, JsonCodec>(
            JsonCodec,
            Vec::new(),
        )
        .await
        .expect("registers malachite lane");

    let (_network_ref, _tx_network) = parts.into_engine_parts();
    assert_eq!(
        service.malachite_lane_status(),
        Some(MalachiteLaneStatus::Registered)
    );
}

#[tokio::test]
async fn persistent_peer_updates_reach_lane_state() {
    let mut service = new_service();
    let parts = service
        .register_malachite_lane_with_persistent_peers::<TestContext, JsonCodec>(
            JsonCodec,
            Vec::new(),
        )
        .await
        .expect("registers malachite lane");
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

    let mut service = new_service();
    let parts = service
        .register_malachite_lane_with_persistent_peers::<TestContext, JsonCodec>(
            JsonCodec,
            Vec::new(),
        )
        .await
        .expect("registers malachite lane");
    let (network_ref, _tx_network) = parts.into_engine_parts();

    let mut peer_service = new_service();
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
async fn malachite_lane_does_not_add_second_ping_or_identify() {
    let mut service = new_service();
    service
        .register_malachite_lane_with_persistent_peers::<TestContext, JsonCodec>(
            JsonCodec,
            Vec::new(),
        )
        .await
        .expect("registers malachite lane");

    let behaviour = service.swarm.behaviour();
    assert!(behaviour.malachite.as_ref().is_some());
}

#[tokio::test]
async fn publish_proposal_part_uses_malachite_lane() {
    let mut service = new_service();
    service
        .register_malachite_lane_with_persistent_peers::<TestContext, JsonCodec>(
            JsonCodec,
            Vec::new(),
        )
        .await
        .expect("registers malachite lane");

    service.handle_malachite_command(adapter::LaneCommand::PublishProposalPart(
        bytes::Bytes::from_static(b"part"),
    ));

    assert_eq!(
        service.malachite_debug_counters().proposal_parts_published,
        1
    );
}
