#!/usr/bin/env bash
#   - libexpat_newest.so: 07108642
#`  - libexpat_oldest.so: ac2b235
set -e

commit_hash=$1

if [ -z "$commit_hash" ]; then
  echo "Usage: $0 <commit_hash>"
  exit 1
fi

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" &>/dev/null && pwd)

APP_PATH="${SCRIPT_DIR}/../.."
expat_BUILD_PATH=$APP_PATH/build/expat_cmplog
expat_SRC_PATH=$APP_PATH/src/expat
expat_INSTALL_PATH=$APP_PATH/build/expat_cmplog
expat_LIB_PATH=$APP_PATH/libs/build_expat_libs/

mkdir -p $expat_BUILD_PATH/
rm -rf $expat_BUILD_PATH/expat_$commit_hash && rm -rf $expat_INSTALL_PATH/$commit_hash
cp -r $expat_SRC_PATH $expat_BUILD_PATH/expat_$commit_hash

CC="${SCRIPT_DIR}/../../../fuzzers/AFLplusplus/afl-clang-fast"
CXX="${SCRIPT_DIR}/../../../fuzzers/AFLplusplus/afl-clang-fast++"
CFLAGS="-fsanitize=address -fPIC"
CXXFLAGS="-fsanitize=address -fPIC"
cd $expat_BUILD_PATH/expat_$commit_hash/expat
git reset --hard $commit_hash

./buildconf.sh
./configure  CC="$CC" CXX="$CXX" CFLAGS="$CFLAGS" CXXFLAGS="$CXXFLAGS" --without-docbook

AFL_LLVM_CMPLOG=1 make -j16

cd $expat_INSTALL_PATH && mkdir -p $commit_hash
cd $expat_INSTALL_PATH/expat_$commit_hash/expat
make install DESTDIR=$expat_INSTALL_PATH/$commit_hash
rm -rf $expat_BUILD_PATH/expat_$commit_hash
