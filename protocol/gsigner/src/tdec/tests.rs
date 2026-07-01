// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

pub use crate::*;

#[test]
fn imports_and_gets_validator_decryption_key_by_public_key() {
    let mut rng = gear_tdec::rand_utils::test_rng();
    let keypair = TdecKeypair::new(&mut rng);
    let store = TdecKeyStore::memory();

    let public_key = store.import_keypair(keypair).unwrap();
    assert!(store.has_key(&public_key).unwrap());
    assert_eq!(
        store.validator_decryption_key(&public_key).unwrap(),
        keypair.decryption_key
    );
}

#[test]
fn creates_decryption_share_from_public_context() {
    let mut rng = gear_tdec::rand_utils::test_rng();
    let dealer = gear_tdec::deal::<Bls12_381>(3, 2, &mut rng);
    let context = dealer.private_contexts[0].clone();
    let public_context = context.public_decryption_contexts[context.index].clone();
    let ciphertext =
        gear_tdec::encrypt_raw::<Bls12_381>(b"hello", b"aad", &dealer.public_key, &mut rng)
            .unwrap();
    let header = ciphertext.header();
    let store = TdecKeyStore::memory();
    store
        .import_decryption_key(context.validator_decryption_key)
        .unwrap();

    let expected = context.create_share(&header, b"aad").unwrap();
    let actual = store
        .create_share(&public_context, &header, b"aad")
        .unwrap();
    assert_eq!(actual, expected);
}
