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

# Steps and repeats for main benchmark.
BENCHMARK_STEPS=50
BENCHMARK_REPEAT=20

# Steps and repeats for benchmarking so called "one-time extrinsics",
# which may be called only once and require a different benchmarking approach with more repeats.
BENCHMARK_STEPS_ONE_TIME_EXTRINSICS=2
BENCHMARK_REPEAT_ONE_TIME_EXTRINSICS=1000

# Get array of isolated cores from the cpuset file.
get_isolated_cores() {
    local isolated_cores=()
    local cpuset_path="/sys/devices/system/cpu/isolated"

    if [ -f "$cpuset_path" ]; then
        # Read isolated cores from the cpuset file
        readarray -t -d, isolated_cores < "$cpuset_path"
    fi

    echo "${isolated_cores[@]}"
}

# Get only first isolated core, as we don't use its sibling HT core.
ISOLATED_CORE=$(get_isolated_cores | cut -d " " -f1)

# List of one-time extrinsics to benchmark.
# They are retrieved automatically from the pallet_gear benchmarks file by their `r` component range 0..1,
# which defines them as one-time extrinsics.
mapfile -t ONE_TIME_EXTRINSICS < <(cat "pallets/gear/src/benchmarking/mod.rs" | grep "0 .. 1;" -B 1 | grep -E "{$" | awk '{print $1}')

while getopts 'bmfps:c:v' flag; do
  case "${flag}" in
    b)
      # Skip build.
      skip_build='true'
      ;;
    m)
      # Skip machine benchmark.
      skip_machine_benchmark='true'
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
      echo "Bad options. Check script."
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

# Populate TASKSET_CMD with taskset command if isolated core is set.
if [ -n "$ISOLATED_CORE" ]; then
  echo "[+] Running benches on isolated core: $ISOLATED_CORE"
  TASKSET_CMD="taskset -c $ISOLATED_CORE"
fi

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

  # Run multithreaded benchmarks (pallet_gear_builtin) on fixed 4 cores.
  if [ -n "$INSTANCE_TYPE" ] && [ "$PALLET" == "pallet_gear_builtin" ]
  then
    TASKSET_CMD="taskset -c 2,3,4,5"
    echo "[+] Running pallet_gear_builtin benches on fixed 4 cores: 2,3,4,5"
  fi

  # Get all the extrinsics for the pallet if the pallet is "pallet_gear".
  if [ "$PALLET" == "pallet_gear" ]
  then
    IFS=',' read -r -a ALL_EXTRINSICS <<< "$(IFS=',' $GEAR benchmark pallet --list \
      --chain=$chain_spec \
      --pallet="$PALLET" |\
      tail -n+2 |\
      cut -d',' -f2 |\
      sort |\
      uniq |\
      awk '{$1=$1}1' ORS=','
    )"

    # Remove the one-time extrinsics from the extrinsics array, so that they can be benchmarked separately.
    EXTRINSICS=()
    for item in "${ALL_EXTRINSICS[@]}"; do
        # Check if the item exists in ONE_TIME_EXTRINSICS array
        if ( [[ ! " ${ONE_TIME_EXTRINSICS[*]} " =~ " ${item} " ]] ); then
            # If not, add the item to the new array
            EXTRINSICS+=("$item")
        fi
    done
  else
    EXTRINSICS=("*")
  fi

  WEIGHT_FILE="./${WEIGHTS_OUTPUT}/${PALLET}.rs"
  echo "[+] Benchmarking $PALLET with weight file $WEIGHT_FILE";

  OUTPUT=$(
    $TASKSET_CMD $GEAR benchmark pallet \
    --chain="$chain_spec" \
    --steps=$BENCHMARK_STEPS \
    --repeat=$BENCHMARK_REPEAT \
    --pallet="$PALLET" \
    --extrinsic="$(IFS=, ; echo "${EXTRINSICS[*]}")" \
    --heap-pages=4096 \
    --output="$WEIGHT_FILE" \
    --template=.maintain/frame-weight-template.hbs 2>&1
  )

  if [ $? -ne 0 ]; then
    echo "$OUTPUT" >> "$ERR_FILE"
    echo "[-] Failed to benchmark $PALLET. Error written to $ERR_FILE; continuing..."
  fi

  # If the pallet is pallet_gear, benchmark the one-time extrinsics.
  if [ "$PALLET" == "pallet_gear" ]
  then
    echo "[+] Benchmarking $PALLET one-time syscalls with weight file ./${WEIGHTS_OUTPUT}/${PALLET}_onetime.rs";
    OUTPUT=$(
        $TASKSET_CMD $GEAR benchmark pallet \
        --chain="$chain_spec" \
        --steps=$BENCHMARK_STEPS_ONE_TIME_EXTRINSICS \
        --repeat=$BENCHMARK_REPEAT_ONE_TIME_EXTRINSICS \
        --pallet="$PALLET" \
        --extrinsic="$(IFS=', '; echo "${ONE_TIME_EXTRINSICS[*]}")" \
        --heap-pages=4096 \
        --output="./${WEIGHTS_OUTPUT}/${PALLET}_onetime.rs" \
        --template=.maintain/frame-weight-template.hbs 2>&1
    )

    if [ $? -ne 0 ]; then
      echo "$OUTPUT" >> "$ERR_FILE"
      echo "[-] Failed to benchmark $PALLET. Error written to $ERR_FILE; continuing..."
    fi
  fi
done

if [ "$skip_machine_benchmark" != true ]
then
  echo "[+] Benchmarking the machine..."
  OUTPUT=$(
    $TASKSET_CMD $GEAR benchmark machine --chain=$chain_spec --allow-fail 2>&1
  )
  # In any case don't write errors to the error file since they're not benchmarking errors.
  echo "[x] Machine benchmark:\n$OUTPUT"
  echo $OUTPUT >> $MACHINE_OUTPUT
fi

# If `-s` is used, run the storage benchmark.
if [ ! -z "$storage_folder" ]; then
  OUTPUT=$(
  $TASKSET_CMD $GEAR benchmark storage \
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

# Merge pallet_gear weights.
./scripts/benchmarking/merge_outputs.sh

# Check if the error file exists.
if [ -f "$ERR_FILE" ]; then
  echo "[-] Some benchmarks failed. See: $ERR_FILE"
  exit 1
else
  echo "[+] All benchmarks passed."
  exit 0
fi
