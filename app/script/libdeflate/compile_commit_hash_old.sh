#!/bin/sh
### version
# oldest: a11c44317
# newest: efc217633

commit_hash=$1

DEFLATEON_ROOT_PATH=/home/xjy/fuzz/else/fuzz_project
DEFLATE_BUILD_PATH=$DEFLATEON_ROOT_PATH/build
DEFLATE_SRC_PATH=$DEFLATEON_ROOT_PATH/src/deflate
DEFLATE_INSTALL_PATH=$DEFLATEON_ROOT_PATH/build
DEFLATE_LIB_PATH=$DEFLATEON_ROOT_PATH/libs/build_deflate_libs/

rm -rf $DEFLATE_BUILD_PATH/deflate_$commit_hash
cp -r $DEFLATE_SRC_PATH $DEFLATE_BUILD_PATH/deflate_$commit_hash
cd $DEFLATE_BUILD_PATH/deflate_$commit_hash
git reset --hard $commit_hash
python3 $DEFLATEON_ROOT_PATH/script/deflate/patch_checksum.py $DEFLATE_BUILD_PATH/deflate_$commit_hash
# # ./autogen.sh
# mkdir -p build
# cd build

CXX="/home/xjy/fuzz/else/llvm-project/build/bin/clang++"
CC="/home/xjy/fuzz/else/llvm-project/build/bin/clang"

# # ../configure --disable-ifunc --disable-sandbox CC=$CC CXX=$CXX CFLAGS="-g -fsanitize=address,fuzzer-no-link -fsanitize-coverage=edge,indirect-calls -fPIC" CXXFLAGS="-g -fsanitize=address,fuzzer-no-link -fsanitize-coverage=edge,indirect-calls -fPIC"
# cmake .. -D CMAKE_C_COMPILER="/home/xjy/fuzz/else/llvm-project/build/bin/clang" -D CMAKE_CXX_COMPILER="/home/xjy/fuzz/else/llvm-project/build/bin/clang++" -DCMAKE_C_FLAGS="-O0 -fsanitize=address,fuzzer-no-link -fsanitize-coverage=edge,indirect-calls -fPIC " -DCMAKE_CXX_FLAGS=" -O0 -fsanitize=address,fuzzer-no-link -fsanitize-coverage=edge,indirect-calls -fPIC " 


make -j8 CC="/home/xjy/fuzz/else/llvm-project/build/bin/clang" CXX="/home/xjy/fuzz/else/llvm-project/build/bin/clang++" BUILD_SHARED_LIBRARY=yes CFLAGS="-std=gnu99 -fsanitize=address,fuzzer-no-link -fsanitize-coverage=edge,indirect-calls -fPIC"
# mkdir -p install
# make install DESTDIR=$DEFLATE_INSTALL_PATH/deflate_$commit_hash/install

# cd $DEFLATE_LIB_PATH
# make genlib_thread_old COMMIT_HASH=$commit_hash
# make genlib_docker_old COMMIT_HASH=$commit_hash
# make test_thread

