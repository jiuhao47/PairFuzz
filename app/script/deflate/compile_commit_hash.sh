#!/bin/sh
### version
# oldest: a11c44317
# newest: efc217633

commit_hash=$1

if [ -z "$commit_hash" ]; then
  echo "Usage: $0 <commit_hash>"
  exit 1
fi

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" &>/dev/null && pwd)

APP_PATH="${SCRIPT_DIR}/../.."
DEFLATE_BUILD_PATH=$APP_PATH/build/deflate
DEFLATE_SRC_PATH=$APP_PATH/src/deflate
DEFLATE_INSTALL_PATH=$APP_PATH/build/deflate
DEFLATE_LIB_PATH=$APP_PATH/libs/build_deflate_libs/

mkdir -p $DEFLATE_BUILD_PATH/
rm -rf $DEFLATE_BUILD_PATH/deflate_$commit_hash && rm -rf $DEFLATE_INSTALL_PATH/$commit_hash
cp -r $DEFLATE_SRC_PATH $DEFLATE_BUILD_PATH/deflate_$commit_hash
cd $DEFLATE_BUILD_PATH/deflate_$commit_hash
git reset --hard $commit_hash
python3 $APP_PATH/script/deflate/patch_checksum.py $DEFLATE_BUILD_PATH/deflate_$commit_hash
# # ./autogen.sh
mkdir -p build
cd build

CC="${SCRIPT_DIR}/../../compiler/target/release/libafl_cc"
CXX="${SCRIPT_DIR}/../../compiler/target/release/libafl_cxx"

# # ../configure --disable-ifunc --disable-sandbox CC=$CC CXX=$CXX CFLAGS="-g -fsanitize=address,fuzzer-no-link -fsanitize-coverage=edge,indirect-calls -fPIC" CXXFLAGS="-g -fsanitize=address,fuzzer-no-link -fsanitize-coverage=edge,indirect-calls -fPIC"
cmake .. -D CMAKE_C_COMPILER=$CC -D CMAKE_CXX_COMPILER=$CXX -DCMAKE_C_FLAGS="-O0 -fsanitize=address" -DCMAKE_CXX_FLAGS=" -O0 -fsanitize=address" 
make -j8
cd $DEFLATE_INSTALL_PATH && mkdir -p $commit_hash
cd $DEFLATE_INSTALL_PATH/deflate_$commit_hash/build
make install DESTDIR=$DEFLATE_INSTALL_PATH/$commit_hash
rm -rf $DEFLATE_BUILD_PATH/deflate_$commit_hash

# cd $DEFLATE_LIB_PATH
# make genlib_thread COMMIT_HASH=$commit_hash
# make genlib_docker COMMIT_HASH=$commit_hash
# make test_thread

