#!/bin/bash
if [ -z "$commit_hash" ]; then
  echo "Usage: $0 <commit_hash>"
  exit 1
fi

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" &>/dev/null && pwd)

APP_PATH="${SCRIPT_DIR}/../.."
PNG_BUILD_PATH=$APP_PATH/build/png
PNG_SRC_PATH=$APP_PATH/src/png
PNG_INSTALL_PATH=$APP_PATH/build/png
PNG_LIB_PATH=$APP_PATH/libs/build_png_libs/

mkdir -p $PNG_BUILD_PATH/
rm -rf $PNG_BUILD_PATH/png_$commit_hash && rm -rf $PNG_INSTALL_PATH/$commit_hash
cp -r $PNG_SRC_PATH $PNG_BUILD_PATH/png_$commit_hash
cd $PNG_BUILD_PATH/png_$commit_hash
git reset --hard $commit_hash

XZON_ROOT_PATH=/home/xjy/fuzz/else/fuzz_project
XZ7Z_BUILD_PATH=$APP_PATH/build/7zxz
XZ7Z_SRC_PATH=$APP_PATH/src/7zxz
XZ7Z_INSTALL_PATH=$APP_PATH/build/7zxz
XZ7Z_LIB_PATH=$APP_PATH/libs/build_7zxz_libs/

mkdir -p $XZ7Z_BUILD_PATH/
rm -rf $XZ7Z_BUILD_PATH/7zxz_count 
# cp -r $XZ7Z_SRC_PATH $XZ7Z_BUILD_PATH/7zxz_count
mkdir -p $XZ7Z_BUILD_PATH/7zxz_count/src_to_build/
cd $XZ7Z_BUILD_PATH/7zxz_count
# cp $XZ7Z_LIB_PATH/Makefile.build ./src_to_build/Makefile
# cp $XZ7Z_LIB_PATH/7z_parse.c ./src_to_build/
mkdir -p $XZ7Z_BUILD_PATH/7zxz_count/src_to_build/output

# git reset --hard $commit_hash


# fileList=(Threads.h Threads.c MtDec.h MtDec.c 7zAlloc.c  7zBuf2.c  7zCrc.c     7zDec.c   7zStream.c  Bcj2.c   Bra.c      CpuArch.c  Lzma2Dec.c  Ppmd7.c Sha256.c   XzCrc64.c  XzDec.c 7zArcIn.c 7zBuf.c   7zCrcOpt.c  7zFile.c  Alloc.c     Bra86.c  BraIA64.c  Delta.c    LzmaDec.c   Ppmd7Dec.c  Sha256Opt.c  Xz.c XzCrc64Opt.c  XzIn.c 7zAlloc.h  7zCrc.h   7z.h       7zVersion.h  Bcj2.h  Compiler.h  Delta.h     LzmaDec.h  Ppmd.h     RotateDefs.h  XzCrc64.h 7zBuf.h    7zFile.h  7zTypes.h  Alloc.h      Bra.h   CpuArch.h   Lzma2Dec.h  Ppmd7.h    Precomp.h  Sha256.h   Xz.h )

# for filename in ${fileList[*]} 
    # do
        # cp /home/fdl/diff-fuzz/examples/src/libs/xz_7znewest/C/$filename .
        # cp $XZ7Z_BUILD_PATH/7zxz_count_$commit_hash/C/$filename ./src_to_build
    # done

cp -r $XZ7Z_LIB_PATH/xz_7znewest/* ./src_to_build
# mv ./src_to_build/Makefile ./src_to_build/Makefile.bak
cp $XZ7Z_LIB_PATH/Makefile.build ./src_to_build/Makefile
# cp $XZON_ROOT_PATH/misc/Makefile ./src_to_build/Makefile
# cp $XZ7Z_LIB_PATH/7z_parse.c ./src_to_build/

cd $XZ7Z_BUILD_PATH/7zxz_count/src_to_build/
CXX="/home/xjy/fuzz/else/llvm-project/build/bin/clang++"
CC="/home/xjy/fuzz/else/llvm-project/build/bin/clang"

make clean
make all CC=$CC CXX=$CXX CFLAGS="-g -fsanitize=address,fuzzer-no-link -fsanitize-coverage=edge,indirect-calls -fPIC" COMMIT_HASH=$commit_hash
cp $XZ7Z_BUILD_PATH/7zxz_count/src_to_build/lib7zxz_.so $XZ7Z_LIB_PATH/lib7zxz.so

