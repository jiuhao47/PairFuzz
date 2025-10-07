#!/bin/sh
# old: 7751bd4
# new: 09155ea

commit_hash=$1

if [ -z "$commit_hash" ]; then
  echo "Usage: $0 <commit_hash>"
  exit 1
fi

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" &>/dev/null && pwd)

APP_PATH="${SCRIPT_DIR}/../.."
ZLIB_BUILD_PATH=$APP_PATH/build/zlib
ZLIB_SRC_PATH=$APP_PATH/src/zlib
ZLIB_INSTALL_PATH=$APP_PATH/build/zlib
ZLIB_LIB_PATH=$APP_PATH/libs/build_zlib_libs/


mkdir -p $ZLIB_BUILD_PATH/ 
rm -rf $ZLIB_BUILD_PATH/zlib_$commit_hash && rm -rf $ZLIB_INSTALL_PATH/$commit_hash
cp -r $ZLIB_SRC_PATH $ZLIB_BUILD_PATH/zlib_$commit_hash
cd $ZLIB_BUILD_PATH/zlib_$commit_hash
if [ "$commit_hash" = "dep" ]; then
  echo "commit_hash is 'dep' — skipping git reset (using dependency snapshot)"
else
  git reset --hard $commit_hash
fi
python3 $APP_PATH/script/zlib/patch_checksum.py $ZLIB_BUILD_PATH/zlib_$commit_hash
# ./autogen.sh
mkdir -p build
cd build

CC="${SCRIPT_DIR}/../../compiler/target/release/libafl_cc"
CXX="${SCRIPT_DIR}/../../compiler/target/release/libafl_cxx"
CFLAGS="-fsanitize=address"
CXXFLAGS="-fsanitize=address"

# ../configure --disable-ifunc --disable-sandbox CC=$CC CXX=$CXX CFLAGS="-g -fsanitize=address,fuzzer-no-link -fsanitize-coverage=edge,indirect-calls -fPIC" CXXFLAGS="-g -fsanitize=address,fuzzer-no-link -fsanitize-coverage=edge,indirect-calls -fPIC"
cmake .. -D CMAKE_C_COMPILER=$CC -D CMAKE_CXX_COMPILER=$CXX -DCMAKE_C_FLAGS=$CFLAGS -DCMAKE_CXX_FLAGS=$CXXFLAGS -DBUILD_SHARED_LIBS=OFF
make -j8
cd $ZLIB_INSTALL_PATH && mkdir -p $commit_hash
cd $ZLIB_INSTALL_PATH/zlib_$commit_hash/build
make install DESTDIR=$ZLIB_INSTALL_PATH/$commit_hash
rm -rf $ZLIB_BUILD_PATH/zlib_$commit_hash

# cd $ZLIB_LIB_PATH
# make genlib_thread COMMIT_HASH=$commit_hash
# make genlib_docker COMMIT_HASH=$commit_hash
# make test_thread

