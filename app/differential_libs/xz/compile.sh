# Create a temporary directory for extracting and combining libraries
TEMP_DIR="temp_differential"
mkdir -p "$TEMP_DIR"
xz_hash=$1
name_suffix=$2

set -e

if [ -z "$name_suffix" ]; then
  echo "Error: Name suffix is required"
  exit 1
fi
if [ -z "$xz_hash" ]; then
  echo "Error: xz hash is required"
  exit 1
fi

# Step 1: Compile harness files to object files
echo "Compiling xz harness..."

"../../compiler/target/release/libafl_cxx" -fPIC -fsanitize=address -c "../../libs/build_xz_libs/xz_harness.cpp" \
  -I "../../build/xz/$xz_hash/usr/local/include/" \
  -o "$TEMP_DIR/xz_harness.o"

echo "Compiling xz7z harness..."
echo "../../build/xz7z/install"
"../../compiler/target/release/libafl_cc" -fPIC -fsanitize=address -c "../../libs/build_xz7z_libs/7z_harness.c" \
  -I "../../build/xz7z/include" \
  -o "$TEMP_DIR/xz7z_harness.o"

# Step 2: Extract existing static libraries
echo "Extracting liblzma.a..."
cd "$TEMP_DIR"
ar x "../../../build/xz/$xz_hash/usr/local/lib/liblzma.a"

echo "Extracting libxz7z.a..."
ar x "../../../build/xz7z/install/libxz7z.a"
cd ..

# Step 3: Create the combined static library
echo "Creating combined static library libdifferential.a..."
ar rcs libdifferential.a "$TEMP_DIR"/*.o

# Step 4: Verify the library contains our functions
echo "Verifying library contents..."
nm libdifferential.a | grep -E "(xz_parse|cli_scanxz)" || echo "Warning: Expected functions not found"

# Step 5: Clean up
echo "Cleaning up temporary files..."
rm -rf "$TEMP_DIR"

mv libdifferential.a ../libs/libdifferential_$name_suffix.a

echo "Done! libdifferential_$name_suffix.a"
