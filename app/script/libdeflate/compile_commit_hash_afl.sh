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

export PATH="/opt/clang+llvm-11.0.0/bin:$PATH"
# check if clang version is 11 (robust extraction of the major version number)
CLANG_VERSION=$(clang --version | head -n 1 | grep -oE '[0-9]+' | head -n1 || true)
if ! [[ "$CLANG_VERSION" =~ ^[0-9]+$ ]]; then
  echo "Error: unable to parse clang version: $CLANG_VERSION"
  exit 1
fi
if [ "$CLANG_VERSION" -ne 11 ]; then
  echo "Error: clang version 11 is required. Current version is $CLANG_VERSION"
  exit 1
fi

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" &>/dev/null && pwd)

APP_PATH="${SCRIPT_DIR}/../.."
DEFLATE_BUILD_PATH=$APP_PATH/build/deflate_afl
DEFLATE_SRC_PATH=$APP_PATH/src/libdeflate
DEFLATE_INSTALL_PATH=$APP_PATH/build/deflate_afl
DEFLATE_LIB_PATH=$APP_PATH/libs/build_deflate_libs/

echo "Preparing build for commit: $commit_hash"

mkdir -p "$DEFLATE_BUILD_PATH/"
rm -rf "$DEFLATE_BUILD_PATH/deflate_$commit_hash" && rm -rf "$DEFLATE_INSTALL_PATH/$commit_hash"
cp -r "$DEFLATE_SRC_PATH" "$DEFLATE_BUILD_PATH/deflate_$commit_hash"
cd "$DEFLATE_BUILD_PATH/deflate_$commit_hash"
git reset --hard "$commit_hash"
python3 "$APP_PATH/script/libdeflate/patch_checksum.py" "$DEFLATE_BUILD_PATH/deflate_$commit_hash"

# Try cmake-based build first. If the inputs for cmake are not present, fall back to the old make-based build.
# We consider CMake usable when a top-level CMakeLists.txt exists.
if [ -f "CMakeLists.txt" ] || [ -f "../CMakeLists.txt" ]; then
  echo "CMake project detected — using cmake build path"
  mkdir -p build
  cd build

  CC="${SCRIPT_DIR}/../../../fuzzers/AFL/afl-clang-fast"
  CXX="${SCRIPT_DIR}/../../../fuzzers/AFL/afl-clang-fast++"
  CFLAGS="-fsanitize=address -fPIC"
  CXXFLAGS=$CFLAGS


  cmake .. -D CMAKE_C_COMPILER="$CC" -D CMAKE_CXX_COMPILER="$CXX" -DCMAKE_C_FLAGS="$CFLAGS" -DCMAKE_CXX_FLAGS="$CXXFLAGS"
  make -j8

  # install if supported
  mkdir -p "$DEFLATE_INSTALL_PATH/$commit_hash"
  if [ -d "$DEFLATE_INSTALL_PATH/deflate_$commit_hash/build" ]; then
    cd "$DEFLATE_INSTALL_PATH/deflate_$commit_hash/build" || true
  else
    # some projects export install targets from the build dir we just used
    cd "$DEFLATE_BUILD_PATH/deflate_$commit_hash/build" || true
  fi
  make install DESTDIR="$DEFLATE_INSTALL_PATH/$commit_hash"
else
  echo "CMake files not found — falling back to legacy make build"

  # Choose sensible CC/CXX: prefer system clang if available; otherwise fall back to libafl wrappers
  CC="${SCRIPT_DIR}/../../../fuzzers/AFL/afl-clang-fast"
  CXX="${SCRIPT_DIR}/../../../fuzzers/AFL/afl-clang-fast++"
  CFLAGS="-fsanitize=address -fPIC"
  CXXFLAGS=$CFLAGS

  # run legacy make-based build from the source root
  cd "$DEFLATE_BUILD_PATH/deflate_$commit_hash"
  make -j8 CC="$CC" CXX="$CXX" CFLAGS="$CFLAGS" CXXFLAGS="$CXXFLAGS" BUILD_SHARED_LIBRARY=yes

  # attempt install if supported by the project's Makefile
  mkdir -p "$DEFLATE_INSTALL_PATH/$commit_hash/usr/local/"
  mkdir -p "$DEFLATE_INSTALL_PATH/$commit_hash/usr/local/lib"
  mkdir -p "$DEFLATE_INSTALL_PATH/$commit_hash/usr/local/include"
  cp $DEFLATE_BUILD_PATH/deflate_$commit_hash/libdeflate.h "$DEFLATE_INSTALL_PATH/$commit_hash/usr/local/include/"
  cp $DEFLATE_BUILD_PATH/deflate_$commit_hash/libdeflate.so* "$DEFLATE_INSTALL_PATH/$commit_hash/usr/local/lib/"
  cp $DEFLATE_BUILD_PATH/deflate_$commit_hash/libdeflate.a "$DEFLATE_INSTALL_PATH/$commit_hash/usr/local/lib/"


  
  # make install DESTDIR="$DEFLATE_INSTALL_PATH/$commit_hash"
fi

# cleanup build copy
rm -rf "$DEFLATE_BUILD_PATH/deflate_$commit_hash"

# legacy helper steps (kept commented)
# cd $DEFLATE_LIB_PATH
# make genlib_thread COMMIT_HASH=$commit_hash
# make genlib_docker COMMIT_HASH=$commit_hash
# make test_thread

