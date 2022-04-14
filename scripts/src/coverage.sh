#!/usr/bin/env sh

coverage_usage() {
  cat << EOF

  Usage:
    ./gear.sh coverage <SUBCOMMAND> [CARGO FLAGS]

  Flags:
    -h, --help     show help message and exit

  Subcommands:
    help           show help message and exit

    gtest          run gear-test testing tool,
                   you can specify yaml list to run using yamls="path/to/yaml1 path/to/yaml2 ..." argument
    rtest          run node runtime testing tool

EOF
}
