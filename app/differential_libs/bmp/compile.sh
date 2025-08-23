# Create a temporary directory for extracting and combining libraries
TEMP_DIR="temp_differential"
mkdir -p "$TEMP_DIR"

# Step 1: Compile harness files to object files
echo "Compiling libpng harness..."

"../../compiler/target/release/libafl_cxx" -fPIC -fsanitize=address -c "../../libs/build_IM_libs/IM_harness.cpp" \
    -I "../../build/IM/0729cc470/usr/local/include/" \
    -o "$TEMP_DIR/IM_harness.o"

echo "Compiling libspng harness..."
"../../compiler/target/release/libafl_cc" -fPIC -fsanitize=address -c "../../libs/build_spng_libs/libspng_harness.c" \
    -I "../../build/spng/adc94393/usr/local/include" \
    -o "$TEMP_DIR/libspng_harness.o"

# Step 2: Extract existing static libraries
echo "Extracting libpng16.a..."
cd "$TEMP_DIR"
ar x "../../../build/png/134ab615/usr/local/lib/libpng16.a"

echo "Extracting libspng_static.a..."
ar x "../../../build/spng/adc94393/usr/local/lib/libspng_static.a"
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

mv libdifferential.a libdifferential_png.a

echo "Done! libdifferential_png.a"