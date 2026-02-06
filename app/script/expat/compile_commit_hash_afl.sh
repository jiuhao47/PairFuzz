#!/usr/bin/env bash
#   - libexpat_newest.so: 07108642
#`  - libexpat_oldest.so: ac2b235
set -e

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
expat_BUILD_PATH=$APP_PATH/build/expat_afl
expat_SRC_PATH=$APP_PATH/src/expat
expat_INSTALL_PATH=$APP_PATH/build/expat_afl
expat_LIB_PATH=$APP_PATH/libs/build_expat_libs/

mkdir -p $expat_BUILD_PATH/
rm -rf $expat_BUILD_PATH/expat_$commit_hash && rm -rf $expat_INSTALL_PATH/$commit_hash
cp -r $expat_SRC_PATH $expat_BUILD_PATH/expat_$commit_hash

CC="${SCRIPT_DIR}/../../../fuzzers/AFL/afl-clang-fast"
CXX="${SCRIPT_DIR}/../../../fuzzers/AFL/afl-clang-fast++"
CFLAGS="-fsanitize=address -fPIC"
CXXFLAGS="-fsanitize=address -fPIC"
cd $expat_BUILD_PATH/expat_$commit_hash/expat
git reset --hard $commit_hash

./buildconf.sh
./configure  CC="$CC" CXX="$CXX" CFLAGS="$CFLAGS" CXXFLAGS="$CXXFLAGS" --without-docbook

make -j16

cd $expat_INSTALL_PATH && mkdir -p $commit_hash
cd $expat_INSTALL_PATH/expat_$commit_hash/expat
make install DESTDIR=$expat_INSTALL_PATH/$commit_hash
rm -rf $expat_BUILD_PATH/expat_$commit_hash


# cd $expat_LIB_PATH
# make genlib_thread COMMIT_HASH=$commit_hash -B
# make genlib_docker COMMIT_HASH=$commit_hash -B
# make test_thread -B

