#!/bin/bash
set -euo pipefail

# Usage: compile.sh <mjson_hash> <cjson_hash> <name_suffix>
if [ "$#" -ne 3 ]; then
  echo "Usage: $0 <mjson_hash> <cjson_hash> <name_suffix>"
  exit 1
fi

mjson_hash=$1
cjson_hash=$2
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
echo "Compiling mjson program..."

$CC_WRAPPER $COMMON_COMPILE_FLAGS "../../libs/build_mjson_libs/mjson_program.c" \
  -I "../../build/mjson/$mjson_hash/usr/local/include/" \
  -o "$TEMP_DIR/mjson_program.o"

echo "Compiling cjson program..."
$CC_WRAPPER $COMMON_COMPILE_FLAGS "../../libs/build_cjson_libs/cjson_program.c" \
  -I "../../build/cjson/$cjson_hash/usr/local/include/" -I "../../build/cjson/$cjson_hash/usr/local/include/cjson" \
  -o "$TEMP_DIR/cjson_program.o"

# Step 2: link static libraries to object files to generate executables
LINK_LIBS="-lm"

# 替换原有直接链接 .a 的段落，改为先检查 .a 是否存在
MJSON_A="../../build/mjson/$mjson_hash/usr/local/lib/libmjson.a"
CJSON_A="../../build/cjson/$cjson_hash/usr/local/lib/libcjson.a"

echo "Linking mjson executable..."
if [ -f "$MJSON_A" ]; then
  echo "Using static library: $MJSON_A"
  $CC_WRAPPER $COMMON_LINK_FLAGS "$TEMP_DIR/mjson_program.o" \
    "$MJSON_A" \
    -o "$TEMP_DIR/mjson_executable" $LINK_LIBS
else
  echo "Warning: $MJSON_A not found — linking without static lib"
  # 不包含 .a 的链接命令（仅链接对象文件和系统库）
  $CC_WRAPPER $COMMON_LINK_FLAGS "$TEMP_DIR/mjson_program.o" \
    -o "$TEMP_DIR/mjson_executable" $LINK_LIBS
fi

echo "Linking cjson executable..."
if [ -f "$CJSON_A" ]; then
  echo "Using static library: $CJSON_A"
  $CC_WRAPPER $COMMON_LINK_FLAGS "$TEMP_DIR/cjson_program.o" \
    "$CJSON_A" \
    -o "$TEMP_DIR/cjson_executable" $LINK_LIBS
else
  echo "Warning: $CJSON_A not found — linking without static lib"
  # 不包含 .a 的链接命令（仅链接对象文件和系统库）
  $CC_WRAPPER $COMMON_LINK_FLAGS "$TEMP_DIR/cjson_program.o" \
    -o "$TEMP_DIR/cjson_executable" $LINK_LIBS
fi

# Step 3: Move executables to bins/.bin
echo "Installing executables to bins/.bin/$name_suffix..."
mkdir -p "../../bins/.bin/$name_suffix"
install -m 0755 "$TEMP_DIR/mjson_executable" "../../bins/.bin/$name_suffix/mjson"
install -m 0755 "$TEMP_DIR/cjson_executable" "../../bins/.bin/$name_suffix/cjson"

# Step 4: Compile core implementation files (no main) for building shared libs
echo "Compiling mjson core..."
$CC_WRAPPER $COMMON_COMPILE_FLAGS "../../libs/build_mjson_libs/mjson_core.c" \
  -I "../../build/mjson/$mjson_hash/usr/local/include/" \
  -o "$TEMP_DIR/mjson_core.o"

echo "Compiling cjson core..."
$CC_WRAPPER $COMMON_COMPILE_FLAGS "../../libs/build_cjson_libs/cjson_core.c" \
  -I "../../build/cjson/$cjson_hash/usr/local/include/" -I "../../build/cjson/$cjson_hash/usr/local/include/cjson" \
  -o "$TEMP_DIR/cjson_core.o"

# Step 5: Build shared libraries (position-independent, bundle static deps if available)
echo "Building shared library for mjson..."
if [ -f "$MJSON_A" ]; then
  $CC_WRAPPER -shared -o "$TEMP_DIR/libmjson_shared.so" "$TEMP_DIR/mjson_core.o" \
    "$MJSON_A" -Wl,-soname,libmjson_local.so -Wl,-rpath,'$ORIGIN' $COMMON_LINK_FLAGS
else
  echo "Warning: $MJSON_A not found — building shared library without static archive"
  $CC_WRAPPER -shared -o "$TEMP_DIR/libmjson_shared.so" "$TEMP_DIR/mjson_core.o" \
    -Wl,-soname,libmjson_local.so -Wl,-rpath,'$ORIGIN' $COMMON_LINK_FLAGS
fi

echo "Building shared library for cjson..."
if [ -f "$CJSON_A" ]; then
  $CC_WRAPPER -shared -o "$TEMP_DIR/libcjson_shared.so" "$TEMP_DIR/cjson_core.o" \
    "$CJSON_A" -Wl,-soname,libcjson_local.so -Wl,-rpath,'$ORIGIN' $COMMON_LINK_FLAGS
else
  echo "Warning: $CJSON_A not found — building shared library without static archive"
  $CC_WRAPPER -shared -o "$TEMP_DIR/libcjson_shared.so" "$TEMP_DIR/cjson_core.o" \
    -Wl,-soname,libcjson_local.so -Wl,-rpath,'$ORIGIN' $COMMON_LINK_FLAGS
fi

echo "Installing shared libraries to bins/.bin/$name_suffix/"
install -m 0755 "$TEMP_DIR/libmjson_shared.so" "../../bins/.bin/$name_suffix/mjson_local.so"
install -m 0755 "$TEMP_DIR/libcjson_shared.so" "../../bins/.bin/$name_suffix/cjson_local.so"

# Step 6: Clean up
echo "Cleaning up temporary files..."
rm -rf "$TEMP_DIR"
