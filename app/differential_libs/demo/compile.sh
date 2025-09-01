# Create a temporary directory for extracting and combining libraries
TEMP_DIR="temp_differential"
mkdir -p "$TEMP_DIR"
name_suffix="demo"

set -e

# Step 1: Compile harness files to object files
echo "Compiling demo harness..."

"../../compiler/target/release/libafl_cc" -fPIC -fsanitize=address -c "../../libs/build_demo_libs/first.c" \
    -o "$TEMP_DIR/first_harness.o"

echo "Compiling second harness..."
"../../compiler/target/release/libafl_cc" -fPIC -fsanitize=address -c "../../libs/build_demo_libs/second.c" \
    -o "$TEMP_DIR/second_harness.o"

# Step 3: Create the combined static library
echo "Creating combined static library libdifferential.a..."
ar rcs libdifferential.a "$TEMP_DIR"/*.o

# Step 4: Verify the library contains our functions
echo "Verifying library contents..."
nm libdifferential.a | grep -E "(diffRayTestOneInput_1|diffRayTestOneInput_2)" || echo "Warning: Expected functions not found"

# Step 5: Clean up
echo "Cleaning up temporary files..."
rm -rf "$TEMP_DIR"

mv libdifferential.a ../libs/libdifferential_$name_suffix.a

echo "Done! libdifferential_$name_suffix.a"