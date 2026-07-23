import os
from duplicate.duplicate_file import duplicate
import time
import cProfile
import pstats

def mirrortree(inpath, outpath):
    try:
        output_root_fd = os.mkdir(outpath, 0o777)
    except Exception as e:
        print(f"we got an error while creating the directory: {e}")
        return
    internal_mirror(inpath, outpath)

def internal_mirror(inpath, outpath):
    with os.scandir(inpath) as it:
        for entry in it:
            if entry.is_file():
                try:
                    os.link(inpath + "/" + entry.name, outpath + "/" + entry.name)
                except Exception as e:
                    print(f"hard link failed due to: {e}")
                return
            elif entry.is_dir():
                os.mkdir(outpath + "/" + entry.name, 0o777)
                internal_mirror(inpath + "/" + entry.name, outpath + "/" + entry.name)

def copytree(inpath, outpath, dry_run=False):
    # load the inpath directory first
    # create an empty new output directory
    # copy the entire tree structure to the new output directory

    # let's use os.scandir: ref: https://peps.python.org/pep-0471/
    # two benefits 1) it avoids stat call with each traversal 2) it doesn't return the entire filename list, it's an iteratable?

    try:
        output_root_fd = os.mkdir(outpath, 0o777)
    except Exception as e:
        print(f"we got an error while creating the directory: {e}")
        return
    internal_copytree(inpath, outpath, dry_run)

def internal_copytree(inpath: str, outpath: str, dry_run: bool):
    with os.scandir(inpath) as it:
        for entry in it:
            if entry.is_file():
                if dry_run:
                    stat_info = os.stat(inpath + "/" + entry.name)
                    if stat_info:
                        print(f"Instead of creating the file: {entry.name}, here's the total number of bytes: {stat_info.st_size}")
                else:
                    duplicate(inpath + "/" + entry.name, outpath + "/" + entry.name)
                continue
            elif entry.is_dir():
                os.mkdir(outpath + "/" + entry.name, 0o777)
                internal_copytree(inpath + "/" + entry.name, outpath + "/" + entry.name, dry_run)

def main():
    profiler = cProfile.Profile()
    profiler.enable()
    mirrortree("test", "xyz")
    profiler.disable()

    stats = pstats.Stats(profiler).sort_stats('tottime')
    stats.print_stats()


if __name__ == '__main__':
    main()
