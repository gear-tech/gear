#!/usr/bin/env bash
# Stop hook: ask Sonnet to review uncommitted changes against CLAUDE.md
# at the project root. Exits 2 with findings on stderr if violations are
# found; exits 0 silently otherwise. Recursion-safe via the
# CLAUDE_MD_REVIEW_RUNNING env-var guard.

# Recursion guard: when our own `claude -p` subprocess fires its own Stop,
# the harness re-invokes this script. We exit immediately in that case.
if [ -n "${CLAUDE_MD_REVIEW_RUNNING:-}" ]; then
    exit 0
fi

set -uo pipefail

# Anchor at the git toplevel so the review still fires from any subdir.
project_root=$(git rev-parse --show-toplevel 2>/dev/null) || exit 0
cd "$project_root"

# Cheap pre-filter — skip the review when no Rust/Solidity/Toml changed.
# We capture into a variable instead of `grep -q` because pipefail + grep's
# early-exit makes upstream commands die from SIGPIPE, which would invert
# the match check.
changed_files=$( { git diff --name-only HEAD 2>/dev/null;
                   git ls-files --others --exclude-standard 2>/dev/null; } )
if ! printf '%s\n' "$changed_files" | grep -E '\.(rs|sol|toml)$' >/dev/null; then
    exit 0
fi

# Build a unified diff covering tracked changes + new file content.
diff=$(git diff --no-color HEAD 2>/dev/null || true)
while IFS= read -r f; do
    [ -z "$f" ] && continue
    [ ! -f "$f" ] && continue
    case "$f" in
        *.rs|*.sol|*.toml) ;;
        *) continue ;;
    esac
    diff+=$'\n--- /dev/null\n+++ b/'"$f"$'\n'
    diff+=$(awk '{print "+" $0}' "$f")
    diff+=$'\n'
done < <(git ls-files --others --exclude-standard 2>/dev/null)

if [ -z "${diff//[[:space:]]/}" ]; then
    exit 0
fi

# Dry-run hatch — runs everything except the Sonnet call. For testing.
if [ -n "${CLAUDE_MD_REVIEW_DRY_RUN:-}" ]; then
    echo "dry-run: $(printf '%s' "$diff" | wc -l) diff lines, would call Sonnet" >&2
    exit 0
fi

prompt=$(cat <<'PROMPT'
You are a CLAUDE.md compliance linter for the gear repository. The project
root is the current working directory.

1. Read CLAUDE.md at the project root for the rules. Particularly enforce:
   - Comment & Doc Sizing tiers (Tiny=1 line for inline body comments;
     Small=≤5 for private items; Medium=≤20 for public items;
     Large=≤200 for crate-level)
   - Test timeout cap (no >120_000 ms without explicit user permission)
   - `unwrap_or` / `unwrap_or_default` / `unwrap_or_else` ban in
     production code (tests/mocks excluded)
   - Any other concrete rule stated in CLAUDE.md

2. Review the diff below for any rule violations.

3. Output format — strict:
   - If NO violations, output the single line `OK` and nothing else.
   - Otherwise, one violation per line as:
     `path:line — rule violated — what to change`
   - No headers, summaries, or commentary outside that format.

DIFF:
PROMPT
)
prompt+=$'\n'"$diff"

# Spawn Sonnet headlessly. Auth inherits from the user's environment.
# CLAUDE_MD_REVIEW_RUNNING=1 short-circuits this same script when the
# spawned `claude -p` fires its own Stop event.
review=$(CLAUDE_MD_REVIEW_RUNNING=1 claude -p "$prompt" \
    --model sonnet \
    --output-format text \
    --max-budget-usd 0.50 \
    --add-dir "$(pwd)" 2>/dev/null) || exit 0

trimmed=$(printf '%s' "$review" | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')

if [ "$trimmed" = "OK" ] || [ -z "$trimmed" ]; then
    exit 0
fi

{
    echo "CLAUDE.md compliance review (Sonnet) — possible issues:"
    echo
    echo "$review"
    echo
    echo "Apply judgment — fix genuine violations of project rules, but skip"
    echo "suggestions that contradict explicit user requests in the current"
    echo "session (e.g. user asked for a verbose comment)."
} >&2
exit 2
