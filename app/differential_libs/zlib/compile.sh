# Create a temporary directory for extracting and combining libraries
TEMP_DIR="temp_differential"
mkdir -p "$TEMP_DIR"
zlib_hash=$1
libdeflate_hash=$2
name_suffix=$3

set -e

if [ -z "$name_suffix" ]; then
  echo "Error: Name suffix is required"
  exit 1
fi
if [ -z "$libdeflate_hash" ]; then
  echo "Error: libdeflate hash is required"
  exit 1
fi
if [ -z "$zlib_hash" ]; then
  echo "Error: zlib hash is required"
  exit 1
fi

# Step 1: Compile harness files to object files
echo "Compiling zlib harness..."

"../../compiler/target/release/libafl_cxx" -fPIC -fsanitize=address -c "../../libs/build_zlib_libs/zlib_harness.c" \
  -I "../../build/zlib/$zlib_hash/usr/local/include/" \
  -o "$TEMP_DIR/zlib_harness.o"

echo "Compiling libdeflate harness..."
echo "../../build/deflate/$libdeflate_hash/usr/local/include"
"../../compiler/target/release/libafl_cc" -fPIC -fsanitize=address -c "../../libs/build_deflate_libs/libdeflate_harness.cpp" \
  -I "../../build/deflate/$libdeflate_hash/usr/local/include" \
  -o "$TEMP_DIR/libdeflate_harness.o"

# Step 2: Extract existing static libraries
echo "Extracting libz.a..."
cd "$TEMP_DIR"
ar x "../../../build/zlib/$zlib_hash/usr/local/lib/libz.a"

echo "Extracting libdeflate.a..."
ar x "../../../build/deflate/$libdeflate_hash/usr/local/lib/libdeflate.a"
cd ..

# Step 3: Create the combined static library
echo "Creating combined static library libdifferential.a..."
ar rcs libdifferential.a "$TEMP_DIR"/*.o

# Step 4: Verify the library contains our functions
echo "Verifying library contents..."
nm libdifferential.a | grep -E "(zlib_parse|libdeflate_parse)" || echo "Warning: Expected functions not found"

# Step 5: Clean up
echo "Cleaning up temporary files..."
rm -rf "$TEMP_DIR"

mv libdifferential.a ../libs/libdifferential_$name_suffix.a

echo "Done! libdifferential_$name_suffix.a"
