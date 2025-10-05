# Create a temporary directory for extracting and combining libraries
TEMP_DIR="temp_differential"
mkdir -p "$TEMP_DIR"
FFmpeg_hash=$1
openh264_hash=$2
name_suffix=$3

# Resolve script dir so we can use absolute paths for extraction
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" &>/dev/null && pwd)"
BUILD_FFMPEG_LIB_DIR="$SCRIPT_DIR/../../build/FFmpeg/$FFmpeg_hash/usr/local/lib"
BUILD_OPENH264_LIB_DIR="$SCRIPT_DIR/../../build/openh264/$openh264_hash/usr/local/lib"

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
echo "Extracting FFmpeg libraries into separate subdirectories..."
mkdir -p "$TEMP_DIR/libavcodec" "$TEMP_DIR/libavdevice" "$TEMP_DIR/libavfilter" \
         "$TEMP_DIR/libavformat" "$TEMP_DIR/libavutil" "$TEMP_DIR/libswresample" "$TEMP_DIR/libswscale"

(cd "$TEMP_DIR/libavcodec" && ar x "$BUILD_FFMPEG_LIB_DIR/libavcodec.a")
(cd "$TEMP_DIR/libavdevice" && ar x "$BUILD_FFMPEG_LIB_DIR/libavdevice.a")
(cd "$TEMP_DIR/libavfilter" && ar x "$BUILD_FFMPEG_LIB_DIR/libavfilter.a")
(cd "$TEMP_DIR/libavformat" && ar x "$BUILD_FFMPEG_LIB_DIR/libavformat.a")
(cd "$TEMP_DIR/libavutil" && ar x "$BUILD_FFMPEG_LIB_DIR/libavutil.a")
(cd "$TEMP_DIR/libswresample" && ar x "$BUILD_FFMPEG_LIB_DIR/libswresample.a")
(cd "$TEMP_DIR/libswscale" && ar x "$BUILD_FFMPEG_LIB_DIR/libswscale.a")

echo "Extracting libopenh264.a into its own subdirectory..."
mkdir -p "$TEMP_DIR/libopenh264"
(cd "$TEMP_DIR/libopenh264" && ar x "$BUILD_OPENH264_LIB_DIR/libopenh264.a")

# Step 3: Create the combined static library (safe merge from subdirs)
echo "Creating combined static library libdifferential.a..."
# collect all object files from the subdirectories
OBJ_LIST_FILE="$TEMP_DIR/obj_list.txt"
rm -f "$OBJ_LIST_FILE"
find "$TEMP_DIR" -type f -name '*.o' > "$OBJ_LIST_FILE"

OBJ_COUNT=$(wc -l < "$OBJ_LIST_FILE" || true)
echo "Found $OBJ_COUNT object files to pack"

if [ "$OBJ_COUNT" -eq 0 ]; then
  echo "Error: no object files found to pack into libdifferential.a"
  exit 1
fi

# create the archive using the collected list (xargs to handle many files)
xargs ar rcs libdifferential.a < "$OBJ_LIST_FILE"

# Step 4: Verify the library contains our functions
echo "Verifying library contents..."
nm libdifferential.a | grep -E "(ffmpeg_h264_parse|openh264_parse)" || echo "Warning: Expected functions not found"

# Step 5: Final diagnostics and clean up
echo "libdifferential.a created. Object count: $OBJ_COUNT"
echo "Temporary directory: $TEMP_DIR (left in place for inspection)"
# Uncomment the following line to remove the temp directory automatically
# rm -rf "$TEMP_DIR"

mv libdifferential.a ../libs/libdifferential_$name_suffix.a

echo "Done! libdifferential_$name_suffix.a"
