#!/usr/bin/env sh

docker_usage() {
   cat << HEREDOC

   Usage: ./gear.sh docker [subcommand]

   Subcommands:
     -h, --help     show help message and exit

     run            runs docker-compose

HEREDOC
}

docker_run() {
    docker-compose down --remove-orphans
    docker-compose run --rm --service-ports dev $@
}
