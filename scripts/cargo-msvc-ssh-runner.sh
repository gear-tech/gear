#!/usr/bin/env bash

set -e

absolute_exe_path=$1
relative_exe_path=${absolute_exe_path:${#CARGO_WORKSPACE_DIR}}
workspace_root=${absolute_exe_path::${#CARGO_WORKSPACE_DIR}}
exe_name=$(basename $absolute_exe_path)
shift

cd $workspace_root

echo "$relative_exe_path $@"

rsync --info=progress2 -ahv $absolute_exe_path $CARGO_MSVC_SSH_RUNNER_HOST:/tmp/$exe_name
ssh $CARGO_MSVC_SSH_RUNNER_HOST "RUST_BACKTRACE=$RUST_BACKTRACE RUST_LOG_STYLE=always RUST_LOG=$RUST_LOG /tmp/$exe_name $@"
