// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

/*
 * C shim for constructor/destructor plumbing.
 *
 * This exists because Rust's `linkage` feature is unstable.
 */

#include <stdint.h>
#include <stddef.h>

typedef int   (*cxa_atexit_fn)(void (*)(void *), void *, void *);
typedef void  (*dtor_fn)(void);

static cxa_atexit_fn CXA_ATEXIT = NULL;
static dtor_fn       DTORS      = NULL;

/**
 * Inject function pointers from Rust runtime.
 *
 * Must be called exactly once during startup by an early-priority constructor.
 */
void __gcore_set_fns(cxa_atexit_fn cxa_atexit, dtor_fn dtors) {
    CXA_ATEXIT = cxa_atexit;
    DTORS = dtors;
}

/**
 * Standard C++ ABI hook for registering destructors of static objects.
 * Forwards to the Rust-side __cxa_atexit_impl provided via __gcore_set_fns().
 */
int __cxa_atexit(void (*f)(void *), void *arg, void *dso_handle) {
    return CXA_ATEXIT(f, arg, dso_handle);
}

static void call(void *f) {
    ((void (*)(void))(uintptr_t)f)();
}

/**
 * Standard libc atexit function.
 */
int atexit(void (*f)(void)) {
    return __cxa_atexit(call, (void *)(uintptr_t)f, NULL);
}

/**
 * Called by wrappers that wasm-ld insert to run all registered destructors.
 * Hidden to avoid polluting WASM exports but visible to the linker.
 */
__attribute__((visibility("hidden")))
void __wasm_call_dtors(void) {
    DTORS();
}
