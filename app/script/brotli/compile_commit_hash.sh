#!/bin/sh
set -e 

commit_hash=$1

if [ -z "$commit_hash" ]; then
  echo "Usage: $0 <commit_hash>"
  exit 1
fi

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" &>/dev/null && pwd)

APP_PATH="${SCRIPT_DIR}/../.."
BROTLI_BUILD_PATH=$APP_PATH/build/brotli
BROTLI_SRC_PATH=$APP_PATH/src/brotli
BROTLI_INSTALL_PATH=$APP_PATH/build/brotli
BROTLI_LIB_PATH=$APP_PATH/libs/build_brotli_libs/

mkdir -p $BROTLI_BUILD_PATH/
rm -rf $BROTLI_BUILD_PATH/brotli_$commit_hash && rm -rf $BROTLI_INSTALL_PATH/$commit_hash
cp -r $BROTLI_SRC_PATH $BROTLI_BUILD_PATH/brotli_$commit_hash
cd $BROTLI_BUILD_PATH/brotli_$commit_hash
if [ "$commit_hash" = "dep" ]; then
  # If you want a pinned dependency commit, set it here. Example:
  # git reset --hard <some-known-commit>
  echo "Using 'dep' special tag but no specific commit configured. Using current tree."
else
  git reset --hard $commit_hash
fi


CC="${SCRIPT_DIR}/../../compiler/target/release/libafl_cc"
CXX="${SCRIPT_DIR}/../../compiler/target/release/libafl_cxx"
CFLAGS="-fsanitize=address -fPIC"
CXXFLAGS="-fsanitize=address -fPIC"

# Create an out/build directory for CMake
mkdir -p out && cd out

# Configure with CMake, telling it to use our CC/CXX wrappers and install to the per-commit prefix
# enable static
cmake -DCMAKE_BUILD_TYPE=Release \
      -DCMAKE_INSTALL_PREFIX="$BROTLI_INSTALL_PATH/$commit_hash/usr/local" \
      -DCMAKE_C_COMPILER="$CC" \
      -DCMAKE_CXX_COMPILER="$CXX" \
      -DCMAKE_C_FLAGS="$CFLAGS" \
      -DCMAKE_CXX_FLAGS="$CXXFLAGS" \
      -DBUILD_SHARED_LIBS=OFF \
      ..

# Build (parallel) and install
cmake --build . --config Release -- -j 16
cmake --install . --config Release

# Cleanup build copy
cd $BROTLI_INSTALL_PATH && mkdir -p $commit_hash
rm -rf $BROTLI_BUILD_PATH/brotli_$commit_hash

# Optional: generate libs in libs/ if you have helper make targets (uncomment and adapt)
# cd $BROTLI_LIB_PATH
# make genlib_thread COMMIT_HASH=$commit_hash
# make genlib_docker COMMIT_HASH=$commit_hash
# make test_thread
