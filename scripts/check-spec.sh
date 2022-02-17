#!/usr/bin/env bash

set -e

PACKAGES_REQUIRE_BUMP_SPEC="common core core-backend node pallets runtime-interface lazy-pages"

CURRENT_BRANCH="$(git branch --show-current)"

SPEC_ON_MASTER="$(git diff $CURRENT_BRANCH origin/master | sed -n -r "s/^\-[[:space:]]+spec_version: +([0-9]+),$/\1/p")"
ACTUAL_SPEC="$(git diff $CURRENT_BRANCH origin/master | sed -n -r "s/^\+[[:space:]]+spec_version: +([0-9]+),$/\1/p")"

for package in $(git diff --name-only $CURRENT_BRANCH origin/master | grep -v "README.md$" | cut -d "/" -f1 | uniq); do
    if [[ " ${PACKAGES_REQUIRE_BUMP_SPEC[@]} " =~ " ${package} " ]]; then
        UPDATED_SPEC="true"
        if [ "$SPEC_ON_MASTER" = "$ACTUAL_SPEC" ]; then
            echo "    These files were changed:\n"
            echo git diff --name-only origin/master | grep "^$package"
            echo "\n    Spec version should be bumped!"
            exit 1
        fi
    fi
done

if [ "$UPDATED_SPEC" != "true" ]; then
    if [ "$SPEC_ON_MASTER" != "$ACTUAL_SPEC" ]; then
        echo "Spec versions are different, but they shouldn't!"
        exit 1
    fi
fi

echo "Spec version is correct"
exit 0
