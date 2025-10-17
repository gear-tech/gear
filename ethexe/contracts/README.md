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

$ forge script script/Deployment.s.sol:DeploymentScript --slow --rpc-url $MAINNET_RPC_URL --broadcast --verify -vvvv
$ forge script script/Deployment.s.sol:DeploymentScript --slow --rpc-url $SEPOLIA_RPC_URL --broadcast --verify -vvvv
$ forge script script/Deployment.s.sol:DeploymentScript --slow --rpc-url $HOLESKY_RPC_URL --broadcast --verify -vvvv
$ forge script script/Deployment.s.sol:DeploymentScript --slow --rpc-url $HOODI_RPC_URL --broadcast --verify -vvvv

# Now need to update `address internal constant ROUTER` in `MirrorProxy.sol` and `MirrorProxySmall.sol`
# to address obtained during deployment (used to verify contracts created by Router)!

$ BROADCAST_PATH="broadcast/Deployment.s.sol/$(cast chain-id --rpc-url $HOODI_RPC_URL)/run-latest.json"
$ ROUTER_ADDRESS=$(cat $BROADCAST_PATH | jq '.transactions[] | select(.contractName == "Router") | .contractAddress' | tr -d '"')
$ PROXY_ADDRESS=$(cat $BROADCAST_PATH | 
  jq ".transactions[] | \
  select(.contractName == \"TransparentUpgradeableProxy\") | \
  select(.transactionType == \"CREATE\") | \
  select(.arguments[] | ascii_downcase | contains(\"${ROUTER_ADDRESS}\")) | \
  .contractAddress" | 
  tr -d '"' |
  cast to-check-sum-address
)
$ echo "Router: $PROXY_ADDRESS"
$ sed -i "s/0xEeeeeEeeeEeEeeEeEeEeeEEEeeeeEeeeeeeeEEeE/${PROXY_ADDRESS}/" src/MirrorProxy.sol src/MirrorProxySmall.sol

$ forge script script/MirrorProxy.s.sol:MirrorProxyScript --slow --rpc-url $MAINNET_RPC_URL --broadcast --verify -vvvv
$ forge script script/MirrorProxy.s.sol:MirrorProxyScript --slow --rpc-url $SEPOLIA_RPC_URL --broadcast --verify -vvvv
$ forge script script/MirrorProxy.s.sol:MirrorProxyScript --slow --rpc-url $HOLESKY_RPC_URL --broadcast --verify -vvvv
$ forge script script/MirrorProxy.s.sol:MirrorProxyScript --slow --rpc-url $HOODI_RPC_URL --broadcast --verify -vvvv

$ forge script script/MirrorProxySmall.s.sol:MirrorProxySmallScript --slow --rpc-url $MAINNET_RPC_URL --broadcast --verify -vvvv
$ forge script script/MirrorProxySmall.s.sol:MirrorProxySmallScript --slow --rpc-url $SEPOLIA_RPC_URL --broadcast --verify -vvvv
$ forge script script/MirrorProxySmall.s.sol:MirrorProxySmallScript --slow --rpc-url $HOLESKY_RPC_URL --broadcast --verify -vvvv
$ forge script script/MirrorProxySmall.s.sol:MirrorProxySmallScript --slow --rpc-url $HOODI_RPC_URL --broadcast --verify -vvvv

$ rm -rf broadcast
$ git checkout src/MirrorProxy.sol src/MirrorProxySmall.sol
```

_Notes:_

_- If environment variable `DEV_MODE` is set to `true` than `DeploymentScript` skips Middleware deployment_

### Upgrade

> [!WARNING]
> Before you run upgrade scripts, edit `reinitialize` method depending on how you want to perform upgrade!

```shell
$ source .env

$ forge script upgrades/Mirror.s.sol:MirrorScript --slow --rpc-url $MAINNET_RPC_URL --broadcast --verify -vvvv
$ forge script upgrades/Mirror.s.sol:MirrorScript --slow --rpc-url $SEPOLIA_RPC_URL --broadcast --verify -vvvv
$ forge script upgrades/Mirror.s.sol:MirrorScript --slow --rpc-url $HOLESKY_RPC_URL --broadcast --verify -vvvv
$ forge script upgrades/Mirror.s.sol:MirrorScript --slow --rpc-url $HOODI_RPC_URL --broadcast --verify -vvvv

$ forge script upgrades/Router.s.sol:RouterScript --slow --rpc-url $MAINNET_RPC_URL --broadcast --verify -vvvv
$ forge script upgrades/Router.s.sol:RouterScript --slow --rpc-url $SEPOLIA_RPC_URL --broadcast --verify -vvvv
$ forge script upgrades/Router.s.sol:RouterScript --slow --rpc-url $HOLESKY_RPC_URL --broadcast --verify -vvvv
$ forge script upgrades/Router.s.sol:RouterScript --slow --rpc-url $HOODI_RPC_URL --broadcast --verify -vvvv

$ forge script upgrades/WrappedVara.s.sol:WrappedVaraScript --slow --rpc-url $MAINNET_RPC_URL --broadcast --verify -vvvv
$ forge script upgrades/WrappedVara.s.sol:WrappedVaraScript --slow --rpc-url $SEPOLIA_RPC_URL --broadcast --verify -vvvv
$ forge script upgrades/WrappedVara.s.sol:WrappedVaraScript --slow --rpc-url $HOLESKY_RPC_URL --broadcast --verify -vvvv
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
