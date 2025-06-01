#!/usr/bin/env bash

export RUSTC_WRAPPER="" # cross compilation fails with sccache
export CARGO_BUILD_TARGET="x86_64-pc-windows-msvc"
export CARGO_TARGET_DIR="target-xwin"
export XWIN_CROSS_COMPILER="clang-cl"
export XWIN_ARCH="x86_64"

if [ "$1" = "--gdb" ]; then
  export CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUNNER="winedbg --gdb --no-start"
  shift
elif [ "$1" = "--lldb" ]; then
  export CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUNNER="/opt/homebrew/opt/llvm/bin/lldb-server g :1234 -- wine"
  shift
fi

cargo xwin "$@"

wineserver -k
