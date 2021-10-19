#!/usr/bin/env sh

docker_usage() {
  cat << EOF

  Usage:
    ./gear.sh docker <FLAG>
    ./gear.sh docker <SUBCOMMAND> [DOCKER FLAGS]

  Flags:
    -h, --help     show help message and exit

  Subcommands:
    help           show help message and exit

    run            runs docker-compose

EOF
}

docker_run() {
  docker-compose down --remove-orphans
  docker-compose run --rm --service-ports dev "$@"
}
