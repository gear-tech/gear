# gsigner CLI

Command-line interface for the gsigner universal cryptographic signing library.

## Architecture

The CLI is built on a unified `KeyringCommandHandler` trait that provides:
- **Default implementations** for `show` and `clear` commands across all schemes
- **Scheme-specific overrides** where needed (e.g., secp256k1 `show` accepts both public keys and Ethereum addresses)
- **Consistent command dispatch** via `execute_keyring_command<H>()` generic function

This means ed25519 and sr25519 share the same `show` and `clear` logic automatically, while secp256k1 can provide Ethereum-specific behavior.

## Installation

```bash
cargo build --release --features cli
```

The binary will be available at `target/release/gsigner`.

## Usage

The top-level groups accept convenient shortcuts: `gsigner secp ...`, `gsigner ed ...`, and `gsigner sr ...` map to the longer `secp256k1`, `ed25519`, and `sr25519` commands respectively.

Operations that need to read or write key storage now live under the `keyring` subcommand:

```text
gsigner <scheme> keyring <command> [--storage <path>]
```

Stateless helpers such as `verify`, `recover`, or `address` remain at the root.

### Output formatting

All commands accept `--format <human|plain|json>` (default: `human`). `plain` pretty-prints the structured response, while `json` emits a single-line JSON payload that is convenient for scripting:

```bash
gsigner secp256k1 --format json address --public-key 0x03...
```

To encrypt keys on disk, pass `--key-password <PASSWORD>` to commands that read or write encrypted key material (generate, create, import, sign, show, vanity). The same password must be supplied consistently for subsequent operations targeting the encrypted keyring.

### Storage locations

Every keyring-aware command accepts the same storage location flags:

- `--path <PATH>` (short form `-s` and alias `--storage`) stores keys on disk. When omitted the CLI falls back to the default `~/.local/share/gsigner/<scheme>` directory.
- `--memory` keeps keys entirely in memory for the lifetime of the process. This is handy for tests or scripting when nothing should touch disk.
- `--key-password <PASSWORD>` enables encryption for the keystore. Supply the same password for operations that read or write encrypted key material. If no password is provided the keyring remains plaintext.

You can mix these flags as needed, e.g. `gsigner secp keyring generate --memory --key-password test` to exercise the encryption path without creating files.

### Stateless commands

These helpers do not require stored keys:

- Verify a signature:

  ```bash
  gsigner sr25519 verify --public-key 0x... --data 48656c6c6f --signature 0x...
  ```

- Recover a secp256k1 public key from a signature:

  ```bash
  gsigner secp256k1 recover --data 48656c6c6f --signature 0x...
  ```
  _Note: recovery is only supported for secp256k1; ed25519/sr25519 will return an error._

- Derive an address; for Substrate schemes you can override the SS58 network with `--network <name|prefix>`:

  ```bash
  gsigner ed25519 address --public-key 0x... --network polkadot
  ```

- Derive a libp2p PeerId (build with `--features peer-id`; supported for secp256k1 and ed25519):

  ```bash
  gsigner secp256k1 peer-id --public-key 0x03...
  ```

### Secp256k1 (Ethereum) Commands

#### Generate a new keypair

```bash
gsigner secp256k1 keyring generate
```

With filesystem storage:

```bash
gsigner secp256k1 keyring generate --path ./keys
```

Or in-memory (ephemeral):

```bash
gsigner secp256k1 keyring generate --memory
```

#### Sign data

```bash
gsigner secp256k1 keyring sign \
  --public-key 0x03... \
  --data 48656c6c6f \
  --path ./keys
```

Sign with EIP-191 (contract-specific):

```bash
gsigner secp256k1 keyring sign \
  --public-key 0x03... \
  --data 48656c6c6f \
  --contract 0x1234567890abcdef1234567890abcdef12345678
```

#### Verify signature

```bash
gsigner secp256k1 verify \
  --public-key 0x03... \
  --data 48656c6c6f \
  --signature 0x...
```

#### Get Ethereum address

```bash
gsigner secp256k1 address --public-key 0x03...
```

#### List keys

```bash
gsigner secp256k1 keyring list --path ./keys
```

#### Clear stored keys

```bash
gsigner secp256k1 keyring clear --path ./keys
```

Removes every stored secp256k1 key from the chosen location.

#### Show key details

```bash
# By public key
gsigner secp256k1 keyring show 0x03... --path ./keys

# By Ethereum address (secp256k1 only)
gsigner secp256k1 keyring show 0x1234567890abcdef1234567890abcdef12345678 --path ./keys
```

Pass either the compressed public key or the `0x...` Ethereum address. Add `--show-secret` to include the private key in the output.

**Note:** The address lookup is a secp256k1-specific feature. Ed25519 and sr25519 only accept public keys for the `show` command.

#### Generate vanity address

```bash
gsigner secp256k1 keyring vanity \
  --name vanity \
  --prefix 0abc \
  --path ./keys \
  --show-secret
```

The prefix is matched against the lowercase Ethereum address (with or without `0x`). Use an empty prefix (`--prefix ""`) to accept the next generated key immediately.

### Ed25519 (Substrate) Commands

#### Generate a new keypair

```bash
gsigner ed25519 keyring generate
```

With filesystem storage:

```bash
gsigner ed25519 keyring generate --path ./keys
```

In-memory:

```bash
gsigner ed25519 keyring generate --memory
```
#### Import from SURI

```bash
gsigner ed25519 keyring import   --suri "//Alice"   --path ./keys
```

With password-protected SURI:

```bash
gsigner ed25519 keyring import   --suri "//Alice"   --password "mypassword"   --path ./keys
```

Import from a hex seed:

```bash
gsigner ed25519 keyring import   --seed 0x0123...   --path ./keys
```

#### Sign data

```bash
gsigner ed25519 keyring sign   --public-key 0x...   --data 48656c6c6f   --path ./keys
```

#### Verify signature

```bash
gsigner ed25519 verify   --public-key 0x...   --data 48656c6c6f   --signature 0x...
```

#### Get SS58 address

```bash
gsigner ed25519 address --public-key 0x...
```

#### List stored keys

```bash
gsigner ed25519 keyring list --path ./keys
```

#### Clear stored keys

```bash
gsigner ed25519 keyring clear --path ./keys
```

Deletes all ed25519 entries from the specified storage.

#### Show key details

```bash
gsigner ed25519 keyring show 0x... --path ./keys
```

Accepts a 32-byte hex-encoded public key. Use `--show-secret` to include the private key seed.

#### Generate vanity address

```bash
gsigner ed25519 keyring vanity \
  --name vanity \
  --prefix 5F \
  --path ./keys \
  --show-secret
```

The prefix is matched against the SS58 address. Empty prefixes (`--prefix ""`) accept the next generated key immediately.

### Sr25519 (Substrate) Commands

#### Generate a new keypair

```bash
gsigner sr25519 keyring generate
```

With filesystem storage:

```bash
gsigner sr25519 keyring generate --path ./keys
```

In-memory:

```bash
gsigner sr25519 keyring generate --memory
```
#### Import key from SURI

Import well-known development accounts:

```bash
gsigner sr25519 keyring import -u "//Alice"
gsigner sr25519 keyring import -u "//Bob"
```

Import with derivation path:

```bash
gsigner sr25519 keyring import -u "//Alice//stash"
```

Import from mnemonic phrase:

```bash
gsigner sr25519 keyring import -u "bottom drive obey lake curtain smoke basket hold race lonely fit walk"
```

Import with password:

```bash
gsigner sr25519 keyring import -u "//Alice" --password "mypassword"
```

Import from hex seed:

```bash
gsigner sr25519 keyring import --seed 0x0000000000000000000000000000000000000000000000000000000000000001
```

#### Sign data

```bash
gsigner sr25519 keyring sign \
  --public-key a1b2c3... \
  --data 48656c6c6f \
  --path ./keys
```

With custom signing context:

```bash
gsigner sr25519 keyring sign \
  --public-key a1b2c3... \
  --data 48656c6c6f \
  --context "my-app-v1"
```

#### Verify signature

```bash
gsigner sr25519 verify \
  --public-key a1b2c3... \
  --data 48656c6c6f \
  --signature d4e5f6...
```

With custom signing context:

```bash
gsigner sr25519 verify \
  --public-key a1b2c3... \
  --data 48656c6c6f \
  --signature d4e5f6... \
  --context "my-app-v1"
```

#### Get SS58 address

```bash
gsigner sr25519 address --public-key a1b2c3...
```

#### List keys

```bash
gsigner sr25519 keyring list --path ./keys
```

#### Clear stored keys

```bash
gsigner sr25519 keyring clear --path ./keys
```

Erases all sr25519 keys from the target storage.

#### Show key details

```bash
gsigner sr25519 keyring show 0x... --path ./keys
```

Accepts a 32-byte hex-encoded public key. Use `--show-secret` to print the private key seed.

#### Generate vanity address

```bash
gsigner sr25519 keyring vanity \
  --name validator \
  --prefix 5F \
  --path ./keys \
  --show-secret
```

The prefix is matched against the SS58 address. Provide a password with `--password` to encrypt the stored key.

### Keyring Management (Secp256k1)

Manage on-disk JSON keystores for Ethereum-style accounts.

#### Initialise a keyring directory

```bash
gsigner secp256k1 keyring init --path ./eth-keyring
```

#### Generate and store a new key

```bash
gsigner secp256k1 keyring create \
  --path ./eth-keyring \
  --name validator
  --key-password hunter2
```

#### Import an existing key (hex or SURI/mnemonic)

```bash
gsigner secp256k1 keyring import \
  --path ./eth-keyring \
  --name deployer \
  --private-key 0xac09...
```

```bash
gsigner secp256k1 keyring import \
  --path ./eth-keyring \
  --name alice \
  --suri "//Alice" \
  --password optional
```

#### List keyring keys

```bash
gsigner secp256k1 keyring list --path ./eth-keyring
```

### Keyring Management (Ed25519)

Manage SS58-compatible ed25519 keystores alongside sr25519.

#### Initialise a keyring directory

```bash
gsigner ed25519 keyring init --path ./ed-keyring
```

#### Generate and store a new key

```bash
gsigner ed25519 keyring create \
  --path ./ed-keyring \
  --name alice
```

#### Import a key (hex seed or SURI)

```bash
gsigner ed25519 keyring import \
  --path ./ed-keyring \
  --name charlie \
  --seed 0x0123...
```

```bash
gsigner ed25519 keyring import \
  --path ./ed-keyring \
  --name bob \
  --suri //Bob \
  --password optional
```

#### List keyring keys

```bash
gsigner ed25519 keyring list --path ./ed-keyring
```

### Keyring Management (Sr25519)

The keyring feature provides polkadot-js compatible encrypted key storage.

#### Create a new keyring

```bash
gsigner sr25519 keyring init --path ./my-keyring
```

#### Generate vanity address

Generate a key with a specific SS58 prefix (use an empty prefix to create a regular key with a custom name):

```bash
gsigner sr25519 keyring vanity \
  --path ./my-keyring \
  --name mykey \
  --prefix "5Abc"
```

**Note**: Vanity generation can take time depending on the prefix complexity.

#### List keyring keys

```bash
gsigner sr25519 keyring list --path ./my-keyring
```
## Examples

### Complete Secp256k1 Workflow

```bash
# Generate a key
gsigner secp256k1 keyring generate --path ./keys --key-password hunter2

# Output:
# ✓ Generated secp256k1 keypair
#   Public key: 0x03ff1bce2f0dfb62c173347c8fa6e1603c6e55d8f0d22091d1660bf2b70d6aa08d
#   Address: 0xbaa1d9c431593fdda5e1d860696ae67f21a607ca

# Sign some data
gsigner secp256k1 keyring sign \
  --public-key 0x03ff1bce2f0dfb62c173347c8fa6e1603c6e55d8f0d22091d1660bf2b70d6aa08d \
  --data 48656c6c6f \
  --path ./keys \
  --key-password hunter2

# Verify the signature
gsigner secp256k1 verify \
  --public-key 0x03ff1bce2f0dfb62c173347c8fa6e1603c6e55d8f0d22091d1660bf2b70d6aa08d \
  --data 48656c6c6f \
  --signature <signature-from-previous-step>
```

### Complete Sr25519 Keyring Workflow

```bash
# Create keyring
gsigner sr25519 keyring init --path ./my-keyring

# Add a named key
gsigner sr25519 keyring create \
  --path ./my-keyring \
  --name alice \
  --key-password secret

# Output:
# ✓ Added key 'alice'
#   Public key: e64d599204740c95a288a2de9814f2085b2e3a631d2e116101f29aa64a676b79
#   SS58 Address: Em4LQQZi4Zb82mVU1e26svnjmARNzHFKKChrhwDBV4A6CdML
#   Keystore: alice

# List all keys
gsigner sr25519 keyring list --path ./my-keyring

# Generate a vanity address
gsigner sr25519 keyring vanity \
  --path ./my-keyring \
  --name vanity \
  --prefix "5GrwvaEF" \
  --key-password secret
```

## Data Format

- **Hex data**: All data inputs should be hex-encoded without the `0x` prefix (e.g., `48656c6c6f` for "Hello")
- **Public keys**:
  - Secp256k1: 33 bytes compressed format with `0x` prefix
  - Ed25519/Sr25519: 32 bytes hex-encoded
- **Signatures**:
  - Secp256k1: 65 bytes (r, s, v format)
  - Ed25519/Sr25519: 64 bytes
- **Addresses**:
  - Ethereum: 20 bytes with `0x` prefix
  - Substrate: SS58 encoded string

## Security Notes

1. **Key Storage**: Keys stored in filesystem use JSON keyring files. Supply `--key-password` to encrypt any scheme; without it the keystore remains plaintext on disk.
2. **Memory Storage**: Pass `--memory` to keep keys in RAM only. Omit both `--memory` and `--path` to default to the per-scheme data directory on disk.
3. **Passwords**: When using password encryption, ensure strong passwords for production use.
4. **Vanity Generation**: Be cautious with long prefixes as generation time increases exponentially.

## Features

The CLI requires the `cli` feature to be enabled:

```bash
cargo build --features cli
```

To build with specific signature schemes:

```bash
# Only secp256k1
cargo build --features "cli,secp256k1"

# Only sr25519
cargo build --features "cli,sr25519"

# Both (default)
cargo build --features cli
```
