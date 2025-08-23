## versions
oldest: 14303
newest: 17300

## command
make distclean
hg revert --all
hg purge

## build
./configure --disable-static --enable-shared  CC="clang" CXX="clang++" CFLAGS="-g -fPIC -fsanitize=address,fuzzer-no-link -fsanitize-coverage=edge,indirect-calls " CXXFLAGS="-g -fPIC -fsanitize=address,fuzzer-no-link -fsanitize-coverage=edge,indirect-calls " --prefix="/home/fdl/diff-fuzz/examples/builds/libs/graphicsmagick_oldest"

## 对于fuzzing OLD GM v.s NEW GM
需要在make 之前将Makefile中的
MAGICK_LIBRARY_AGE = 13
改成
MAGICK_LIBRARY_AGE = 14