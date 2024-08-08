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
$ forge script script/Deployment.s.sol:DeploymentScript --rpc-url $SEPOLIA_RPC_URL --broadcast --verify -vvvv
$ forge script script/Deployment.s.sol:DeploymentScript --rpc-url $HOLESKY_RPC_URL --broadcast --verify -vvvv
```

### Upgrade

> [!WARNING]  
> Before you run upgrade scripts, edit them depending on how you want to perform upgrade!

```shell
$ source .env

$ forge script upgrades/Program.s.sol:ProgramScript --rpc-url $SEPOLIA_RPC_URL --broadcast --verify -vvvv
$ forge script upgrades/Program.s.sol:ProgramScript --rpc-url $HOLESKY_RPC_URL --broadcast --verify -vvvv

$ forge script upgrades/Router.s.sol:RouterScript --rpc-url $SEPOLIA_RPC_URL --broadcast --verify -vvvv
$ forge script upgrades/Router.s.sol:RouterScript --rpc-url $HOLESKY_RPC_URL --broadcast --verify -vvvv

$ forge script upgrades/WrappedVara.s.sol:WrappedVaraScript --rpc-url $SEPOLIA_RPC_URL --broadcast --verify -vvvv
$ forge script upgrades/WrappedVara.s.sol:WrappedVaraScript --rpc-url $HOLESKY_RPC_URL --broadcast --verify -vvvv
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
