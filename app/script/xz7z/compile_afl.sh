#!/usr/bin/env bash
set -e


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
XZ7Z_BUILD_PATH=$APP_PATH/build/xz7z_afl
XZ7Z_SRC_PATH=$APP_PATH/src/xz_7znewest
XZ7Z_LIB_PATH=$APP_PATH/libs/build_xz7z_libs/

rm -rf $XZ7Z_BUILD_PATH/xz7z
mkdir -p $XZ7Z_BUILD_PATH/xz7z/src_to_build/
cd $XZ7Z_BUILD_PATH/xz7z

mkdir -p $XZ7Z_BUILD_PATH/xz7z/src_to_build/output


cp -r $XZ7Z_SRC_PATH/* ./src_to_build

cp $XZ7Z_LIB_PATH/Makefile.build ./src_to_build/Makefile


cd $XZ7Z_BUILD_PATH/xz7z/src_to_build/
CC="${SCRIPT_DIR}/../../../AFL/afl-clang-fast"
CXX="${SCRIPT_DIR}/../../../AFL/afl-clang-fast++"
CFLAGS="-fsanitize=address -fPIC"
CXXFLAGS="-fsanitize=address -fPIC"
mkdir -p $XZ7Z_BUILD_PATH/install
mkdir -p $XZ7Z_BUILD_PATH/include
make clean
make all CC=$CC CXX=$CXX CFLAGS="$CFLAGS" CXXFLAGS="$CXXFLAGS" COMMIT_HASH=$commit_hash
cp $XZ7Z_BUILD_PATH/xz7z/src_to_build/libxz7z_.a $XZ7Z_BUILD_PATH/install/libxz7z.a
cp -r $XZ7Z_SRC_PATH/* $XZ7Z_BUILD_PATH/include/
rm -rf $XZ7Z_BUILD_PATH/xz7z

