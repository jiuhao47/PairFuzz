#!/bin/bash
set -euo pipefail

# Usage: compile.sh <openssl_hash> <wolfssl_hash> <name_suffix>
if [ "$#" -ne 3 ]; then
  echo "Usage: $0 <openssl_hash> <wolfssl_hash> <name_suffix>"
  exit 1
fi

openssl_hash=$1
wolfssl_hash=$2
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

# Step 1: Compile program files to object files for OpenSSL and wolfSSL
echo "Compiling OpenSSL program..."

$CXX_WRAPPER $COMMON_COMPILE_FLAGS "../../libs/build_openssl_libs/openssl_verify.cpp" \
  -I "../../build/openssl/$openssl_hash/usr/local/include/" \
  -o "$TEMP_DIR/openssl_program.o"

echo "Compiling wolfSSL program..."
$CXX_WRAPPER $COMMON_COMPILE_FLAGS "../../libs/build_wolfssl_libs/wolfssl_verify.cpp" \
  -I "../../build/wolfssl/$wolfssl_hash/usr/local/include/" \
  -o "$TEMP_DIR/wolfssl_program.o"

# Step 2: link static libraries to object files to generate executables
LINK_LIBS="-lm -lpthread -ldl"

echo "Linking OpenSSL executable..."
$CXX_WRAPPER $COMMON_LINK_FLAGS "$TEMP_DIR/openssl_program.o" \
  "../../build/openssl/$openssl_hash/usr/local/lib64/libssl.a" \
  "../../build/openssl/$openssl_hash/usr/local/lib64/libcrypto.a" \
  "../../build/zlib/dep/usr/local/lib/libz.a" \
  -o "$TEMP_DIR/openssl_executable" $LINK_LIBS

echo "Linking wolfSSL executable..."
$CXX_WRAPPER $COMMON_LINK_FLAGS "$TEMP_DIR/wolfssl_program.o" \
  "../../build/wolfssl/$wolfssl_hash/usr/local/lib/libwolfssl.a" \
  "../../build/zlib/dep/usr/local/lib/libz.a" \
  -o "$TEMP_DIR/wolfssl_executable" $LINK_LIBS

# Step 3: Move executables to bins/.verify
echo "Installing executables to bins/.verify/$name_suffix..."
mkdir -p "../../bins/.verify/$name_suffix"
install -m 0755 "$TEMP_DIR/openssl_executable" "../../bins/.verify/$name_suffix/openssl"
install -m 0755 "$TEMP_DIR/wolfssl_executable" "../../bins/.verify/$name_suffix/wolfssl"

# Clean up
echo "Cleaning up temporary files..."
rm -rf "$TEMP_DIR"
