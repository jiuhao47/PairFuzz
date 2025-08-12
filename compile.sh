# fuzzer feat="": (lib feat) (cxx feat)
#     {{LIBAFL_CXX}} {{PROJECT_DIR}}/harness.cc \
#         "{{LIBPNG_BUILD}}/.libs/libpng16.a" \
#         "{{ZLIB_BUILD}}/libz.a" \
#         -I"{{LIBPNG_INCLUDE}}" \
#         -I"{{LIBPNG_BUILD}}" \
#         -I"{{ZLIB_INCLUDE}}" \
#         -L"{{ZLIB_LIB}}" \
#         -o {{FUZZER_NAME}} \
#         -lm -lz

# Create a temporary directory for extracting and combining libraries
TEMP_DIR="temp_differential"
mkdir -p "$TEMP_DIR"

# Step 1: Compile harness files to object files
echo "编译 libpng harness..."

"./app/compiler/target/release/libafl_cxx" -fPIC -fsanitize=address -c "./app/libs/build_png_libs/libpng_harness.cc" \
    -I "./app/build/png/134ab615/usr/local/include/" \
    -o "$TEMP_DIR/libpng_harness.o"

echo "编译 libspng harness..."
"./app/compiler/target/release/libafl_cc" -fPIC -fsanitize=address -c "./app/libs/build_spng_libs/libspng_harness.c" \
    -I "./app/build/spng/adc94393/usr/local/include" \
    -o "$TEMP_DIR/libspng_harness.o"

# Step 2: Extract existing static libraries
echo "解压 libpng16.a..."
cd "$TEMP_DIR"
ar x "../app/build/png/134ab615/usr/local/lib/libpng16.a"

echo "解压 libspng_static.a..."
ar x "../app/build/spng/adc94393/usr/local/lib/libspng_static.a"
cd ..

# Step 3: Create the combined static library
echo "创建合并的静态库 libdifferential.a..."
ar rcs libdifferential.a "$TEMP_DIR"/*.o

# Step 4: Verify the library contains our functions
echo "验证库内容..."
nm libdifferential.a | grep -E "(libpng_parse|libspng_parse)" || echo "警告: 未找到预期的函数"

# Step 5: Clean up
echo "清理临时文件..."
rm -rf "$TEMP_DIR"

echo "完成! libdifferential.a 已创建"