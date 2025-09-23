#!/usr/bin/env bash
# oldest: 14303
# newest: 17300
commit_hash=$1

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" &>/dev/null && pwd)

APP_PATH="${SCRIPT_DIR}/../.."
GM_BUILD_PATH=$APP_PATH/build/GM
GM_SRC_PATH=$APP_PATH/src
GM_INSTALL_PATH=$APP_PATH/build/GM
GM_LIB_PATH=$APP_PATH/libs/build_GM_libs/


mkdir -p $GM_BUILD_PATH/
rm -rf $GM_BUILD_PATH/GM_$commit_hash && rm -rf $GM_INSTALL_PATH/$commit_hash
cp -r $GM_SRC_PATH/GM $GM_BUILD_PATH/GM_$commit_hash

CC="${SCRIPT_DIR}/../../compiler/target/release/libafl_cc"
CXX="${SCRIPT_DIR}/../../compiler/target/release/libafl_cxx"
CFLAGS="-fsanitize=address"
CXXFLAGS="-fsanitize=address"
cd $GM_BUILD_PATH/GM_$commit_hash


hg update --clean $commit_hash



./configure  CC="$CC" CXX="$CXX" CFLAGS="$CFLAGS" CXXFLAGS="$CXXFLAGS"

make -j 16

cd $GM_INSTALL_PATH && mkdir -p $commit_hash
cd $GM_INSTALL_PATH/GM_$commit_hash/
make install DESTDIR=$GM_INSTALL_PATH/$commit_hash
rm -rf $GM_BUILD_PATH/GM_$commit_hash

# mkdir -p install
# make install DESTDIR=$GM_INSTALL_PATH/GM_$commit_hash/install

# cd $GM_LIB_PATH
# make genlib_thread COMMIT_HASH=$commit_hash
# make genlib_docker COMMIT_HASH=$commit_hash
# make test_thread
