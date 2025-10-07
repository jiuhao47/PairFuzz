#!/usr/bin/env bash
set -e
#   - libbzip2_newest.so: <hash placeholder>
#   - libbzip2_oldest.so: <hash placeholder>
commit_hash=$1

if [ -z "$commit_hash" ]; then
  echo "Usage: $0 <commit_hash>"
  exit 1
fi

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" &>/dev/null && pwd)

APP_PATH="${SCRIPT_DIR}/../.."
BZ2_BUILD_PATH=$APP_PATH/build/bzip2
BZ2_SRC_PATH=$APP_PATH/src/bzip2
BZ2_INSTALL_PATH=$APP_PATH/build/bzip2

mkdir -p $BZ2_BUILD_PATH/
rm -rf $BZ2_BUILD_PATH/bzip2_$commit_hash  && rm -rf $BZ2_INSTALL_PATH/$commit_hash
cp -r $BZ2_SRC_PATH $BZ2_BUILD_PATH/bzip2_$commit_hash
cd $BZ2_BUILD_PATH/bzip2_$commit_hash
if [ "$commit_hash" = "dep" ]; then
  echo "commit_hash is 'dep' — skipping git reset (using dependency snapshot)"
else
  git reset --hard $commit_hash
fi

# bzip2 typically uses a simple Makefile
CC="${SCRIPT_DIR}/../../compiler/target/release/libafl_cc"
CXX="${SCRIPT_DIR}/../../compiler/target/release/libafl_cxx"
CFLAGS="-fsanitize=address"
CXXFLAGS="-fsanitize=address"

export CC CXX CFLAGS CXXFLAGS

# Build with the repository-provided CC wrapper
make -j16

mkdir -p $BZ2_INSTALL_PATH && mkdir -p $BZ2_INSTALL_PATH/$commit_hash
# bzip2's Makefile supports install to DESTDIR
make install PREFIX=$BZ2_INSTALL_PATH/$commit_hash/usr/local

# cleanup source copy
rm -rf $BZ2_BUILD_PATH/bzip2_$commit_hash

# Optionally you could generate library packaging or tests here
# e.g. cd $APP_PATH/libs/build_bzip2_libs && make genlib_thread COMMIT_HASH=$commit_hash
