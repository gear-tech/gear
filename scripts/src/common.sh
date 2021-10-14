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
