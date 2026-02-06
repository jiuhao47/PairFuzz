#!/usr/bin/env bash

commit_hash=$1

set -e


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
SPNG_BUILD_PATH=$APP_PATH/build/spng_afl
SPNG_SRC_PATH=$APP_PATH/src/libspng
SPNG_INSTALL_PATH=$APP_PATH/build/spng_afl
SPNG_LIB_PATH=$APP_PATH/libs/build_spng_libs/

mkdir -p $SPNG_BUILD_PATH/
rm -rf $SPNG_BUILD_PATH/spng_$commit_hash && rm -rf $SPNG_INSTALL_PATH/$commit_hash
cp -r $SPNG_SRC_PATH $SPNG_BUILD_PATH/spng_$commit_hash
cd $SPNG_BUILD_PATH/spng_$commit_hash
git reset --hard $commit_hash


CC="${SCRIPT_DIR}/../../../fuzzers/AFL/afl-clang-fast"
CXX="${SCRIPT_DIR}/../../../fuzzers/AFL/afl-clang-fast++"
CFLAGS="-fPIC -fsanitize=address"
CXXFLAGS="-fPIC -fsanitize=address"

# cmake build for spng
mkdir build
cd build
cmake .. -DCMAKE_C_COMPILER="$CC" -DCMAKE_CXX_COMPILER="$CXX" -DCMAKE_C_FLAGS="$CFLAGS" -DCMAKE_CXX_FLAGS="$CXXFLAGS" -DCMAKE_POLICY_VERSION_MINIMUM=3.5
make -j 16

cp $SPNG_BUILD_PATH/spng_$commit_hash/src/spng.h $SPNG_BUILD_PATH/spng_$commit_hash/spng.h || cp $SPNG_BUILD_PATH/spng_$commit_hash/spng/spng.h $SPNG_BUILD_PATH/spng_$commit_hash/spng.h


cd $SPNG_INSTALL_PATH && mkdir -p $commit_hash
cd $SPNG_BUILD_PATH/spng_$commit_hash/build
make install DESTDIR=$SPNG_INSTALL_PATH/$commit_hash
cp $SPNG_BUILD_PATH/spng_$commit_hash/build/libspng_static.a $SPNG_INSTALL_PATH/$commit_hash/usr/local/lib/libspng_static.a

rm -rf $SPNG_BUILD_PATH/spng_$commit_hash

# cd $SPNG_LIB_PATH
# make genlib_thread COMMIT_HASH=$commit_hash
# make genlib_docker COMMIT_HASH=$commit_hash
# make test_thread
