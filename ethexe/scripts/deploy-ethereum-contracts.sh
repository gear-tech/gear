#!/usr/bin/env bash

if [ $# -lt 1 ]; then
  echo "Usage: $0 <RPC_URL>"
  exit 1
fi

set -ex

RPC_URL="$1"

forge clean

forge script script/Deployment.s.sol:DeploymentScript --slow --rpc-url "$RPC_URL" --broadcast --verify -vvvv

# Now need to update `address internal constant ROUTER` in `MirrorProxy.sol` and `MirrorProxySmall.sol`
# to address obtained during deployment (used to verify contracts created by Router)!

BROADCAST_PATH="broadcast/Deployment.s.sol/$(cast chain-id --rpc-url "$RPC_URL")/run-latest.json"
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
sed -i "s/0xEeeeeEeeeEeEeeEeEeEeeEEEeeeeEeeeeeeeEEeE/${ROUTER_PROXY_ADDRESS}/" src/MirrorProxy.sol src/MirrorProxySmall.sol

# Now it's necessary to verify contacts so that users can click "Read/Write as Proxy" on etherscan.

curl \
    --data "address=$ROUTER_PROXY_ADDRESS" \
    --data "expectedimplementation=$ROUTER_ADDRESS" \
    "https://api.etherscan.io/v2/api?chainid=560048&module=contract&action=verifyproxycontract&apikey=$ETHERSCAN_API_KEY"
curl \
    --data "address=$WVARA_PROXY_ADDRESS" \
    --data "expectedimplementation=$WVARA_ADDRESS" \
    "https://api.etherscan.io/v2/api?chainid=560048&module=contract&action=verifyproxycontract&apikey=$ETHERSCAN_API_KEY"

# We also need to upload the MirrorProxy and MirrorProxySmall contracts
# at least once to etherscan so that the Mirror creations by Router are shown as verified.

forge script script/MirrorProxy.s.sol:MirrorProxyScript --slow --rpc-url "$RPC_URL" --broadcast --verify -vvvv
forge script script/MirrorProxySmall.s.sol:MirrorProxySmallScript --slow --rpc-url "$RPC_URL" --broadcast --verify -vvvv

# Cleaning up unused/dirty files.

rm -rf broadcast
git checkout src/MirrorProxy.sol src/MirrorProxySmall.sol
