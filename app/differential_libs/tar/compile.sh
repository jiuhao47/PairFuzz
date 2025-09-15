# Create a temporary directory for extracting and combining libraries
TEMP_DIR="temp_differential"
mkdir -p "$TEMP_DIR"
archive_hash=$1
libtar_hash=$2
name_suffix=$3

set -e

if [ -z "$name_suffix" ]; then
  echo "Error: Name suffix is required"
  exit 1
fi
if [ -z "$libtar_hash" ]; then
  echo "Error: libtar hash is required"
  exit 1
fi
if [ -z "$archive_hash" ]; then
  echo "Error: archive hash is required"
  exit 1
fi

# Step 1: Compile harness files to object files
echo "Compiling archive harness..."

"../../compiler/target/release/libafl_cxx" -fPIC -fsanitize=address -c "../../libs/build_archive_libs/archive_harness.cpp" \
  -I "../../build/archive/$archive_hash/usr/local/include/" \
  -o "$TEMP_DIR/archive_harness.o"

echo "Compiling libtar harness..."
echo "../../build/libtar/$libtar_hash/usr/local/include"
"../../compiler/target/release/libafl_cxx" -fPIC -fsanitize=address -c "../../libs/build_libtar_libs/libtar_harness.cpp" \
  -I "../../build/libtar/$libtar_hash/usr/local/include" \
  -o "$TEMP_DIR/libtar_harness.o"

# Step 2: Extract existing static libraries
echo "Extracting libarchive.a..."
cd "$TEMP_DIR"
ar x "../../../build/archive/$archive_hash/usr/local/lib/libarchive.a"

echo "Extracting libtar.a..."
ar x "../../../build/libtar/$libtar_hash/usr/local/lib/libtar.a"
cd ..

# Step 3: Create the combined static library
echo "Creating combined static library libdifferential.a..."
ar rcs libdifferential.a "$TEMP_DIR"/*.o

# Step 4: Verify the library contains our functions
echo "Verifying library contents..."
nm libdifferential.a | grep -E "(archive_parse|tar_parse)" || echo "Warning: Expected functions not found"

# Step 5: Clean up
echo "Cleaning up temporary files..."
rm -rf "$TEMP_DIR"

mv libdifferential.a ../libs/libdifferential_$name_suffix.a

echo "Done! libdifferential_$name_suffix.a"
