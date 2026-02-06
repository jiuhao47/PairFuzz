#!/bin/bash
set -euo pipefail

# Usage: compile_honggfuzz.sh <mjson_hash> <cjson_hash> <name_suffix>
if [ "$#" -ne 3 ]; then
  echo "Usage: $0 <mjson_hash> <cjson_hash> <name_suffix>"
  exit 1
fi

mjson_hash=$1
cjson_hash=$2
name_suffix=$3

# Create a temporary build directory and ensure cleanup on exit
TEMP_DIR=$(mktemp -d "temp_differential.XXXX")
trap 'rm -rf "${TEMP_DIR}"' EXIT

# Compiler wrappers
CC_WRAPPER="/usr/bin/clang"
CXX_WRAPPER="/usr/bin/clang++"
if [ ! -x "$CC_WRAPPER" ]; then
  echo "Error: required compiler '$CC_WRAPPER' not found or not executable"
  exit 1
fi
if [ ! -x "$CXX_WRAPPER" ]; then
  echo "Error: required compiler '$CXX_WRAPPER' not found or not executable"
  exit 1
fi

COMMON_COMPILE_FLAGS="-fPIC -fsanitize=address -c"
COMMON_LINK_FLAGS="-fsanitize=address"


# Step 2: link static libraries to object files to generate executables
LINK_LIBS="-lm"

#替换原有直接链接 .a 的段落，改为先检查 .a 是否存在
MJSON_A="../../build/mjson_honggfuzz/$mjson_hash/usr/local/lib/libmjson.a"
CJSON_A="../../build/cjson_honggfuzz/$cjson_hash/usr/local/lib/libcjson.a"


# Step 4: Compile core implementation files (no main) for building shared libs
echo "Compiling mjson core..."
$CC_WRAPPER $COMMON_COMPILE_FLAGS "../../libs/build_mjson_libs/mjson_core.c" \
  -I "../../build/mjson_honggfuzz/$mjson_hash/usr/local/include/" \
  -o "$TEMP_DIR/mjson_core.o"

echo "Compiling cjson core..."
$CC_WRAPPER $COMMON_COMPILE_FLAGS "../../libs/build_cjson_libs/cjson_core.c" \
  -I "../../build/cjson_honggfuzz/$cjson_hash/usr/local/include/" -I "../../build/cjson_honggfuzz/$cjson_hash/usr/local/include/cjson" \
  -o "$TEMP_DIR/cjson_core.o"

echo "Building shared library for mjson..."
if [ -f "$MJSON_A" ]; then
  $CC_WRAPPER -shared -o "$TEMP_DIR/libmjson_shared.so" "$TEMP_DIR/mjson_core.o" \
    "$MJSON_A" -Wl,-soname,libmjson_local.so -Wl,-rpath,'$ORIGIN' $COMMON_LINK_FLAGS
else
  echo "Warning: $MJSON_A not found — building shared library without static archive"
  $CC_WRAPPER -shared -o "$TEMP_DIR/libmjson_shared.so" "$TEMP_DIR/mjson_core.o" \
    -Wl,-soname,libmjson_local.so -Wl,-rpath,'$ORIGIN' $COMMON_LINK_FLAGS
fi

echo "Building shared library for cjson..."
if [ -f "$CJSON_A" ]; then
  $CC_WRAPPER -shared -o "$TEMP_DIR/libcjson_shared.so" "$TEMP_DIR/cjson_core.o" \
    "$CJSON_A" -Wl,-soname,libcjson_local.so -Wl,-rpath,'$ORIGIN' $COMMON_LINK_FLAGS
else
  echo "Warning: $CJSON_A not found — building shared library without static archive"
  $CC_WRAPPER -shared -o "$TEMP_DIR/libcjson_shared.so" "$TEMP_DIR/cjson_core.o" \
    -Wl,-soname,libcjson_local.so -Wl,-rpath,'$ORIGIN' $COMMON_LINK_FLAGS
fi

echo "Installing shared libraries to bins/.bin/$name_suffix/"
install -m 0755 "$TEMP_DIR/libmjson_shared.so" "../../bins/.bin/$name_suffix/mjson_local_honggfuzz.so"
install -m 0755 "$TEMP_DIR/libcjson_shared.so" "../../bins/.bin/$name_suffix/cjson_local_honggfuzz.so"

# Step 6: Clean up
echo "Cleaning up temporary files..."
rm -rf "$TEMP_DIR"
