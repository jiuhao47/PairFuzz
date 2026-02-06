#!/usr/bin/env bash

# - libIM_newest.so: 0729cc470
# - libIM_oldest.so: 4c717b784
commit_hash=$1

if [ -z "$commit_hash" ]; then
  echo "Usage: $0 <commit_hash>"
  exit 1
fi

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" &>/dev/null && pwd)

APP_PATH="${SCRIPT_DIR}/../.."
IM_BUILD_PATH=$APP_PATH/build/IM_cmplog
IM_SRC_PATH=$APP_PATH/src
IM_INSTALL_PATH=$APP_PATH/build/IM_cmplog
IM_LIB_PATH=$APP_PATH/libs/build_IM_libs/

mkdir -p $IM_BUILD_PATH/
rm -rf $IM_BUILD_PATH/IM_$commit_hash && rm -rf $IM_INSTALL_PATH/$commit_hash
cp -r $IM_SRC_PATH/IM $IM_BUILD_PATH/IM_$commit_hash


CC="${SCRIPT_DIR}/../../../fuzzers/AFLplusplus/afl-clang-fast"
CXX="${SCRIPT_DIR}/../../../fuzzers/AFLplusplus/afl-clang-fast++" # because AFL compiler fail to compile IM
# CC="/home/dev/workspace/AFL/afl-clang-fast"
# CXX="/home/dev/workspace/AFL/afl-clang-fast++"
CFLAGS="-fsanitize=address -fPIC"
CXXFLAGS="-fsanitize=address -fPIC"
cd $IM_BUILD_PATH/IM_$commit_hash
git reset --hard $commit_hash

./configure CC="$CC" CXX="$CXX" CFLAGS="$CFLAGS" CXXFLAGS="$CXXFLAGS"

AFL_LLVM_CMPLOG=1 make -j 16

cd $IM_INSTALL_PATH && mkdir -p $commit_hash
cd $IM_INSTALL_PATH/IM_$commit_hash/
make install DESTDIR=$IM_INSTALL_PATH/$commit_hash
rm -rf $IM_BUILD_PATH/IM_$commit_hash
