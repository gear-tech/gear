// This file is part of Gear.
//
// Copyright (C) 2025 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

static int (*CXA_ATEXIT)(void (*)(void*), void*, void*);
static void (*DTOR_FN)(void);

void __gcore_set_fns(
    int (*cxa_atexit)(void (*)(void*), void*, void*),
    void (*dtor_fn)(void)
) {
    CXA_ATEXIT = cxa_atexit;
    DTOR_FN = dtor_fn;
}

int __cxa_atexit(void (*f)(void*), void* arg, void* dso) {
    return CXA_ATEXIT(f, arg, dso);
}

static void call(void *f) {
    ((void (*)(void)) (unsigned int) f)();
}

int atexit(void (*f)(void)) {
    return __cxa_atexit(call, (void *) (unsigned int) f, 0);
}

__attribute__((visibility("hidden")))
void __wasm_call_dtors(void) {
    DTOR_FN();
}
