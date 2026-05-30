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
SPDX="// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0"

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
    awk -v spdx="$SPDX" -v f="$ROOT/$file" '
        { prev = cur; cur = $0 }

        cur ~ /SPDX-License-Identifier/ {
            if (cur != spdx) {
                print "wrong SPDX value: " f
            } else {
                if (prev !~ /^\/\/ Copyright/)
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
