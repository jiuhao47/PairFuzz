# Create a temporary directory for extracting and combining libraries
TEMP_DIR="temp_differential"
mkdir -p "$TEMP_DIR"
libxml2_hash=$1
expat_hash=$2
name_suffix=$3

set -e

if [ -z "$name_suffix" ]; then
  echo "Error: Name suffix is required"
  exit 1
fi
if [ -z "$libxml2_hash" ]; then
  echo "Error: libxml2 hash is required"
  exit 1
fi
if [ -z "$expat_hash" ]; then
  echo "Error: expat hash is required"
  exit 1
fi

# Step 1: Compile harness files to object files
echo "Compiling libxml2 harness..."

"../../compiler/target/release/libafl_cxx" -fPIC -fsanitize=address -c "../../libs/build_xml2_libs/libxml2_harness.cc" \
    -I "../../build/xml2/$libxml2_hash/usr/local/include/" \
    -o "$TEMP_DIR/libxml2_harness.o"

echo "Compiling libexpat harness..."
"../../compiler/target/release/libafl_cc" -fPIC -fsanitize=address -c "../../libs/build_expat_libs/libexpat_harness.cc" \
    -I "../../build/expat/$expat_hash/usr/local/include" \
    -o "$TEMP_DIR/libexpat_harness.o"

# Step 2: Extract existing static libraries
echo "Extracting libxml2.a..."
cd "$TEMP_DIR"
ar x "../../../build/xml2/$libxml2_hash/usr/local/lib/libxml2.a"

echo "Extracting libexpat.a..."
ar x "../../../build/expat/$expat_hash/usr/local/lib/libexpat.a"
cd ..

# Step 3: Create the combined static library
echo "Creating combined static library libdifferential.a..."
ar rcs libdifferential.a "$TEMP_DIR"/*.o

# Step 4: Verify the library contains our functions
echo "Verifying library contents..."
nm libdifferential.a | grep -E "(xml2_xmlparse|expat_xmlparse)" || echo "Warning: Expected functions not found"

# Step 5: Clean up
echo "Cleaning up temporary files..."
rm -rf "$TEMP_DIR"

mv libdifferential.a ../libs/libdifferential_$name_suffix.a

echo "Done! libdifferential_$name_suffix.a"