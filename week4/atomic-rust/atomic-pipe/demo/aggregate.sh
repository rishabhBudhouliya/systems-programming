#!/usr/bin/env bash
# Act 1: several unrelated processes, one shared pipe, one aggregating parent.
#
#   ( ./writer 1 & ./writer 2 & ... ) | ./reader
#
# The shell creates the pipe. Every writer inherits the same write end as its stdout;
# the reader gets the read end as stdin. Nothing here forks inside Rust -- these are
# genuinely separate programs, which is the point.
#
# The reader verifies each payload against the writer it claims to come from. That is
# the only check that can distinguish the two modes: interleaving reorders bytes without
# losing them, so any whole-stream total is identical either way.

set -uo pipefail

cd "$(dirname "$0")/.."

MODE=${1:-framed}
WRITERS=${WRITERS:-4}
BYTES=${BYTES:-400000}
RUNS=${RUNS:-1}

case "$MODE" in
    framed|raw) ;;
    *) echo "usage: $0 [framed|raw]   (env: WRITERS, BYTES, RUNS)" >&2; exit 2 ;;
esac

if [ "$MODE" = framed ] && [ ! -x target/release/reader ]; then
    cat >&2 <<'EOF'
target/release/reader is missing.

It needs Pipe::reader_on_stdin(), which does not exist in src/pipe.rs yet, so it is
gated behind a cargo feature. See demo/HANDOFF.md. Once the constructor is in:

    cargo build --release --features stdio-pipe
EOF
    exit 1
fi

echo
echo "act 1: $WRITERS writers x $BYTES bytes, mode=$MODE"
echo "  each writer is a separate process; all share one pipe via stdout"
echo

pass=0
fail=0

for run in $(seq 1 "$RUNS"); do
    [ "$RUNS" -gt 1 ] && echo "--- run $run/$RUNS ---"

    # The subshell holds a write end too, so the reader sees EOF only after every
    # writer has exited and the subshell has closed its copy.
    (
        for i in $(seq 1 "$WRITERS"); do
            ./target/release/writer --index "$i" --bytes "$BYTES" --mode "$MODE" &
        done
        wait
    ) | ./target/release/reader --bytes "$BYTES" --writers "$WRITERS" --mode "$MODE"

    if [ "${PIPESTATUS[1]}" -eq 0 ]; then
        pass=$((pass + 1))
    else
        fail=$((fail + 1))
    fi
done

if [ "$RUNS" -gt 1 ]; then
    echo "over $RUNS runs: $pass passed, $fail failed"
    echo
fi

case "$MODE" in
    framed)
        if [ "$fail" -eq 0 ]; then
            echo "Every writer's payload arrived whole and attributable to its writer,"
            echo "even though $WRITERS processes were interleaving on one pipe."
        else
            echo "UNEXPECTED: framed mode should not fail. Investigate before the demo."
        fi
        ;;
    raw)
        if [ "$fail" -gt 0 ]; then
            echo "The naive length-prefixed protocol desynced: a write larger than"
            echo "PIPE_BUF was split by the kernel and another writer landed in the gap."
        else
            echo "No tearing observed this time -- raw mode depends on the scheduler."
            echo "Raise BYTES or WRITERS and try again, e.g. BYTES=4000000 RUNS=10 $0 raw"
        fi
        ;;
esac
echo
