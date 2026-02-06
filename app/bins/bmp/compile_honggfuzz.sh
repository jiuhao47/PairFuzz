#!/bin/bash
set -euo pipefail

# Usage: compile_honggfuzz.sh <im_hash> <gm_hash> <name_suffix>
if [ "$#" -ne 3 ]; then
  echo "Usage: $0 <im_hash> <gm_hash> <name_suffix>"
  exit 1
fi

im_hash=$1
gm_hash=$2
name_suffix=$3

# Create a temporary build directory and ensure cleanup on exit
TEMP_DIR=$(mktemp -d "temp_differential.XXXX")
trap 'rm -rf "${TEMP_DIR}"' EXIT

# Compiler wrappers: use system clang for honggfuzz builds
CC_WRAPPER="/usr/bin/clang"
CXX_WRAPPER="/usr/bin/clang++"
if [ ! -x "$CC_WRAPPER" ]; then
  echo "Error: required compiler '$CC_WRAPPER' not found or not executable"
  exit 1
fi
if [ ! -x "$CXX_WRAPPER" ]; then
  echo "Error: required compiler '$CXX_WRAPPER' not found or not executable"
  exit 1
fi

# Common flags and include dirs
COMMON_COMPILE_FLAGS="-fPIC -fsanitize=address -c"
COMMON_LINK_FLAGS="-fsanitize=address"


echo "Compiling IM core..."
$CXX_WRAPPER $COMMON_COMPILE_FLAGS "../../libs/build_IM_libs/IM_core.cpp" \
  -DMAGICKCORE_HDRI_ENABLE=1 -DMAGICKCORE_QUANTUM_DEPTH=16 \
  -I "../../build/IM_honggfuzz/$im_hash/usr/local/include/ImageMagick-7" \
  -o "$TEMP_DIR/IM_core.o"

echo "Compiling GM core..."
$CXX_WRAPPER $COMMON_COMPILE_FLAGS "../../libs/build_GM_libs/GM_core.cpp" \
  -I "../..//build/GM_honggfuzz/$gm_hash/usr/local/include/GraphicsMagick" \
  -o "$TEMP_DIR/GM_core.o"

LINK_LIBS="-lm -ldl -lpthread"

echo "Building shared library for IM..."
$CXX_WRAPPER -shared -o "$TEMP_DIR/IM_shared.so" "$TEMP_DIR/IM_core.o" \
  "../../build/IM_honggfuzz/$im_hash/usr/local/lib/libMagick++-7.Q16HDRI.a" \
  "../../build/IM_honggfuzz/$im_hash/usr/local/lib/libMagickWand-7.Q16HDRI.a" \
  "../../build/IM_honggfuzz/$im_hash/usr/local/lib/libMagickCore-7.Q16HDRI.a" \
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
  "../../build/GM_honggfuzz/$gm_hash/usr/local/lib/libGraphicsMagick++.a" \
  "../../build/GM_honggfuzz/$gm_hash/usr/local/lib/libGraphicsMagick.a" \
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
install -m 0755 "$TEMP_DIR/IM_shared.so" "../../bins/.bin/$name_suffix/IM_local_honggfuzz.so"
install -m 0755 "$TEMP_DIR/GM_shared.so" "../../bins/.bin/$name_suffix/GM_local_honggfuzz.so"

echo "Cleaning up temporary files..."
rm -rf "$TEMP_DIR"
