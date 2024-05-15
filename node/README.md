# Gear Node

Gear substrate-based node, ready for hacking :rocket:

## Building Gear node from source

### 1. Install dependencies

#### Ubuntu/Debian
```
sudo apt update
# May prompt for location information
sudo apt install -y git clang curl libssl-dev llvm libudev-dev cmake protobuf-compiler
```

#### MacOS
```
# Install Homebrew if necessary https://brew.sh/
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/master/install.sh)"

# Make sure Homebrew is up-to-date, install openssl
brew update
brew install openssl
```

Additionally, if you use Apple Silicon (M1/M1 Pro/M1 Max), install Rosetta:
```
/usr/sbin/softwareupdate --install-rosetta --agree-to-license
```

#### Windows

Windows 10 is supported with WSL!

- Install WSL and upgrade it to version 2 use instructions from https://docs.microsoft.com/en-us/windows/wsl/install-win10.
- Ensure VM feature is enabled in bios in processor advanced menu.
- Install Ubuntu 20.04 LTS https://www.microsoft.com/store/apps/9n6svws3rx71.
- Launch installed app and setup root user - exit ubuntu app (first time launch takes time).
- Install windows terminal from app store or use VSCode with remote plugin (auto suggested once wsl is detected by VSCode).
- Follow instructions for linux.

### 2. Rust and all toolchains

If Rust is not yet installed, read the [Installation](https://doc.rust-lang.org/book/ch01-01-installation.html) part from [The Book](https://doc.rust-lang.org/book/index.html) to install it.

Make sure the `wasm` target is enabled:
```bash
rustup target add wasm32-unknown-unknown
```

Set the environment variables:
```
source ~/.cargo/env
```

### 3. Build Gear node

Run the following commands to build the node:
```bash
make node-release
```

The resulting binary will be located at `./target/release/gear`.

## Running a node

To run a local dev network, execute the following command:

  ```bash
  gear --dev
  ```

The complete list of available command-line options can be found by running:

  ```bash
  gear --help
  ```

Purge of an existing dev chain state:

```bash
gear purge-chain --dev
```
