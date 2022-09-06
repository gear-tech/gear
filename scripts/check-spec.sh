#!/usr/bin/env bash

set -e

SELF="$0"
ROOT_DIR="$(cd "$(dirname "$SELF")"/.. && pwd)"
SCRIPTS="$ROOT_DIR/scripts/src"

. "$SCRIPTS"/common.sh

check_spec() {
    # $1 is master version, $2 is actual one and $3 changes requirement
    version="$(echo $1 $2 $3 | awk '{
        if ($3 == "true")
            if ($1 > $2)
                if ($1 - $2 >= 50 && $2 == 100)
                    print "" # Ok case. We have reset the network
                else
                    print "be bumped from",$1 # Should be bumped and be greater than master one
            else if ($1 == $2)
                print "be bumped from", $2 # We should bump for next version
            else
                print "" # Ok case. We had our spec bumped
        else if ($1 - $2 >= 50 && $2 == 100)
            print "" # Ok case. We have reseted the network
        else if ($1 != $2)
            print "equal",$1
    }')"

    if [ -z "$version" ]
    then
        printf "\n    Spec version is correct.\n"
    else
        printf "\n    Spec version should $version.\n"
        EXIT_CODE=1
    fi
}

PACKAGES_REQUIRE_BUMP_SPEC="common core core-backend core-processor node pallets runtime-interface"

SPEC_ON_MASTER="$(git diff origin/master | sed -n -r "s/^\-[[:space:]]+spec_version: +([0-9]+),$/\1/p")"
ACTUAL_SPEC_GEAR="$(cat $ROOT_DIR/runtime/gear/src/lib.rs | grep "spec_version: " | awk -F " " '{print substr($2, 1, length($2)-1)}')"
ACTUAL_SPEC_VARA="$(cat $ROOT_DIR/runtime/vara/src/lib.rs | grep "spec_version: " | awk -F " " '{print substr($2, 1, length($2)-1)}')"

if [ -z "$SPEC_ON_MASTER" ]; then
    SPEC_ON_MASTER=$ACTUAL_SPEC_GEAR
fi

for package in $(git diff --name-only origin/master | grep ".rs$" | cut -d "/" -f1 | uniq); do
    if [[ " ${PACKAGES_REQUIRE_BUMP_SPEC[@]} " =~ " ${package} " ]]; then
        CHANGES="true"
    fi
done

EXIT_CODE=0

header "Checking spec for Gear runtime"
check_spec "$SPEC_ON_MASTER" "$ACTUAL_SPEC_GEAR" "$CHANGES"

header "Checking spec for Vara runtime"
check_spec "$SPEC_ON_MASTER" "$ACTUAL_SPEC_VARA" "$CHANGES"

exit $EXIT_CODE
