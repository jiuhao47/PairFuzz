#!/bin/bash
set -euo pipefail

# Usage: compile_honggfuzz.sh <archive_hash> <libtar_hash> <name_suffix>
if [ "$#" -ne 3 ]; then
  echo "Usage: $0 <archive_hash> <libtar_hash> <name_suffix>"
  exit 1
fi

archive_hash=$1
libtar_hash=$2
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

echo "Compiling archive core..."
$CXX_WRAPPER $COMMON_COMPILE_FLAGS "../../libs/build_archive_libs/archive_core.cpp" \
  -I "../../build/archive_honggfuzz/$archive_hash/usr/local/include/" \
  -o "$TEMP_DIR/archive_core.o"

echo "Compiling libtar core..."
$CXX_WRAPPER $COMMON_COMPILE_FLAGS "../../libs/build_libtar_libs/libtar_core.cpp" \
  -I "../../build/libtar_honggfuzz/$libtar_hash/usr/local/include" \
  -o "$TEMP_DIR/libtar_core.o"

LINK_LIBS="-lm -lbsd"

echo "Building shared library for libtar..."
$CXX_WRAPPER -shared -o "$TEMP_DIR/libtar_shared.so" "$TEMP_DIR/libtar_core.o" \
  "../../build/libtar_honggfuzz/$libtar_hash/usr/local/lib/libtar.a" \
  -Wl,-soname,libtar_local.so -Wl,-rpath,'$ORIGIN' $COMMON_LINK_FLAGS $LINK_LIBS

echo "Installing shared library to bins/.bin/$name_suffix/"
install -m 0755 "$TEMP_DIR/libtar_shared.so" "../../bins/.bin/$name_suffix/libtar_local_honggfuzz.so"

echo "Building shared library for archive..."
$CXX_WRAPPER -shared -o "$TEMP_DIR/archive_shared.so" "$TEMP_DIR/archive_core.o" \
  "../../build/archive_honggfuzz/$archive_hash/usr/local/lib/libarchive.a" \
  -Wl,-soname,libarchive_local.so -Wl,-rpath,'$ORIGIN' $COMMON_LINK_FLAGS $LINK_LIBS

echo "Installing shared library to bins/.bin/$name_suffix/"
install -m 0755 "$TEMP_DIR/archive_shared.so" "../../bins/.bin/$name_suffix/archive_local_honggfuzz.so"

echo "Cleaning up temporary files..."
rm -rf "$TEMP_DIR"
