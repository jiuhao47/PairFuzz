#!/bin/bash
set -euo pipefail

# Usage: compile.sh <libpng_hash> <libspng_hash> <name_suffix>
if [ "$#" -ne 3 ]; then
  echo "Usage: $0 <libpng_hash> <libspng_hash> <name_suffix>"
  exit 1
fi

libpng_hash=$1
libspng_hash=$2
name_suffix=$3

# Create a temporary build directory and ensure cleanup on exit
TEMP_DIR=$(mktemp -d "temp_differential.XXXX")
trap 'rm -rf "${TEMP_DIR}"' EXIT

# Compiler wrappers (fall back to system compilers if wrappers missing)
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

# Common flags and include dirs
COMMON_COMPILE_FLAGS="-fPIC -fsanitize=address -c"
COMMON_LINK_FLAGS="-fsanitize=address"

# AFL++ runtime object providing cmplog hooks
AFL_RT_OBJ="../../../fuzzers/AFLplusplus/afl-compiler-rt.o"


# Step 1: Compile program files to object files
echo "Compiling libpng program..."

# Also compile the core implementation separately (no main) for building the shared lib
echo "Compiling libpng core..."
AFL_LLVM_CMPLOG=1 $CXX_WRAPPER $COMMON_COMPILE_FLAGS "../../libs/build_png_libs/libpng_core.cc" \
  -I "../../build/png_cmplog/$libpng_hash/usr/local/include/" \
  -o "$TEMP_DIR/libpng_core.o"

echo "Compiling libspng core..."
AFL_LLVM_CMPLOG=1 $CC_WRAPPER $COMMON_COMPILE_FLAGS "../../libs/build_spng_libs/libspng_core.c" \
  -I "../../build/spng_cmplog/$libspng_hash/usr/local/include" \
  -o "$TEMP_DIR/libspng_core.o"

# Step 2: link static libraries to object files to generate executables
LINK_LIBS="-lm"


# Step 4: build shared libraries (position-independent, bundle static deps)
echo "Building shared library for libpng..."
## Use the core object (without main) when building the shared library
AFL_LLVM_CMPLOG=1 $CXX_WRAPPER -shared -o "$TEMP_DIR/libpng_shared.so" "$TEMP_DIR/libpng_core.o" \
  "../../build/png_cmplog/$libpng_hash/usr/local/lib/libpng.a" \
  "../../build/zlib/dep/usr/local/lib/libz.a" "$AFL_RT_OBJ" \
  -Wl,-soname,libpng_local.so -Wl,-rpath,'$ORIGIN' $COMMON_LINK_FLAGS

echo "Building shared library for libspng..."
AFL_LLVM_CMPLOG=1 $CC_WRAPPER -shared -o "$TEMP_DIR/libspng_shared.so" "$TEMP_DIR/libspng_core.o" \
  "../../build/spng_cmplog/$libspng_hash/usr/local/lib/libspng_static.a" \
  "../../build/zlib/dep/usr/local/lib/libz.a" "$AFL_RT_OBJ" \
  -Wl,-soname,libspng_local.so -Wl,-rpath,'$ORIGIN' $COMMON_LINK_FLAGS

echo "Installing shared libraries to bins/.bin/$name_suffix/"
install -m 0755 "$TEMP_DIR/libpng_shared.so" "../../bins/.bin/$name_suffix/libpng_local_cmplog.so"
install -m 0755 "$TEMP_DIR/libspng_shared.so" "../../bins/.bin/$name_suffix/libspng_local_cmplog.so"

# Step 5: Clean up
echo "Cleaning up temporary files..."
rm -rf "$TEMP_DIR"
