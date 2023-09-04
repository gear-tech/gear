#!/usr/bin/env sh

bold() {
  tput bold
}

normal() {
  tput sgr0
}

header() {
  bold && printf "\n  >> $1\n" && normal
}
