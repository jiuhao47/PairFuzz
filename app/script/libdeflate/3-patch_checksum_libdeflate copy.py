
# two purpose
# 1.patch the checksum
# 2.patch the test program in testApp

import sys
import os
import subprocess
from subprocess import call

LOG_DIR = "/home/fdl/diff-fuzz/LocateFixes/LocateFix-libdeflate/logs"
REPO_DIR = "/home/fdl/diff-fuzz/LocateFixes/worklibs/libdeflate"
WORK_DIR = "/home/fdl/diff-fuzz/LocateFixes/LocateFix-libdeflate"
EXP_FILE = LOG_DIR + "/exception.txt"
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
        stdout =  proc.communicate(timeout=TIMEOUT)[0]
    except Exception as e:
        print(f"error, cmd:{cmd}", e)
        record_logs("timeout error, cmd:{}".format(cmd), EXP_FILE)
        return None
    if len(stdout) == 0:
        return None
    return stdout.decode('utf-8')
# dynamically get the path

# 1.zlib checksum; 2.test program


def patch_checksum():
    cmdList = [
        "cd "+ REPO_DIR + """&& find . -name zlib_decompress.c -exec sed -i 's|\\b\([a-z_]*adler32\)(\\b|0 \&\& \\1\(|g' {} \;"""
    ]
    for cmd in cmdList:
        print("cmd:{}".format(cmd))
        os.system(cmd)
    cmd = f"find {REPO_DIR} -name zlib_decompress.c -exec grep 'struct deflate_decompressor' {{}} \;"
    ret = pureRun(cmd)
    print("pureRun ret",ret)
    # ret is None indicating this is new
    if ret is None:
        cp_cmd = f"cp {WORK_DIR}/scripts/new_test_zlib.cpp {WORK_DIR}/testApp/test_zlib.cpp"
    else:
        cp_cmd = f"cp {WORK_DIR}/scripts/old_test_zlib.cpp {WORK_DIR}/testApp/test_zlib.cpp"
    os.system(cp_cmd)

def record_exception():
    cmdList = [
        f"cd {REPO_DIR} && git log|head -n 1 >> {LOG_DIR}/patch_exception.txt",
    ]
    for cmd in cmdList:
        os.system(cmd)

def main():
    patch_checksum()
    

if __name__ == "__main__":
    main()

