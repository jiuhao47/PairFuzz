#!/usr/bin/env bash
set -euo pipefail

# Usage: compile.sh <src_cpp> <name_suffix>
if [ "$#" -ne 2 ]; then
  echo "Usage: $0 <source_cpp> <name_suffix>"
  exit 1
fi

SRC_CPP=$1
NAME_SUFFIX=$2

# Create a temporary build directory and ensure cleanup on exit
TEMP_DIR=$(mktemp -d "temp_build.XXXX")
trap 'rm -rf "${TEMP_DIR}"' EXIT

CC_WRAPPER="../../../fuzzers/honggfuzz/hfuzz_cc/hfuzz-clang"
CXX_WRAPPER="../../../fuzzers/honggfuzz/hfuzz_cc/hfuzz-clang++"

if [ ! -x "$CC_WRAPPER" ]; then
  echo "Error: required wrapper '$CC_WRAPPER' not found or not executable"
  exit 1
fi
if [ ! -x "$CXX_WRAPPER" ]; then
  echo "Error: required wrapper '$CXX_WRAPPER' not found or not executable"
  exit 1
fi

# Common flags
COMMON_COMPILE_FLAGS="-fPIC -fsanitize=address -c"
COMMON_LINK_FLAGS="-fsanitize=address"

# Compile
echo "Compiling $SRC_CPP..."
$CXX_WRAPPER $COMMON_COMPILE_FLAGS "$SRC_CPP" -o "$TEMP_DIR/program.o"

# Link
echo "Linking executable..."
$CXX_WRAPPER $COMMON_LINK_FLAGS "$TEMP_DIR/program.o" -o "$TEMP_DIR/program"

# Install
echo "Installing executable to bins/.bin/$NAME_SUFFIX..."
mkdir -p "../../bins/.bin/$NAME_SUFFIX"
install -m 0755 "$TEMP_DIR/program" "../../bins/.bin/$NAME_SUFFIX/$NAME_SUFFIX"_program

# Cleanup
echo "Cleaning up temporary files..."
rm -rf "$TEMP_DIR"

echo "Done: ../../bins/.bin/$NAME_SUFFIX/$(basename "$SRC_CPP" .cpp)"
