import os
import sys
def subprocess(path: str, arguments: list, stdout_path):
    pid = os.fork()
    if pid == 0:
        # this is the child
        try:
            fd2 = os.open(stdout_path, os.O_CREAT | os.O_RDWR)
            fd2 = os.dup2(fd2, 1)
            os.execve(path, arguments, {})
        finally:
            os._exit(127)
    else:
        _, waitstatus = os.waitpid(pid, os.WNOHANG)
        return os.waitstatus_to_exitcode(waitstatus)

def main():
    path = sys.argv[1]
    i_arguments = sys.argv[2]
    stdout_path = sys.argv[3]
    i_arguments = i_arguments.split()
    print(f"the arguments for the binary is: {i_arguments}")
    arguments = [path, i_arguments]
    print(f"result is {subprocess(path, arguments, stdout_path)}")

if __name__ == '__main__':
    main()
