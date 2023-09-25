use gear_common::storage::{IterableByKeyMap, Messenger};
use gear_core::ids::MessageId;
use gear_runtime::{AccountId, Runtime};
use pallet_gear::Config;

pub fn get_mailbox_messages(sender: &AccountId) -> Vec<MessageId> {
    <<Runtime as Config>::Messenger as Messenger>::Mailbox::iter_key(sender.clone())
        .map(|(msg, _bn)| msg.id())
        .collect()
}
