#!/usr/bin/env bash

XWIN_ARCH="x86_64" CARGO_BUILD_TARGET=x86_64-pc-windows-msvc CARGO_TARGET_DIR="target-xwin" cargo xwin $@
