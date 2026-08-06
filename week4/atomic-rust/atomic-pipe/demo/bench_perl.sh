#!/usr/bin/env bash
# Act 3: Perl Atomic::Pipe vs this implementation, same workload both sides.
set -euo pipefail

cd "$(dirname "$0")/.."

TOTAL=${TOTAL_BYTES:-$((256 * 1024 * 1024))}
MSG=${MESSAGE_BYTES:-$((1024 * 1024))}
REPS=${REPS:-5}

# Pick up a no-root vendored copy if demo/vendor_perl.sh has been run.
for d in demo/vendor/Atomic-Pipe-*/lib; do
    [ -d "$d" ] && export PERL5LIB="$PWD/$d${PERL5LIB:+:$PERL5LIB}"
done

if ! perl -MAtomic::Pipe -e1 >/dev/null 2>&1; then
    echo "Atomic::Pipe is not importable." >&2
    echo "No root needed -- it is pure Perl on core deps. Run: ./demo/vendor_perl.sh" >&2
    exit 1
fi

echo "building..." >&2
cargo build --release --bin bench >/dev/null 2>&1

echo
echo "act 3: framing implementations, same workload"
echo "  $((TOTAL / 1024 / 1024)) MiB total, $((MSG / 1024)) KiB messages, $REPS reps, median"
echo

# Both sides read 64 KiB at a time, which is Atomic::Pipe's DEFAULT_READ_SIZE.
# Matching it removes read-side buffering as a variable.
rust=$(./target/release/bench --only-atomic --read-size 65536 \
    --total-bytes "$TOTAL" --message-bytes "$MSG" --reps "$REPS")
perl_mbps=$(perl demo/bench_perl.pl --quiet \
    --total-bytes "$TOTAL" --message-bytes "$MSG" --reps "$REPS")

printf '  %-28s %9s\n' "" "MB/s"
printf '  %-28s %9.0f\n' "this implementation (Rust)" "$rust"
printf '  %-28s %9.0f\n' "Perl Atomic::Pipe" "$perl_mbps"
printf '  %-28s %9.2fx\n' "ratio" "$(echo "$rust / $perl_mbps" | bc -l)"

cat <<'EOF'

  Why this comparison is fair, and where it is not:

  Fair: both are pinned to the same syscall floor. Every physical write is one frame
  at or under PIPE_BUF, so at 4096 this implementation moves 4091 payload bytes per
  write(2) and Atomic::Pipe moves 4080 -- within 0.3%. Neither can use bigger writes
  without giving up the atomicity guarantee. The write syscall count is set by the
  problem, not by the language.

  Not fair: the remaining per-frame work runs in a Perl interpreter against optimised
  native code. The gap is dominated by that, not by protocol design.

  protocol                     this impl        Atomic::Pipe
  header per frame             3 B cont / 5 B terminal   16 B every frame
  id space                     22-bit pid       32-bit pid + 32-bit tid
  payload per frame @4096      4091 B           4080 B
  end-of-message signal        1 last-bit       part id counting down to 0
  size field                   terminal only    every frame
  pipe resize (F_SETPIPE_SZ)   no               yes

  The design consequence worth stating: because Atomic::Pipe puts a size on every
  frame, a continuation frame need not be full, so it never has the dead remainder
  window that pinned this implementation's payload constant. It pays 16 bytes a frame
  for that flexibility -- 5x the header, for a case that only matters at the tail.
EOF
