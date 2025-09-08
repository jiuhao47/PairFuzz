# Create a temporary directory for extracting and combining libraries
TEMP_DIR="temp_differential"
mkdir -p "$TEMP_DIR"
FFmpeg_hash=$1
openh264_hash=$2
name_suffix=$3

set -e

if [ -z "$name_suffix" ]; then
  echo "Error: Name suffix is required"
  exit 1
fi
if [ -z "$openh264_hash" ]; then
  echo "Error: openh264 hash is required"
  exit 1
fi
if [ -z "$FFmpeg_hash" ]; then
  echo "Error: FFmpeg hash is required"
  exit 1
fi

# Step 1: Compile harness files to object files
echo "Compiling FFmpeg harness..."

"../../compiler/target/release/libafl_cxx" -fPIC -fsanitize=address -c "../../libs/build_FFmpeg_libs/ffmpeg_harness.cpp" \
  -I "../../build/FFmpeg/$FFmpeg_hash/usr/local/include/" \
  -o "$TEMP_DIR/FFmpeg_harness.o"

echo "Compiling openh264 harness..."
echo "../../build/openh264/$openh264_hash/usr/local/include"
"../../compiler/target/release/libafl_cc" -fPIC -fsanitize=address -c "../../libs/build_openh264_libs/openh264_harness.cpp" \
  -I "../../build/openh264/$openh264_hash/usr/local/include" \
  -o "$TEMP_DIR/openh264_harness.o"

# Step 2: Extract existing static libraries
echo "Extracting FFmpeg: libavcodec.a libavdevice.a libavfilter.a libavformat.a  libavutil.a libswresample.a libswscale.a..."
cd "$TEMP_DIR"
ar x "../../../build/FFmpeg/$FFmpeg_hash/usr/local/lib/libavcodec.a"
ar x "../../../build/FFmpeg/$FFmpeg_hash/usr/local/lib/libavdevice.a"
ar x "../../../build/FFmpeg/$FFmpeg_hash/usr/local/lib/libavfilter.a"
ar x "../../../build/FFmpeg/$FFmpeg_hash/usr/local/lib/libavformat.a"
ar x "../../../build/FFmpeg/$FFmpeg_hash/usr/local/lib/libavutil.a"
ar x "../../../build/FFmpeg/$FFmpeg_hash/usr/local/lib/libswresample.a"
ar x "../../../build/FFmpeg/$FFmpeg_hash/usr/local/lib/libswscale.a"


echo "Extracting libopenh264.a..."
ar x "../../../build/openh264/$openh264_hash/usr/local/lib/libopenh264.a"
cd ..

# Step 3: Create the combined static library
echo "Creating combined static library libdifferential.a..."
ar rcs libdifferential.a "$TEMP_DIR"/*.o

# Step 4: Verify the library contains our functions
echo "Verifying library contents..."
nm libdifferential.a | grep -E "(FFmpeg_h264_parse|openh264_parse)" || echo "Warning: Expected functions not found"

# Step 5: Clean up
echo "Cleaning up temporary files..."
rm -rf "$TEMP_DIR"

mv libdifferential.a ../libs/libdifferential_$name_suffix.a

echo "Done! libdifferential_$name_suffix.a"
