# Create a temporary directory for extracting and combining libraries
TEMP_DIR="temp_differential"
mkdir -p "$TEMP_DIR"
mjson_hash=$1
cjson_hash=$2
name_suffix=$3

set -e

if [ -z "$name_suffix" ]; then
  echo "Error: Name suffix is required"
  exit 1
fi
if [ -z "$mjson_hash" ]; then
  echo "Error: mjson hash is required"
  exit 1
fi
if [ -z "$cjson_hash" ]; then
  echo "Error: cjson hash is required"
  exit 1
fi

# Step 1: Compile harness files to object files
echo "Compiling mjson harness..."

"../../compiler/target/release/libafl_cc" -fPIC -fsanitize=address -c "../../libs/build_mjson_libs/mjson_harness.c" \
    -I "../../build/mjson/$mjson_hash/usr/local/include/" \
    -o "$TEMP_DIR/mjson_harness.o"

echo "Compiling cjson harness..."
"../../compiler/target/release/libafl_cc" -fPIC -fsanitize=address -c "../../libs/build_cjson_libs/cjson_harness.c" \
    -I "../../build/cjson/$cjson_hash/usr/local/include/" -I "../../build/cjson/$cjson_hash/usr/local/include/cjson" \
    -o "$TEMP_DIR/cjson_harness.o"

# Step 2: Extract existing static libraries
echo "Extracting libmjson.a..."
cd "$TEMP_DIR"
MJSON_LIB_PATH="../../../build/mjson/$mjson_hash/usr/local/lib/libmjson.a"
if [ -f "$MJSON_LIB_PATH" ]; then
  ar x "$MJSON_LIB_PATH"
else
  echo "Warning: $MJSON_LIB_PATH not found — skipping libmjson.a extraction"
fi

echo "Extracting libcjson.a..."
CJSON_LIB_PATH="../../../build/cjson/$cjson_hash/usr/local/lib/libcjson.a"
if [ -f "$CJSON_LIB_PATH" ]; then
  ar x "$CJSON_LIB_PATH"
else
  echo "Warning: $CJSON_LIB_PATH not found — skipping libcjson.a extraction"
fi
cd ..

# Step 3: Create the combined static library
echo "Creating combined static library libdifferential.a..."
ar rcs libdifferential.a "$TEMP_DIR"/*.o

# Step 4: Verify the library contains our functions
echo "Verifying library contents..."
nm libdifferential.a | grep -E "(cjson_parse|mjson_parse)" || echo "Warning: Expected functions not found"

# Step 5: Clean up
echo "Cleaning up temporary files..."
rm -rf "$TEMP_DIR"

mv libdifferential.a ../libs/libdifferential_$name_suffix.a

echo "Done! libdifferential_$name_suffix.a"