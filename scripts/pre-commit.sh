#!/usr/bin/env sh

set -e

ROOT_DIR="$(cd "$(dirname "$0")"/.. && pwd)"

bold() {
    tput bold
}

normal() {
    tput sgr0
}

bold && echo "*** Run format\n" && normal
./scripts/format.sh all

bold && echo "*** Run clippy\n" && normal
./scripts/clippy.sh all

bold && echo "*** Run tests\n" && normal
./scripts/test.sh all
