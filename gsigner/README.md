# gsigner

Universal cryptographic signer library supporting multiple signature schemes.

## Overview

`gsigner` provides a unified interface for cryptographic signing operations supporting both:
- **secp256k1** (ECDSA) - Ethereum-compatible signatures
- **ed25519** (EdDSA) - Substrate-compatible signatures
- **sr25519** (Schnorrkel) - Substrate/Polkadot-compatible signatures

This crate combines and extends the functionality from both `ethexe-signer` and `gring` crates.

### Key improvements in this refactor

- **sp_core-backed key material across every scheme** – secp256k1, ed25519, and sr25519 now all wrap the upstream `sp_core` pairs/public/signature types, so SURIs, SS58 addresses, and SCALE codecs behave exactly like Substrate tooling.
- **Production-parity Ethereum signing** – recoverable signatures are still generated with canonical low-S normalisation and exposed as `sp_core::ecdsa::Signature`, preserving compatibility with existing JSON keystores and RPC consumers.
- **CLI parity for every scheme** – the keyring workflow that previously existed only for sr25519 is now available for secp256k1 and ed25519, including the short aliases `secp`, `ed`, and `sr`.
- **Unified storage abstraction** – every keyring command (CLI and API) understands the same storage location flags. Choose a filesystem directory with `--path`, keep keys ephemeral with `--memory`, and optionally encrypt any scheme by passing `--key-password`.
- **Consistent address handling** – SS58 encoding relies on the upstream codec (default Vara prefix 137) while Ethereum addresses remain the standard Keccak-256 derivation.

## Features

- `secp256k1` - Enable Ethereum/secp256k1 ECDSA support (enabled by default)
- `ed25519` - Enable Substrate-compatible ed25519 support (enabled by default)
- `sr25519` - Enable Substrate/sr25519 Schnorrkel support (enabled by default)
- `cli` - Enable command-line interface tools
- `peer-id` - Enable libp2p PeerId derivation helpers (secp256k1, ed25519)

## Usage

### Basic Example

```rust
use gsigner::secp256k1;

// Create an in-memory signer
let signer = secp256k1::Signer::memory();

// Generate a new key
let public_key = signer.generate_key_with_password(None)?;

// Sign some data
let message = b"hello world";
let signature = signer.sign_with_password(public_key, message, None)?;

// Verify signature
signer.verify(public_key, message, &signature)?;
```

### Using Different Schemes

```rust
use gsigner::{ed25519, secp256k1, sr25519};

// Ethereum signer
let eth_signer = secp256k1::Signer::memory();
let eth_key = eth_signer.generate_key_with_password(None)?;

// Ed25519 signer
let ed_signer = ed25519::Signer::memory();
let ed_key = ed_signer.generate_key_with_password(None)?;

// Sr25519 signer
let sub_signer = sr25519::Signer::memory();
let sub_key = sub_signer.generate_key_with_password(None)?;
```

### Storage Options

```rust
use gsigner::secp256k1;
use std::path::PathBuf;

// In-memory storage (ephemeral)
let memory_signer = secp256k1::Signer::memory();

// Filesystem storage (persistent)
let fs_signer = secp256k1::Signer::fs(PathBuf::from("./keys"))?;

// Encrypted filesystem storage
let encrypted = secp256k1::Signer::fs(PathBuf::from("./keys"))?;
let public_key = encrypted.generate_key_with_password(Some("hunter2"))?;

// Temporary filesystem storage
let tmp_signer = secp256k1::Signer::fs_temporary()?;

// Pass a password per key operation if you plan to export/import encrypted keystores later
let memory_with_password = secp256k1::Signer::memory();
let imported = memory_with_password.import_key_with_password(private_key, Some("hunter2"))?;
```

### CLI Highlights

- All stateful commands now live under `<scheme> keyring ...` and accept the unified storage flags (disk path, in-memory mode); commands that read or write encrypted key material also accept `--key-password`.
- Stateless helpers such as `verify`, `recover`, `address`, and `peer-id` remain at the scheme root.
- The CLI automatically resolves default storage locations per scheme (`$XDG_DATA_HOME/gsigner/<scheme>`), so most commands work without explicitly passing `--path`.
- `recover` is only available for secp256k1; ed25519 and sr25519 will report that recovery is unsupported.
- Responses can be rendered as human-readable text, pretty JSON, or compact JSON using `--format human|plain|json` (default: `human`).
- `peer-id` is available when built with `--features peer-id` and currently supports secp256k1 and ed25519 keys.

See [CLI.md](./CLI.md) for a full command reference with examples.

## Advanced Features

### Secp256k1 (Ethereum) Extensions

```rust
use gsigner::secp256k1::{self, Secp256k1SignerExt};
use gsigner::Address;

let signer = secp256k1::Signer::memory();
let key = signer.generate_key_with_password(None)?;

// Create signed data wrapper
let signed_data = signer.signed_data_with_password(key, b"hello world", None)?;
assert_eq!(signed_data.data(), &b"hello world");
assert_eq!(signed_data.public_key(), key);

// Create contract-specific signature (EIP-191)
let contract_addr = Address([0x42; 20]);
let contract_sig = signer.sign_for_contract_with_password(contract_addr, key, b"data", None)?;
```

### Sr25519 (Substrate) Extensions

```rust
use gsigner::sr25519::{self, Sr25519SignerExt, Keyring, PrivateKey};

// Sign with custom context
let signer = sr25519::Signer::memory();
let key = signer.generate_key_with_password(None)?;
let sig = signer.sign_with_context_with_password(key, b"my-app", b"message", None)?;

// Verify with context
signer.verify_with_context(key, b"my-app", b"message", &sig)?;

// Generate vanity key
let vanity_key = signer.generate_vanity_key_with_password("5Ge", None)?; // SS58 address starting with "5Ge"
```

### Ed25519 (Substrate) Basics

```rust
use gsigner::ed25519::{self, PrivateKey};

let signer = ed25519::Signer::memory();
let key = signer.generate_key_with_password(None)?;

// Sign and verify
let message = b"hello";
let signature = signer.sign_with_password(key, message, None)?;
signer.verify(key, message, &signature)?;

// Import from SURI
let alice = PrivateKey::from_suri("//Alice", None)?;
let imported = signer.import_key_with_password(alice, None)?;
let address = signer.address(imported);
println!("ed25519 SS58: {}", address.as_ss58());
```


### SURI Support (Ed25519 & Sr25519)

The library supports Substrate URI (SURI) format for key derivation across both ed25519 and sr25519 keys, compatible with Polkadot/Substrate tooling:

```rust
use gsigner::{ed25519, sr25519};
use gsigner::ed25519::PrivateKey as EdPrivateKey;
use gsigner::sr25519::PrivateKey as SrPrivateKey;

// Well-known development accounts
let alice = SrPrivateKey::from_suri("//Alice", None)?;
let ed_alice = EdPrivateKey::from_suri("//Alice", None)?;

// Derivation paths
let alice_stash = SrPrivateKey::from_suri("//Alice//stash", None)?;
let custom_path = SrPrivateKey::from_suri("//Alice//my//custom//path", None)?;

// From hex seed
let from_hex = SrPrivateKey::from_suri(
    "0x0000000000000000000000000000000000000000000000000000000000000001",
    None,
)?;

// From mnemonic phrase (12 or 24 words)
let from_mnemonic = SrPrivateKey::from_phrase(
    "bottom drive obey lake curtain smoke basket hold race lonely fit walk",
    None,
)?;

// With password for derivation
let with_password = SrPrivateKey::from_suri("//Alice", Some("mypassword"))?;

// From raw seed bytes
let seed = [0u8; 32];
let from_seed = SrPrivateKey::from_seed(seed)?;

// Import into signers
let sr_signer = sr25519::Signer::memory();
let sr_public = sr_signer.import_key_with_password(alice, None)?;
let ed_signer = ed25519::Signer::memory();
let ed_public = ed_signer.import_key_with_password(ed_alice, None)?;
```


**Supported SURI formats:**
- Named accounts: `//Alice`, `//Bob`, `//Charlie`, `//Dave`, `//Eve`, `//Ferdie`
- Derivation paths: `//Alice//stash`, `//Alice//0//1`
- Hex seeds: `0x<64 hex chars>`
- Mnemonic phrases: 12 or 24 word phrases
- Password protection: Any SURI with optional password parameter

### Keyring Management

The `keyring` feature now covers every supported scheme. Each module exposes a
scheme-specific alias around the generic [`gsigner::keyring::Keyring`] type
alongside helpers for its native key formats.

#### sr25519 (Substrate)

```rust
use gsigner::sr25519::Keyring;
use std::path::PathBuf;

let mut keyring = Keyring::load(PathBuf::from("./sr-keyring"))?;

// Create a new key with optional encryption
let (keystore, private_key) = keyring.create("alice", Some(b"password"))?;
let public_key = private_key.public_key();

// Create vanity key
let (vanity_keystore, vanity_private) = keyring.create_vanity("bob", "5Ge", Some(b"pass"))?;

// Set primary key
keyring.set_primary("alice")?;

// Import polkadot-js keystore
keyring.import(PathBuf::from("./alice.json"))?;
```

#### ed25519 (Substrate)

```rust
use gsigner::ed25519::Keyring;
use std::path::PathBuf;

let mut keyring = Keyring::load(PathBuf::from("./ed-keyring"))?;

// Generate a new keypair
let (keystore, private_key) = keyring.create("alice")?;

// Import from SURI or raw seed
let (_, imported) = keyring.import_suri("bob", "//Bob", None)?;
let _ = keyring.add_hex("charlie", "0x0123...")?; // 32-byte hex seed

// Access derived data
let public_key = keystore.public_key()?;
let address = keystore.address()?.as_ss58().to_string();
```

#### secp256k1 (Ethereum)

```rust
use gsigner::secp256k1::Keyring;
use std::path::PathBuf;

let mut keyring = Keyring::load(PathBuf::from("./eth-keyring"))?;

// Generate a fresh account
let (keystore, private_key) = keyring.create("validator")?;

// Import an existing private key (0x-prefixed hex)
let imported = keyring.add_hex("deployer", "0xac09...")?;

// Import from a SURI or mnemonic
let (_, suri_private) = keyring.import_suri("alice", "//Alice", None)?;

// Inspect stored metadata
let address = imported.address()?.to_hex();
```

## Architecture

### Trait-Based Design

The library uses trait-based abstraction to support multiple signature schemes:

```rust
pub trait SignatureScheme {
    type PrivateKey;
    type PublicKey;
    type Signature;
    type Address;
    
    fn generate_keypair() -> (Self::PrivateKey, Self::PublicKey);
    fn sign(key: &Self::PrivateKey, data: &[u8]) -> Result<Self::Signature>;
    fn verify(key: &Self::PublicKey, data: &[u8], sig: &Self::Signature) -> Result<()>;
}
```

### Storage Abstraction

Every signer is backed by the JSON keyring defined in [`gsigner::keyring`](src/keyring/mod.rs).
`Signer::fs(path)` stores keys in a namespaced directory inside the provided path, while
`Signer::memory()` keeps everything in-memory without touching disk. The keyring exposes the full
keystore metadata (name, creation time, associated address) and reuses the same format for both the
CLI and the library APIs. Keys can be listed, imported, or removed directly through the `Signer`
methods:

```rust
let signer = gsigner::secp256k1::Signer::fs("~/.local/share/gsigner".into());
let public = signer.generate_key_with_password(None)?;
let private = signer.get_private_key_with_password(public, None)?;
signer.clear_keys()?;
```

## Compatibility

### Ethereum (secp256k1)

- Compatible with standard Ethereum tooling
- Supports EIP-191 signatures
- Keccak256 hashing
- 20-byte addresses (0x...)

### Substrate (ed25519 & sr25519)

- Polkadot-js keystore format compatible
- SS58 address encoding (VARA network)
- Scrypt + XSalsa20-Poly1305 encryption
- PKCS8 key format

## License

GPL-3.0-or-later WITH Classpath-exception-2.0
