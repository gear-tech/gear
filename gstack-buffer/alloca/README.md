### What is alloca?

This library allows to allocate N bytes on the stack and then pass uninitialized memory to rust code.

### Contents of this directory

- [`alloca.c`](alloca.c) - library source code
- [`libcalloca.a`](libcalloca.a) - pre-built static library for the `wasm32-unknown-unknown` target
