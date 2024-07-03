use crate::{
    event::{CreateProgram, UpdatedProgram, UserMessageSent, UserReplySent},
    observer::{read_block_events, ObserverProvider},
    BlockEvent,
};
use alloy::{
    primitives::Address,
    providers::{Provider, ProviderBuilder},
    rpc::types::eth::{BlockTransactionsKind, Filter},
};
use anyhow::{anyhow, bail, Result};
use gear_core::ids::ProgramId;
use gprimitives::H256;
use hypercore_db::BlockMetaInfo;
use hypercore_signer::Address as HypercoreAddress;
use parity_scale_codec::{Decode, Encode};
use std::collections::BTreeMap;

pub struct Query {
    database: Box<dyn BlockMetaInfo>,
    provider: ObserverProvider,
    router_address: Address,
}

impl Query {
    pub async fn new(
        database: Box<dyn BlockMetaInfo>,
        ethereum_rpc: &str,
        router_address: HypercoreAddress,
    ) -> Result<Self> {
        Ok(Self {
            database,
            provider: ProviderBuilder::new().on_builtin(ethereum_rpc).await?,
            router_address: Address::new(router_address.0),
        })
    }

    async fn calculate_block_start_program_hashes(
        &mut self,
        block_hash: H256,
        recursion_height: usize,
    ) -> Result<BTreeMap<ProgramId, H256>> {
        if let Some(program_hashes) = self.database.block_start_program_states(block_hash) {
            return Ok(program_hashes);
        }

        let parent_block = self.get_block_parent_hash(block_hash).await?;

        if let Some(program_states) = self.database.block_end_program_states(parent_block) {
            self.database
                .set_block_start_program_states(block_hash, program_states.clone());
            return Ok(program_states);
        }

        let mut parent_program_hashes =
            match self.database.block_start_program_states(parent_block) {
                Some(hashes) => hashes,
                None => {
                    if recursion_height >= 256 {
                        // TODO: cannot found program hashes in local history, need to fetch from ethereum
                        // or request from p2p network
                        // TODO: no need to search before genesis block
                        Default::default()
                    } else {
                        Box::pin(self.calculate_block_start_program_hashes(
                            parent_block,
                            recursion_height + 1,
                        ))
                        .await?
                    }
                }
            };

        for event in self.get_block_events(block_hash).await? {
            match event {
                BlockEvent::CreateProgram(CreateProgram { actor_id, .. }) => {
                    parent_program_hashes.insert(actor_id, H256::zero());
                }
                BlockEvent::UpdatedProgram(UpdatedProgram {
                    actor_id,
                    old_state_hash,
                    new_state_hash,
                }) => {
                    let state = parent_program_hashes
                        .get_mut(&actor_id)
                        .ok_or_else(|| anyhow!("previous state not found"))?;

                    if *state != old_state_hash {
                        bail!("incorrect state transition");
                    }

                    *state = new_state_hash;
                }
                _ => {}
            }
        }

        self.database
            .set_block_start_program_states(block_hash, parent_program_hashes.clone());
        self.database
            .set_block_end_program_states(parent_block, parent_program_hashes.clone());

        Ok(parent_program_hashes)
    }

    pub async fn preset_block_program_hashes(&mut self, block_hash: H256) -> Result<()> {
        self.calculate_block_start_program_hashes(block_hash, 0)
            .await
            .map(|states| {
                log::trace!("Block {block_hash} has program states: {states:?}");
            })
    }

    pub async fn get_block_parent_hash(&mut self, block_hash: H256) -> Result<H256> {
        match self.database.parent_hash(block_hash) {
            Some(parent_hash) => Ok(parent_hash),
            None => {
                let parent_hash = H256(
                    self.provider
                        .get_block_by_hash(block_hash.0.into(), BlockTransactionsKind::Hashes)
                        .await?
                        .unwrap()
                        .header
                        .parent_hash
                        .0,
                );
                self.database.set_parent_hash(block_hash, parent_hash);
                Ok(parent_hash)
            }
        }
    }

    pub async fn block_has_commitment(&mut self, block_hash: H256) -> Result<bool> {
        if let Some(has_commitment) = self.database.block_has_commitment(block_hash) {
            log::trace!("Block {block_hash} commitment info in database: {has_commitment}");
            return Ok(has_commitment);
        }

        // Search for program updates in block logs
        // TODO: this is not correct way, because block commitment can be empty
        let filter = Filter::new()
            .at_block_hash(block_hash.0)
            .address(self.router_address);
        let has_commitment = self
            .provider
            .get_logs(&filter)
            .await?
            .into_iter()
            .any(|log| {
                matches!(
                    log.topic0().copied().map(|bytes| bytes.0),
                    Some(UpdatedProgram::SIGNATURE_HASH)
                        | Some(UserMessageSent::SIGNATURE_HASH)
                        | Some(UserReplySent::SIGNATURE_HASH)
                )
            });

        log::trace!("Block {block_hash} commitment info from ethereum: {has_commitment}");

        self.database
            .set_block_has_commitment(block_hash, has_commitment);

        Ok(has_commitment)
    }

    pub async fn get_commitment_chain(&mut self, block_hash: H256) -> Result<Vec<H256>> {
        let mut chain = vec![];
        let mut current_hash = block_hash;

        let mut height = 0;
        loop {
            if self.database.block_outcome(current_hash).is_some() {
                log::trace!("Block {current_hash} already has outcome, so no need to process it");
                break;
            }

            chain.push(current_hash);

            if self.block_has_commitment(current_hash).await? {
                break;
            }

            current_hash = self.get_block_parent_hash(current_hash).await?;

            height += 1;
            if height >= 16 {
                // TODO: support too long commitment chains.
                // TODO: no need to search before genesis block.
                break;
            }
        }

        Ok(chain)
    }

    pub async fn get_block_events(&mut self, block_hash: H256) -> Result<Vec<BlockEvent>> {
        if let Some(events) = self
            .database
            .block_events(block_hash)
            .and_then(|events_encoded| {
                Vec::<BlockEvent>::decode(&mut events_encoded.as_slice()).ok()
            })
        {
            return Ok(events);
        }

        // Events not found or corrupted, need to query them from ethereum
        let (_, events) =
            read_block_events(block_hash, &self.provider, self.router_address).await?;
        self.database.set_block_events(block_hash, events.encode());

        Ok(events)
    }
}
