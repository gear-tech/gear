#!/usr/bin/env bash

set -e

PACKAGES_REQUIRE_BUMP_SPEC="common core core-backend node pallets runtime-interface lazy-pages"

SPEC_ON_MASTER="$(git diff origin/master | sed -n -r "s/^\-[[:space:]]+spec_version: +([0-9]+),$/\1/p")"
ACTUAL_SPEC="$(git diff origin/master | sed -n -r "s/^\+[[:space:]]+spec_version: +([0-9]+),$/\1/p")"

if [ -z "$SPEC_ON_MASTER" ]; then
    SPEC_ON_MASTER="0"
fi

if [ -z "$ACTUAL_SPEC" ]; then
    ACTUAL_SPEC="0"
fi

for package in $(git diff --name-only $CURRENT_BRANCH origin/master | grep -v "README.md$" | cut -d "/" -f1 | uniq); do
    if [[ " ${PACKAGES_REQUIRE_BUMP_SPEC[@]} " =~ " ${package} " ]]; then
        UPDATED_SPEC="true"
        if [ "$SPEC_ON_MASTER" = "$ACTUAL_SPEC" ]; then
            printf "\n   These files were changed:\n\n"
            echo "$(git diff --name-only origin/master | grep "^$package")"
            printf "\n   Spec version should be bumped!\n\n"
            exit 1
        fi
    fi
done

if [ "$UPDATED_SPEC" != "true" ]; then
    if [ "$SPEC_ON_MASTER" != "$ACTUAL_SPEC" ]; then
        printf "\n   Spec versions are different, but they shouldn't!\n\n"
        exit 1
    fi
fi

printf "\n   Spec version is correct!\n\n"
exit 0
