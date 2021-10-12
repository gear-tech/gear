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

gear_usage() {
   cat << HEREDOC

   Usage: ./gear.sh [command] [subcommand] [OPTIONAL]

   Commands:
     -h, --help     show help message and exit
     -s, --show     show env versioning and installed toolchains

     build          build gear parts
     check          check that gear parts are compilable
     clippy         check clippy errors for gear parts
     docker         docker functionality
     format         format gear parts via rustfmt
     init           initializes and updates packages and toolchains
     test           test tool
    
    Try ./gear.sh -h (or --help) to learn more about each command.

HEREDOC
}
