import os
import sys
def subprocess(path: str, arguments: list, stdout_path):
    pid = os.fork()
    if pid == 0:
        # this is the child
        fd2 = os.open(stdout_path, os.O_CREAT | os.O_RDWR)
        fd2 = os.dup2(fd2, 1)
        os.execve(path, arguments, {})
    else:
        result = os.waitpid(pid, os.WNOHANG)
        return result

def main():
    path = sys.argv[1]
    i_arguments = sys.argv[2]
    stdout_path = sys.argv[3]
    arguments = [path, i_arguments]
    result = subprocess(path, arguments, stdout_path)
    print("hello am I being printed where?")

if __name__ == '__main__':
    main()
