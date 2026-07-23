import os
import time

def read(inpath: str):
    read_fd = os.open(inpath, os.O_RDONLY)
    chunk = 4096
    while True:
        data = os.read(read_fd, chunk)
        if not data:
            time.sleep(0.001)
        else:
            print(data)


def main():
    inpath = "exercise-3.txt"
    read(inpath)

if __name__ == '__main__':
    main()
