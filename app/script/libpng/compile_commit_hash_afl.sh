#!/usr/bin/env bash

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
PNG_BUILD_PATH=$APP_PATH/build/png_afl
PNG_SRC_PATH=$APP_PATH/src/libpng
PNG_INSTALL_PATH=$APP_PATH/build/png_afl
PNG_LIB_PATH=$APP_PATH/libs/build_png_libs/

mkdir -p $PNG_BUILD_PATH/
rm -rf $PNG_BUILD_PATH/png_$commit_hash && rm -rf $PNG_INSTALL_PATH/$commit_hash
cp -r $PNG_SRC_PATH $PNG_BUILD_PATH/png_$commit_hash
cd $PNG_BUILD_PATH/png_$commit_hash
if [ "$commit_hash" = "dep" ]; then
  git reset --hard 872555f # 1.6.47
else
  git reset --hard $commit_hash
fi



CC="${SCRIPT_DIR}/../../../fuzzers/AFL/afl-clang-fast"
CXX="${SCRIPT_DIR}/../../../fuzzers/AFL/afl-clang-fast++"
CFLAGS="-fPIC -fsanitize=address"
CXXFLAGS="-fPIC -fsanitize=address"

# ./configure --disable-static --enable-shared CC="$CC" CXX="$CXX" CFLAGS="-g -fPIC -fsanitize=address,fuzzer-no-link -fsanitize-coverage=edge,indirect-calls" CXXFLAGS="-g -fPIC -fsanitize=address,fuzzer-no-link -fsanitize-coverage=edge,indirect-calls"
# ./configure --disable-static --enable-shared CC="$CC" CXX="$CXX" 
./configure CC="$CC" CXX="$CXX" CFLAGS="$CFLAGS" CXXFLAGS="$CXXFLAGS"

make -j 16


cd $PNG_INSTALL_PATH && mkdir -p $commit_hash
cd $PNG_INSTALL_PATH/png_$commit_hash
make install DESTDIR=$PNG_INSTALL_PATH/$commit_hash
rm -rf $PNG_BUILD_PATH/png_$commit_hash

# cd $PNG_LIB_PATH
# make genlib_thread COMMIT_HASH=$commit_hash
# make genlib_docker COMMIT_HASH=$commit_hash
# make test_thread
