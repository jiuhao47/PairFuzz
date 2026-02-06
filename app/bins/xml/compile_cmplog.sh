#!/bin/bash
set -euo pipefail

# Usage: compile.sh <libxml2_hash> <expat_hash> <name_suffix>
if [ "$#" -ne 3 ]; then
  echo "Usage: $0 <libxml2_hash> <expat_hash> <name_suffix>"
  exit 1
fi

libxml2_hash=$1
expat_hash=$2
name_suffix=$3

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

echo "Compiling libxml2 core..."
AFL_LLVM_CMPLOG=1 $CXX_WRAPPER $COMMON_COMPILE_FLAGS "../../libs/build_xml2_libs/libxml2_core.cc" \
  -I "../../build/xml2_cmplog/$libxml2_hash/usr/local/include/libxml2/" \
  -o "$TEMP_DIR/libxml2_core.o"

echo "Compiling expat core..."
AFL_LLVM_CMPLOG=1 $CXX_WRAPPER $COMMON_COMPILE_FLAGS "../../libs/build_expat_libs/expat_core.cpp" \
  -I "../../build/expat_cmplog/$expat_hash/usr/local/include/" \
  -o "$TEMP_DIR/expat_core.o"

# Step 2: link static libraries to object files to generate executables
# Start with some common libs; if additional libs are required they should be added here
EXTRA_LIBS=(
  "../../build/xz/dep/usr/local/lib/liblzma.a"
  "../../build/zlib/dep/usr/local/lib/libz.a"
)

LINK_LIBS="-lm"

# Step 4: build shared libraries (position-independent, bundle static deps)
echo "Building shared library for libxml2..."
AFL_LLVM_CMPLOG=1 $CXX_WRAPPER -shared -o "$TEMP_DIR/libxml2_shared.so" "$TEMP_DIR/libxml2_core.o" \
  "../../build/xml2_cmplog/$libxml2_hash/usr/local/lib/libxml2.a" \
  "${EXTRA_LIBS[@]}" "$AFL_RT_OBJ" \
  -Wl,-soname,libxml2_local.so -Wl,-rpath,'$ORIGIN' $COMMON_LINK_FLAGS

echo "Building shared library for expat..."
AFL_LLVM_CMPLOG=1 $CXX_WRAPPER -shared -o "$TEMP_DIR/libexpat_shared.so" "$TEMP_DIR/expat_core.o" \
  "../../build/expat_cmplog/$expat_hash/usr/local/lib/libexpat.a" \
  "${EXTRA_LIBS[@]}" "$AFL_RT_OBJ" \
  -Wl,-soname,libexpat_local.so -Wl,-rpath,'$ORIGIN' $COMMON_LINK_FLAGS

echo "Installing shared libraries to bins/.bin/$name_suffix/"
install -m 0755 "$TEMP_DIR/libxml2_shared.so" "../../bins/.bin/$name_suffix/xml2_local_cmplog.so"
install -m 0755 "$TEMP_DIR/libexpat_shared.so" "../../bins/.bin/$name_suffix/expat_local_cmplog.so"

# Clean up
echo "Cleaning up temporary files..."
rm -rf "$TEMP_DIR"
