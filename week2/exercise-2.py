import os

# implement rmtree that recursively deletes a given file directory structure
# first version of the rmtree
def rmtree(inpath):
    '''
    step 1: scan the given directory
    step 2: recurse down if the child is a directory, if it's a file unlink it
    step 3:
    '''
    # interesting behavior: the file is deleted by the the error says no file or directroy with a name like that? that means
    # scandir has a stale entry?
    with os.scandir(inpath) as it:
        for entry in it:
            if entry.is_dir():
                current_path=inpath +"/"+entry.name
                rmtree(current_path)
            elif entry.is_file():
                current_path=inpath +"/"+entry.name
                os.unlink(current_path)
    os.rmdir(inpath)

'''
Follow up
What does your rmtree implementation do if the directory contains a symlink? Is there a security vulnerability? How can you fix it?

Symlink races can be created if the program is not careful
https://en.wikipedia.org/wiki/Symlink_race
'''

def rmtree2(inpath):
    with os.scandir(inpath) as it:
        for entry in it:
            if entry.is_dir(follow_symlinks=False):
                current_path = inpath + "/" + entry.name
                rmtree(current_path)
            elif entry.is_file():
                current_path = inpath + "/" + entry.name
                os.unlink(current_path)
    os.rmdir(inpath)

'''
rmtree2 still has a flaw that can be exploited with timing/race condition attacks
reference article: https://michael.orlitzky.com/articles/posix_hardlink_heartache.xhtml

Avoiding TOCTOU w.r.t directory scan: avoid using paths -- use fds
enforces a no follow symlink rule by induction (as we pass fd each time we open a dir/read its contents)

Even hardlinks (with a safety check that it has only one name) are vulnerable to a TOCTOU attack where
the bad actor can remove the unsafe hardlink in between the check and the good actor might proceed to move forward
with the understanding that the hardlink is safe
'''

def rmtree3(inpath):
    # Step 1: open a file descriptor for the given path to prepare for
    # recursive expedition
    try:
        fd = os.open(inpath, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
        rmtree3_internal(fd)
        os.rmdir(inpath)
    finally:
        os.close(fd)

def rmtree3_internal(dir_fd):
    # Step 2: we need to be able to list all entries within the dir_fd
    # force a on-demand evaluation of all the elements of the tree instead of lazy
    entries = list(os.scandir(dir_fd))
    for entry in entries:
        if entry.is_dir(follow_symlinks=False):
            try:
                # we use open syscall with flags that will avoid symlinks
                subdir_fd = os.open(entry.name, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd = dir_fd)
            except Exception as e:
                print(f"I will try to delete: {entry.name}")
                os.unlink(entry.name, dir_fd = dir_fd)
            else:
                # now we know that we have a directory in our hand, start the recursive call
                try:
                    rmtree3_internal(subdir_fd)
                    print(f"I will try to delete: {entry.name}")
                    os.rmdir(entry.name, dir_fd = dir_fd)
                finally:
                    os.close(subdir_fd)
        else:
            print(f"I will try to delete: {entry.name}")
            os.unlink(entry.name, dir_fd = dir_fd)



def main():
    rmtree3("xyz")


if __name__ == '__main__':
    main()
