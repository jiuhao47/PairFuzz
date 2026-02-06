#!/usr/bin/env bash
set -euo pipefail

# Usage: compile.sh <src_cpp> <name_suffix>
if [ "$#" -ne 2 ]; then
  echo "Usage: $0 <source_cpp> <name_suffix>"
  exit 1
fi

export PATH="/opt/clang+llvm-11.0.0/bin:$PATH"
# check if clang version is 11 (robust extraction of the major version number)
CLANG_VERSION=$(clang --version | head -n 1 | grep -oE '[0-9]+' | head -n1 || true)
if ! [[ "$CLANG_VERSION" =~ ^[0-9]+$ ]]; then
  echo "Error: unable to parse clang version: $CLANG_VERSION"
  exit 1
fi
if [ "$CLANG_VERSION" -ne 11 ]; then
  echo "Error: clang version 11 is required. Current version is $CLANG_VERSION"
  exit 1
fi

SRC_CPP=$1
NAME_SUFFIX=$2

# Create a temporary build directory and ensure cleanup on exit
TEMP_DIR=$(mktemp -d "temp_build.XXXX")
trap 'rm -rf "${TEMP_DIR}"' EXIT

# Compiler wrappers (same paths as PNG compile script)
# src==afl-plusplus_program.cpp:
if [ "$NAME_SUFFIX" == "aflplusplus" ]; then
  CC_WRAPPER="../../compiler/target/release/libafl_forkserver_cc"
  CXX_WRAPPER="../../compiler/target/release/libafl_forkserver_cxx"
# src==libafl_fuzz_program.cpp:
elif [ "$NAME_SUFFIX" == "libafl_fuzz" ]; then
  CC_WRAPPER="../../compiler/target/release/libafl_forkserver_cc"
  CXX_WRAPPER="../../compiler/target/release/libafl_forkserver_cxx"
elif [ "$NAME_SUFFIX" == "afl" ]; then
  CC_WRAPPER="../../../fuzzers/AFL/afl-clang-fast"
  CXX_WRAPPER="../../../fuzzers/AFL/afl-clang-fast++"
else
  echo "Error: unknown name_suffix '$NAME_SUFFIX'"
  exit 1
fi

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
