#!/bin/bash
set -euo pipefail

# Usage: compile.sh <FFmpeg_hash> <openh264_hash> <name_suffix>
if [ "$#" -ne 3 ]; then
  echo "Usage: $0 <FFmpeg_hash> <openh264_hash> <name_suffix>"
  exit 1
fi

FFmpeg_hash=$1
openh264_hash=$2
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

# Resolve build lib directories
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" &>/dev/null && pwd)"
BUILD_FFMPEG_LIB_DIR="$SCRIPT_DIR/../../build/FFmpeg/$FFmpeg_hash/usr/local/lib"
BUILD_OPENH264_LIB_DIR="$SCRIPT_DIR/../../build/openh264/$openh264_hash/usr/local/lib"

# Step 1: Compile program files to object files
echo "Compiling FFmpeg program..."

$CXX_WRAPPER $COMMON_COMPILE_FLAGS "../../libs/build_FFmpeg_libs/ffmpeg_program.cpp" \
  -I "../../build/FFmpeg/$FFmpeg_hash/usr/local/include/" \
  -o "$TEMP_DIR/FFmpeg_program.o"

echo "Compiling openh264 program..."
$CXX_WRAPPER $COMMON_COMPILE_FLAGS "../../libs/build_openh264_libs/openh264_program.cpp" \
  -I "../../build/openh264/$openh264_hash/usr/local/include" \
  -o "$TEMP_DIR/openh264_program.o"

# temp
$CXX_WRAPPER $COMMON_COMPILE_FLAGS "../../libs/build_FFmpeg_libs/test_ffmpeg.cc" \
  -I "../../build/FFmpeg/$FFmpeg_hash/usr/local/include/" \
  -o "../../libs/build_FFmpeg_libs/test_ffmpeg.o"
$CXX_WRAPPER $COMMON_COMPILE_FLAGS "../../libs/build_openh264_libs/test_openh264.cc" \
  -I "../../build/openh264/$openh264_hash/usr/local/include" \
  -o "../../libs/build_openh264_libs/test_openh264.o"


# Add common compression libraries required by FFmpeg's codecs (zlib, lzma, bzip2)
LINK_LIBS="-lm -lpthread -ldl"

echo "Linking FFmpeg executable..."
$CXX_WRAPPER $COMMON_LINK_FLAGS "$TEMP_DIR/FFmpeg_program.o" \
  "$BUILD_FFMPEG_LIB_DIR/libavcodec.a" \
  "$BUILD_FFMPEG_LIB_DIR/libavdevice.a" \
  "$BUILD_FFMPEG_LIB_DIR/libavfilter.a" \
  "$BUILD_FFMPEG_LIB_DIR/libavformat.a" \
  "$BUILD_FFMPEG_LIB_DIR/libavutil.a" \
  "$BUILD_FFMPEG_LIB_DIR/libswresample.a" \
  "$BUILD_FFMPEG_LIB_DIR/libswscale.a" \
  "../../build/xz/dep/usr/local/lib/liblzma.a" \
  "../../build/zlib/dep/usr/local/lib/libz.a" \
  "../../build/bzip2/dep/usr/local/lib/libbz2.a" \
  -o "$TEMP_DIR/ffmpeg_executable" $LINK_LIBS

echo "Linking FFmpeg test executable..."
$CXX_WRAPPER $COMMON_LINK_FLAGS "../../libs/build_FFmpeg_libs/test_ffmpeg.o" \
  "$BUILD_FFMPEG_LIB_DIR/libavcodec.a" \
  "$BUILD_FFMPEG_LIB_DIR/libavdevice.a" \
  "$BUILD_FFMPEG_LIB_DIR/libavfilter.a" \
  "$BUILD_FFMPEG_LIB_DIR/libavformat.a" \
  "$BUILD_FFMPEG_LIB_DIR/libavutil.a" \
  "$BUILD_FFMPEG_LIB_DIR/libswresample.a" \
  "$BUILD_FFMPEG_LIB_DIR/libswscale.a" \
  "../../build/xz/dep/usr/local/lib/liblzma.a" \
  "../../build/zlib/dep/usr/local/lib/libz.a" \
  "../../build/bzip2/dep/usr/local/lib/libbz2.a" \
  -o "../../libs/build_FFmpeg_libs/ffmpeg_test_executable" $LINK_LIBS

echo "Linking openh264 executable..."
$CXX_WRAPPER $COMMON_LINK_FLAGS "$TEMP_DIR/openh264_program.o" \
  "$BUILD_OPENH264_LIB_DIR/libopenh264.a" \
  -o "$TEMP_DIR/openh264_executable" $LINK_LIBS

echo "Linking openh264 test executable..."
$CXX_WRAPPER $COMMON_LINK_FLAGS "../../libs/build_openh264_libs/test_openh264.o" \
  "$BUILD_OPENH264_LIB_DIR/libopenh264.a" \
  -o "../../libs/build_openh264_libs/openh264_test_executable" $LINK_LIBS

# Step 3: Move executables to bins/.bin
echo "Installing executables to bins/.bin/$name_suffix..."
mkdir -p "../../bins/.bin/$name_suffix"
install -m 0755 "$TEMP_DIR/ffmpeg_executable" "../../bins/.bin/$name_suffix/ffmpeg"
install -m 0755 "$TEMP_DIR/openh264_executable" "../../bins/.bin/$name_suffix/openh264"

# Clean up
echo "Cleaning up temporary files..."
rm -rf "$TEMP_DIR"

echo "Done! installed ffmpeg and openh264 in ../../bins/.bin/$name_suffix"
