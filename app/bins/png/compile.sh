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

echo "Compiling libspng program..."
$CC_WRAPPER $COMMON_COMPILE_FLAGS "../../libs/build_spng_libs/libspng_program.c" \
  -I "../../build/spng/$libspng_hash/usr/local/include" \
  -o "$TEMP_DIR/libspng_program.o"

# Step 2: link static libraries to object files to generate executables
LINK_LIBS="-lz -lm"
echo "Linking libpng executable..."
$CXX_WRAPPER $COMMON_LINK_FLAGS "$TEMP_DIR/libpng_program.o" \
  "../../build/png/$libpng_hash/usr/local/lib/libpng.a" \
  -o "$TEMP_DIR/libpng_executable" $LINK_LIBS

echo "Linking libspng executable..."
$CC_WRAPPER $COMMON_LINK_FLAGS "$TEMP_DIR/libspng_program.o" \
  "../../build/spng/$libspng_hash/usr/local/lib/libspng_static.a" \
  -o "$TEMP_DIR/libspng_executable" $LINK_LIBS

# Step 3: Move executables to bins/.bins
echo "Installing executables to bins/.bin/$name_suffix..."
mkdir -p "../../bins/.bin/$name_suffix"
install -m 0755 "$TEMP_DIR/libpng_executable" "../../bins/.bin/$name_suffix/libpng"
install -m 0755 "$TEMP_DIR/libspng_executable" "../../bins/.bin/$name_suffix/libspng"

# Step 5: Clean up
echo "Cleaning up temporary files..."
rm -rf "$TEMP_DIR"