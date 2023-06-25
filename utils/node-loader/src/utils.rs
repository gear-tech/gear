use anyhow::{anyhow, Result};
use futures::Future;
use futures_timer::Delay;
use gclient::{Event, GearApi, GearEvent, WSAddress};
use gear_call_gen::GearProgGenConfig;
use gear_core::ids::{MessageId, ProgramId};
use gear_core_errors::ReplyCode;
use gsdk::metadata::runtime_types::{
    gear_common::event::DispatchStatus as GenDispatchStatus,
    gear_core::{
        ids::MessageId as GenMId,
        message::{common::ReplyDetails, user::UserMessage as GenUserMessage},
    },
};
use rand::rngs::SmallRng;
use reqwest::Client;
use std::{
    collections::{BTreeSet, HashMap},
    fs::File,
    io::Write,
    result::Result as StdResult,
    time::Duration,
};

/// subxt's GenericError::Rpc::RequestError::RestartNeeded
pub const SUBXT_RPC_REQUEST_ERR_STR: &str = "Rpc error: The background task been terminated because: Networking or low-level protocol error";
/// subxt's GenericError::Rpc::RequestError::Call (CallError::Failed)
pub const SUBXT_RPC_CALL_ERR_STR: &str = "Transaction would exhaust the block limits";
pub const EVENTS_TIMEOUT_ERR_STR: &str = "Block events timeout";
pub const TRANSACTION_INVALID: &str = "Transaction Invalid";
pub const TRANSACTION_DROPPED: &str = "Transaction Dropped";
pub const WAITING_TX_FINALIZED_TIMEOUT_ERR_STR: &str =
    "Transaction finalization wait timeout is reached";

pub fn dump_with_seed(seed: u64) -> Result<()> {
    let code = gear_call_gen::generate_gear_program::<SmallRng>(
        seed,
        GearProgGenConfig::new_normal(),
        Default::default(),
    );

    let mut file = File::create("out.wasm")?;
    file.write_all(&code)?;

    Ok(())
}

pub fn str_to_wsaddr(endpoint: String) -> WSAddress {
    let endpoint = endpoint.replace("://", ":");

    let mut addr_parts = endpoint.split(':');

    let domain = format!(
        "{}://{}",
        addr_parts.next().unwrap_or("ws"),
        addr_parts.next().unwrap_or("127.0.0.1")
    );
    let port = addr_parts.next().and_then(|v| v.parse().ok());

    WSAddress::new(domain, port)
}

pub fn convert_iter<V, T: Into<V> + Clone>(args: Vec<T>) -> impl IntoIterator<Item = V> + Clone {
    args.into_iter().map(Into::into)
}

pub trait SwapResult {
    type SwappedOk;
    type SwappedErr;

    fn swap_result(self) -> StdResult<Self::SwappedOk, Self::SwappedErr>;
}

impl<T, E> SwapResult for StdResult<T, E> {
    type SwappedOk = E;
    type SwappedErr = T;

    fn swap_result(self) -> StdResult<Self::SwappedOk, Self::SwappedErr> {
        match self {
            Ok(t) => Err(t),
            Err(e) => Ok(e),
        }
    }
}

pub async fn with_timeout<T>(fut: impl Future<Output = T>) -> Result<T> {
    // 5 minute as default
    let wait_task = Delay::new(Duration::from_millis(5 * 60 * 1_000));

    tokio::select! {
        output = fut => Ok(output),
        _ = wait_task => {
            Err(anyhow!("Timeout occurred while running the action"))
        }
    }
}

pub async fn stop_node(monitor_url: String) -> Result<()> {
    let client = Client::new();
    let mut params = HashMap::new();
    params.insert("__script_name", "stop");

    client
        .post(monitor_url)
        .form(&params)
        .send()
        .await
        .map(|resp| tracing::debug!("{resp:?}"))?;

    Ok(())
}

pub async fn capture_mailbox_messages(
    api: &GearApi,
    event_source: &mut [gsdk::metadata::Event],
) -> Result<BTreeSet<MessageId>> {
    let to = ProgramId::from(api.account_id().as_ref());
    // Mailbox message expiration threshold block number: current(last) block number + 20.
    let bn_threshold = api.last_block_number().await? + 20;
    let mailbox_messages: Vec<_> = event_source
        .iter()
        .filter_map(|event| match event {
            Event::Gear(GearEvent::UserMessageSent {
                message,
                expiration: Some(exp_bn),
            }) if exp_bn >= &bn_threshold && message.destination == to.into() => {
                Some(message.id.into())
            }
            _ => None,
        })
        .collect();

    let mut ret = BTreeSet::new();

    // The loop is needed, because when you call the function multiple times in
    // a short time interval, you can receive same events. That's quite annoying,
    // because you can reply to or claim value from the message, that was removed
    // from mailbox, although you consider it existing, because of the event.
    //
    // Better solution after #1876
    for mid in mailbox_messages {
        if api.get_mailbox_message(mid).await?.is_some() {
            ret.insert(mid);
        }
    }

    Ok(ret)
}
/// Check whether processing batch of messages identified by corresponding
/// `message_ids` resulted in errors or has been successful.
///
/// This function returns a vector of statuses with an associated message
/// identifier ([`MessageId`]). Each status can be an error message in case
/// of an error.
pub fn err_or_succeed_batch(
    event_source: &mut [gsdk::metadata::Event],
    message_ids: impl IntoIterator<Item = MessageId>,
) -> Vec<(MessageId, Option<String>)> {
    let message_ids: Vec<GenMId> = message_ids.into_iter().map(Into::into).collect();

    event_source
        .iter_mut()
        .filter_map(|e| match e {
            Event::Gear(GearEvent::UserMessageSent {
                message:
                    GenUserMessage {
                        payload,
                        details: Some(ReplyDetails { to, code }),
                        ..
                    },
                ..
            }) => {
                if message_ids.contains(to) && ReplyCode::from(code.clone()).is_success() {
                    Some(vec![(
                        (*to).into(),
                        Some(String::from_utf8(payload.0.to_vec()).expect("Infallible")),
                    )])
                } else {
                    None
                }
            }
            Event::Gear(GearEvent::MessagesDispatched { statuses, .. }) => {
                let requested: Vec<_> = statuses
                    .iter_mut()
                    .filter_map(|(mid, status)| {
                        if message_ids.contains(mid) && !matches!(status, GenDispatchStatus::Failed)
                        {
                            Some((MessageId::from(*mid), None))
                        } else {
                            None
                        }
                    })
                    .collect();

                (!requested.is_empty()).then_some(requested)
            }
            _ => None,
        })
        .flatten()
        .collect()
}
