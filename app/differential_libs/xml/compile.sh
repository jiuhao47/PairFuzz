# Create a temporary directory for extracting and combining libraries
TEMP_DIR="temp_differential"
mkdir -p "$TEMP_DIR"

# Step 1: Compile harness files to object files
echo "Compiling libxml2 harness..."

"../../compiler/target/release/libafl_cxx" -fPIC -fsanitize=address -c "../../libs/build_xml2_libs/libxml2_harness.cc" \
    -I "../../build/xml2/73514f2d/usr/local/include/" \
    -o "$TEMP_DIR/libxml2_harness.o"

echo "Compiling libexpat harness..."
"../../compiler/target/release/libafl_cc" -fPIC -fsanitize=address -c "../../libs/build_expat_libs/libexpat_harness.cc" \
    -I "../../build/expat/ac2b235/usr/local/include" \
    -o "$TEMP_DIR/libexpat_harness.o"

# Step 2: Extract existing static libraries
echo "Extracting libxml2.a..."
cd "$TEMP_DIR"
ar x "../../../build/xml2/73514f2d/usr/local/lib/libxml2.a"

echo "Extracting libspng_static.a..."
ar x "../../../build/expat/ac2b235/usr/local/lib/libexpat.a"
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

mv libdifferential.a libdifferential_xml.a

echo "Done! libdifferential_xml.a"