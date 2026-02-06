#!/usr/bin/env bash
# libtar :6d0ab4c 只用这一个commit hash

set -e

commit_hash=$1

if [ -z "$commit_hash" ]; then
  echo "Usage: $0 <commit_hash>"
  exit 1
fi

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" &>/dev/null && pwd)

APP_PATH="${SCRIPT_DIR}/../.."
libtar_BUILD_PATH=$APP_PATH/build/libtar_cmplog
libtar_SRC_PATH=$APP_PATH/src/libtar
libtar_INSTALL_PATH=$APP_PATH/build/libtar_cmplog
libtar_LIB_PATH=$APP_PATH/libs/build_libtar_libs/

mkdir -p $libtar_BUILD_PATH/
rm -rf $libtar_BUILD_PATH/libtar_$commit_hash && rm -rf $libtar_INSTALL_PATH/$commit_hash
cp -r $libtar_SRC_PATH $libtar_BUILD_PATH/libtar_$commit_hash
cd $libtar_BUILD_PATH/libtar_$commit_hash

CC="${SCRIPT_DIR}/../../../fuzzers/AFLplusplus/afl-clang-fast"
CXX="${SCRIPT_DIR}/../../../fuzzers/AFLplusplus/afl-clang-fast++"
CFLAGS="-fsanitize=address -fPIC"
CXXFLAGS="-fsanitize=address -fPIC"

cd $libtar_BUILD_PATH/libtar_$commit_hash

autoreconf --force --install
./configure  CXX=$CXX CC=$CC CFLAGS="$CFLAGS" CXXFLAGS="$CXXFLAGS"

AFL_LLVM_CMPLOG=1 make -j16

cd $libtar_INSTALL_PATH && mkdir -p $commit_hash
cd $libtar_INSTALL_PATH/libtar_$commit_hash
make install DESTDIR=$libtar_INSTALL_PATH/$commit_hash
rm -rf $libtar_BUILD_PATH/libtar_$commit_hash
