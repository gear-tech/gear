// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::{
    MalachiteService, MalachiteServiceConfig, Mempool,
    config::ValidatorConfig,
    externalities::{EthexeExternalities, ExternalitiesConfig},
    types::{ChainHead, MalachiteEvent},
};
use anyhow::{Context as _, Result, anyhow};
use ethexe_common::{
    Address, SimpleBlockData,
    db::{ConfigStorageRO, GlobalsStorageRO},
};
use ethexe_db::Database;
use ethexe_malachite_core::{MalachiteCore, MalachiteCoreConfig, NodeRole};
use gsigner::schemes::secp256k1::{PrivateKey, PublicKey};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::{
    Notify, RwLock,
    mpsc::{self, UnboundedReceiver},
};

/// Consensus service starter: prepares all [`MalachiteService`] components
/// up front so [`Self::start`] only has to launch the consensus core.
pub struct MalachiteServiceStarter {
    events_rx: UnboundedReceiver<Result<MalachiteEvent>>,
    chain_head: Arc<ChainHead>,
    mempool: Option<Arc<dyn Mempool>>,
    externalities: Arc<EthexeExternalities>,
    validators: HashMap<Address, PublicKey>,
    active_era: u64,
    core_config: MalachiteCoreConfig,
}

impl MalachiteServiceStarter {
    /// Prepare a service: resolve the node role (validator / full node),
    /// build the externalities and the core config.
    /// Fails if `config.validators` is empty or the validator key is absent in the signer.
    pub fn new<M: Mempool>(
        config: MalachiteServiceConfig,
        validator_config: Option<ValidatorConfig<M>>,
        db: Database,
        initial_chain_head: SimpleBlockData,
    ) -> Result<Self> {
        std::fs::create_dir_all(&config.home_dir)
            .with_context(|| format!("creating Malachite home dir {:?}", config.home_dir))?;

        if config.validators.is_empty() {
            return Err(anyhow!("MalachiteServiceConfig::validators is empty"));
        }

        let active_era = db
            .config()
            .timelines
            .era_from_ts(initial_chain_head.header.timestamp)
            .context("initial chain head must be after genesis")?;

        // Validators sign votes/proposals using their on-chain key;
        // full nodes get an ephemeral secret used only as the libp2p
        // peer identity.
        let (role, validator_secret, mempool) = match validator_config {
            Some(ValidatorConfig {
                pub_key: public_key,
                mempool,
                signer,
            }) => {
                let secret = signer
                    .private_key(public_key)
                    .context("extracting validator private key from signer")?;

                (
                    NodeRole::Validator,
                    secret,
                    Some(Arc::new(mempool) as Arc<dyn Mempool>),
                )
            }
            None => (NodeRole::FullNode, PrivateKey::random(), None),
        };

        let core_config = MalachiteCoreConfig {
            listen_addr: config.listen_addr,
            base: config.home_dir.clone(),
            persistent_peers: config.persistent_peers.clone(),
            validator_secret,
            validators: config.validators.clone(),
            role,
            propose_timeout: config.propose_timeout,
        };

        let chain_head = Arc::new(ChainHead {
            latest: RwLock::new(initial_chain_head),
            latest_synced: RwLock::new(db.globals().latest_synced_eb),
            notify: Notify::new(),
        });

        let (event_tx, events_rx) = mpsc::unbounded_channel();

        // On-chain addresses → pub keys, so era rotations resolve back without an out-of-band lookup.
        let validators: HashMap<Address, PublicKey> = config
            .validators
            .iter()
            .map(|v| (v.public_key.to_address(), v.public_key))
            .collect();

        let externalities = Arc::new(EthexeExternalities {
            db,
            cfg: ExternalitiesConfig {
                gas_allowance: config.gas_allowance,
                canonical_quarantine: config.canonical_quarantine,
                post_quarantine_delay: config.post_quarantine_delay,
            },
            mempool: mempool.clone(),
            chain_head: chain_head.clone(),
            pending_events: Default::default(),
            event_tx,
            validators: validators.clone(),
        });

        Ok(Self {
            events_rx,
            chain_head,
            mempool,
            externalities,
            validators,
            active_era,
            core_config,
        })
    }

    /// Launch the consensus core and assemble the running [`MalachiteService`].
    pub async fn start(self) -> Result<MalachiteService> {
        let MalachiteServiceStarter {
            events_rx,
            chain_head,
            mempool,
            externalities,
            validators,
            active_era,
            core_config,
        } = self;

        let inner = MalachiteCore::new(core_config, externalities.clone())
            .await
            .context("starting ethexe-malachite-core")?;

        Ok(MalachiteService {
            events_rx,
            chain_head,
            mempool,
            externalities,
            validators,
            active_era,
            inner: Some(inner),
        })
    }
}
