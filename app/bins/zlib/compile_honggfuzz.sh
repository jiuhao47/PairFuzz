#!/bin/bash
set -euo pipefail

# Usage: compile_honggfuzz.sh <zlib_hash> <libdeflate_hash> <name_suffix>
if [ "$#" -ne 3 ]; then
  echo "Usage: $0 <zlib_hash> <libdeflate_hash> <name_suffix>"
  exit 1
fi

zlib_hash=$1
libdeflate_hash=$2
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

LINK_LIBS="-lm"

echo "Compiling zlib core..."
$CC_WRAPPER $COMMON_COMPILE_FLAGS "../../libs/build_zlib_libs/zlib_core.c" \
  -I "../../build/zlib_honggfuzz/$zlib_hash/usr/local/include/" \
  -o "$TEMP_DIR/zlib_core.o"

echo "Compiling libdeflate core..."
# Try primary core cpp, fall back to old version if present
if $CXX_WRAPPER $COMMON_COMPILE_FLAGS "../../libs/build_deflate_libs/libdeflate_core.cpp" \
  -I "../../build/deflate_honggfuzz/$libdeflate_hash/usr/local/include" -I "../../libs/build_deflate_libs/" \
  -o "$TEMP_DIR/libdeflate_core.o"; then
  echo "Compiled libdeflate_core.cpp -> $TEMP_DIR/libdeflate_core.o"
elif [ -f "../../libs/build_deflate_libs/libdeflate_core_old.cpp" ] && \
  $CXX_WRAPPER $COMMON_COMPILE_FLAGS "../../libs/build_deflate_libs/libdeflate_core_old.cpp" \
  -I "../../build/deflate_honggfuzz/$libdeflate_hash/usr/local/include" -I "../../libs/build_deflate_libs/" \
  -o "$TEMP_DIR/libdeflate_core.o"; then
  echo "Compiled libdeflate_core_old.cpp -> $TEMP_DIR/libdeflate_core.o"
else
  echo "Error: failed to compile libdeflate core (tried libdeflate_core.cpp and libdeflate_core_old.cpp)" >&2
  exit 1
fi

ZLIB_A="../../build/zlib_honggfuzz/$zlib_hash/usr/local/lib/libz.a"
LIBDEFLATE_A="../../build/deflate_honggfuzz/$libdeflate_hash/usr/local/lib/libdeflate.a"

echo "Building shared library for zlib..."
if [ -f "$ZLIB_A" ]; then
  $CC_WRAPPER -shared -o "$TEMP_DIR/libz_shared.so" "$TEMP_DIR/zlib_core.o" \
    "$ZLIB_A" -Wl,-soname,libz_local.so -Wl,-rpath,'$ORIGIN' $COMMON_LINK_FLAGS
else
  echo "Warning: $ZLIB_A not found — building shared library without static archive"
  $CC_WRAPPER -shared -o "$TEMP_DIR/libz_shared.so" "$TEMP_DIR/zlib_core.o" \
    -Wl,-soname,libz_local.so -Wl,-rpath,'$ORIGIN' $COMMON_LINK_FLAGS
fi

echo "Building shared library for libdeflate..."
if [ -f "$LIBDEFLATE_A" ]; then
  $CXX_WRAPPER -shared -o "$TEMP_DIR/libdeflate_shared.so" "$TEMP_DIR/libdeflate_core.o" \
    "$LIBDEFLATE_A" -Wl,-soname,libdeflate_local.so -Wl,-rpath,'$ORIGIN' $COMMON_LINK_FLAGS
else
  echo "Warning: $LIBDEFLATE_A not found — building shared library without static archive"
  $CXX_WRAPPER -shared -o "$TEMP_DIR/libdeflate_shared.so" "$TEMP_DIR/libdeflate_core.o" \
    -Wl,-soname,libdeflate_local.so -Wl,-rpath,'$ORIGIN' $COMMON_LINK_FLAGS
fi

echo "Installing shared libraries to bins/.bin/$name_suffix/"
install -m 0755 "$TEMP_DIR/libz_shared.so" "../../bins/.bin/$name_suffix/zlib_local_honggfuzz.so"
install -m 0755 "$TEMP_DIR/libdeflate_shared.so" "../../bins/.bin/$name_suffix/libdeflate_local_honggfuzz.so"

# Clean up
echo "Cleaning up temporary files..."
rm -rf "$TEMP_DIR"
