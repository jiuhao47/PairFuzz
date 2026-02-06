#!/usr/bin/env bash

commit_hash=$1

if [ -z "$commit_hash" ]; then
  echo "Usage: $0 <commit_hash>"
  exit 1
fi


SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" &>/dev/null && pwd)

APP_PATH="${SCRIPT_DIR}/../.."
PNG_BUILD_PATH=$APP_PATH/build/png_dict
PNG_SRC_PATH=$APP_PATH/src/libpng
PNG_INSTALL_PATH=$APP_PATH/build/png_dict
PNG_LIB_PATH=$APP_PATH/libs/build_png_libs/

mkdir -p $PNG_BUILD_PATH/
rm -rf $PNG_BUILD_PATH/png_$commit_hash && rm -rf $PNG_INSTALL_PATH/$commit_hash
cp -r $PNG_SRC_PATH $PNG_BUILD_PATH/png_$commit_hash
cd $PNG_BUILD_PATH/png_$commit_hash
if [ "$commit_hash" = "dep" ]; then
  git reset --hard 872555f # 1.6.47
else
  git reset --hard $commit_hash
fi



CC="${SCRIPT_DIR}/../../../fuzzers/AFLplusplus/afl-clang-lto"
CXX="${SCRIPT_DIR}/../../../fuzzers/AFLplusplus/afl-clang-lto++"
CFLAGS="-fPIC -fsanitize=address"
CXXFLAGS="-fPIC -fsanitize=address"



# ./configure --disable-static --enable-shared CC="$CC" CXX="$CXX" CFLAGS="-g -fPIC -fsanitize=address,fuzzer-no-link -fsanitize-coverage=edge,indirect-calls" CXXFLAGS="-g -fPIC -fsanitize=address,fuzzer-no-link -fsanitize-coverage=edge,indirect-calls"
# ./configure --disable-static --enable-shared CC="$CC" CXX="$CXX" 
./configure CC="$CC" CXX="$CXX" CFLAGS="$CFLAGS" CXXFLAGS="$CXXFLAGS"

rm -f dict_${commit_hash}.txt
export AFL_LLVM_DICT2FILE="${SCRIPT_DIR}/dict_${commit_hash}.txt"

make -j 16


cd $PNG_INSTALL_PATH && mkdir -p $commit_hash
cd $PNG_INSTALL_PATH/png_$commit_hash
make install DESTDIR=$PNG_INSTALL_PATH/$commit_hash
rm -rf $PNG_BUILD_PATH/png_$commit_hash

# cd $PNG_LIB_PATH
# make genlib_thread COMMIT_HASH=$commit_hash
# make genlib_docker COMMIT_HASH=$commit_hash
# make test_thread
