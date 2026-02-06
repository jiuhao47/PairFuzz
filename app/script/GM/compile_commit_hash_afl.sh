#!/usr/bin/env bash
# oldest: 14303
# newest: 17300
commit_hash=$1

if [ -z "$commit_hash" ]; then
  echo "Usage: $0 <commit_hash>"
  exit 1
fi



SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" &>/dev/null && pwd)

APP_PATH="${SCRIPT_DIR}/../.."
GM_BUILD_PATH=$APP_PATH/build/GM_afl
GM_SRC_PATH=$APP_PATH/src
GM_INSTALL_PATH=$APP_PATH/build/GM_afl
GM_LIB_PATH=$APP_PATH/libs/build_GM_libs/


mkdir -p $GM_BUILD_PATH/
rm -rf $GM_BUILD_PATH/GM_$commit_hash && rm -rf $GM_INSTALL_PATH/$commit_hash
cp -r $GM_SRC_PATH/GM $GM_BUILD_PATH/GM_$commit_hash

CC="${SCRIPT_DIR}/../../../fuzzers/AFL/afl-clang-fast"
CXX="${SCRIPT_DIR}/../../../fuzzers/AFL/afl-clang-fast++"
CFLAGS="-fsanitize=address -fPIC"
CXXFLAGS="-fsanitize=address -fPIC"
cd $GM_BUILD_PATH/GM_$commit_hash


hg update --clean $commit_hash



# Use the real clang/clang++ for configure checks to avoid wrapper-induced
# preprocessor/compile-time issues (e.g. sizeof(off_t) checks).
REAL_CC="$(command -v clang || true)"
REAL_CXX="$(command -v clang++ || true)"
if [ -z "$REAL_CC" ] || [ -z "$REAL_CXX" ]; then
  echo "Error: clang/clang++ not found in PATH"
  exit 1
fi



# Configure with the real compiler to get correct feature detection
./configure CC="$REAL_CC" CXX="$REAL_CXX" CFLAGS="$CFLAGS" CXXFLAGS="$CXXFLAGS"

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

# Build with AFL wrappers so the produced binaries are instrumented
make CC="$CC" CXX="$CXX" -j 16

cd $GM_INSTALL_PATH && mkdir -p $commit_hash
cd $GM_INSTALL_PATH/GM_$commit_hash/
make install DESTDIR=$GM_INSTALL_PATH/$commit_hash
rm -rf $GM_BUILD_PATH/GM_$commit_hash

# mkdir -p install
# make install DESTDIR=$GM_INSTALL_PATH/GM_$commit_hash/install

# cd $GM_LIB_PATH
# make genlib_thread COMMIT_HASH=$commit_hash
# make genlib_docker COMMIT_HASH=$commit_hash
# make test_thread
