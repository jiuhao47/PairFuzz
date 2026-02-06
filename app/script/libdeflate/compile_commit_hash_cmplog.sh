#!/usr/bin/env bash
### version
# oldest: a11c44317
# newest: efc217633

set -e

commit_hash=$1

if [ -z "$commit_hash" ]; then
  echo "Usage: $0 <commit_hash>"
  exit 1
fi

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" &>/dev/null && pwd)

APP_PATH="${SCRIPT_DIR}/../.."
DEFLATE_BUILD_PATH=$APP_PATH/build/deflate_cmplog
DEFLATE_SRC_PATH=$APP_PATH/src/libdeflate
DEFLATE_INSTALL_PATH=$APP_PATH/build/deflate_cmplog
DEFLATE_LIB_PATH=$APP_PATH/libs/build_deflate_libs/

echo "Preparing build for commit: $commit_hash"

mkdir -p "$DEFLATE_BUILD_PATH/"
rm -rf "$DEFLATE_BUILD_PATH/deflate_$commit_hash" && rm -rf "$DEFLATE_INSTALL_PATH/$commit_hash"
cp -r "$DEFLATE_SRC_PATH" "$DEFLATE_BUILD_PATH/deflate_$commit_hash"
cd "$DEFLATE_BUILD_PATH/deflate_$commit_hash"
git reset --hard "$commit_hash"
python3 "$APP_PATH/script/libdeflate/patch_checksum.py" "$DEFLATE_BUILD_PATH/deflate_$commit_hash"

if [ -f "CMakeLists.txt" ] || [ -f "../CMakeLists.txt" ]; then
  echo "CMake project detected — using cmake build path"
  mkdir -p build
  cd build

  CC="${SCRIPT_DIR}/../../../fuzzers/AFLplusplus/afl-clang-fast"
  CXX="${SCRIPT_DIR}/../../../fuzzers/AFLplusplus/afl-clang-fast++"
  CFLAGS="-fsanitize=address -fPIC"
  CXXFLAGS=$CFLAGS


  cmake .. -D CMAKE_C_COMPILER="$CC" -D CMAKE_CXX_COMPILER="$CXX" -DCMAKE_C_FLAGS="$CFLAGS" -DCMAKE_CXX_FLAGS="$CXXFLAGS"
  AFL_LLVM_CMPLOG=1 make -j8

  mkdir -p "$DEFLATE_INSTALL_PATH/$commit_hash"
  if [ -d "$DEFLATE_INSTALL_PATH/deflate_$commit_hash/build" ]; then
    cd "$DEFLATE_INSTALL_PATH/deflate_$commit_hash/build" || true
  else
    cd "$DEFLATE_BUILD_PATH/deflate_$commit_hash/build" || true
  fi
  make install DESTDIR="$DEFLATE_INSTALL_PATH/$commit_hash"
else
  echo "CMake files not found — falling back to legacy make build"

  CC="${SCRIPT_DIR}/../../../fuzzers/AFLplusplus/afl-clang-fast"
  CXX="${SCRIPT_DIR}/../../../fuzzers/AFLplusplus/afl-clang-fast++"
  CFLAGS="-fsanitize=address -fPIC"
  CXXFLAGS=$CFLAGS

  cd "$DEFLATE_BUILD_PATH/deflate_$commit_hash"
  AFL_LLVM_CMPLOG=1 make -j8 CC="$CC" CXX="$CXX" CFLAGS="$CFLAGS" CXXFLAGS="$CXXFLAGS" BUILD_SHARED_LIBRARY=yes

  mkdir -p "$DEFLATE_INSTALL_PATH/$commit_hash/usr/local/"
  mkdir -p "$DEFLATE_INSTALL_PATH/$commit_hash/usr/local/lib"
  mkdir -p "$DEFLATE_INSTALL_PATH/$commit_hash/usr/local/include"
  cp $DEFLATE_BUILD_PATH/deflate_$commit_hash/libdeflate.h "$DEFLATE_INSTALL_PATH/$commit_hash/usr/local/include/"
  cp $DEFLATE_BUILD_PATH/deflate_$commit_hash/libdeflate.so* "$DEFLATE_INSTALL_PATH/$commit_hash/usr/local/lib/"
  cp $DEFLATE_BUILD_PATH/deflate_$commit_hash/libdeflate.a "$DEFLATE_INSTALL_PATH/$commit_hash/usr/local/lib/"
fi

rm -rf "$DEFLATE_BUILD_PATH/deflate_$commit_hash"
