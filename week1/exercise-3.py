# Simultaneous read/write
# one process writes in a file, while the other process tries to read it
import os
import time
'''
Note that a successful write() may transfer fewer than count bytes.  Such partial writes can occur for various reasons; for example, because there was insufficient space on
the disk device to write all of the requested bytes, or because a blocked write() to a socket, pipe, or similar was interrupted by a signal handler after it had transferred
some,  but before it had transferred all of the requested bytes.  In the event of a partial write, the caller can make another write() call to transfer the remaining bytes.
The subsequent call will either transfer further bytes or may result in an error (e.g., if the disk is now full)
'''

def write(inpath: str):
    write_fd = os.open(inpath, os.O_CREAT | os.O_WRONLY)

    for i in range(1000):
        data = b"chunk %d\n" % i
        buf_len = os.write(write_fd, data)
        time.sleep(0.1)
        assert len(data) == buf_len

def main():
    inpath = "exercise-3.txt"
    write(inpath)


if __name__ == '__main__':
    main()
