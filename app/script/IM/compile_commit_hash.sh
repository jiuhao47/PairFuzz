#!/bin/sh
# - libIM_newest.so: 0729cc470
# - libIM_oldest.so: 4c717b784
commit_hash=$1

if [ -z "$commit_hash" ]; then
  echo "Usage: $0 <commit_hash>"
  exit 1
fi

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" &>/dev/null && pwd)

APP_PATH="${SCRIPT_DIR}/../.."
IM_BUILD_PATH=$APP_PATH/build/IM
IM_SRC_PATH=$APP_PATH/src
IM_INSTALL_PATH=$APP_PATH/build/IM
IM_LIB_PATH=$APP_PATH/libs/build_IM_libs/

mkdir -p $IM_BUILD_PATH/
rm -rf $IM_BUILD_PATH/IM_$commit_hash && rm -rf $IM_INSTALL_PATH/$commit_hash
cp -r $IM_SRC_PATH/IM $IM_BUILD_PATH/IM_$commit_hash


CC="${SCRIPT_DIR}/../../compiler/target/release/libafl_cc"
CXX="${SCRIPT_DIR}/../../compiler/target/release/libafl_cxx"
CFLAGS="-fsanitize=address"
CXXFLAGS="-fsanitize=address"
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
