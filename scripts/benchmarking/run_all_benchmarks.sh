#!/usr/bin/env bash

# This script has three parts which all use the Substrate runtime:
# - Pallet benchmarking to update the pallet weights
# - Overhead benchmarking for the Extrinsic and Block weights
# - Machine benchmarking
#
# Should be run on a reference machine to gain accurate benchmarks
# current reference machine: https://github.com/paritytech/substrate/pull/5848
#
# Should be run from the root of the repo.

while getopts 'bfps:c:v' flag; do
  case "${flag}" in
    b)
      # Skip build.
      skip_build='true'
      ;;
    c)
      # Which chain spec to use.
      chain_spec="${OPTARG}"
      ;;
    f)
      # Fail if any sub-command in a pipe fails, not just the last one.
      set -o pipefail
      # Fail on undeclared variables.
      set -u
      # Fail if any sub-command fails.
      set -e
      # Fail on traps.
      set -E
      ;;
    p)
      # Start at pallet
      start_pallet="${OPTARG}"
      ;;
    s)
      # Storage snapshot url
      storage_folder="${OPTARG}"
      ;;
    v)
      # Echo all executed commands.
      set -x
      ;;
    *)
      # Exit early.
      echo "Bad options. Check Script."
      exit 1
      ;;
  esac
done


if [ "$skip_build" != true ]
then
  echo "[+] Compiling Gear benchmarks..."
  cargo build --profile=production --locked --features=runtime-benchmarks
fi

# The executable to use.
GEAR=./target/production/gear

# Manually exclude some pallets.
EXCLUDED_PALLETS=(
)

# Load all pallet names in an array.
ALL_PALLETS=($(
  $GEAR benchmark pallet --list --chain=$chain_spec |\
    tail -n+2 |\
    cut -d',' -f1 |\
    sort |\
    uniq
))

# Filter out the excluded pallets by concatenating the arrays and discarding duplicates.
PALLETS=($({ printf '%s\n' "${ALL_PALLETS[@]}" "${EXCLUDED_PALLETS[@]}"; } | sort | uniq -u))

echo "[+] Benchmarking ${#PALLETS[@]} Gear pallets by excluding ${#EXCLUDED_PALLETS[@]} from ${#ALL_PALLETS[@]}."

# Define the error file.
ERR_FILE="scripts/benchmarking/benchmarking_errors.txt"
# Delete the error file before each run.
rm -f $ERR_FILE

WEIGHTS_OUTPUT="scripts/benchmarking/weights-output"
# Delete the weights output folders before each run.
rm -R ${WEIGHTS_OUTPUT}
# Create the weights output folders.
mkdir ${WEIGHTS_OUTPUT}

STORAGE_OUTPUT="scripts/benchmarking/rocksdb_weights.rs"
rm -f ${STORAGE_OUTPUT}

MACHINE_OUTPUT="scripts/benchmarking/machine_benchmark_result.txt"
rm -f $MACHINE_OUTPUT

# Benchmark each pallet.
for PALLET in "${PALLETS[@]}"; do
  # If `-p` is used, skip benchmarks until the start pallet.
  if [ ! -z "$start_pallet" ] && [ "$start_pallet" != "$PALLET" ]
  then
    echo "[+] Skipping ${PALLET}..."
    continue
  else
    unset start_pallet
  fi

  FOLDER="$(echo "${PALLET#*_}" | tr '_' '-')";
  WEIGHT_FILE="./${WEIGHTS_OUTPUT}/${PALLET}.rs"
  echo "[+] Benchmarking $PALLET with weight file $WEIGHT_FILE";

  OUTPUT=$(
    $GEAR benchmark pallet \
    --chain=$chain_spec \
    --steps=50 \
    --repeat=20 \
    --pallet="$PALLET" \
    --extrinsic="*" \
    --execution=wasm \
    --wasm-execution=compiled \
    --heap-pages=4096 \
    --output="$WEIGHT_FILE" \
    --template=.maintain/frame-weight-template.hbs 2>&1
  )
  if [ $? -ne 0 ]; then
    echo "$OUTPUT" >> "$ERR_FILE"
    echo "[-] Failed to benchmark $PALLET. Error written to $ERR_FILE; continuing..."
  fi
done

echo "[+] Benchmarking the machine..."
OUTPUT=$(
  $GEAR benchmark machine --chain=$chain_spec --allow-fail 2>&1
)
# In any case don't write errors to the error file since they're not benchmarking errors.
echo "[x] Machine benchmark:\n$OUTPUT"
echo $OUTPUT >> $MACHINE_OUTPUT

# If `-s` is used, run the storage benchmark.
if [ ! -z "$storage_folder" ]; then
  OUTPUT=$(
  $GEAR benchmark storage \
    --chain=$chain_spec \
    --state-version=1 \
    --warmups=10 \
    --base-path=$storage_folder \
    --weight-path=./$STORAGE_OUTPUT 2>&1
  )
  if [ $? -ne 0 ]; then
    echo "$OUTPUT" >> "$ERR_FILE"
    echo "[-] Failed the storage benchmark. Error written to $ERR_FILE; continuing..."
  fi
else
  unset storage_folder
fi

# Check if the error file exists.
if [ -f "$ERR_FILE" ]; then
  echo "[-] Some benchmarks failed. See: $ERR_FILE"
  exit 1
else
  echo "[+] All benchmarks passed."
  exit 0
fi
