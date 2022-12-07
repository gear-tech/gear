# Gear Examples

You can write your own smart contract or try to build from examples. Let's Rock!

## Requirements

To develop your first Rust smart-contract you would have to:

- Install Rustup:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

- Add wasm target to your toolchain:

```bash
rustup target add wasm32-unknown-unknown
```

## First steps

At least 10. x `npm` and `node` versions must be installed

To create our app project use the command **cargo**:

```bash
cargo new gear-app --lib
```

The project structure is following:

    gear-app/
      ---Cargo.toml
      ---src
      ------lib.rs

`Cargo.toml` is a project manifest in Rust, it contains all metadata necessary for compiling the project.
Configure the `Cargo.toml` similarly to how it is configured [examples/ping/Cargo.toml](https://github.com/gear-tech/gear/blob/master/examples/ping/Cargo.toml);

## PING-PONG

Gear is very easy to write code for!

Here is a minimal program for a classic ping-pong contract:

```rust
use gstd::{ext, msg};

#[no_mangle]
extern "C" fn handle() {
    let new_msg = String::from_utf8(msg::load_bytes().expect("Failed to load payload"))
        .expect("Invalid message: should be utf-8");

    if &new_msg == "PING" {
        msg::send_bytes(msg::source(), b"PONG", 0);
    }
}

#[no_mangle]
extern "C" fn init() {}
```

It will just send `PONG` back to the original sender (this can be you!)

## Building Rust Contract

We should compile our smart contract in the app folder:

```bash
cargo build --target wasm32-unknown-unknown --release
```

Our application should compile successfully and the final file `target/wasm32-unknown-unknown/release/gear-app.wasm` should appear.
