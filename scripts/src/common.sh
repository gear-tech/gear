#!/usr/bin/env sh

bold() {
  if [ -t 1 ] && [ -n "${TERM:-}" ]; then
    tput bold || true
  fi
}

normal() {
  if [ -t 1 ] && [ -n "${TERM:-}" ]; then
    tput sgr0 || true
  fi
}

header() {
  bold
  printf "\n  >> %s\n" "$1"
  normal
}
