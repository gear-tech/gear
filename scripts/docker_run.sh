#!/usr/bin/env sh

set -e

echo "*** Start Substrate node template ***"

cd "$(dirname "$0")/.."

docker-compose down --remove-orphans
docker-compose run --rm --service-ports dev $@
