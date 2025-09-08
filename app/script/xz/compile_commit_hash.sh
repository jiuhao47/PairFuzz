#!/bin/sh
#   - libxz_newest.so:15cf3f04
#   - libxz_oldest.so: 98f3cac1a
commit_hash=$1

if [ -z "$commit_hash" ]; then
  echo "Usage: $0 <commit_hash>"
  exit 1
fi

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" &>/dev/null && pwd)

APP_PATH="${SCRIPT_DIR}/../.."
XZON_ROOT_PATH=/home/xjy/fuzz/else/fuzz_project
XZ_BUILD_PATH=$APP_PATH/build/xz
XZ_SRC_PATH=$APP_PATH/src/xz
XZ_INSTALL_PATH=$APP_PATH/build/xz
XZ_LIB_PATH=$APP_PATH/libs/build_xz_libs/

mkdir -p $XZ_BUILD_PATH/
rm -rf $XZ_BUILD_PATH/xz_$commit_hash  rm -rf $XZ_INSTALL_PATH/$commit_hash
cp -r $XZ_SRC_PATH $XZ_BUILD_PATH/xz_$commit_hash
cd $XZ_BUILD_PATH/xz_$commit_hash
git reset --hard $commit_hash

python3 $XZON_ROOT_PATH/script/xz/patch_checksum.py $XZ_BUILD_PATH/xz_$commit_hash
./autogen.sh
mkdir -p build
cd build

CC="${SCRIPT_DIR}/../../compiler/target/release/libafl_cc"
CXX="${SCRIPT_DIR}/../../compiler/target/release/libafl_cxx"
CFLAGS="-fsanitize=address"
CXXFLAGS="-fsanitize=address"

../configure --disable-ifunc --disable-sandbox CC=$CC CXX=$CXX CFLAGS=$CFLAGS CXXFLAGS=$CXXFLAGS
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
