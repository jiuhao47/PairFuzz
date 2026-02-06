#!/bin/bash
set -euo pipefail

# Usage: compile.sh <xz_hash> <name_suffix>
if [ "$#" -ne 2 ]; then
  echo "Usage: $0 <xz_hash> <name_suffix>"
  exit 1
fi

xz_hash=$1
name_suffix=$2

# Create a temporary build directory and ensure cleanup on exit
TEMP_DIR=$(mktemp -d "temp_differential.XXXX")
trap 'rm -rf "${TEMP_DIR}"' EXIT

# Compiler wrappers
CC_WRAPPER="../../../fuzzers/AFLplusplus/afl-clang-fast"
CXX_WRAPPER="../../../fuzzers/AFLplusplus/afl-clang-fast++"
if [ ! -x "$CC_WRAPPER" ]; then
  echo "Error: required wrapper '$CC_WRAPPER' not found or not executable"
  exit 1
fi
if [ ! -x "$CXX_WRAPPER" ]; then
  echo "Error: required wrapper '$CXX_WRAPPER' not found or not executable"
  exit 1
fi

COMMON_COMPILE_FLAGS="-fPIC -fsanitize=address -c"
COMMON_LINK_FLAGS="-fsanitize=address"

# AFL++ runtime object providing cmplog hooks
AFL_RT_OBJ="../../../fuzzers/AFLplusplus/afl-compiler-rt.o"

# Step 1: Compile program files to object files
echo "Compiling xz program..."

echo "Compiling xz core..."
AFL_LLVM_CMPLOG=1 $CXX_WRAPPER $COMMON_COMPILE_FLAGS "../../libs/build_xz_libs/xz_core.cpp" \
  -I "../../build/xz_cmplog/$xz_hash/usr/local/include/" \
  -o "$TEMP_DIR/xz_core.o"

echo "Compiling xz7z core..."
AFL_LLVM_CMPLOG=1 $CC_WRAPPER $COMMON_COMPILE_FLAGS "../../libs/build_xz7z_libs/7z_core.c" \
  -I "../../build/xz7z_cmplog/include" \
  -o "$TEMP_DIR/xz7z_core.o"

# Step 2: link static libraries to object files to generate executables
LINK_LIBS="-lm"


# Clean up
# Step 4: build shared libraries (position-independent, bundle static deps)
echo "Building shared library for xz..."
AFL_LLVM_CMPLOG=1 $CXX_WRAPPER -shared -o "$TEMP_DIR/xz_shared.so" "$TEMP_DIR/xz_core.o" \
  "../../build/xz_cmplog/$xz_hash/usr/local/lib/liblzma.a" "$AFL_RT_OBJ" \
  -Wl,-soname,libxz_local.so -Wl,-rpath,'$ORIGIN' $COMMON_LINK_FLAGS

echo "Building shared library for xz7z..."
AFL_LLVM_CMPLOG=1 $CC_WRAPPER -shared -o "$TEMP_DIR/xz7z_shared.so" "$TEMP_DIR/xz7z_core.o" \
  "../../build/xz7z_cmplog/install/libxz7z.a" "$AFL_RT_OBJ" \
  -Wl,-soname,libxz7z_local.so -Wl,-rpath,'$ORIGIN' $COMMON_LINK_FLAGS

echo "Installing shared libraries to bins/.bin/$name_suffix/"
install -m 0755 "$TEMP_DIR/xz_shared.so" "../../bins/.bin/$name_suffix/xz_local_cmplog.so"
install -m 0755 "$TEMP_DIR/xz7z_shared.so" "../../bins/.bin/$name_suffix/xz7z_local_cmplog.so"

echo "Cleaning up temporary files..."
rm -rf "$TEMP_DIR"
