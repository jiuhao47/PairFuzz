#!/usr/bin/env bash

commit_hash=$1

if [ -z "$commit_hash" ]; then
  echo "Usage: $0 <commit_hash>"
  exit 1
fi

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" &>/dev/null && pwd)

APP_PATH="${SCRIPT_DIR}/../.."
PNG_BUILD_PATH=$APP_PATH/build/png_cmplog
PNG_SRC_PATH=$APP_PATH/src/libpng
PNG_INSTALL_PATH=$APP_PATH/build/png_cmplog
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


CC="${SCRIPT_DIR}/../../../fuzzers/AFLplusplus/afl-clang-fast"
CXX="${SCRIPT_DIR}/../../../fuzzers/AFLplusplus/afl-clang-fast++"
CFLAGS="-fPIC -fsanitize=address"
CXXFLAGS="-fPIC -fsanitize=address"

./configure CC="$CC" CXX="$CXX" CFLAGS="$CFLAGS" CXXFLAGS="$CXXFLAGS"

AFL_LLVM_CMPLOG=1 make -j 16


cd $PNG_INSTALL_PATH && mkdir -p $commit_hash
cd $PNG_INSTALL_PATH/png_$commit_hash
make install DESTDIR=$PNG_INSTALL_PATH/$commit_hash
rm -rf $PNG_BUILD_PATH/png_$commit_hash
