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
echo "Compiling libxml2 program..."
$CXX_WRAPPER $COMMON_COMPILE_FLAGS "../../libs/build_xml2_libs/libxml2_program.cc" \
  -I "../../build/xml2/$libxml2_hash/usr/local/include/libxml2/" \
  -o "$TEMP_DIR/libxml2_program.o"

echo "Compiling libexpat program..."
$CXX_WRAPPER $COMMON_COMPILE_FLAGS "../../libs/build_expat_libs/expat_program.cpp" \
  -I "../../build/expat/$expat_hash/usr/local/include/" \
  -o "$TEMP_DIR/expat_program.o"

# Step 2: link static libraries to object files to generate executables
# Start with some common libs; if additional libs are required they should be added here
EXTRA_LIBS=(
  "../../build/xz/dep/usr/local/lib/liblzma.a"
  "../../build/zlib/dep/usr/local/lib/libz.a"
)

LINK_LIBS="-lm"

echo "Linking libxml2 executable..."
$CXX_WRAPPER $COMMON_LINK_FLAGS "$TEMP_DIR/libxml2_program.o" \
  "../../build/xml2/$libxml2_hash/usr/local/lib/libxml2.a" \
  "${EXTRA_LIBS[@]}" \
  -o "$TEMP_DIR/libxml2_executable" $LINK_LIBS

echo "Linking libexpat executable..."
$CXX_WRAPPER $COMMON_LINK_FLAGS "$TEMP_DIR/expat_program.o" \
  "../../build/expat/$expat_hash/usr/local/lib/libexpat.a" \
  "${EXTRA_LIBS[@]}" \
  -o "$TEMP_DIR/libexpat_executable" $LINK_LIBS

# Step 3: Move executables to bins/.bin
echo "Installing executables to bins/.bin/$name_suffix..."
mkdir -p "../../bins/.bin/$name_suffix"
install -m 0755 "$TEMP_DIR/libxml2_executable" "../../bins/.bin/$name_suffix/xml2"
install -m 0755 "$TEMP_DIR/libexpat_executable" "../../bins/.bin/$name_suffix/expat"

# Clean up
echo "Cleaning up temporary files..."
rm -rf "$TEMP_DIR"
