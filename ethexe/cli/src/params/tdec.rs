// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
use crate::params::MergeParams;
use ethexe_service::config::ThresholdDecryptionCliConfig;
use gear_tdec::bls12_381::{
    DkgPublicKey, PublicDecryptionContextSimple as PublicDecryptionContext,
};
use gsigner::Address;

/// Threshold-decryption parameters.
#[derive(Clone, Debug, serde::Deserialize, clap::Parser)]
pub struct TdecParams {
    /// Minimal number of validator decryption shares required to decrypt a
    /// shielded transaction.
    #[arg(long)]
    pub threshold: std::num::NonZeroUsize,

    /// DKG public key used by clients to encrypt shielded transaction fields.
    #[arg(long = "dkg-public-key", alias = "pubic-key")]
    #[serde(rename = "dkg-public-key")]
    pub dkg_public_key: DkgPublicKey,

    /// Public decryption contexts for validators participating in threshold
    /// decryption.
    ///
    /// Pass one option per validator as `ADDRESS=CONTEXT`, where `ADDRESS` is
    /// a secp256k1 validator address and `CONTEXT` is the hex string produced
    /// by `PublicDecryptionContextSimple`.
    #[arg(long = "validators-contexts", value_name = "ADDRESS=CONTEXT")]
    #[serde(rename = "validators-contexts")]
    pub validators_contexts: Option<Vec<ValidatorContext>>,
}

impl TdecParams {
    pub fn into_config(self) -> ThresholdDecryptionCliConfig {
        ThresholdDecryptionCliConfig {
            threshold: self.threshold,
            dkg_public_key: self.dkg_public_key,
            validators_contexts: self
                .validators_contexts
                .map(|ctxs| ctxs.into_iter().map(ValidatorContext::into_parts).collect()),
        }
    }
}

impl MergeParams for TdecParams {
    fn merge(self, with: Self) -> Self {
        let validators_contexts = match with.validators_contexts {
            Some(mut contexts) => {
                if let Some(my_contexts) = self.validators_contexts {
                    contexts.extend(my_contexts);
                }
                Some(contexts)
            }
            None => self.validators_contexts,
        };
        Self {
            threshold: self.threshold,
            dkg_public_key: self.dkg_public_key,
            validators_contexts,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ValidatorContext {
    pub address: Address,
    pub context: PublicDecryptionContext,
}

impl ValidatorContext {
    fn into_parts(self) -> (Address, PublicDecryptionContext) {
        (self.address, self.context)
    }
}

impl From<(Address, PublicDecryptionContext)> for ValidatorContext {
    fn from((address, context): (Address, PublicDecryptionContext)) -> Self {
        Self { address, context }
    }
}

impl std::str::FromStr for ValidatorContext {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let (address, context) = value.split_once('=').ok_or_else(|| {
            anyhow::anyhow!("expected validator context in ADDRESS=CONTEXT format")
        })?;

        Ok(Self {
            address: address.parse()?,
            context: context.parse()?,
        })
    }
}

impl<'de> serde::Deserialize<'de> for ValidatorContext {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        #[serde(untagged)]
        enum ValidatorContextRepr {
            Named {
                address: Address,
                context: PublicDecryptionContext,
            },
            Tuple((Address, PublicDecryptionContext)),
        }

        match ValidatorContextRepr::deserialize(deserializer)? {
            ValidatorContextRepr::Named { address, context } => Ok(Self { address, context }),
            ValidatorContextRepr::Tuple(tuple) => Ok(tuple.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    fn public_decryption_context(
        dealer: &gear_tdec::DealerOutput<gear_tdec::bls12_381::E>,
    ) -> PublicDecryptionContext {
        dealer.private_contexts[0].public_decryption_contexts[0].clone()
    }

    #[test]
    fn validator_context_parses_from_cli_value() {
        let dealer = gear_tdec::deal::<gear_tdec::bls12_381::E>(
            1,
            1,
            &mut gear_tdec::rand_utils::test_rng(),
        );
        let address = Address::from([1; 20]);
        let context = public_decryption_context(&dealer);

        let parsed = format!("{address}={context}")
            .parse::<ValidatorContext>()
            .expect("validator context must parse");

        assert_eq!(parsed.address, address);
        assert_eq!(parsed.context.to_string(), context.to_string());
    }

    #[test]
    fn tdec_params_accepts_validator_contexts_from_clap() {
        let dealer = gear_tdec::deal::<gear_tdec::bls12_381::E>(
            1,
            1,
            &mut gear_tdec::rand_utils::test_rng(),
        );
        let address = Address::from([1; 20]);
        let context = public_decryption_context(&dealer);
        let context_arg = format!("{address}={context}");

        let params = TdecParams::try_parse_from([
            "ethexe",
            "--threshold",
            "1",
            "--dkg-public-key",
            &dealer.public_key.to_string(),
            "--validators-contexts",
            &context_arg,
        ])
        .expect("tdec params must parse");

        let contexts = params
            .validators_contexts
            .expect("validator contexts must be present");
        assert_eq!(contexts.len(), 1);
        assert_eq!(contexts[0].address, address);
        assert_eq!(contexts[0].context.to_string(), context.to_string());
    }
}
