#!/usr/bin/env bash

# - libIM_newest.so: 0729cc470
# - libIM_oldest.so: 4c717b784
commit_hash=$1

if [ -z "$commit_hash" ]; then
  echo "Usage: $0 <commit_hash>"
  exit 1
fi

export PATH="/opt/clang+llvm-11.0.0/bin:$PATH"
# check if clang version is 11 (robust extraction of the major version number)
CLANG_VERSION=$(clang --version | head -n 1 | grep -oE '[0-9]+' | head -n1 || true)
if ! [[ "$CLANG_VERSION" =~ ^[0-9]+$ ]]; then
  echo "Error: unable to parse clang version: $CLANG_VERSION"
  exit 1
fi
if [ "$CLANG_VERSION" -ne 11 ]; then
  echo "Error: clang version 11 is required. Current version is $CLANG_VERSION"
  exit 1
fi

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" &>/dev/null && pwd)

APP_PATH="${SCRIPT_DIR}/../.."
IM_BUILD_PATH=$APP_PATH/build/IM_afl
IM_SRC_PATH=$APP_PATH/src
IM_INSTALL_PATH=$APP_PATH/build/IM_afl
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

make -j 16


# mkdir build
# cd build
# cmake .. -DCMAKE_C_COMPILER=$CC -DCMAKE_CXX_COMPILER=$CXX -DCMAKE_C_FLAGS="-O0 -fsanitize=address,fuzzer-no-link -fsanitize-coverage=edge,indirect-calls -fPIC" -DCMAKE_CXX_FLAGS="-O0 -fsanitize=address,fuzzer-no-link -fsanitize-coverage=edge,indirect-calls -fPIC"
# make -j8

cd $IM_INSTALL_PATH && mkdir -p $commit_hash
cd $IM_INSTALL_PATH/IM_$commit_hash/
make install DESTDIR=$IM_INSTALL_PATH/$commit_hash
rm -rf $IM_BUILD_PATH/IM_$commit_hash
# cd $IM_LIB_PATH
# make genlib_thread COMMIT_HASH=$commit_hash
# make genlib_docker COMMIT_HASH=$commit_hash
# make test_thread
