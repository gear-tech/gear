#!/bin/sh

set -eu

if [ -n "${GITHUB_ACTIONS-}" ]; then
  set -x
fi

help() {
  cat << EOF
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
say() {
  echo "$@"
}

say_err() {
  say "$@" >&2
}

err() {
  if [ -n "${td-}" ]; then
    rm -rf "$td"
  fi

  say_err "error: $*"
  exit 1
}

need() {
  command -v "$1" > /dev/null 2>&1 || err "need $1 (command not found)"
}

while [ $# -gt 0 ]; do
  case $1 in
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
need sudo
need curl
need install
need mkdir
need mktemp
need tar
need xz

# Optional dependencies
if [ -z "${tag-}" ]; then
  need cut
  need rev
fi

if [ -z "${dest-}" ]; then
  dest="/usr/local/bin"
fi

if [ -z "${tag-}" ]; then
  json=$(curl --proto '=https' --tlsv1.2 -sSf "https://api.github.com/repos/$git/releases/latest" || err "failed to get latest release of $git")
  tag_name=$(echo "$json" | grep tag_name || err "failed to parse tag_name")
  tag=$(echo "$tag_name" | cut -d'"' -f4)
fi

if [ -z "${target-}" ]; then
  uname_target=$(uname -m)-$(uname -s)

  case $uname_target in
    arm64-Darwin) target=aarch64-apple-darwin;;
    x86_64-Darwin) target=x86_64-apple-darwin;;
    x86_64-Linux) target=x86_64-unknown-linux-gnu;;
    *)
      err "Could not determine target from output of \`uname -m\`-\`uname -s\`, please use --target: $uname_target
Target architecture is not supported by this install script.
Consider opening an issue or building from source: https://github.com/$git"
    ;;
  esac
fi

archive_name="$crate-$tag-$target.tar.xz"
archive_url="$url$archive_name"

say "Crate:       $crate"
say "Tag:         $tag"
say "Target:      $target"
say "Destination: $dest"
say "Archive URL: $archive_url"

td=$(mktemp -d || mktemp -d -t tmp)
archive_tmp="$td/$archive_name"

curl --proto '=https' --tlsv1.2 -SfL "$archive_url" -o "$archive_tmp" || err "failed to download $archive_name"
tar -C "$td" -xf "$archive_tmp" || err "failed to extract $archive_name"

for f in "$td"/*; do
  [ -x "$f" ] || continue

  mkdir -p "$dest"
  sudo install -m 755 "$f" "$dest"

done

rm -rf "$td"
