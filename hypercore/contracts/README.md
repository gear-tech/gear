## Foundry

**Foundry is a blazing fast, portable and modular toolkit for Ethereum application development written in Rust.**

Foundry consists of:

-   **Forge**: Ethereum testing framework (like Truffle, Hardhat and DappTools).
-   **Cast**: Swiss army knife for interacting with EVM smart contracts, sending transactions and getting chain data.
-   **Anvil**: Local Ethereum node, akin to Ganache, Hardhat Network.
-   **Chisel**: Fast, utilitarian, and verbose solidity REPL.

## Documentation

https://book.getfoundry.sh/

## Usage

### Build

```shell
$ forge build
```

### Test

```shell
$ forge test
```

### Format

```shell
$ forge fmt
```

### Gas Snapshots

```shell
$ forge snapshot
```

### Anvil

```shell
$ anvil
```

### Deploy

```shell
$ source .env

$ forge script script/Router.s.sol:RouterScript --rpc-url $SEPOLIA_RPC_URL --broadcast --verify -vvvv
$ forge script script/Program.s.sol:ProgramScript --rpc-url $SEPOLIA_RPC_URL --broadcast --verify -vvvv

$ forge script script/Router.s.sol:RouterScript --rpc-url $HOLESKY_RPC_URL --broadcast --verify -vvvv
$ forge script script/Program.s.sol:ProgramScript --rpc-url $HOLESKY_RPC_URL --broadcast --verify -vvvv
```

### Cast

```shell
$ cast <subcommand>
```

### Help

```shell
$ forge --help
$ anvil --help
$ cast --help
```
