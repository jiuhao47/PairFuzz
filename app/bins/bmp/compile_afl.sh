#!/bin/bash
set -euo pipefail

# Usage: compile.sh <im_hash> <gm_hash> <name_suffix>
if [ "$#" -ne 3 ]; then
  echo "Usage: $0 <im_hash> <gm_hash> <name_suffix>"
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

im_hash=$1
gm_hash=$2
name_suffix=$3

# Create a temporary build directory and ensure cleanup on exit
TEMP_DIR=$(mktemp -d "temp_differential.XXXX")
trap 'rm -rf "${TEMP_DIR}"' EXIT

# Compiler wrappers (fall back to system compilers if wrappers missing)
CC_WRAPPER="../../../AFL/afl-clang-fast"
CXX_WRAPPER="../../../AFL/afl-clang-fast++"
if [ ! -x "$CC_WRAPPER" ]; then
  echo "Error: required wrapper '$CC_WRAPPER' not found or not executable"
  exit 1
fi
if [ ! -x "$CXX_WRAPPER" ]; then
  echo "Error: required wrapper '$CXX_WRAPPER' not found or not executable"
  exit 1
fi

# Common flags and include dirs
COMMON_COMPILE_FLAGS="-fPIC -fsanitize=address -c"
COMMON_LINK_FLAGS="-fsanitize=address"


echo "Compiling IM core..."
$CXX_WRAPPER $COMMON_COMPILE_FLAGS "../../libs/build_IM_libs/IM_core.cpp" \
  -DMAGICKCORE_HDRI_ENABLE=1 -DMAGICKCORE_QUANTUM_DEPTH=16 \
  -I "../../build/IM_afl/$im_hash/usr/local/include/ImageMagick-7" \
  -o "$TEMP_DIR/IM_core.o"

echo "Compiling GM core..."
$CXX_WRAPPER $COMMON_COMPILE_FLAGS "../../libs/build_GM_libs/GM_core.cpp" \
  -I "../../build/GM_afl/$gm_hash/usr/local/include/GraphicsMagick" \
  -o "$TEMP_DIR/GM_core.o"

LINK_LIBS="-lm -ldl -lpthread"

echo "Building shared library for IM..."
$CXX_WRAPPER -shared -o "$TEMP_DIR/IM_shared.so" "$TEMP_DIR/IM_core.o" \
  "../../build/IM_afl/$im_hash/usr/local/lib/libMagick++-7.Q16HDRI.a" \
  "../../build/IM_afl/$im_hash/usr/local/lib/libMagickWand-7.Q16HDRI.a" \
  "../../build/IM_afl/$im_hash/usr/local/lib/libMagickCore-7.Q16HDRI.a" \
  "../../build/xml2/dep/usr/local/lib/libxml2.a" \
  "../../build/png/dep/usr/local/lib/libpng16.a" \
  "../../build/jpeg/dep/usr/local/lib/libjpeg.a" \
  "../../build/freetype/dep/usr/local/lib/libfreetype.a" \
  "../../build/brotli/dep/usr/local/lib/libbrotlienc.a" \
  "../../build/brotli/dep/usr/local/lib/libbrotlidec.a" \
  "../../build/brotli/dep/usr/local/lib/libbrotlicommon.a" \
  "../../build/xz/dep/usr/local/lib/liblzma.a" \
  "../../build/bzip2/dep/usr/local/lib/libbz2.a" \
  "../../build/zlib/dep/usr/local/lib/libz.a" \
  -Wl,-soname,libIM_local.so -Wl,-rpath,'$ORIGIN' $COMMON_LINK_FLAGS

echo "Building shared library for GM..."
$CXX_WRAPPER -shared -o "$TEMP_DIR/GM_shared.so" "$TEMP_DIR/GM_core.o" \
  "../../build/GM_afl/$gm_hash/usr/local/lib/libGraphicsMagick++.a" \
  "../../build/GM_afl/$gm_hash/usr/local/lib/libGraphicsMagick.a" \
  "../../build/jpeg/dep/usr/local/lib/libjpeg.a" \
  "../../build/xml2/dep/usr/local/lib/libxml2.a" \
  "../../build/png/dep/usr/local/lib/libpng16.a" \
  "../../build/freetype/dep/usr/local/lib/libfreetype.a" \
  "../../build/brotli/dep/usr/local/lib/libbrotlienc.a" \
  "../../build/brotli/dep/usr/local/lib/libbrotlidec.a" \
  "../../build/brotli/dep/usr/local/lib/libbrotlicommon.a" \
  "../../build/bzip2/dep/usr/local/lib/libbz2.a" \
  "../../build/zlib/dep/usr/local/lib/libz.a" \
  -Wl,-soname,libGM_local.so -Wl,-rpath,'$ORIGIN' $COMMON_LINK_FLAGS

echo "Installing shared libraries to bins/.bin/$name_suffix/"
install -m 0755 "$TEMP_DIR/IM_shared.so" "../../bins/.bin/$name_suffix/IM_local_afl.so"
install -m 0755 "$TEMP_DIR/GM_shared.so" "../../bins/.bin/$name_suffix/GM_local_afl.so"

echo "Cleaning up temporary files..."
rm -rf "$TEMP_DIR"
