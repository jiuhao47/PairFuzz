# Create a temporary directory for extracting and combining libraries
TEMP_DIR="temp_differential"
mkdir -p "$TEMP_DIR"
im_hash=$1
gm_hash=$2
name_suffix=$3

set -e

if [ -z "$name_suffix" ]; then
  echo "Error: Name suffix is required"
  exit 1
fi
if [ -z "$im_hash" ]; then
  echo "Error: IM hash is required"
  exit 1
fi
if [ -z "$gm_hash" ]; then
  echo "Error: GM hash is required"
  exit 1
fi

# Step 1: Compile harness files to object files
echo "Compiling IM harness..."

"../../compiler/target/release/libafl_cxx" -DMAGICKCORE_HDRI_ENABLE=1 -DMAGICKCORE_QUANTUM_DEPTH=16 -fPIC -fsanitize=address -c "../../libs/build_IM_libs/IM_harness.cpp" \
    -I "../../build/IM/$im_hash/usr/local/include/ImageMagick-7" \
    -o "$TEMP_DIR/IM_harness.o"

echo "Compiling GM harness..."
"../../compiler/target/release/libafl_cc" -fPIC -fsanitize=address -c "../../libs/build_GM_libs/GM_harness.cpp" \
    -I "../../build/GM/$gm_hash/usr/local/include/GraphicsMagick" \
    -o "$TEMP_DIR/GM_harness.o"

# Step 2: Extract existing static libraries
cd "$TEMP_DIR"
echo "Extracting libMagick++-7.Q16HDRI.a..."
ar x "../../../build/IM/$im_hash/usr/local/lib/libMagick++-7.Q16HDRI.a"

echo "Extracting libMagickCore-7.Q16HDRI.a"
ar x "../../../build/IM/$im_hash/usr/local/lib/libMagickCore-7.Q16HDRI.a"

echo "Extracting libMagickWand-7.Q16HDRI.a"
ar x "../../../build/IM/$im_hash/usr/local/lib/libMagickWand-7.Q16HDRI.a"


echo "Extracting libGraphicsMagick.a..."
ar x "../../../build/GM/$gm_hash/usr/local/lib/libGraphicsMagick.a"
echo "Extracting libGraphicsMagick++.a..."
ar x "../../../build/GM/$gm_hash/usr/local/lib/libGraphicsMagick++.a"
cd ..

# Step 3: Create the combined static library
echo "Creating combined static library libdifferential.a..."
ar rcs libdifferential.a "$TEMP_DIR"/*.o

# Step 4: Verify the library contains our functions
echo "Verifying library contents..."
nm libdifferential.a | grep -E "(GM_parse|imagemagick_parse)" || echo "Warning: Expected functions not found"

# Step 5: Clean up
echo "Cleaning up temporary files..."
rm -rf "$TEMP_DIR"

mv libdifferential.a libdifferential_$name_suffix.a

echo "Done! libdifferential_$name_suffix.a"