# two purpose
# 1.patch the checksum
# 2.patch the test program in testApp

import sys
import os
import subprocess
from subprocess import call

TIMEOUT = 5

"""
desc: dump logs (msg) to the given file
"""


def record_logs(message, filename):
    if not os.path.exists(filename):
        p_dir = os.path.dirname(filename)
        cmd = "mkdir -p {}".format(p_dir)
        os.system(cmd)
    cmd = "echo '{}' >> {}".format(message, filename)
    os.system(cmd)


def pureRun(cmd):
    proc = subprocess.Popen(cmd, shell=True, stdout=subprocess.PIPE)
    try:
        stdout = proc.communicate(timeout=TIMEOUT)[0]
    except Exception as e:
        print(f"error, cmd:{cmd}", e)
        record_logs("timeout error, cmd:{}".format(cmd), EXP_FILE)
        return None
    if len(stdout) == 0:
        return None
    return stdout.decode("utf-8")


# patch it with line
def replace_pattern(filename):
    pattern = "incorrect data check"
    fp = open(filename, "r")
    lines = fp.readlines()
    fp.close()
    comment_lines_cnt = 0
    with open(filename, "w") as outfile:
        for item in lines:
            if item.find(pattern) >= 0:
                comment_lines_cnt = 3
            if comment_lines_cnt > 0:
                item = "// " + item
                comment_lines_cnt -= 1
            outfile.write(item)


def patch_checksum(repo_dir):
    filename = os.path.join(repo_dir, "inflate.c")
    replace_pattern(filename)


def main():
    if len(sys.argv) < 2:
        print("Usage: python patch_checksum.py <repo_dir>")
        sys.exit(1)
    REPO_DIR = sys.argv[1]
    patch_checksum(REPO_DIR)


if __name__ == "__main__":
    main()
