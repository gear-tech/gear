// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Test accounts available in runtime for testing.

use parity_scale_codec::Encode;
use runtime_primitives::{AccountId, Nonce};
use sp_keyring::{Ed25519Keyring, Sr25519Keyring};
use sp_runtime::generic::{self, Era, ExtrinsicFormat};
use vara_runtime::{
    CustomChargeTransactionPayment, CustomCheckNonce, RuntimeCall, SessionKeys, SignedExtra,
    StakingBlackList, UncheckedExtrinsic,
};

pub type CheckedExtrinsic =
    sp_runtime::generic::CheckedExtrinsic<AccountId, RuntimeCall, SignedExtra>;

/// Alice's account id.
pub fn alice() -> AccountId {
    Sr25519Keyring::Alice.into()
}

/// Bob's account id.
pub fn bob() -> AccountId {
    Sr25519Keyring::Bob.into()
}

/// Charlie's account id.
pub fn charlie() -> AccountId {
    Sr25519Keyring::Charlie.into()
}

/// Dave's account id.
pub fn dave() -> AccountId {
    Sr25519Keyring::Dave.into()
}

/// Eve's account id.
pub fn eve() -> AccountId {
    Sr25519Keyring::Eve.into()
}

/// Ferdie's account id.
pub fn ferdie() -> AccountId {
    Sr25519Keyring::Ferdie.into()
}

/// Convert keyrings into `SessionKeys`.
pub fn to_session_keys(
    ed25519_keyring: &Ed25519Keyring,
    sr25519_keyring: &Sr25519Keyring,
) -> SessionKeys {
    SessionKeys {
        babe: sr25519_keyring.to_owned().public().into(),
        grandpa: ed25519_keyring.to_owned().public().into(),
        im_online: sr25519_keyring.to_owned().public().into(),
        authority_discovery: sr25519_keyring.to_owned().public().into(),
    }
}

/// Creates transaction extra.
pub fn signed_extra(nonce: Nonce) -> SignedExtra {
    (
        StakingBlackList::new(),
        frame_system::CheckNonZeroSender::new(),
        frame_system::CheckSpecVersion::new(),
        frame_system::CheckTxVersion::new(),
        frame_system::CheckGenesis::new(),
        frame_system::CheckEra::from(Era::mortal(256, 0)),
        CustomCheckNonce::from(nonce),
        frame_system::CheckWeight::new(),
        CustomChargeTransactionPayment::from(0),
        frame_metadata_hash_extension::CheckMetadataHash::new(false),
    )
}

/// Sign given `CheckedExtrinsic`.
pub fn sign(
    xt: CheckedExtrinsic,
    spec_version: u32,
    tx_version: u32,
    genesis_hash: [u8; 32],
    metadata_hash: Option<[u8; 32]>,
) -> UncheckedExtrinsic {
    match xt.format {
        ExtrinsicFormat::Signed(signed, extra) => {
            let payload = (
                xt.function,
                extra.clone(),
                spec_version,
                tx_version,
                genesis_hash,
                genesis_hash,
                metadata_hash,
            );
            let key = Sr25519Keyring::from_account_id(&signed).unwrap();
            let signature = payload
                .using_encoded(|b| {
                    if b.len() > 256 {
                        key.sign(&sp_io::hashing::blake2_256(b))
                    } else {
                        key.sign(b)
                    }
                })
                .into();
            generic::UncheckedExtrinsic::new_signed(
                payload.0,
                sp_runtime::MultiAddress::Id(signed),
                signature,
                extra,
            )
            .into()
        }
        ExtrinsicFormat::Bare => generic::UncheckedExtrinsic::new_bare(xt.function).into(),
        ExtrinsicFormat::General(ext_version, extra) => generic::UncheckedExtrinsic::from_parts(
            xt.function,
            generic::Preamble::General(ext_version, extra),
        )
        .into(),
    }
}
