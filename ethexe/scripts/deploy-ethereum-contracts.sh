#!/usr/bin/env bash

if [ $# -lt 1 ]; then
  echo "Usage: $0 <RPC_URL>"
  exit 1
fi

VERIFY=""

if [ -n "$ETHERSCAN_API_KEY" ]; then
  VERIFY="--verify"
fi

set -ex

RPC_URL="$1"

forge clean

forge script script/Deployment.s.sol:DeploymentScript --rpc-url "$RPC_URL" --broadcast $VERIFY -vvvv

# Now need to update `address internal constant ROUTER` in `MirrorProxy.sol` and `MirrorProxySmall.sol`
# to address obtained during deployment (used to verify contracts created by Router)!
# It is also useful to display the Router address and the WrappedVara address.

CHAIN_ID=$(cast chain-id --rpc-url "$RPC_URL")
BROADCAST_PATH="broadcast/Deployment.s.sol/$CHAIN_ID/run-latest.json"
ROUTER_ADDRESS=$(cat "$BROADCAST_PATH" | jq '.transactions[] | select(.contractName == "Router") | .contractAddress' | tr -d '"')
WVARA_ADDRESS=$(cat "$BROADCAST_PATH" | jq '.transactions[] | select(.contractName == "WrappedVara") | .contractAddress' | tr -d '"')
ROUTER_PROXY_ADDRESS=$(cat "$BROADCAST_PATH" |
  jq ".transactions[] | \
  select(.contractName == \"TransparentUpgradeableProxy\") | \
  select(.transactionType == \"CREATE\") | \
  select(.arguments[] | ascii_downcase | contains(\"${ROUTER_ADDRESS}\")) | \
  .contractAddress" |
  tr -d '"' |
  cast to-check-sum-address
)
WVARA_PROXY_ADDRESS=$(cat "$BROADCAST_PATH" |
  jq ".transactions[] | \
  select(.contractName == \"TransparentUpgradeableProxy\") | \
  select(.transactionType == \"CREATE\") | \
  select(.arguments[] | ascii_downcase | contains(\"${WVARA_ADDRESS}\")) | \
  .contractAddress" |
  tr -d '"' |
  cast to-check-sum-address
)
echo "Router: $ROUTER_PROXY_ADDRESS"
echo "WrappedVara: $WVARA_PROXY_ADDRESS"

# Separate script calls `IRouter(routerAddress).lookupGenesisHash()` to simplify deployment without using `--slow` for `Deployment.s.sol`.

ROUTER_ADDRESS="$ROUTER_PROXY_ADDRESS" forge script script/LookupGenesisHash.s.sol:LookupGenesisHashScript --slow --rpc-url "$RPC_URL" --broadcast $VERIFY -vvvv

# Now it's necessary to verify contacts so that users can click "Read/Write as Proxy" on etherscan.

if [ -n "$ETHERSCAN_API_KEY" ]; then
  curl \
    --data "address=$ROUTER_PROXY_ADDRESS" \
    --data "expectedimplementation=$ROUTER_ADDRESS" \
    "https://api.etherscan.io/v2/api?chainid=$CHAIN_ID&module=contract&action=verifyproxycontract&apikey=$ETHERSCAN_API_KEY"
  curl \
    --data "address=$WVARA_PROXY_ADDRESS" \
    --data "expectedimplementation=$WVARA_ADDRESS" \
    "https://api.etherscan.io/v2/api?chainid=$CHAIN_ID&module=contract&action=verifyproxycontract&apikey=$ETHERSCAN_API_KEY"

  # We also need to upload the MirrorProxy and MirrorProxySmall contracts
  # at least once to etherscan so that the Mirror creations by Router are shown as verified.

  sed -i "s/0xEeeeeEeeeEeEeeEeEeEeeEEEeeeeEeeeeeeeEEeE/${ROUTER_PROXY_ADDRESS}/" src/MirrorProxy.sol src/MirrorProxySmall.sol

  forge script script/MirrorProxy.s.sol:MirrorProxyScript --rpc-url "$RPC_URL" --broadcast $VERIFY -vvvv
  forge script script/MirrorProxySmall.s.sol:MirrorProxySmallScript --rpc-url "$RPC_URL" --broadcast $VERIFY -vvvv

  # Cleaning up unused/dirty files.

  git checkout src/MirrorProxy.sol src/MirrorProxySmall.sol
fi

# Cleaning up unused/dirty files.

rm -rf broadcast
