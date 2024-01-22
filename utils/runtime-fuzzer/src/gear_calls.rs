
// /// Extrinsic generator that's capable of generating `SendReply` calls.
// pub(crate) struct SendReplyGenerator {
//     pub mailbox_provider: Box<dyn MailboxProvider>,

//     pub gas: u64,
//     pub value: u128,
// }

// impl SendReplyGenerator {
//     fn generate(&self, unstructured: &mut Unstructured) -> Result<Option<GearCall>> {
//         log::trace!(
//             "Random data before payload (send_reply) gen {}",
//             unstructured.len()
//         );
//         let message_id =
//             arbitrary_message_id_from_mailbox(unstructured, self.mailbox_provider.as_ref())?;

//         Ok(match message_id {
//             None => None,
//             Some(message_id) => {
//                 let payload = arbitrary_payload(unstructured)?;
//                 log::trace!(
//                     "Random data after payload (send_reply) gen {}",
//                     unstructured.len()
//                 );
//                 log::trace!("Payload (send_reply) length {:?}", payload.len());

//                 Some(SendReplyArgs((message_id, payload, self.gas, self.value)).into())
//             }
//         })
//     }

//     const fn unstructured_size_hint(&self) -> usize {
//         ID_SIZE + MAX_PAYLOAD_SIZE + GAS_AND_VALUE_SIZE + AUXILIARY_SIZE
//     }
// }

// impl From<SendReplyGenerator> for ExtrinsicGenerator {
//     fn from(g: SendReplyGenerator) -> ExtrinsicGenerator {
//         ExtrinsicGenerator::SendReply(g)
//     }
// }

// /// Extrinsic generator that's capable of generating `ClaimValue` calls.
// pub(crate) struct ClaimValueGenerator {
//     pub mailbox_provider: Box<dyn MailboxProvider>,
// }

// impl ClaimValueGenerator {
//     fn generate(&self, unstructured: &mut Unstructured) -> Result<Option<GearCall>> {
//         log::trace!("Generating claim_value call");
//         let message_id =
//             arbitrary_message_id_from_mailbox(unstructured, self.mailbox_provider.as_ref())?;
//         Ok(message_id.map(|msg_id| ClaimValueArgs(msg_id).into()))
//     }

//     const fn unstructured_size_hint(&self) -> usize {
//         ID_SIZE + AUXILIARY_SIZE
//     }
// }

// impl From<ClaimValueGenerator> for ExtrinsicGenerator {
//     fn from(g: ClaimValueGenerator) -> ExtrinsicGenerator {
//         ExtrinsicGenerator::ClaimValue(g)
//     }
// }

// fn arbitrary_message_id_from_mailbox(
//     u: &mut Unstructured,
//     mailbox_provider: &dyn MailboxProvider,
// ) -> Result<Option<MessageId>> {
//     let messages = mailbox_provider.fetch_messages();

//     if messages.is_empty() {
//         log::trace!("Mailbox is empty.");
//         Ok(None)
//     } else {
//         log::trace!("Mailbox is not empty, len = {}", messages.len());
//         u.choose(&messages).cloned().map(Some)
//     }
// }
