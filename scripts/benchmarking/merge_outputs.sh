#!/bin/bash

# This script merges the output of the pallet_gear benchmarking functions into
# a single file. This is necessary because the Substrate CLI outputs the weights
# as one file per run, and the run_all_benchmarks.sh script makes multiple runs
# of the pallet_gear benchmarking functions.

# Output file for the merged pallet_gear functions.
MAIN_FILE="./scripts/benchmarking/weights-output/pallet_gear.rs"

# List of pallet_gear weight files to merge.
ADDITIONAL_FILES=(
  "./scripts/benchmarking/weights-output/pallet_gear_onetime.rs"
)

# Loop through the list of pallet_gear files and merge their functions.
for FILE in "${ADDITIONAL_FILES[@]}"; do
  echo "[+] Merging outputs from $FILE into $MAIN_FILE"

  ALL_WEIGHTS=$(perl -0777 -nle 'print $1 if /(?:\G(?!^)|pallet_gear::WeightInfo for SubstrateWeight<T> {)\s(.*)}\s+\/\/ For backwards/gms' "$FILE")

  while IFS= read -r match; do
    weights+=("$match")
  done < <(echo "$ALL_WEIGHTS" | perl -0777 -nle 'print "$1\n" while / +(\/\/\/ The range of component [`\[\]\w\s,.]*?(^\s+fn gr_[\w\s\(\)->]+{$(?:.*)}))/gms')

  DEFINITIONS=$(perl -0777 -nle 'print "$&\n" while / *fn gr_[\w_]+[\(\w:, \)->]+$/gms' "$FILE")
done

# Iterate over lines in MAIN_FILE and append DEFINITIONS after "pub trait WeightInfo {"
while IFS= read -r line; do
  echo "$line"
  if [[ "$line" == "pub trait WeightInfo {" ]]; then
    # Insert the DEFINITIONS array here
    for def_line in "${DEFINITIONS[@]}"; do
      echo "$def_line"
    done
  elif [[ "$line" =~ ^impl.*WeightInfo\ for.*\{$ ]]; then
    # Insert the weights array here
    for weight in "${weights[@]}"; do
      echo "$weight"
    done
  fi
done < "$MAIN_FILE" > "$MAIN_FILE.tmp"

# Rename the temporary file to the original file
mv "$MAIN_FILE.tmp" "$MAIN_FILE"

echo "[+] Merged pallet_gear functions into $MAIN_FILE"
