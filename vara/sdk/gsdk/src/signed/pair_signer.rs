// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::GearConfig;
use sp_core::{Pair as PairT, sr25519};
use sp_runtime::{
    AccountId32 as SpAccountId32, MultiSignature as SpMultiSignature,
    traits::{IdentifyAccount, Verify},
};
use subxt::{Config, tx::Signer};

/// A [`Signer`] implementation that can be constructed from an [`sp_core::Pair`].
#[derive(Clone, Debug)]
pub struct PairSigner<T: Config, Pair> {
    account_id: T::AccountId,
    signer: Pair,
}

impl<T, Pair> PairSigner<T, Pair>
where
    T: Config,
    Pair: PairT,
    // We go via an sp_runtime::MultiSignature. We can probably generalise this
    // by implementing some of these traits on our built-in MultiSignature and then
    // requiring them on all T::Signatures, to avoid any go-between.
    <SpMultiSignature as Verify>::Signer: From<Pair::Public>,
    T::AccountId: From<SpAccountId32>,
{
    /// Creates a new [`Signer`] from an [`sp_core::Pair`].
    pub fn new(signer: Pair) -> Self {
        let account_id = <SpMultiSignature as Verify>::Signer::from(signer.public()).into_account();
        Self {
            account_id: account_id.into(),
            signer,
        }
    }

    /// Returns the [`sp_core::Pair`] implementation used to construct this.
    pub fn signer(&self) -> &Pair {
        &self.signer
    }

    /// Return the account ID.
    pub fn account_id(&self) -> &T::AccountId {
        &self.account_id
    }
}

impl<T, Pair> Signer<T> for PairSigner<T, Pair>
where
    T: Config,
    Pair: PairT,
    Pair::Signature: Into<T::Signature>,
{
    fn account_id(&self) -> T::AccountId {
        self.account_id.clone()
    }

    fn sign(&self, signer_payload: &[u8]) -> T::Signature {
        self.signer.sign(signer_payload).into()
    }
}

impl From<sr25519::Pair> for PairSigner<GearConfig, sr25519::Pair> {
    fn from(pair: sr25519::Pair) -> Self {
        Self::new(pair)
    }
}
