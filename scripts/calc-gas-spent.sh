#!/bin/sh
#
# Prerequisites:
#
#     RUST_LOG=gwasm=debug,pallet_gear=debug cargo run -p gear-node -- --dev --tmp -l0
#
# Then upload the PING program and copy it's ID.
# URL: https://github.com/gear-tech/apps/releases/download/build/demo_ping.opt.wasm
#
# Usage:
#
#     ./calc-gas-spent.sh 0x<HEX-PROGRAM-ID>
#

ALICE="5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY"
PROG_ID="$1"
PING="0x50494e47"

set -e

echo "Init message:"
curl http://localhost:9933 -H "Content-Type:application/json;charset=utf-8" -d "{
    \"jsonrpc\":\"2.0\",
    \"id\":1,
    \"method\":\"gear_getGasSpent\",
    \"params\": [\"$ALICE\",
        \"$PROG_ID\",
        \"$PING\", \"init\"]
    }"

echo
echo "Handle message:"
curl http://localhost:9933 -H "Content-Type:application/json;charset=utf-8" -d "{
    \"jsonrpc\":\"2.0\",
    \"id\":2,
    \"method\":\"gear_getGasSpent\",
    \"params\": [\"$ALICE\",
        \"$PROG_ID\",
        \"$PING\", \"handle\"]
    }"

echo
echo "Reply message:"
curl http://localhost:9933 -H "Content-Type:application/json;charset=utf-8" -d "{
    \"jsonrpc\":\"2.0\",
    \"id\":3,
    \"method\":\"gear_getGasSpent\",
    \"params\": [\"$ALICE\",
        \"$PROG_ID\",
        \"$PING\", \"handle_reply\"]
    }"
