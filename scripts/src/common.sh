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

parse_yamls_list() {
  has_yamls=$(echo "$1" | grep "yamls=" || true)

  if  [ -n "$has_yamls" ]
  then
    if ! hash perl 2>/dev/null
    then
      echo "Can not parse yamls without \"perl\" installed =("
      exit 1
    fi

    YAMLS=$(echo $1 | perl -ne 'print $1 if /yamls=(.*)/s' | tr "," " ")
  else
    YAMLS=""
  fi

  echo $YAMLS
}

get_demo_list() {
  ROOT_DIR=$1
  YAMLS=$2

  demo_list=""
  for yaml in $YAMLS
  do
    names=$(cat $yaml | perl -ne 'print "$1 " if /.*path: .*\/(.*?)\./s')
    names=$(echo $names | tr _ -)
    for name in $names
    do
      path=$(grep -rbnl --include \*.toml \"$name\" "$ROOT_DIR"/examples/)
      path=$(echo "$path" | tail -1 )
      path=$(echo $path | perl -ne 'print $1 if /(.*)Cargo\.toml/s')
      demo_list="$demo_list $path"
    done
  done

  echo $demo_list
}
