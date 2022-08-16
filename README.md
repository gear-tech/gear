# gear-program

[![CI][ci1]][ci2]
[![crates.io][c1]][c2]
[![docs][docs1]][docs2]
[![downloads][d1]][d2]
[![License][l1]][l2]

[c1]: https://img.shields.io/crates/v/gear-program.svg
[c2]: https://crates.io/crates/gear-program

[ci1]: https://github.com/clearloop/gear-program/workflows/CI/badge.svg
[ci2]: https://github.com/clearloop/gear-program/actions/workflows/CI.yaml

[docs1]: https://img.shields.io/badge/current-docs-brightgreen.svg
[docs2]: https://docs.rs/gear-program/

[d1]: https://img.shields.io/crates/d/gear-program.svg
[d2]: https://crates.io/crates/gear-program

[l1]: https://img.shields.io/badge/License-GPL%203.0-success
[l2]: https://github.com/clearloop/gear-program/blob/master/LICENSE


## Getting Started

To install gear-program via <kbd>cargo</kbd>

```sh
$ cargo install gear-program
```

Usages:

```sh
$ gear 
gear-program 0.1.1

USAGE:
    gear [FLAGS] [OPTIONS] <SUBCOMMAND>

FLAGS:
    -d, --debug      Enable debug logs
    -h, --help       Prints help information
    -V, --version    Prints version information

OPTIONS:
    -e, --endpoint <endpoint>    Gear node rpc endpoint
    -p, --passwd <passwd>        Password of the signer account

SUBCOMMANDS:
    claim       Claim value from mailbox
    deploy      Deploy program to gear node
    help        Prints this message or the help of the given subcommand(s)
    info        Get account info from ss58address
    login       Log in to account
    meta        Show metadata structure, read types from registry, etc
    new         Create a new gear program
    program     Read program state, etc
    reply       Sends a reply message
    send        Sends a message to a program or to another account
    submit      Saves program `code` in storage
    transfer    Transfer value
    update      Update resources
```

Now, let's create a <kbd>new</kbd> gear program and deploy it to the staging testnet.

```
$ gear new hello-world
Cloning into '/home/clearloop/.gear/apps'...
remote: Enumerating objects: 156, done.
remote: Counting objects: 100% (156/156), done.
remote: Compressing objects: 100% (121/121), done.
remote: Total 156 (delta 41), reused 83 (delta 15), pack-reused 0
Receiving objects: 100% (156/156), 89.78 KiB | 723.00 KiB/s, done.
Resolving deltas: 100% (41/41), done.
Successfully created registry at /home/clearloop/.gear/apps!
Successfully created hello-world!
```

Compile you gear program via <kbd>cargo</kbd>

```
$ cargo build --manifest-path hello-world/Cargo.toml --release
```

<kbd>login</kbd> to your gear account

```
$ gear login //Alice
Successfully logged in as 5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY!
```

<kbd>deploy</kbd> your gear program

```
$ gear deploy hello-world/target/wasm32-unknown-unknown/release/hello_world.wasm
Submited call Gear::submit_program
        Status: Broadcast( ["12D3KooWRGfgDJMYJm6zZeQyhum1gdCjQNpx2KXiK7MMF68Q4dZY", "12D3KooWCrVbKnk9NbQJT8Frs8jen4WwN12bwFN13XfMVAw7PVpN", "12D3KooWESKNqBMzP5in5h211zxUdVBT1Yr2tfxD7wjpcfaR3BF8", "12D3KooWMaNVy5CcU8UBvTaaNYTqSRc2inrGVLnw9g4PQYiHUQAr", "12D3KooWQi1oRarMGEuXjWLLETErrowbHB4fPZn9HDk6rjeMa1ch", "12D3KooWAiHom6TQ7NYcQyd5gNyqsTgXGdfpKXorSxxcSyBGni17", "12D3KooWCszCnbfkZLFpwFmMpeSN383oMftdYrmcNyucXth99soB", "12D3KooWAdgN6TL7qeEKDdquonezGGg6EsRKofYuLqKfJV3dRWuc", "12D3KooWAL8TCjDAYuLqrowXDxHpbHvtERY43wJ3pJnUPNSJtPyW", "12D3KooWDa6MYSbgqwcypZzaWkpFU1VqLGb8xrZZdj68nYJnm1zr", "12D3KooWEcxU1S9TB7o5U85LeQbrQvwvT68AxKyiRHEU93EfEatU", "12D3KooWBF3qedMHwiP4XS2eRVV7g6ffsZpgXRc7L48ziRmexPoa", "12D3KooWHvSKfBzLJsAih3YfxrBVPxUTzJHuhVbHEVkapgY4JqXW", "12D3KooWHWgttEPxZgap2HzRa4X5kcSxBdhXprA4LY5x6pdRKrXj", "12D3KooWNEd5b23pMndC9xK11v7CNoRquh1yeXAFKLYsix5D8gD3", "12D3KooWKEhJTAg9vP42QUGV81LAaYGmfMTiDxE7DiQivPXZ185R", "12D3KooWACXR3rAh6cmEw6aWaCi2o1bpT3NeRwf4jgRcbnhaMMQK", "12D3KooWAoCGLbzx8UCuSHm1xhF1bpJVJTftvKM3cr24yngcpW7B", "12D3KooWPhQWEavGFYhCGDTMR2NADmKp3fgNXQ9js6H5fpr8vYUb", "12D3KooWP7W7jDMWJyT7BAeajL9j21Hmn2M5vpp7YUsNAqkjJVZD", "12D3KooWBkMbLRBsCUGVpr3CVrrjB8AuFXkF32UbGKUqc5ejGuW1", "12D3KooWCfm5T1uRq3rCNzqjsq3eFwu8fjHkUo1XBZmZvo611BwA", "12D3KooWNsqtRGKLFWiNfLqddE1jzfc9nxD83YirDzVXyLwKgQqP", "12D3KooWJK2qCja1xWMhZAGUqTELoVqYGyJt5WL7eSdeZw7agTh3", "12D3KooWH7cbomg41DuVqFBbcj1Yx6dJL5wicFUGgkK4XBzCQpmh", "12D3KooWEhsupQWc43mRo9t2bA9rymhC3NuUwbgFDhmBGU867GBz", "12D3KooWMxTvswYJB4swSJxukkWM6orJGcRi21DMCuWYb4PVGD4R", "12D3KooWLiRcF7hZnL1aqXNeiZiSNa7KaC4xGwsijBwRUi23GwRv", "12D3KooWQKV4rRCkLJuMnq6aWnknhr2ybS6T5kGMsqho2kpJMqJ1", "12D3KooWFejqJdDE9p5A7BXnhACFuRZNvaFym68fghN6qE3k1MUv", "12D3KooWGBV5YKikNV7JErmcvMhESoJnE8oZK1SBzFnm2osQviXG", "12D3KooWBW6F949rHu7NHfbCWNHqaxmrsvPZPURbWewkJA1gvGqB", "12D3KooWJXseuSYHydHPNmRGMnwiXpCxPpZTyYJkcwjMQDpDFyVa", "12D3KooWE7E8e4JozpnnLFFF4XFDLSm4Ww8kt6VNSfFvr5oCP11y", "12D3KooWLqJhjqbjrvzUxTtGNMg61LbZgQ6a5E59xpBRagQyHYsw", "12D3KooWQV1CZAT28vPvMzEYUvbBB9kgj3UM12BgaBC51UnEpMpG", "12D3KooWGoBi8tH4CqYEmpEYP5iBSCfXFKC5fhnGB8WnCC524o9X", "12D3KooWKRfahWtJQ73AeAyFkjVYa1k35rFpmk9aR4XtsiMTjF4s", "12D3KooWRFJWri1m5n3nYgsLEwwq8mSmGDrcY5o68ksXx3KJWPKn", "12D3KooWKPvLRDRbphK3bwHDCKPMrrLoFKYBsSbrYb7LhAYow3cB", "12D3KooWHcnYy1nAAtE4iSDX23oJexCMYs5z4RRct3MMoJLuuitg", "12D3KooWG5G3jaQ8ZVMCSTbyTRjfzhSpn55f8DGKDpFKvU4A3SPQ"] )
        Status: InBlock( block_hash: 0xde8e…9411, extrinsic_hash: 0x919c…3f92 )
        Status: Finalized( block_hash: 0xde8e…9411, extrinsic_hash: 0x919c…3f92 )
Successfully submited call Gear::submit_program 0x919c…3f92 at 0xde8e…9411!
```

## LICENSE

GPL v3.0
