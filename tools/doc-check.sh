#!/bin/bash
# doc-check — compile the documentation.
#
# Every ```bhdl fence in docs/spec/*.md is REAL SYNTAX and must PARSE
# with today's compiler. Fences are often FRAGMENTS (a lone attribute,
# a run of board statements), so each fence is tried in three forms —
# raw, wrapped in an entity body, wrapped in a board body — and passes
# if ANY form parses. A fence marked `build` must fully SYNTHESIZE.
# A checker with no coverage number can pass vacuously, so the summary
# prints found / parsed / built / skipped — read the numbers, not just
# the exit code.
#
# Markers (HTML comment on the line directly above a fence):
#   <!-- doc-check: skip [reason] -->   not checked (pseudo-code,
#                                       deliberately partial syntax)
#   <!-- doc-check: build -->           complete board — full
#                                       `synthesize` gate
#
# Excluded: *_ARCHIVED*.md and *.old (historical record, not spec).
set -u
cd "$(dirname "$0")/.."
ROOT="$PWD"
CLI="$ROOT/target/release/bhdl-cli"
if [ ! -x "$CLI" ]; then
    echo "doc-check: build bhdl-cli first (cargo build -p bhdl-cli --release)" >&2
    exit 2
fi
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

total=0 parsed=0 built=0 skipped=0 failed=0
declare -a failures=()
declare -a proposals=()

try_parse() { # $1 = fragment file → 0 if any wrapping parses
    local frag="$1"
    "$CLI" "$frag" parse >/dev/null 2>&1 && return 0
    { echo "entity DocCheckWrapE() {"; cat "$frag"; echo "}"; } > "$WORK/wrap.bhdl"
    "$CLI" "$WORK/wrap.bhdl" parse >/dev/null 2>&1 && return 0
    { echo "board DocCheckWrapB {"; cat "$frag"; echo "}"; } > "$WORK/wrap.bhdl"
    "$CLI" "$WORK/wrap.bhdl" parse >/dev/null 2>&1 && return 0
    { echo "interface DocCheckWrapI {"; cat "$frag"; echo "}"; } > "$WORK/wrap.bhdl"
    "$CLI" "$WORK/wrap.bhdl" parse >/dev/null 2>&1 && return 0
    return 1
}

for doc in docs/spec/*.md; do
    case "$doc" in
        *_ARCHIVED*|*.old) continue ;;
    esac
    # A document honestly headed "Status: Proposal" specifies syntax
    # that is NOT YET implemented — its fences are design surface,
    # not shipped truth. Auto-skip (counted, named) so proposals
    # never rot the gate and never vacuously pass as "checked".
    if head -15 "$doc" | grep -qiE "Status:.*Proposal"; then
        n_prop=$(grep -c "^\`\`\`bhdl" "$doc" || true)
        skipped=$((skipped+n_prop))
        total=$((total+n_prop))
        proposals+=("$(basename "$doc") ($n_prop fences)")
        continue
    fi
    awk -v out="$WORK/$(basename "$doc")" '
        /^```bhdl[[:space:]]*$/ { infence=1; n+=1; marker=prev;
            if (prev ~ /doc-check:/) { sub(/.*doc-check:[[:space:]]*/, "", marker); sub(/[[:space:]]*-->.*/, "", marker) } else marker="parse";
            print marker > (out "." n ".marker"); next }
        /^```/ { infence=0; next }
        infence { print >> (out "." n ".bhdl") }
        { prev=$0 }
    ' "$doc"
    for frag in "$WORK/$(basename "$doc")".*.bhdl; do
        [ -e "$frag" ] || continue
        total=$((total+1))
        marker="parse"
        mfile="${frag%.bhdl}.marker"
        [ -f "$mfile" ] && marker="$(head -1 "$mfile" | awk '{print $1}')"
        label="$(basename "$doc")#$(basename "$frag" .bhdl | sed 's/.*\.//')"
        case "$marker" in
            skip*)
                skipped=$((skipped+1)) ;;
            build)
                if (cd "$ROOT" && "$CLI" -I "$ROOT" "$frag" synthesize >/dev/null 2>"$WORK/err"); then
                    built=$((built+1))
                else
                    failed=$((failed+1)); failures+=("$label [build]: $(grep -m1 -iE "error|failed" "$WORK/err" || tail -1 "$WORK/err")")
                fi ;;
            *)
                if try_parse "$frag"; then
                    parsed=$((parsed+1))
                else
                    failed=$((failed+1)); failures+=("$label [parse] (raw + entity/board/interface wraps all refused)")
                fi ;;
        esac
    done
done

echo "doc-check: $total fences — $parsed parsed, $built built, $skipped skipped, $failed FAILED"
if [ "${#proposals[@]}" -gt 0 ]; then
    echo "  proposals auto-skipped: ${proposals[*]}"
fi
if [ "$total" -eq 0 ]; then
    echo "doc-check: ZERO fences found — the extractor is broken (vacuous pass refused)" >&2
    exit 2
fi
if [ "$failed" -gt 0 ]; then
    printf '  ✗ %s\n' "${failures[@]}" >&2
    exit 1
fi
