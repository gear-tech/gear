#!/bin/sh
#
# Prerequisites:
#
#     RUST_LOG=gwasm=debug,pallet_gear=debug cargo run -p gear-cli --release -- --dev --tmp -l0
#
# Usage:
#
#     ./read-state.sh
#

SELF="$0"
ROOT_DIR="$(cd "$(dirname "$SELF")"/.. && pwd)"
SCRIPTS="$ROOT_DIR/scripts"

. "$SCRIPTS"/metawasm.sh

PROGRAM_ID="0xaa2ee698eda4df80c98eb85c3be65c27d1154f096af68f2d9771ce0b99dfd0c9"
ALL_WALLETS="0x616c6c5f77616c6c657473"
SPECIFIC_WALLET="0x73706563696669635f77616c6c6574"
ARGUMENT="0x01000000000000000401"

set -e

echo "Read metahash:"
curl http://localhost:9933 -H "Content-Type:application/json;charset=utf-8" -d "{
    \"jsonrpc\":\"2.0\",
    \"id\":1,
    \"method\":\"gear_readMetahash\",
    \"params\": [\"$PROGRAM_ID\"]
}"
echo
echo

echo "Read state:"
curl http://localhost:9933 -H "Content-Type:application/json;charset=utf-8" -d "{
    \"jsonrpc\":\"2.0\",
    \"id\":1,
    \"method\":\"gear_readState\",
    \"params\": [\"$PROGRAM_ID\"]
}"
echo
echo

echo "Read state using wasm (fn all_wallets):"
curl http://localhost:9933 -H "Content-Type:application/json;charset=utf-8" -d "{
    \"jsonrpc\":\"2.0\",
    \"id\":1,
    \"method\":\"gear_readStateUsingWasm\",
    \"params\": [\"$PROGRAM_ID\", \"$ALL_WALLETS\", \"$BINARY\"]
}"
echo
echo

echo "Read state using wasm (fn specific_wallet):"
curl http://localhost:9933 -H "Content-Type:application/json;charset=utf-8" -d "{
    \"jsonrpc\":\"2.0\",
    \"id\":1,
    \"method\":\"gear_readStateUsingWasm\",
    \"params\": [\"$PROGRAM_ID\", \"$SPECIFIC_WALLET\", \"$BINARY\", \"$ARGUMENT\"]
}"
echo
echo
