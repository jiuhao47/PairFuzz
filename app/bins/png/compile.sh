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

# Step 1: Compile program files to object files
echo "Compiling libpng program..."

"../../compiler/target/release/libafl_forkserver_cxx" -fPIC -fsanitize=address -c "../../libs/build_png_libs/libpng_program.cc" \
    -I "../../build/png/$libpng_hash/usr/local/include/" \
    -o "$TEMP_DIR/libpng_program.o"

echo "Compiling libspng program..."
"../../compiler/target/release/libafl_forkserver_cc" -fPIC -fsanitize=address -c "../../libs/build_spng_libs/libspng_program.c" \
    -I "../../build/spng/$libspng_hash/usr/local/include" \
    -o "$TEMP_DIR/libspng_program.o"

# Step 2: link static libraries to object files to generate executable
echo "Linking libpng executable..."
"../../compiler/target/release/libafl_forkserver_cxx" -fsanitize=address "$TEMP_DIR/libpng_program.o" \
    "../../build/png/$libpng_hash/usr/local/lib/libpng.a" \
    -o "$TEMP_DIR/libpng_executable" -lz -lm

echo "Linking libspng executable..."
"../../compiler/target/release/libafl_forkserver_cc" -fsanitize=address "$TEMP_DIR/libspng_program.o" \
    "../../build/spng/$libspng_hash/usr/local/lib/libspng_static.a" \
    -o "$TEMP_DIR/libspng_executable" -lz -lm

# Step 3: Move executables to bins/.bins
echo "Moving executables to bins/.bin..."
rm -rf "../../bins/.bin/$name_suffix"
mkdir -p "../../bins/.bin/$name_suffix"
mv "$TEMP_DIR/libpng_executable" "../../bins/.bin/$name_suffix/libpng"
mv "$TEMP_DIR/libspng_executable" "../../bins/.bin/$name_suffix/libspng"

# Step 5: Clean up
echo "Cleaning up temporary files..."
rm -rf "$TEMP_DIR"