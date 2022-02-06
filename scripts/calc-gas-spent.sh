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
#     ./calc-gas-spent.sh
#

ALICE="0xd43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d"
PROG_ID="$ALICE" # Replace it by the real program ID
MSG_ID="$ALICE" # Replace it by the real message ID
PAYLOAD="0x50494e47" # "PING"

set -e

echo "Init message:"
curl http://localhost:9933 -H "Content-Type:application/json;charset=utf-8" -d "{
    \"jsonrpc\":\"2.0\",
    \"id\":1,
    \"method\":\"gear_getGasSpent\",
    \"params\": [
        \"$ALICE\",
        { \"Init\": [1,2,3,4] },
        \"$PAYLOAD\"]
    }"

echo
echo "Handle message:"
curl http://localhost:9933 -H "Content-Type:application/json;charset=utf-8" -d "{
    \"jsonrpc\":\"2.0\",
    \"id\":2,
    \"method\":\"gear_getGasSpent\",
    \"params\": [
        \"$ALICE\",
        { \"Handle\": \"$PROG_ID\" },
        \"$PAYLOAD\"]
    }"

echo
echo "Reply message:"
curl http://localhost:9933 -H "Content-Type:application/json;charset=utf-8" -d "{
    \"jsonrpc\":\"2.0\",
    \"id\":3,
    \"method\":\"gear_getGasSpent\",
    \"params\": [
        \"$ALICE\",
        { \"Reply\": [\"$MSG_ID\",0] },
        \"$PAYLOAD\"]
    }"
