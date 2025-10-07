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
echo "Compiling xz program..."

$CXX_WRAPPER $COMMON_COMPILE_FLAGS "../../libs/build_xz_libs/xz_program.cpp" \
  -I "../../build/xz/$xz_hash/usr/local/include/" \
  -o "$TEMP_DIR/xz_program.o"

echo "Compiling xz7z program..."
echo "../../build/xz7z/install"
$CC_WRAPPER $COMMON_COMPILE_FLAGS "../../libs/build_xz7z_libs/7z_program.c" \
  -I "../../build/xz7z/include" \
  -o "$TEMP_DIR/xz7z_program.o"

# Step 2: link static libraries to object files to generate executables
LINK_LIBS="-lm"

echo "Linking xz executable..."
$CXX_WRAPPER $COMMON_LINK_FLAGS "$TEMP_DIR/xz_program.o" \
  "../../build/xz/$xz_hash/usr/local/lib/liblzma.a" \
  -o "$TEMP_DIR/xz_executable" $LINK_LIBS

echo "Linking xz7z executable..."
$CC_WRAPPER $COMMON_LINK_FLAGS "$TEMP_DIR/xz7z_program.o" \
  "../../build/xz7z/install/libxz7z.a" \
  -o "$TEMP_DIR/xz7z_executable" $LINK_LIBS

# Step 3: Move executables to bins/.bin
echo "Installing executables to bins/.bin/$name_suffix..."
mkdir -p "../../bins/.bin/$name_suffix"
install -m 0755 "$TEMP_DIR/xz_executable" "../../bins/.bin/$name_suffix/xz"
install -m 0755 "$TEMP_DIR/xz7z_executable" "../../bins/.bin/$name_suffix/xz7z"

# Clean up
echo "Cleaning up temporary files..."
rm -rf "$TEMP_DIR"
