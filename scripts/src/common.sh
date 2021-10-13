#!/usr/bin/env sh

bold() {
  tput bold
}

normal() {
  tput sgr0
}

header() {
  bold && echo "$1" && normal
}

# Get newline-separated list of all workspace members in `$1/Cargo.toml`
get_members() {
  tr -d "\n" < "$1/Cargo.toml" |
    sed -n -e 's/.*members[[:space:]]*=[[:space:]]*\[\([^]]*\)\].*/\1/p' |
    sed -n -e 's/,/ /gp' |
    sed -n -e 's/"\([^"]*\)"/\1/gp'
}
