#!/usr/bin/env sh

docker_usage() {
  cat << EOF

  Usage: ./gear.sh docker [subcommand] [FLAGS]

  Subcommands:
    -h, --help     show help message and exit

    run            runs docker-compose

EOF
}

docker_run() {
  docker-compose down --remove-orphans
  docker-compose run --rm --service-ports dev "$@"
}
