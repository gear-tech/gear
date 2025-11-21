## Foundry

**Foundry is a blazing fast, portable and modular toolkit for Ethereum application development written in Rust.**

Foundry consists of:

- **Forge**: Ethereum testing framework (like Truffle, Hardhat and DappTools).
- **Cast**: Swiss army knife for interacting with EVM smart contracts, sending transactions and getting chain data.
- **Anvil**: Local Ethereum node, akin to Ganache, Hardhat Network.
- **Chisel**: Fast, utilitarian, and verbose solidity REPL.

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

# When running local node, execute `unset ETHERSCAN_API_KEY` to skip verification
$ unset ETHERSCAN_API_KEY
$ ../scripts/deploy-ethereum-contracts.sh $LOCAL_RPC_URL

# When deploying to network, execute `export ETHERSCAN_API_KEY=$ETHERSCAN_API_KEY`
$ export ETHERSCAN_API_KEY=$ETHERSCAN_API_KEY
$ ../scripts/deploy-ethereum-contracts.sh $MAINNET_RPC_URL
$ ../scripts/deploy-ethereum-contracts.sh $HOODI_RPC_URL
```

_Notes:_

_- If environment variable `DEV_MODE` is set to `true` than `DeploymentScript` skips Middleware deployment_

### Upgrade

> [!WARNING]
> Before you run upgrade scripts, edit `reinitialize` method depending on how you want to perform upgrade!

```shell
$ source .env

$ forge script upgrades/Mirror.s.sol:MirrorScript --slow --rpc-url $MAINNET_RPC_URL --broadcast --verify -vvvv
$ forge script upgrades/Mirror.s.sol:MirrorScript --slow --rpc-url $HOODI_RPC_URL --broadcast --verify -vvvv

$ forge script upgrades/Router.s.sol:RouterScript --slow --rpc-url $MAINNET_RPC_URL --broadcast --verify -vvvv
$ forge script upgrades/Router.s.sol:RouterScript --slow --rpc-url $HOODI_RPC_URL --broadcast --verify -vvvv

$ forge script upgrades/WrappedVara.s.sol:WrappedVaraScript --slow --rpc-url $MAINNET_RPC_URL --broadcast --verify -vvvv
$ forge script upgrades/WrappedVara.s.sol:WrappedVaraScript --slow --rpc-url $HOODI_RPC_URL --broadcast --verify -vvvv
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
