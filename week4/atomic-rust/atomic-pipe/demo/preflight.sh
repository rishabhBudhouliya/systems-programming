#!/usr/bin/env bash
# Run this first on the demo box. Every check that matters, in one place.
# Exits non-zero if something would break the demo.

set -uo pipefail

ok=0
fail=0
warn=0

pass() { printf '  \033[32mOK\033[0m    %s\n' "$1"; ok=$((ok + 1)); }
bad()  { printf '  \033[31mFAIL\033[0m  %s\n' "$1"; fail=$((fail + 1)); }
soft() { printf '  \033[33mWARN\033[0m  %s\n' "$1"; warn=$((warn + 1)); }

echo
echo "atomic-pipe demo preflight"
echo "  host: $(hostname)   os: $(uname -s) $(uname -r)   arch: $(uname -m)"
echo

# --- toolchain --------------------------------------------------------------
# std::io::pipe stabilised in 1.87; Cargo.toml is edition 2024 (needs 1.85+).
# A distro Rust (Debian stable ships 1.63) will not compile this project at all.
echo "toolchain"
if ! command -v rustc >/dev/null 2>&1; then
    bad "rustc not found. rustup toolchain install stable"
else
    rv=$(rustc --version | awk '{print $2}')
    rv_major=${rv%%.*}
    rv_rest=${rv#*.}
    rv_minor=${rv_rest%%.*}
    if [ "$rv_major" -gt 1 ] || { [ "$rv_major" -eq 1 ] && [ "$rv_minor" -ge 87 ]; }; then
        pass "rustc $rv (>= 1.87, std::io::pipe available)"
    else
        bad "rustc $rv is too old. Need >= 1.87 for std::io::pipe. Run: rustup update stable"
    fi
fi

# --- the number the whole talk turns on -------------------------------------
echo
echo "pipe limits"
if command -v getconf >/dev/null 2>&1 && getconf PIPE_BUF /tmp >/dev/null 2>&1; then
    pb=$(getconf PIPE_BUF /tmp)
    case "$(uname -s)" in
        Linux)  want=4096 ;;
        Darwin) want=512  ;;
        *)      want=""   ;;
    esac
    if [ -n "$want" ] && [ "$pb" = "$want" ]; then
        pass "PIPE_BUF = $pb (matches the compiled-in constant for this platform)"
    elif [ -n "$want" ]; then
        bad "PIPE_BUF = $pb but pipe.rs is compiled for $want on this platform. Frames would not be atomic."
    else
        soft "PIPE_BUF = $pb (unknown platform, check pipe.rs constant by hand)"
    fi
else
    soft "getconf PIPE_BUF unavailable; check the constant in src/pipe.rs by hand"
fi

# Default pipe capacity, i.e. how much a writer can push before it blocks.
# Act 1 needs writers to actually block mid-payload for tearing to show up.
if [ -r /proc/sys/fs/pipe-max-size ]; then
    pass "pipe-max-size = $(cat /proc/sys/fs/pipe-max-size) (default capacity is 64 KiB)"
fi

# --- the 22-bit id field ----------------------------------------------------
# Ids are 22 bits. Linux pid_max defaults to 4194304, so the largest pid is
# 4194303 = 2^22-1, which fits exactly. If the box raised pid_max, a large pid
# overflows into the last-frame bit and the reader desyncs in a way that is
# very hard to explain live.
echo
echo "id space"
if [ -r /proc/sys/kernel/pid_max ]; then
    pm=$(cat /proc/sys/kernel/pid_max)
    if [ "$pm" -le 4194304 ]; then
        pass "pid_max = $pm -> max pid $((pm - 1)) fits the 22-bit id field (max 4194303)"
    else
        bad "pid_max = $pm exceeds 2^22. A large pid will corrupt the last-frame bit."
    fi
    echo "        current pid $$ -> $([ $$ -lt 4194304 ] && echo 'fits' || echo 'OVERFLOWS')"
else
    soft "no /proc/sys/kernel/pid_max (not Linux?), skipping id-space check"
fi

# --- perl side of act 3 -----------------------------------------------------
echo
echo "perl (act 3)"
if ! command -v perl >/dev/null 2>&1; then
    bad "perl not found"
else
    pass "perl $(perl -e 'print $^V')"
    if perl -MAtomic::Pipe -e1 >/dev/null 2>&1; then
        pass "Atomic::Pipe $(perl -MAtomic::Pipe -e 'print $Atomic::Pipe::VERSION')"
    else
        bad "Atomic::Pipe not installed. Run: cpanm Atomic::Pipe   (or: cpan Atomic::Pipe)"
    fi
fi

# --- optional instrumentation ----------------------------------------------
echo
echo "optional"
if command -v strace >/dev/null 2>&1; then
    pass "strace present (syscall counts for act 2)"
else
    soft "no strace; act 2 falls back to in-process syscall counters"
fi

echo
printf 'summary: %d ok, %d warn, %d fail\n\n' "$ok" "$warn" "$fail"
[ "$fail" -eq 0 ] || { echo "Fix the FAIL lines before running the demo."; exit 1; }
echo "Good to go."
