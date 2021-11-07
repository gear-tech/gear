#!/usr/bin/env sh

run_usage() {
  cat << EOF

  Usage:
    ./gear.sh run <FLAG>
    ./gear.sh run <SUBCOMMAND> [CARGO FLAGS]

  Flags:
    -h, --help      show help message and exit

  Subcommands:
    help            show help message and exit

    node            runs gear-node
    purge-chain     purges gear node chain
    purge-dev-chain purges gear dev node chain

EOF
}

run_node() {
  RUST_LOG="gwasm=debug,gear_core_backend=debug" cargo run -p gear-node "$@"
}

purge_chain() {
  cargo run -p gear-node "$@" -- purge-chain
}

purge_dev_chain() {
  cargo run -p gear-node "$@" -- purge-chain --dev
}
