#!/usr/bin/env bash

set -e

SCRIPTS_BENCHMARKING_DIR="$(cd "$(dirname "$0")" && pwd)"
WEIGHTS_OUTPUT_DIR="$SCRIPTS_BENCHMARKING_DIR/weights-output"
PALLETS_DIR="$(cd "$SCRIPTS_BENCHMARKING_DIR/../.." && pwd)/pallets"

if [ -d "$WEIGHTS_OUTPUT_DIR" ]; then
    echo "[+] Applying weights from $WEIGHTS_OUTPUT_DIR"
else
    echo "[-] Weights output directory wasn't found: $WEIGHTS_OUTPUT_DIR"
    echo "[-] Make sure to:"
    echo "1. Install benchmarking artifact from GitHub Actions"
    echo "2. Unpack them using 'unzip -o weights-output.zip'"
    echo "3. Move them to weights output directory"
    exit 1
fi

# This will fail on MacOS, if it's not homebrew's gsed.
# To fix it, install gnu-sed or add '' after -i.
echo "[+] Changing trait paths for gear pallets"
sed -i 's/pallet_gear[[:alnum:]_]*::WeightInfo for SubstrateWeight/WeightInfo for SubstrateWeight/' "$WEIGHTS_OUTPUT_DIR"/*

echo "[+] Moving gear pallets weights to their respective directories"
for f in "$WEIGHTS_OUTPUT_DIR"/pallet_gear*; do
    base=$(basename "$f" .rs)
    name=${base#pallet_}
    name=${name//_/-}
    mv "$f" "$PALLETS_DIR/$name/src/weight.rs"
done

echo "[+] Moving substrate pallets weights to their respective directories"
mv "$WEIGHTS_OUTPUT_DIR"/* "$SCRIPTS_BENCHMARKING_DIR/../../runtime/vara/src/weights"

echo "[+] Done applying weights. Removing (empty) weights-output directory"
rm -r "$WEIGHTS_OUTPUT_DIR"

echo "[+] Making dump for gear-core"
"$SCRIPTS_BENCHMARKING_DIR/../weight-dump.sh"
