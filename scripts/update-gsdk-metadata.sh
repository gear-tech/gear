#!/usr/bin/env bash

# This script is used to update the generated code in the gsdk crate
# Also see `gsdk/HOW-TO-UPDATE.md` for more details

set -ex

GENERATE_GSDK_METADATA=1 cargo build --package vara-runtime --features dev
