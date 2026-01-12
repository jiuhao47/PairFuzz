set -euo pipefail

# Usage: compile.sh <libpng_hash> <libspng_hash> <name_suffix>
if [ "$#" -ne 3 ]; then
  echo "Usage: $0 <libpng_hash> <libspng_hash> <name_suffix>"
  exit 1
fi

libpng_hash=$1
libspng_hash=$2
name_suffix=$3

# Create a temporary build directory and ensure cleanup on exit
TEMP_DIR=$(mktemp -d "temp_differential.XXXX")
trap 'rm -rf "${TEMP_DIR}"' EXIT

# Compiler wrappers (fall back to system compilers if wrappers missing)
CC_WRAPPER="../../compiler/target/release/libafl_forkserver_cc"
CXX_WRAPPER="../../compiler/target/release/libafl_forkserver_cxx"
if [ ! -x "$CC_WRAPPER" ]; then
  echo "Error: required wrapper '$CC_WRAPPER' not found or not executable"
  exit 1
fi
if [ ! -x "$CXX_WRAPPER" ]; then
  echo "Error: required wrapper '$CXX_WRAPPER' not found or not executable"
  exit 1
fi

# Common flags and include dirs
COMMON_COMPILE_FLAGS="-fPIC -fsanitize=address -c"
COMMON_LINK_FLAGS="-fsanitize=address"


# Step 1: Compile program files to object files
echo "Compiling libpng program..."

$CXX_WRAPPER $COMMON_COMPILE_FLAGS "../../libs/build_png_libs/libpng_program.cc" \
  -I "../../build/png/$libpng_hash/usr/local/include/" \
  -o "$TEMP_DIR/libpng_program.o"

# Also compile the core implementation separately (no main) for building the shared lib
echo "Compiling libpng core..."
$CXX_WRAPPER $COMMON_COMPILE_FLAGS "../../libs/build_png_libs/libpng_core.cc" \
  -I "../../build/png/$libpng_hash/usr/local/include/" \
  -o "$TEMP_DIR/libpng_core.o"

echo "Compiling libspng program..."
$CC_WRAPPER $COMMON_COMPILE_FLAGS "../../libs/build_spng_libs/libspng_program.c" \
  -I "../../build/spng/$libspng_hash/usr/local/include" \
  -o "$TEMP_DIR/libspng_program.o"

echo "Compiling libspng core..."
$CC_WRAPPER $COMMON_COMPILE_FLAGS "../../libs/build_spng_libs/libspng_core.c" \
  -I "../../build/spng/$libspng_hash/usr/local/include" \
  -o "$TEMP_DIR/libspng_core.o"

# Step 2: link static libraries to object files to generate executables
LINK_LIBS="-lm"
echo "Linking libpng executable..."
$CXX_WRAPPER $COMMON_LINK_FLAGS -Wl,-export-dynamic "$TEMP_DIR/libpng_program.o" \
  "../../build/png/$libpng_hash/usr/local/lib/libpng.a" \
  "../../build/zlib/dep/usr/local/lib/libz.a" \
  -o "$TEMP_DIR/libpng_executable" $LINK_LIBS

echo "Linking libspng executable..."
$CC_WRAPPER $COMMON_LINK_FLAGS -Wl,-export-dynamic "$TEMP_DIR/libspng_program.o" \
  "../../build/spng/$libspng_hash/usr/local/lib/libspng_static.a" \
  "../../build/zlib/dep/usr/local/lib/libz.a" \
  -o "$TEMP_DIR/libspng_executable" $LINK_LIBS

# Step 3: Move executables to bins/.bins
echo "Installing executables to bins/.bin/$name_suffix..."
mkdir -p "../../bins/.bin/$name_suffix"
install -m 0755 "$TEMP_DIR/libpng_executable" "../../bins/.bin/$name_suffix/libpng"
install -m 0755 "$TEMP_DIR/libspng_executable" "../../bins/.bin/$name_suffix/libspng"

# Step 4: build shared libraries (position-independent, bundle static deps)
echo "Building shared library for libpng..."
## Use the core object (without main) when building the shared library
$CXX_WRAPPER -shared -o "$TEMP_DIR/libpng_shared.so" "$TEMP_DIR/libpng_core.o" \
  "../../build/png/$libpng_hash/usr/local/lib/libpng.a" \
  "../../build/zlib/dep/usr/local/lib/libz.a" \
  -Wl,-soname,libpng_local.so -Wl,-rpath,'$ORIGIN' $COMMON_LINK_FLAGS

echo "Building shared library for libspng..."
$CC_WRAPPER -shared -o "$TEMP_DIR/libspng_shared.so" "$TEMP_DIR/libspng_core.o" \
  "../../build/spng/$libspng_hash/usr/local/lib/libspng_static.a" \
  "../../build/zlib/dep/usr/local/lib/libz.a" \
  -Wl,-soname,libspng_local.so -Wl,-rpath,'$ORIGIN' $COMMON_LINK_FLAGS

echo "Installing shared libraries to bins/.bin/$name_suffix/"
install -m 0755 "$TEMP_DIR/libpng_shared.so" "../../bins/.bin/$name_suffix/libpng_local.so"
install -m 0755 "$TEMP_DIR/libspng_shared.so" "../../bins/.bin/$name_suffix/libspng_local.so"

# Step 5: Clean up
echo "Cleaning up temporary files..."
rm -rf "$TEMP_DIR"