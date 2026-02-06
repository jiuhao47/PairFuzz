#!/bin/bash
set -euo pipefail

# Usage: compile_honggfuzz.sh <libxml2_hash> <expat_hash> <name_suffix>
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

echo "Compiling libxml2 core..."
$CXX_WRAPPER $COMMON_COMPILE_FLAGS "../../libs/build_xml2_libs/libxml2_core.cc" \
  -I "../../build/xml2_honggfuzz/$libxml2_hash/usr/local/include/libxml2/" \
  -o "$TEMP_DIR/libxml2_core.o"

echo "Compiling expat core..."
$CXX_WRAPPER $COMMON_COMPILE_FLAGS "../../libs/build_expat_libs/expat_core.cpp" \
  -I "../../build/expat_honggfuzz/$expat_hash/usr/local/include/" \
  -o "$TEMP_DIR/expat_core.o"

EXTRA_LIBS=(
  "../../build/xz/dep/usr/local/lib/liblzma.a"
  "../../build/zlib/dep/usr/local/lib/libz.a"
)

LINK_LIBS="-lm"

echo "Building shared library for libxml2..."
$CXX_WRAPPER -shared -o "$TEMP_DIR/libxml2_shared.so" "$TEMP_DIR/libxml2_core.o" \
  "../../build/xml2_honggfuzz/$libxml2_hash/usr/local/lib/libxml2.a" \
  "${EXTRA_LIBS[@]}" \
  -Wl,-soname,libxml2_local.so -Wl,-rpath,'$ORIGIN' $COMMON_LINK_FLAGS

echo "Building shared library for expat..."
$CXX_WRAPPER -shared -o "$TEMP_DIR/libexpat_shared.so" "$TEMP_DIR/expat_core.o" \
  "../../build/expat_honggfuzz/$expat_hash/usr/local/lib/libexpat.a" \
  "${EXTRA_LIBS[@]}" \
  -Wl,-soname,libexpat_local.so -Wl,-rpath,'$ORIGIN' $COMMON_LINK_FLAGS

echo "Installing shared libraries to bins/.bin/$name_suffix/"
install -m 0755 "$TEMP_DIR/libxml2_shared.so" "../../bins/.bin/$name_suffix/xml2_local_honggfuzz.so"
install -m 0755 "$TEMP_DIR/libexpat_shared.so" "../../bins/.bin/$name_suffix/expat_local_honggfuzz.so"

# Clean up
echo "Cleaning up temporary files..."
rm -rf "$TEMP_DIR"
