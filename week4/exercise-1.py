import os
import time
from itertools import groupby
from collections import Counter

'''
Design a multi-write pipe consistency test
'''

def pipe_concurrent_writes(N: int):
    # common pipe meant to be used by all
    (r, w) = os.pipe()
    os.set_blocking(w, False)
    # attempt to create 4 processess
    pids = []
    for i in range(4):
        pid = os.fork()
        counter = i
        pids.append(pid)
        if pid == 0:
            # the idea is that once a child is spawned, it should not follow the loop and break out the loop
            break

    if pid == 0:
        try:
            payload = bytes([65 + counter]) * N
            n = os.write(w, payload)
            if n < len(payload):
                print(f"writer {counter}: {'TORN' if n < len(payload) else 'intact'} ({n}/{N})")
        except BlockingIOError:
            print(f"writer {counter}: EAGAIN - nothing written")
        finally:
            os.close(w)
            os._exit(0)
    else:
        os.close(w)
        buf = b''
        # let the parent sleep for half a second
        while True:
                chunk = os.read(r, 4096)
                if chunk == b'':
                    break
                buf += chunk
        print(f"read buffer is: {len(buf)}")
        print(f"writes are consistent : {checker(buf, N)}")
        for pid in pids:
            (_, status) = os.waitpid(pid, 0)
            print(f"Wait pid for {pid} with exit code: {os.waitstatus_to_exitcode(status)}")

# this checker's objective: each writer's messages are recoverable, intact and in-order
# should check against 4 processes that is hardcoded
def checker(buf: bytes, N: int) -> bool:
    output = buf.decode()
    runs = [(k, len(list(g))) for k, g in groupby(output)]
    totals = Counter()
    for k, n in runs:
        totals[k] += n
    print(totals)
    if len(runs) > 4:
        print("".join(k for k, _ in runs))
        print([(k, n // 4096) for k, n in runs])
        print("Write might be torn!!")
    return len(runs) == 4



def pipe_tests():
    r, w = os.pipe()
    pid = os.fork()
    if pid == 0:
        # this is the child
        try:
            os.dup2(w, 1)
            print("Hello from child?")
            # os.close(w)
        finally:
            os._exit(127)
    else:
        os.close(w)
        print(f"the parent process is reading: {os.read(r, 1024)}")
        print(f"the parent process is reading: {os.read(r, 1024)}")
        (_, status) = os.waitpid(pid, os.WNOHANG)

def main():
    PIPE_BUF = 4096
    N = 256*1024
    pipe_concurrent_writes(N)

if __name__ == '__main__':
    main()
