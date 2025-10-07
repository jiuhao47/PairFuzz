#!/bin/bash
set -euo pipefail

# Usage: compile.sh <im_hash> <gm_hash> <name_suffix>
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

# Compiler wrappers (fall back to system compilers if wrappers missing)
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

# Common flags and include dirs
COMMON_COMPILE_FLAGS="-fPIC -fsanitize=address -c"
COMMON_LINK_FLAGS="-fsanitize=address"


# Step 1: Compile IM and GM program/programs to object files
echo "Compiling IM program..."

$CXX_WRAPPER $COMMON_COMPILE_FLAGS "../../libs/build_IM_libs/IM_program.cpp" \
  -DMAGICKCORE_HDRI_ENABLE=1 -DMAGICKCORE_QUANTUM_DEPTH=16 \
  -I "../../build/IM/$im_hash/usr/local/include/ImageMagick-7" \
  -o "$TEMP_DIR/IM_program.o"

echo "Compiling GM program..."
$CXX_WRAPPER $COMMON_COMPILE_FLAGS "../../libs/build_GM_libs/GM_program.cpp" \
  -I "../../build/GM/$gm_hash/usr/local/include/GraphicsMagick" \
  -o "$TEMP_DIR/GM_program.o"

# Step 2: link static libraries to program objects to create executables
# Add common system libs frequently required by ImageMagick/GraphicsMagick
LINK_LIBS="-lm -ldl -lpthread"
echo "Linking IM executable..."
$CXX_WRAPPER $COMMON_LINK_FLAGS "$TEMP_DIR/IM_program.o" \
  "../../build/IM/$im_hash/usr/local/lib/libMagick++-7.Q16HDRI.a" \
  "../../build/IM/$im_hash/usr/local/lib/libMagickWand-7.Q16HDRI.a" \
  "../../build/IM/$im_hash/usr/local/lib/libMagickCore-7.Q16HDRI.a" \
  "../../build/xml2/dep/usr/local/lib/libxml2.a" \
  "../../build/png/dep/usr/local/lib/libpng16.a" \
  "../../build/jpeg/dep/usr/local/lib/libjpeg.a" \
  "../../build/xz/dep/usr/local/lib/liblzma.a" \
  "../../build/bzip2/dep/usr/local/lib/libbz2.a" \
  "../../build/zlib/dep/usr/local/lib/libz.a" \
  -o "$TEMP_DIR/IM_executable" $LINK_LIBS

echo "Linking GM executable..."
$CXX_WRAPPER $COMMON_LINK_FLAGS "$TEMP_DIR/GM_program.o" \
  "../../build/GM/$gm_hash/usr/local/lib/libGraphicsMagick++.a" \
  "../../build/GM/$gm_hash/usr/local/lib/libGraphicsMagick.a" \
  "../../build/jpeg/dep/usr/local/lib/libjpeg.a" \
  "../../build/xml2/dep/usr/local/lib/libxml2.a" \
  "../../build/png/dep/usr/local/lib/libpng16.a" \
  "../../build/bzip2/dep/usr/local/lib/libbz2.a" \
  "../../build/zlib/dep/usr/local/lib/libz.a" \
  -o "$TEMP_DIR/GM_executable" $LINK_LIBS

# Step 3: Move executables to bins/.bin
echo "Installing executables to bins/.bin/$name_suffix..."
mkdir -p "../../bins/.bin/$name_suffix"
install -m 0755 "$TEMP_DIR/IM_executable" "../../bins/.bin/$name_suffix/IM"
install -m 0755 "$TEMP_DIR/GM_executable" "../../bins/.bin/$name_suffix/GM"

# Step 4: Clean up
echo "Cleaning up temporary files..."
rm -rf "$TEMP_DIR"
