#!/usr/bin/env bash
set -e
#   - libxz_newest.so: 15cf3f04
#   - libxz_oldest.so: 98f3cac1a
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
XZ_BUILD_PATH=$APP_PATH/build/xz_afl
XZ_SRC_PATH=$APP_PATH/src/xz
XZ_INSTALL_PATH=$APP_PATH/build/xz_afl
XZ_LIB_PATH=$APP_PATH/libs/build_xz_libs/

mkdir -p $XZ_BUILD_PATH/
rm -rf $XZ_BUILD_PATH/xz_$commit_hash  rm -rf $XZ_INSTALL_PATH/$commit_hash
cp -r $XZ_SRC_PATH $XZ_BUILD_PATH/xz_$commit_hash
cd $XZ_BUILD_PATH/xz_$commit_hash
if [ "$commit_hash" = "dep" ]; then
  git reset --hard a522a22 # 5.8.1
else
  git reset --hard $commit_hash
  python3 $APP_PATH/script/xz/patch_checksum.py $XZ_BUILD_PATH/xz_$commit_hash
fi

./autogen.sh
mkdir -p build
cd build

CC="${SCRIPT_DIR}/../../../fuzzers/AFL/afl-clang-fast"
CXX="${SCRIPT_DIR}/../../../fuzzers/AFL/afl-clang-fast++"
CFLAGS="-fsanitize=address -fPIC"
CXXFLAGS="-fsanitize=address -fPIC"

../configure --disable-ifunc --disable-sandbox CC=$CC CXX=$CXX CFLAGS="$CFLAGS" CXXFLAGS="$CXXFLAGS"
# cmake .. -D CMAKE_C_COMPILER="clang" -D CMAKE_CXX_COMPILER="clang++" -DCMAKE_C_FLAGS="-O0 -fsanitize=address,fuzzer-no-link -fsanitize-coverage=edge,indirect-calls -fPIC " -DCMAKE_CXX_FLAGS=" -O0 -fsanitize=address,fuzzer-no-link -fsanitize-coverage=edge,indirect-calls -fPIC "
make -j16
mkdir -p ../install
cd $XZ_INSTALL_PATH && mkdir -p $commit_hash
cd $XZ_INSTALL_PATH/xz_$commit_hash/build
make install DESTDIR=$XZ_INSTALL_PATH/$commit_hash
rm -rf $XZ_BUILD_PATH/xz_$commit_hash

# cd $XZ_LIB_PATH
# make genlib_thread COMMIT_HASH=$commit_hash
# make genlib_docker COMMIT_HASH=$commit_hash
# make test_thread
