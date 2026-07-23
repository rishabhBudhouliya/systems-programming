
# Duplicating a file
import os
import time
import cProfile
import pstats

# creates or returns active file descriptors for given paths
def open_create(inpath, outpath):
    try:
        input_fd = os.open(inpath, os.O_CREAT | os.O_RDWR)
    except:
        print("Could not create the input file")

    try:
        output_fd = os.open(outpath,  os.O_CREAT | os.O_EXCL | os.O_RDWR)
    except Exception as e:
        print(f"Could not create the output file: {e}")

    # content = "Hello, this is my first systemsy work".encode('UTF-8')
    count = 0
    content = []
    while count < 100000:
        content.append("Hello, this is my first systemsy work".encode('UTF-8'))
        count+=1

    content = b''.join(content)
    try:
        buf = os.write(input_fd, content)
    except Exception as e:
        print(f"Could not write to input file: {e}")

    if len(content) != buf:
        print("Failure in writing content to input txt")
        return
    os.close(input_fd)
    os.close(output_fd)

# duplicates the inpath file's content to outpath
def duplicate(inpath, outpath):
    '''
    Create an input file, seed it with content
    Create an output file
    Write the content from input to output
    Close the opened file
    '''
    #open_create(inpath, outpath)
    try:
        input_fd = os.open(inpath, os.O_RDONLY)
    except Exception as e:
        print(f"Could not open the input file: {e}")
    try:
        output_fd = os.open(outpath,  os.O_CREAT | os.O_EXCL | os.O_RDWR)
    except Exception as e:
        print(f"Could not create the output file: {e}")

    try:
        input_stat = os.stat(input_fd)
    except Exception as e:
        print(f"os.stat failed due to {e}")
    written_buf = os.sendfile(output_fd, input_fd, 0, input_stat.st_size)
    assert input_stat.st_size == written_buf
    # chunk = 4096
    # input_buf = []
    # while True:
    #     buf = os.read(input_fd, chunk)
    #     if not buf:
    #         break
    #     input_buf.append(buf)

    # written_buf = os.write(output_fd, b''.join(input_buf))

    os.close(input_fd)
    os.close(output_fd)

def duplicate_with_range(inpath: str, outpath: str, range_start: int, range_end: int):
    open_create(inpath, outpath)

    input_fd = os.open(inpath, os.O_RDONLY)
    output_fd = os.open(outpath, os.O_WRONLY)


    pointer = os.lseek(input_fd, range_start, os.SEEK_CUR)
    actual_offset = range_end - range_start
    buf = os.read(input_fd, actual_offset)
    os.write(output_fd, buf)

    os.close(input_fd)
    os.close(output_fd)

# block size = 1024, time = 0.211 seconds
# block size = 2048, time = 0.171 seconds
# block size = 3172, time = 0.178 seconds
# block size = 4096, time = 0.174 seconds
def main():
    input_file = "test.txt"
    output_file = "test_output.txt"
    profiler = cProfile.Profile()
    profiler.enable()
    duplicate(input_file, output_file)
    profiler.disable()

    stats = pstats.Stats(profiler).sort_stats('tottime')
    stats.print_stats()


if __name__ == "__main__":
    main()
