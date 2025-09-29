#!/usr/bin/env bash

set -euo pipefail

if [[ $# -ne 2 ]]; then
  echo "Usage: $0 <url> <output-path>" >&2
  exit 1
fi

url=$1
output=$2

if ! command -v aria2c >/dev/null 2>&1; then
  echo "aria2c not found. Please install aria2 before running this script." >&2
  exit 1
fi

# Use segmented downloads with resume support to handle flaky network connections.
aria2c \
  --retry-wait=10 \
  --max-tries=15 \
  --timeout=60 \
  --connect-timeout=60 \
  --max-connection-per-server=8 \
  --split=8 \
  --min-split-size=64M \
  --allow-overwrite=true \
  --auto-file-renaming=false \
  --continue=true \
  --file-allocation=none \
  --console-log-level=warn \
  --summary-interval=30 \
  --out="$output" \
  "$url"

if [[ ! -s "$output" ]]; then
  echo "Downloaded file '$output' is empty." >&2
  exit 1
fi

echo "Snapshot downloaded to $output"
