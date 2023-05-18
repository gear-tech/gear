#!/usr/bin/env bash

set -euo pipefail

if [ ! -z ${GITHUB_ACTIONS-} ]; then
  set -x
fi

help() {
  cat <<'EOF'
Install a binary release of gear hosted on get.gear.rs

USAGE:
    install [options]

FLAGS:
    -h, --help      Display this message

OPTIONS:
    --tag TAG       Tag (version) of the crate to install, defaults to latest release
    --to LOCATION   Where to install the binary [default: /usr/local/bin]
    --target TARGET
EOF
}

git=gear-tech/gear
crate=gear
url=https://get.gear.rs/
releases=$url
say() {
  echo "$@"
}

say_err() {
  say "$@" >&2
}

err() {
  if [ ! -z ${td-} ]; then
    rm -rf $td
  fi

  say_err "error: $@"
  exit 1
}

need() {
  if ! command -v $1 > /dev/null 2>&1; then
    err "need $1 (command not found)"
  fi
}

force=false
while test $# -gt 0; do
  case $1 in
    --force | -f)
      force=true
      ;;
    --help | -h)
      help
      exit 0
      ;;
    --tag)
      tag=$2
      shift
      ;;
    --target)
      target=$2
      shift
      ;;
    --to)
      dest=$2
      shift
      ;;
    *)
      ;;
  esac
  shift
done

# Dependencies
need curl
need install
need mkdir
need mktemp
need tar

# Optional dependencies
if [ -z ${tag-} ]; then
  need cut
  need rev
fi

if [ -z ${dest-} ]; then
  dest="/usr/local/bin"
fi

if [ -z ${tag-} ]; then
  tag=$(curl --proto =https --tlsv1.2 -sSf https://api.github.com/repos/gear-tech/gear/releases/latest |
    grep tag_name |
    cut -d'"' -f4
  )
fi

if [ -z ${target-} ]; then
  uname_target=`uname -m`-`uname -s`

  case $uname_target in
    arm64-Darwin) target=aarch64-apple-darwin;;
    x86_64-Darwin) target=x86_64-apple-darwin;;
    x86_64-Linux) target=x86_64-unknown-linux-gnu;;
    *)
      err 'Could not determine target from output of `uname -m`-`uname -s`, please use `--target`:' $uname_target
      err 'Target architecture is not supported by this install script.'
      err 'Consider opening an issue or building from source: https://github.com/gear-tech/gear'
    ;;
  esac
fi

archive="$url$crate-$tag-$target.tar.xz"

say "Crate:       $crate"
say "Tag:         $tag"
say "Target:      $target"
say "Destination: $dest"
say "Archive:     $archive"

td=$(mktemp -d || mktemp -d -t tmp)
curl --proto =https --tlsv1.2 -SfL $archive | tar -C $td -xJv

for f in $(ls $td); do
  test -x $td/$f || continue

  mkdir -p $dest
  sudo install -m 755 $td/$f $dest

done

rm -rf $td
