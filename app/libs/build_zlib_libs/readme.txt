## version
old: 7751bd4
old: 09155ea

## patch相关逻辑
old: strm->msg = (char *)"incorrect data check";

## 生成lib时，需要改一下符号表

在old version中，将 build.make, link.txt中的libz.so.1改为libz.so.2 

### build 相关命令
cmake .. -D CMAKE_C_COMPILER="clang" -D CMAKE_CXX_COMPILER="clang++" -DCMAKE_C_FLAGS="-O0 -fsanitize=address,fuzzer-no-link -fsanitize-coverage=edge,indirect-calls -fPIC " -DCMAKE_CXX_FLAGS=" -O0 -fsanitize=address,fuzzer-no-link -fsanitize-coverage=edge,indirect-calls -fPIC"