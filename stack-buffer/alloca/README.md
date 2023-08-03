### What is alloca?

This library allows to allocate N bytes on the stack and then pass uninitialized memory to rust code.

### Contents of this directory

- [`alloca.c`](alloca.c) - library source code
- [`libcalloca.a`](libcalloca.a) - pre-built static library for the `wasm32-unknown-unknown` target

### Compiling vs pre-built library

Compilation should not happen in the general case. Because this dependency is used in gcore and gstd, we use a pre-built
library to not require the clang compiler.

However, if for some reason you want to compile the C library at build time, use the `compile-alloca` feature.
