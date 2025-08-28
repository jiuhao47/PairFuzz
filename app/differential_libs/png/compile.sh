# Create a temporary directory for extracting and combining libraries
TEMP_DIR="temp_differential"
mkdir -p "$TEMP_DIR"
libpng_hash=$1
libspng_hash=$2
name_suffix=$3

set -e

if [ -z "$name_suffix" ]; then
  echo "Error: Name suffix is required"
  exit 1
fi
if [ -z "$libspng_hash" ]; then
  echo "Error: libspng hash is required"
  exit 1
fi
if [ -z "$libpng_hash" ]; then
  echo "Error: libpng hash is required"
  exit 1
fi

# Step 1: Compile harness files to object files
echo "Compiling libpng harness..."

"../../compiler/target/release/libafl_cxx" -fPIC -fsanitize=address -c "../../libs/build_png_libs/libpng_harness.cc" \
    -I "../../build/png/$libpng_hash/usr/local/include/" \
    -o "$TEMP_DIR/libpng_harness.o"

echo "Compiling libspng harness..."
echo "../../build/spng/$libspng_hash/usr/local/include"
"../../compiler/target/release/libafl_cc" -fPIC -fsanitize=address -c "../../libs/build_spng_libs/libspng_harness.c" \
    -I "../../build/spng/$libspng_hash/usr/local/include" \
    -o "$TEMP_DIR/libspng_harness.o"

# Step 2: Extract existing static libraries
echo "Extracting libpng.a..."
cd "$TEMP_DIR"
ar x "../../../build/png/$libpng_hash/usr/local/lib/libpng.a"

echo "Extracting libspng_static.a..."
ar x "../../../build/spng/$libspng_hash/usr/local/lib/libspng_static.a"
cd ..

# Step 3: Create the combined static library
echo "Creating combined static library libdifferential.a..."
ar rcs libdifferential.a "$TEMP_DIR"/*.o

# Step 4: Verify the library contains our functions
echo "Verifying library contents..."
nm libdifferential.a | grep -E "(libpng_parse|libspng_parse)" || echo "Warning: Expected functions not found"

# Step 5: Clean up
echo "Cleaning up temporary files..."
rm -rf "$TEMP_DIR"

mv libdifferential.a ../libs/libdifferential_$name_suffix.a

echo "Done! libdifferential_$name_suffix.a"