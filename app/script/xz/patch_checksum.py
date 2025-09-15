# two purpose
# 1.patch the checksum
# 2.patch the test program in testApp

import sys
import os
import subprocess
from subprocess import call


# dynamically get the path


# 1.zlib checksum; 2.test program
# patch it with the first return LZMA_DATA_ERROR;
def replace_pattern(filename, pattern, line_cnt):
    fp = open(filename, "r")
    lines = fp.readlines()
    fp.close()
    startFlag = False
    with open(filename, "w") as outfile:
        for item in lines:
            if startFlag:
                if item.find("return LZMA_DATA_ERROR;") >= 0:
                    item = " {int FDL_TEST=2;} // " + item
                    startFlag = False
            if item.find(pattern) >= 0:
                startFlag = True
            outfile.write(item)


# patch it with line if (0)
# def replace_pattern(filename, pattern, line_cnt):
#     fp = open(filename,'r')
#     lines = fp.readlines()
#     fp.close()
#     comment_lines_cnt = 0
#     with open(filename, 'w') as outfile:
#         for item in lines:
#             if item.find(pattern) >= 0:
#                 comment_lines_cnt = line_cnt
#             if comment_lines_cnt > 0:
#                 item = "if (0) " + item
#                 comment_lines_cnt -= 1
#             outfile.write(item)


def patch_checksum(repo_dir):
    # 1. the file index_hash.c
    filename = repo_dir + "/" + "src/liblzma/common/index_hash.c"
    pattern = "(index_hash->crc32"
    line_cnt = 1
    replace_pattern(filename, pattern, line_cnt)

    # 2. the file block_header_decoder.c
    filename = repo_dir + "/" + "src/liblzma/common/block_header_decoder.c"
    pattern = "lzma_crc32(in, in_size, 0) !="
    line_cnt = 1
    replace_pattern(filename, pattern, line_cnt)

    # 3. index_decoder.c
    filename = repo_dir + "/" + "src/liblzma/common/index_decoder.c"
    pattern = "(((coder->crc32 >>"
    line_cnt = 1
    replace_pattern(filename, pattern, line_cnt)

    # 4. stream_flags_decoder
    filename = repo_dir + "/" + "src/liblzma/common/stream_flags_decoder.c"
    pattern = "crc !="
    line_cnt = 1
    replace_pattern(filename, pattern, line_cnt)


def main():
    if len(sys.argv) < 2:
        print("Usage: python 3-patch_checksum.py <repo_dir>")
        sys.exit
    REPO_DIR = sys.argv[1]
    patch_checksum(REPO_DIR)


if __name__ == "__main__":
    main()
