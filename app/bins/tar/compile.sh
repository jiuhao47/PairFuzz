#!/bin/bash
set -euo pipefail

# Usage: compile.sh <archive_hash> <libtar_hash> <name_suffix>
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
echo "Compiling archive program..."

$CXX_WRAPPER $COMMON_COMPILE_FLAGS "../../libs/build_archive_libs/archive_program.cpp" \
  -I "../../build/archive/$archive_hash/usr/local/include/" \
  -o "$TEMP_DIR/archive_program.o"

echo "Compiling archive core..."
$CXX_WRAPPER $COMMON_COMPILE_FLAGS "../../libs/build_archive_libs/archive_core.cpp" \
  -I "../../build/archive/$archive_hash/usr/local/include/" \
  -o "$TEMP_DIR/archive_core.o"

echo "Compiling libtar program..."
$CXX_WRAPPER $COMMON_COMPILE_FLAGS "../../libs/build_libtar_libs/libtar_program.cpp" \
  -I "../../build/libtar/$libtar_hash/usr/local/include" \
  -o "$TEMP_DIR/libtar_program.o"

echo "Compiling libtar core..."
$CXX_WRAPPER $COMMON_COMPILE_FLAGS "../../libs/build_libtar_libs/libtar_core.cpp" \
  -I "../../build/libtar/$libtar_hash/usr/local/include" \
  -o "$TEMP_DIR/libtar_core.o"

# Step 2: link static libraries to program objects to create executables
LINK_LIBS="-lm"

echo "Linking archive executable..."
$CXX_WRAPPER $COMMON_LINK_FLAGS "$TEMP_DIR/archive_program.o" \
  "../../build/archive/$archive_hash/usr/local/lib/libarchive.a" \
  -o "$TEMP_DIR/archive_executable" $LINK_LIBS

echo "Linking libtar executable..."
$CXX_WRAPPER $COMMON_LINK_FLAGS "$TEMP_DIR/libtar_program.o" \
  "../../build/libtar/$libtar_hash/usr/local/lib/libtar.a" \
  -o "$TEMP_DIR/libtar_executable" $LINK_LIBS

# Step 3: Move executables to bins/.bin
echo "Installing executables to bins/.bin/$name_suffix..."
mkdir -p "../../bins/.bin/$name_suffix"
install -m 0755 "$TEMP_DIR/archive_executable" "../../bins/.bin/$name_suffix/archive"
install -m 0755 "$TEMP_DIR/libtar_executable" "../../bins/.bin/$name_suffix/libtar"

# Step 4: build shared library (position-independent, bundle static deps)
echo "Building shared library for libtar..."
$CXX_WRAPPER -shared -o "$TEMP_DIR/libtar_shared.so" "$TEMP_DIR/libtar_core.o" \
  "../../build/libtar/$libtar_hash/usr/local/lib/libtar.a" \
  -Wl,-soname,libtar_local.so -Wl,-rpath,'$ORIGIN' $COMMON_LINK_FLAGS

echo "Installing shared library to bins/.bin/$name_suffix/"
install -m 0755 "$TEMP_DIR/libtar_shared.so" "../../bins/.bin/$name_suffix/libtar_local.so"

echo "Building shared library for archive..."
$CXX_WRAPPER -shared -o "$TEMP_DIR/archive_shared.so" "$TEMP_DIR/archive_core.o" \
  "../../build/archive/$archive_hash/usr/local/lib/libarchive.a" \
  -Wl,-soname,libarchive_local.so -Wl,-rpath,'$ORIGIN' $COMMON_LINK_FLAGS

echo "Installing shared library to bins/.bin/$name_suffix/"
install -m 0755 "$TEMP_DIR/archive_shared.so" "../../bins/.bin/$name_suffix/archive_local.so"

# Clean up
echo "Cleaning up temporary files..."
rm -rf "$TEMP_DIR"
