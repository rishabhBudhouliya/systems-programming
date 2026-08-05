#!/usr/bin/env perl
# Act 3: Perl's Atomic::Pipe on the same workload as src/bin/bench.rs --only-atomic.
#
# Deliberately the same shape as the Rust side: one forked writer, one reading parent,
# the same total bytes in the same message size, one warmup, median of N reps.
#
# This comparison is fairer than "Rust vs Perl" usually is, and it is worth knowing why.
# Both implementations are pinned to the same syscall floor by PIPE_BUF: every physical
# write is one frame at or under PIPE_BUF, so at 4096 the Rust side moves 4091 payload
# bytes per write(2) and Atomic::Pipe moves 4080 (its header is 16 bytes: pid, tid,
# part id, size). Within 0.3%. Neither side can buy its way out with bigger writes
# without giving up atomicity, so the write syscall count is fixed by the problem, not
# chosen by the implementation. What is left to measure is per-frame userspace cost.

use strict;
use warnings;
use Time::HiRes qw(time);
use POSIX ();

my %opt = (
    'total-bytes'   => 256 * 1024 * 1024,
    'message-bytes' => 1024 * 1024,
    'reps'          => 5,
    'quiet'         => 0,
);

while (@ARGV) {
    my $k = shift @ARGV;
    $k =~ s/^--//;
    if ($k eq 'quiet') { $opt{quiet} = 1; next }
    die "unknown option --$k\n" unless exists $opt{$k};
    $opt{$k} = shift @ARGV;
}

eval { require Atomic::Pipe; 1 }
    or die "Atomic::Pipe is not installed. Run: cpanm Atomic::Pipe\n";

my $total    = $opt{'total-bytes'};
my $msg_size = $opt{'message-bytes'};
my $count    = int($total / $msg_size);
my $payload  = 'Z' x $msg_size;

sub one_run {
    my ($r, $w) = Atomic::Pipe->pair;

    my $start = time();

    my $pid = fork();
    die "fork failed: $!" unless defined $pid;

    if ($pid == 0) {
        # Child writes. Drop the reader so we are not holding the read end open.
        undef $r;
        $w->write_message($payload) for 1 .. $count;
        $w->close;
        POSIX::_exit(0);
    }

    # Parent must drop its copy of the write end or read_message never sees EOF.
    $w->close;
    undef $w;

    my $got = 0;
    while (defined(my $m = $r->read_message)) {
        $got += length($m);
    }
    my $elapsed = time() - $start;

    waitpid($pid, 0);
    die "perl: short read, got $got wanted @{[ $count * $msg_size ]}\n"
        unless $got == $count * $msg_size;

    return $elapsed;
}

# One warmup, then the timed reps. Median, matching the Rust harness.
one_run();
my @runs = sort { $a <=> $b } map { one_run() } 1 .. ($opt{reps} || 1);
my $median = $runs[ int(@runs / 2) ];
my $mbps   = ($count * $msg_size) / $median / 1e6;

if ($opt{quiet}) {
    printf "%.1f\n", $mbps;
}
else {
    printf "Atomic::Pipe %s (perl %vd)\n", $Atomic::Pipe::VERSION, $^V;
    printf "  PIPE_BUF %d, header 16 B/frame, payload %d B/frame\n",
        Atomic::Pipe::PIPE_BUF(), Atomic::Pipe::PIPE_BUF() - 16;
    printf "  total %d MiB, message %d KiB, %d reps\n",
        $total / (1024 * 1024), $msg_size / 1024, $opt{reps};
    printf "  %.0f MB/s\n", $mbps;
}
