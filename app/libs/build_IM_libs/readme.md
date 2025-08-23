## versions

newest: 0729cc470
oldest: 4c717b784

## build command

./configure CC="clang" CXX="clang++" CFLAGS="-g -fsanitize=address,fuzzer-no-link -fsanitize-coverage=edge,indirect-calls" CXXFLAGS="-g -fsanitize=address,fuzzer-no-link -fsanitize-coverage=edge,indirect-calls " --prefix="/home/fdl/diff-fuzz/examples/builds/libs/imagemagick_oldest"

### 配置不同版本的库

在 old 版本中，编译之前，将 Makefile 中的
MAGICK_LIBRARY_AGE = 0
改为
MAGICK_LIBRARY_AGE = 4
