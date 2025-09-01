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

# Step 1: Compile mjson.c to object file
echo "Compiling mjson.c..."
"../../compiler/target/release/libafl_cc" -fPIC -fsanitize=address -c "mjson.c" \
    -o "$TEMP_DIR/mjson.o"

# Step 2: Create the static library
echo "Creating static library libmjson.a..."
ar rcs libmjson.a "$TEMP_DIR"/*.o

# Step 3: Verify the library contains main functions
echo "Verifying library contents..."
nm libmjson.a | grep -E "(mjson_parse|mjson_print)" || echo "Warning: Expected mjson functions not found"

# Step 4: Clean up
echo "Cleaning up temporary files..."
rm -rf "$TEMP_DIR"

# Move and rename the library
mv libmjson.a ../libs/libmjson_$name_suffix.a

echo "Done! libmjson_$name_suffix.a created in ../libs directory"
    