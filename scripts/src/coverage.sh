#!/usr/bin/env sh

coverage_usage() {
  cat << EOF

  Usage:
    ./gear.sh coverage <SUBCOMMAND> [CARGO FLAGS]

  Flags:
    -h, --help     show help message and exit

  Subcommands:
    help           show help message and exit

EOF
}
