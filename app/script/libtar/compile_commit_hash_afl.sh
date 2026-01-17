#!/usr/bin/env bash
# libtar :6d0ab4c 只用这一个commit hash

set -e

# https://repo.or.cz/libtar.git
# 不是git库，而是安装包，自己挑版本的那种……

# 不考虑commit_hash了吧……

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
libtar_BUILD_PATH=$APP_PATH/build/libtar_afl
libtar_SRC_PATH=$APP_PATH/src/libtar
libtar_INSTALL_PATH=$APP_PATH/build/libtar_afl
libtar_LIB_PATH=$APP_PATH/libs/build_libtar_libs/

mkdir -p $libtar_BUILD_PATH/
rm -rf $libtar_BUILD_PATH/libtar_$commit_hash && rm -rf $libtar_INSTALL_PATH/$commit_hash
cp -r $libtar_SRC_PATH $libtar_BUILD_PATH/libtar_$commit_hash
cd $libtar_BUILD_PATH/libtar_$commit_hash
# git reset --hard $commit_hash

CC="${SCRIPT_DIR}/../../../AFL/afl-clang-fast"
CXX="${SCRIPT_DIR}/../../../AFL/afl-clang-fast++"
CFLAGS="-fsanitize=address -fPIC"
CXXFLAGS="-fsanitize=address -fPIC"

cd $libtar_BUILD_PATH/libtar_$commit_hash

autoreconf --force --install
./configure  CXX=$CXX CC=$CC CFLAGS="$CFLAGS" CXXFLAGS="$CXXFLAGS"

make -j16

cd $libtar_INSTALL_PATH && mkdir -p $commit_hash
cd $libtar_INSTALL_PATH/libtar_$commit_hash
make install DESTDIR=$libtar_INSTALL_PATH/$commit_hash
rm -rf $libtar_BUILD_PATH/libtar_$commit_hash


# cd $libtar_LIB_PATH
# make genlib_thread COMMIT_HASH=$commit_hash -B
# make genlib_docker COMMIT_HASH=$commit_hash -B
# make test_thread -B




