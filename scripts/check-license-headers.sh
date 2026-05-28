#!/usr/bin/env bash
# CI check: every .rs file must have a correct minimal SPDX header.
#
# Checks:
#   1. No file contains old GPL boilerplate ("This program is free software").
#   2. Every file has exactly the expected SPDX identifier line.
#   3. The line directly above SPDX is a "// Copyright" line.
#   4. The line directly below SPDX is blank (or end of file).
#
# Usage: ./check-headers.sh [dir]    (default: .)
# Exit:  0 = all good, 1 = violations found.

set -euo pipefail

ROOT="${1:-.}"
GEAR_SPDX="// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0"
APACHE_SPDX="// SPDX-License-Identifier: Apache-2.0"

expected_spdx_for() {
    case "$1" in
        substrate/sp-wasm-interface-common/src/util.rs)
            printf '%s\n' "$GEAR_SPDX"
            ;;
        substrate/sp-allocator/* | \
        substrate/sp-runtime-interface-proc-macro/* | \
        substrate/sp-wasm-interface/* | \
        substrate/sp-wasm-interface-common/* | \
        substrate/substrate-wasm-builder/*)
            printf '%s\n' "$APACHE_SPDX"
            ;;
        *)
            printf '%s\n' "$GEAR_SPDX"
            ;;
    esac
}

copyright_pattern_for() {
    case "$1" in
        substrate/sp-wasm-interface-common/src/util.rs)
            printf '%s\n' '^// Copyright'
            ;;
        substrate/runtime-executor/wasmtime/src/host_state.rs | \
        substrate/runtime-executor/wasmtime/src/memory_wrapper.rs | \
        substrate/runtime-executor/wasmtime/src/store_data.rs)
            printf '%s\n' '^// Copyright'
            ;;
        substrate/runtime-executor/* | \
        substrate/sp-allocator/* | \
        substrate/sp-runtime-interface-proc-macro/* | \
        substrate/sp-wasm-interface/* | \
        substrate/sp-wasm-interface-common/* | \
        substrate/substrate-wasm-builder/*)
            printf '%s\n' '^// Copyright [(]C[)]( [0-9]{4}(-[0-9]{4})?)? Parity Technologies'
            ;;
        *)
            printf '%s\n' '^// Copyright'
            ;;
    esac
}

ISSUES=$(mktemp)
trap "rm -f '$ISSUES'" EXIT

# ── 1. Old GPL boilerplate ────────────────────────────────────────────────────
git -C "$ROOT" ls-files -z '*.rs' \
    | xargs -0 grep -lF "This program is free software" 2>/dev/null \
    | sort \
    | sed 's/^/old GPL boilerplate: /' \
    >> "$ISSUES" || true

# ── 2-4. Per-file SPDX checks (awk reads each file once) ─────────────────────
while IFS= read -r -d '' file; do
    spdx=$(expected_spdx_for "$file")
    copyright=$(copyright_pattern_for "$file")

    awk -v spdx="$spdx" -v copyright="$copyright" -v f="$ROOT/$file" '
        { prev = cur; cur = $0 }

        cur ~ /SPDX-License-Identifier/ {
            if (cur != spdx) {
                print "wrong SPDX value: " f " (expected: " spdx ")"
            } else {
                if (prev !~ copyright)
                    print "no Copyright line above SPDX: " f
                getline nxt
                if (nxt !~ /^[[:space:]]*$/ && nxt != "")
                    print "no blank line after SPDX: " f " (got: " nxt ")"
            }
            found = 1
            exit
        }

        END { if (!found) print "missing SPDX: " f }
    ' "$ROOT/$file"
done < <(git -C "$ROOT" ls-files -z '*.rs') >> "$ISSUES"

# ── Result ────────────────────────────────────────────────────────────────────
TOTAL=$(git -C "$ROOT" ls-files '*.rs' | wc -l | tr -d ' ')

if [[ -s "$ISSUES" ]]; then
    echo "FAIL: header violations in ${TOTAL} .rs files:"
    sed 's/^/  /' "$ISSUES"
    exit 1
fi

echo "OK: all ${TOTAL} .rs files have correct headers."
