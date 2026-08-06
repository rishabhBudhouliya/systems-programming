#!/usr/bin/env bash
# Get Atomic::Pipe without root, without a compiler, without CPAN.
#
# `cpan Atomic::Pipe` tries to build Compress::Zstd, which needs libzstd headers and
# a system install -- neither of which you have without sudo. But Compress::Zstd is
# only *recommended*, not required, and Atomic::Pipe lazily require()s it solely for
# compression, which this benchmark never turns on.
#
# Every hard dependency of Atomic::Pipe (Carp, Errno, Fcntl, IO::Handle, List::Util,
# POSIX, Scalar::Util, bytes, constant) is a core module. So the module is pure Perl
# and needs no installation at all -- unpack it and point PERL5LIB at its lib/.

set -euo pipefail

cd "$(dirname "$0")/.."

VERSION=${VERSION:-0.032}
URL="https://cpan.metacpan.org/authors/id/E/EX/EXODIST/Atomic-Pipe-${VERSION}.tar.gz"
DEST="demo/vendor"
LIB="$DEST/Atomic-Pipe-${VERSION}/lib"

if perl -MAtomic::Pipe -e1 >/dev/null 2>&1; then
    echo "Atomic::Pipe $(perl -MAtomic::Pipe -e 'print $Atomic::Pipe::VERSION') is already"
    echo "importable system-wide. Nothing to do."
    exit 0
fi

if [ -d "$LIB" ]; then
    echo "already vendored at $LIB"
else
    mkdir -p "$DEST"
    echo "fetching $URL"
    if command -v curl >/dev/null 2>&1; then
        curl -fsSL "$URL" -o "$DEST/atomic-pipe.tar.gz"
    elif command -v wget >/dev/null 2>&1; then
        wget -qO "$DEST/atomic-pipe.tar.gz" "$URL"
    else
        echo "need curl or wget" >&2
        exit 1
    fi
    tar xzf "$DEST/atomic-pipe.tar.gz" -C "$DEST"
    rm -f "$DEST/atomic-pipe.tar.gz"
fi

if ! PERL5LIB="$PWD/$LIB" perl -MAtomic::Pipe -e1 2>/dev/null; then
    echo "vendored copy at $LIB does not load. Check perl version (need >= 5.10)." >&2
    exit 1
fi

echo
echo "OK: Atomic::Pipe $(PERL5LIB="$PWD/$LIB" perl -MAtomic::Pipe -e 'print $Atomic::Pipe::VERSION') loads from $LIB"
echo
echo "demo/bench_perl.sh and demo/preflight.sh pick this up automatically."
echo "For an interactive shell:  export PERL5LIB=\"$PWD/$LIB:\$PERL5LIB\""
