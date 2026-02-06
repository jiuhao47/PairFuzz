#!/bin/bash
set -euo pipefail

# Usage: compile_honggfuzz.sh <xz_hash> <name_suffix>
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

echo "Compiling xz program..."

echo "Compiling xz core..."
$CXX_WRAPPER $COMMON_COMPILE_FLAGS "../../libs/build_xz_libs/xz_core.cpp" \
  -I "../../build/xz_honggfuzz/$xz_hash/usr/local/include/" \
  -o "$TEMP_DIR/xz_core.o"

echo "Compiling xz7z core..."
$CC_WRAPPER $COMMON_COMPILE_FLAGS "../../libs/build_xz7z_libs/7z_core.c" \
  -I "../../build/xz7z_honggfuzz/include" \
  -o "$TEMP_DIR/xz7z_core.o"

LINK_LIBS="-lm"

echo "Building shared library for xz..."
$CXX_WRAPPER -shared -o "$TEMP_DIR/xz_shared.so" "$TEMP_DIR/xz_core.o" \
  "../../build/xz_honggfuzz/$xz_hash/usr/local/lib/liblzma.a" \
  -Wl,-soname,libxz_local.so -Wl,-rpath,'$ORIGIN' $COMMON_LINK_FLAGS

echo "Building shared library for xz7z..."
$CC_WRAPPER -shared -o "$TEMP_DIR/xz7z_shared.so" "$TEMP_DIR/xz7z_core.o" \
  "../../build/xz7z_honggfuzz/install/libxz7z.a" \
  -Wl,-soname,libxz7z_local.so -Wl,-rpath,'$ORIGIN' $COMMON_LINK_FLAGS

echo "Installing shared libraries to bins/.bin/$name_suffix/"
install -m 0755 "$TEMP_DIR/xz_shared.so" "../../bins/.bin/$name_suffix/xz_local_honggfuzz.so"
install -m 0755 "$TEMP_DIR/xz7z_shared.so" "../../bins/.bin/$name_suffix/xz7z_local_honggfuzz.so"

echo "Cleaning up temporary files..."
rm -rf "$TEMP_DIR"
