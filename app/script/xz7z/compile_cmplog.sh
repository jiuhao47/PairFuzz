#!/usr/bin/env bash
set -e

commit_hash=$1


SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" &>/dev/null && pwd)

APP_PATH="${SCRIPT_DIR}/../.."
XZ7Z_BUILD_PATH=$APP_PATH/build/xz7z_cmplog
XZ7Z_SRC_PATH=$APP_PATH/src/xz_7znewest
XZ7Z_LIB_PATH=$APP_PATH/libs/build_xz7z_libs/

rm -rf $XZ7Z_BUILD_PATH/xz7z
mkdir -p $XZ7Z_BUILD_PATH/xz7z/src_to_build/
cd $XZ7Z_BUILD_PATH/xz7z

mkdir -p $XZ7Z_BUILD_PATH/xz7z/src_to_build/output


cp -r $XZ7Z_SRC_PATH/* ./src_to_build

cp $XZ7Z_LIB_PATH/Makefile.build ./src_to_build/Makefile


cd $XZ7Z_BUILD_PATH/xz7z/src_to_build/
CC="${SCRIPT_DIR}/../../../fuzzers/AFLplusplus/afl-clang-fast"
CXX="${SCRIPT_DIR}/../../../fuzzers/AFLplusplus/afl-clang-fast++"
CFLAGS="-fsanitize=address -fPIC"
CXXFLAGS="-fsanitize=address -fPIC"
mkdir -p $XZ7Z_BUILD_PATH/install
mkdir -p $XZ7Z_BUILD_PATH/include
make clean
AFL_LLVM_CMPLOG=1 make all CC=$CC CXX=$CXX CFLAGS="$CFLAGS" CXXFLAGS="$CXXFLAGS" COMMIT_HASH=$commit_hash
cp $XZ7Z_BUILD_PATH/xz7z/src_to_build/libxz7z_.a $XZ7Z_BUILD_PATH/install/libxz7z.a
cp -r $XZ7Z_SRC_PATH/* $XZ7Z_BUILD_PATH/include/
rm -rf $XZ7Z_BUILD_PATH/xz7z
