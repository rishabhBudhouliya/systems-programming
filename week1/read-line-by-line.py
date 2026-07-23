# Read a file line by line
import os

def read(inpath):
    try:
        input_fd = os.open(inpath, os.O_RDWR)
    except Exception as e:
        print(f"Could not open the file: {e}")

    chunk = 1024
    buf = []
    while True:
       input = os.read(input_fd, chunk)
       if input == b'':
           break
       else:
           buf.append(input)
    result = []
    for line_buf in buf:
        line = []
        for ch in line_buf:
            if bytes([ch]) == b'\n':
                result.append(line)
                line = []
            else:
                line.append(ch)

    for chunk in result:
        print(bytes(chunk))


def main():
    read('exercise-2-input.txt')


if __name__ == "__main__":
    main()
