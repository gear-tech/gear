#[cfg(feature = "sr25519")]
#[cfg(test)]
mod suri_tests {
    use gsigner::{SignatureScheme, sr25519};
    use sp_core::{Pair as _, sr25519 as substrate_sr25519};
    use sr25519::PrivateKey;

    #[test]
    fn test_alice_account() {
        let key = PrivateKey::from_suri("//Alice", None).expect("Failed to create Alice key");
        let public_key = sr25519::Sr25519::public_key(&key);

        // Alice's public key should be deterministic
        println!("Alice public key: {}", hex::encode(public_key.to_bytes()));

        // Should create the same key every time
        let key2 = PrivateKey::from_suri("//Alice", None).expect("Failed to create Alice key");
        let public_key2 = sr25519::Sr25519::public_key(&key2);

        assert_eq!(
            public_key.to_bytes(),
            public_key2.to_bytes(),
            "Alice keys should be deterministic"
        );

        let sp_public = substrate_sr25519::Pair::from_string("//Alice", None)
            .expect("sp_core Alice")
            .public()
            .0;
        assert_eq!(
            public_key.to_bytes(),
            sp_public,
            "Alice public key should match sp_core"
        );
    }

    #[test]
    fn test_bob_account() {
        let key = PrivateKey::from_suri("//Bob", None).expect("Failed to create Bob key");
        let public_key = sr25519::Sr25519::public_key(&key);

        println!("Bob public key: {}", hex::encode(public_key.to_bytes()));

        // Bob should be different from Alice
        let alice = PrivateKey::from_suri("//Alice", None).expect("Failed to create Alice key");
        let alice_pub = sr25519::Sr25519::public_key(&alice);

        assert_ne!(
            public_key.to_bytes(),
            alice_pub.to_bytes(),
            "Bob and Alice should have different keys"
        );

        let sp_bob = substrate_sr25519::Pair::from_string("//Bob", None)
            .expect("sp_core Bob")
            .public()
            .0;
        assert_eq!(
            public_key.to_bytes(),
            sp_bob,
            "Bob public key should match sp_core"
        );
    }

    #[test]
    fn test_derivation_path() {
        let key1 = PrivateKey::from_suri("//Alice//stash", None).expect("Failed to create key");
        let key2 = PrivateKey::from_suri("//Alice//stash", None).expect("Failed to create key");

        let pub1 = sr25519::Sr25519::public_key(&key1);
        let pub2 = sr25519::Sr25519::public_key(&key2);

        assert_eq!(
            pub1.to_bytes(),
            pub2.to_bytes(),
            "Derived keys should be deterministic"
        );

        // Derived key should be different from base key
        let base = PrivateKey::from_suri("//Alice", None).expect("Failed to create base key");
        let base_pub = sr25519::Sr25519::public_key(&base);

        assert_ne!(
            pub1.to_bytes(),
            base_pub.to_bytes(),
            "Derived key should differ from base"
        );
    }

    #[test]
    fn test_hex_seed() {
        let seed = "0x0000000000000000000000000000000000000000000000000000000000000001";
        let key = PrivateKey::from_suri(seed, None).expect("Failed to create key from seed");
        let public_key = sr25519::Sr25519::public_key(&key);

        println!(
            "Hex seed public key: {}",
            hex::encode(public_key.to_bytes())
        );

        // Should be deterministic
        let key2 = PrivateKey::from_suri(seed, None).expect("Failed to create key from seed");
        let public_key2 = sr25519::Sr25519::public_key(&key2);

        assert_eq!(public_key.to_bytes(), public_key2.to_bytes());
    }

    #[test]
    fn test_mnemonic_phrase() {
        let phrase = "bottom drive obey lake curtain smoke basket hold race lonely fit walk";
        let key = PrivateKey::from_phrase(phrase, None).expect("Failed to create key from phrase");
        let public_key = sr25519::Sr25519::public_key(&key);

        println!(
            "Mnemonic public key: {}",
            hex::encode(public_key.to_bytes())
        );

        // Should be deterministic
        let key2 = PrivateKey::from_phrase(phrase, None).expect("Failed to create key from phrase");
        let public_key2 = sr25519::Sr25519::public_key(&key2);

        assert_eq!(public_key.to_bytes(), public_key2.to_bytes());

        let sp_public = substrate_sr25519::Pair::from_phrase(phrase, None)
            .expect("sp_core phrase")
            .0
            .public()
            .0;
        assert_eq!(
            public_key.to_bytes(),
            sp_public,
            "Mnemonic derived public key should match sp_core"
        );
    }

    #[test]
    fn test_from_seed() {
        let seed = [1u8; 32];
        let key = PrivateKey::from_seed(seed).expect("Failed to create key from seed");
        let public_key = sr25519::Sr25519::public_key(&key);

        println!(
            "Raw seed public key: {}",
            hex::encode(public_key.to_bytes())
        );

        // Should be deterministic
        let key2 = PrivateKey::from_seed(seed).expect("Failed to create key from seed");
        let public_key2 = sr25519::Sr25519::public_key(&key2);

        assert_eq!(public_key.to_bytes(), public_key2.to_bytes());

        let sp_public = substrate_sr25519::Pair::from_seed(&seed).public().0;
        assert_eq!(
            public_key.to_bytes(),
            sp_public,
            "Seed-derived public key should match sp_core"
        );
    }
}
