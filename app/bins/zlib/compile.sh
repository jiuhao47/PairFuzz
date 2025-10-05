#!/bin/bash
set -euo pipefail

# Usage: compile.sh <zlib_hash> <libdeflate_hash> <name_suffix>
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
CC_WRAPPER="../../compiler/target/release/libafl_forkserver_cc"
CXX_WRAPPER="../../compiler/target/release/libafl_forkserver_cxx"
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

# Step 1: Compile program files to object files
echo "Compiling zlib program..."

$CC_WRAPPER $COMMON_COMPILE_FLAGS "../../libs/build_zlib_libs/zlib_program.c" \
  -I "../../build/zlib/$zlib_hash/usr/local/include/" \
  -o "$TEMP_DIR/zlib_program.o"

echo "Compiling libdeflate program..."
echo "../../build/deflate/$libdeflate_hash/usr/local/include"
# Try to compile the new program; if it fails, try the old program file as a fallback.
if $CXX_WRAPPER $COMMON_COMPILE_FLAGS "../../libs/build_deflate_libs/libdeflate_program.cpp" \
  -I "../../build/deflate/$libdeflate_hash/usr/local/include" \
  -I "../../libs/build_deflate_libs/" \
  -o "$TEMP_DIR/libdeflate_program.o"; then
  echo "Compiled libdeflate_program.cpp -> $TEMP_DIR/libdeflate_program.o"
elif [ -f "../../libs/build_deflate_libs/libdeflate_program_old.cpp" ] && \
  $CXX_WRAPPER $COMMON_COMPILE_FLAGS "../../libs/build_deflate_libs/libdeflate_program_old.cpp" \
  -I "../../build/deflate/$libdeflate_hash/usr/local/include" \
  -I "../../libs/build_deflate_libs/" \
  -o "$TEMP_DIR/libdeflate_program.o"; then
  echo "Compiled libdeflate_program_old.cpp -> $TEMP_DIR/libdeflate_program.o"
else
  echo "Error: failed to compile libdeflate program (tried libdeflate_program.cpp and libdeflate_program_old.cpp)" >&2
  exit 1
fi

LINK_LIBS="-lm"

echo "Linking zlib executable..."
$CC_WRAPPER $COMMON_LINK_FLAGS "$TEMP_DIR/zlib_program.o" \
  "../../build/zlib/$zlib_hash/usr/local/lib/libz.a" \
  -o "$TEMP_DIR/zlib_executable" $LINK_LIBS

echo "Linking libdeflate executable..."
$CXX_WRAPPER $COMMON_LINK_FLAGS "$TEMP_DIR/libdeflate_program.o" \
  "../../build/deflate/$libdeflate_hash/usr/local/lib/libdeflate.a" \
  -o "$TEMP_DIR/libdeflate_executable" $LINK_LIBS

# Step 3: Move executables to bins/.bin
echo "Installing executables to bins/.bin/$name_suffix..."
mkdir -p "../../bins/.bin/$name_suffix"
install -m 0755 "$TEMP_DIR/zlib_executable" "../../bins/.bin/$name_suffix/zlib"
install -m 0755 "$TEMP_DIR/libdeflate_executable" "../../bins/.bin/$name_suffix/libdeflate"

# Clean up
echo "Cleaning up temporary files..."
rm -rf "$TEMP_DIR"
