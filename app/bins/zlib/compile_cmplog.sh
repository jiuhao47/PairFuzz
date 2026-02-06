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


LINK_LIBS="-lm"

# Step 4: Compile core implementation files (no main) for building shared libs
echo "Compiling zlib core..."
$CC_WRAPPER $COMMON_COMPILE_FLAGS "../../libs/build_zlib_libs/zlib_core.c" \
  -I "../../build/zlib_cmplog/$zlib_hash/usr/local/include/" \
  -o "$TEMP_DIR/zlib_core.o"

echo "Compiling libdeflate core..."
# Try primary core cpp, fall back to old version if present
if $CXX_WRAPPER $COMMON_COMPILE_FLAGS "../../libs/build_deflate_libs/libdeflate_core.cpp" \
  -I "../../build/deflate_cmplog/$libdeflate_hash/usr/local/include" -I "../../libs/build_deflate_libs/" \
  -o "$TEMP_DIR/libdeflate_core.o"; then
  echo "Compiled libdeflate_core.cpp -> $TEMP_DIR/libdeflate_core.o"
elif [ -f "../../libs/build_deflate_libs/libdeflate_core_old.cpp" ] && \
  $CXX_WRAPPER $COMMON_COMPILE_FLAGS "../../libs/build_deflate_libs/libdeflate_core_old.cpp" \
  -I "../../build/deflate_cmplog/$libdeflate_hash/usr/local/include" -I "../../libs/build_deflate_libs/" \
  -o "$TEMP_DIR/libdeflate_core.o"; then
  echo "Compiled libdeflate_core_old.cpp -> $TEMP_DIR/libdeflate_core.o"
else
  echo "Error: failed to compile libdeflate core (tried libdeflate_core.cpp and libdeflate_core_old.cpp)" >&2
  exit 1
fi

# Step 5: Build shared libraries (position-independent, bundle static deps if available)
ZLIB_A="../../build/zlib_cmplog/$zlib_hash/usr/local/lib/libz.a"
LIBDEFLATE_A="../../build/deflate_cmplog/$libdeflate_hash/usr/local/lib/libdeflate.a"

echo "Building shared library for zlib..."
# If afl-clang-fast/ASAN skipped linking the compiler runtime object, try to add it explicitly.
AFL_COMPILER_RT_OBJ="/home/dev/workspace/fuzzers/AFLplusplus/afl-compiler-rt.o"

EXTRA_LINK_OBJS=""
if [ -f "$AFL_COMPILER_RT_OBJ" ]; then
  echo "Found afl-compiler-rt.o; creating temporary archive to force linkage into shared libs."
  AR_LIB="$TEMP_DIR/libafl_compiler_rt.a"
  cp "$AFL_COMPILER_RT_OBJ" "$TEMP_DIR/"
  (cd "$TEMP_DIR" && ar rcs "$(basename "$AR_LIB")" "$(basename "$AFL_COMPILER_RT_OBJ")")
  EXTRA_LINK_OBJS="-Wl,--whole-archive $AR_LIB -Wl,--no-whole-archive"
fi

if [ -f "$ZLIB_A" ]; then
  $CC_WRAPPER -shared -o "$TEMP_DIR/libz_shared.so" "$TEMP_DIR/zlib_core.o" \
    $EXTRA_LINK_OBJS "$ZLIB_A" -Wl,-soname,libz_local.so -Wl,-rpath,'$ORIGIN' $COMMON_LINK_FLAGS
else
  echo "Warning: $ZLIB_A not found — building shared library without static archive"
  $CC_WRAPPER -shared -o "$TEMP_DIR/libz_shared.so" "$TEMP_DIR/zlib_core.o" \
    $EXTRA_LINK_OBJS -Wl,-soname,libz_local.so -Wl,-rpath,'$ORIGIN' $COMMON_LINK_FLAGS
fi

echo "Building shared library for libdeflate..."
if [ -f "$LIBDEFLATE_A" ]; then
  $CXX_WRAPPER -shared -o "$TEMP_DIR/libdeflate_shared.so" "$TEMP_DIR/libdeflate_core.o" \
    $EXTRA_LINK_OBJS "$LIBDEFLATE_A" -Wl,-soname,libdeflate_local.so -Wl,-rpath,'$ORIGIN' $COMMON_LINK_FLAGS
else
  echo "Warning: $LIBDEFLATE_A not found — building shared library without static archive"
  $CXX_WRAPPER -shared -o "$TEMP_DIR/libdeflate_shared.so" "$TEMP_DIR/libdeflate_core.o" \
    $EXTRA_LINK_OBJS -Wl,-soname,libdeflate_local.so -Wl,-rpath,'$ORIGIN' $COMMON_LINK_FLAGS
fi

echo "Installing shared libraries to bins/.bin/$name_suffix/"
install -m 0755 "$TEMP_DIR/libz_shared.so" "../../bins/.bin/$name_suffix/zlib_local_cmplog.so"
install -m 0755 "$TEMP_DIR/libdeflate_shared.so" "../../bins/.bin/$name_suffix/libdeflate_local_cmplog.so"

# Clean up
echo "Cleaning up temporary files..."
rm -rf "$TEMP_DIR"
