#!/usr/bin/env bash

export CARGO_BUILD_TARGET="x86_64-pc-windows-msvc"
export XWIN_ARCH="x86_64"
export CARGO_TARGET_DIR="target-xwin"

if [ "$1" = "--gdb" ]; then
  export CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUNNER="winedbg --gdb --no-start"
  shift
fi

if [ "$1" = "--lldb" ]; then
  export CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUNNER="/opt/homebrew/opt/llvm/bin/lldb-server g :1234 -- wine"
  shift
fi

cargo xwin $@

wineserver -k
