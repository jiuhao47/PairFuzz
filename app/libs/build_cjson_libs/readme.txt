
## version
newest: 87d8f096
oldest: 174c629

jiuhao_newest: 12c4bf19

## command

cmake .. -D CMAKE_C_COMPILER="clang" -D CMAKE_CXX_COMPILER="clang++" -DCMAKE_C_FLAGS="-O0 -fsanitize=address,fuzzer-no-link -fsanitize-coverage=edge,indirect-calls -fPIC " -DCMAKE_CXX_FLAGS=" -O0 -fsanitize=address,fuzzer-no-link -fsanitize-coverage=edge,indirect-calls -fPIC "

