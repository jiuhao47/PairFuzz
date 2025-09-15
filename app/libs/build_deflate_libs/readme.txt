### version
oldest: a11c44317
newest: efc217633

### Notes
编译生成libdeflate_oldest/newest.so时，
注意不能和libdeflate.so名称重合，否则容易报奇怪的错误

### patch相关逻辑
#### 对于new version: 将zlib_decompress.c中的
 if (libdeflate_adler32(1, out, actual_out_nbytes) !=
 改为
 if (0 && libdeflate_adler32(1, out, actual_out_nbytes) !=
 ===
#### 对于oldversion:
将zlib_decompress.c中的
if (adler32(out, actual_out_nbytes) != get_unaligned_be32(in_next))
改为
if (0 && adler32(out, actual_out_nbytes) != get_unaligned_be32(in_next))
即可

### build相关
+ 相关头文件: libdeflate.h
####  对于new version
+ 相关build命令: 
cmake .. -D CMAKE_C_COMPILER="clang" -D CMAKE_CXX_COMPILER="clang++" -DCMAKE_C_FLAGS="-O0 -fsanitize=address,fuzzer-no-link -fsanitize-coverage=edge,indirect-calls -fPIC " -DCMAKE_CXX_FLAGS=" -O0 -fsanitize=address,fuzzer-no-link -fsanitize-coverage=edge,indirect-calls -fPIC"

#### 对于old version: 
生成的Makfile中需要改一下相关参数，将
CC改为clang
将Flags加上-fsanitize=address,fuzzer-no-link -fsanitize-coverage=edge,indirect-calls

BUILD_SHARED_LIBRARY := yes
override CFLAGS += -std=gnu99 -fsanitize=address,fuzzer-no-link -fsanitize-coverage=edge,indirect-calls
生成object的逻辑加上-fPIC


