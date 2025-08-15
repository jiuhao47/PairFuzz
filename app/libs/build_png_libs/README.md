### version
+ old: 5905484c
+ new: 71b9b5e1

### build

../configure --prefix=/home/fdl/diff-fuzz/examples/builds/libs/libpng_oldest CC="clang" CXX="clang++" CFLAGS="-g -fsanitize=address,fuzzer-no-link -fsanitize-coverage=edge,indirect-calls -fPIC" CXXFLAGS="-g -fsanitize=address,fuzzer-no-link -fsanitize-coverage=edge,indirect-calls -fPIC"

../configure --prefix=/home/fdl/diff-fuzz/examples/builds/libs/libpng_newest CC="clang" CXX="clang++" CFLAGS="-g -fsanitize=address,fuzzer-no-link -fsanitize-coverage=edge,indirect-calls -fPIC" CXXFLAGS="-g -fsanitize=address,fuzzer-no-link -fsanitize-coverage=edge,indirect-calls -fPIC"