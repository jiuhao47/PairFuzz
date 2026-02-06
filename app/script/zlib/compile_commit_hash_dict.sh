#!/usr/bin/env bash
# old: 7751bd4
# new: 09155ea

commit_hash=$1

if [ -z "$commit_hash" ]; then
  echo "Usage: $0 <commit_hash>"
  exit 1
fi

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" &>/dev/null && pwd)

APP_PATH="${SCRIPT_DIR}/../.."
ZLIB_BUILD_PATH=$APP_PATH/build/zlib_dict
ZLIB_SRC_PATH=$APP_PATH/src/zlib
ZLIB_INSTALL_PATH=$APP_PATH/build/zlib_dict
ZLIB_LIB_PATH=$APP_PATH/libs/build_zlib_libs/


mkdir -p $ZLIB_BUILD_PATH/ 
rm -rf $ZLIB_BUILD_PATH/zlib_$commit_hash && rm -rf $ZLIB_INSTALL_PATH/$commit_hash
cp -r $ZLIB_SRC_PATH $ZLIB_BUILD_PATH/zlib_$commit_hash
cd $ZLIB_BUILD_PATH/zlib_$commit_hash
if [ "$commit_hash" = "dep" ]; then
  git reset --hard 09155ea # zlib 1.3
else
  git reset --hard $commit_hash
  python3 $APP_PATH/script/zlib/patch_checksum.py $ZLIB_BUILD_PATH/zlib_$commit_hash
fi

# Workaround: remove gz_intmax from version script in the build copy
# to avoid LLD error during LTO/linking when the symbol isn't emitted.
# This modifies only the copied source in $ZLIB_BUILD_PATH/zlib_$commit_hash.
if [ -f zlib.map ]; then
  if grep -q 'gz_intmax' zlib.map; then
    sed -i.bak '/\bgz_intmax\b;/d' zlib.map && \
      echo "Patched zlib.map: removed gz_intmax (backup at zlib.map.bak)"
  fi
fi

# ./autogen.sh
mkdir -p build
cd build

CC="${SCRIPT_DIR}/../../../fuzzers/AFLplusplus/afl-clang-lto"
CXX="${SCRIPT_DIR}/../../../fuzzers/AFLplusplus/afl-clang-lto++"
CFLAGS="-fPIC -fsanitize=address"
CXXFLAGS="-fPIC -fsanitize=address"

# ../configure --disable-ifunc --disable-sandbox CC=$CC CXX=$CXX CFLAGS="-g -fsanitize=address,fuzzer-no-link -fsanitize-coverage=edge,indirect-calls -fPIC" CXXFLAGS="-g -fsanitize=address,fuzzer-no-link -fsanitize-coverage=edge,indirect-calls -fPIC"
cmake .. -D CMAKE_C_COMPILER=$CC -D CMAKE_CXX_COMPILER=$CXX -DCMAKE_C_FLAGS="$CFLAGS" -DCMAKE_CXX_FLAGS="$CXXFLAGS" -DBUILD_SHARED_LIBS=OFF

rm -f dict_${commit_hash}.txt
export AFL_LLVM_DICT2FILE="${SCRIPT_DIR}/dict_${commit_hash}.txt"

make -j8
cd $ZLIB_INSTALL_PATH && mkdir -p $commit_hash
cd $ZLIB_INSTALL_PATH/zlib_$commit_hash/build
make install DESTDIR=$ZLIB_INSTALL_PATH/$commit_hash
rm -rf $ZLIB_BUILD_PATH/zlib_$commit_hash

# cd $ZLIB_LIB_PATH
# make genlib_thread COMMIT_HASH=$commit_hash
# make genlib_docker COMMIT_HASH=$commit_hash
# make test_thread

