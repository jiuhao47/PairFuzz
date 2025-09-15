CC := /home/xjy/fuzz/else/llvm-project/build/bin/clang
CXX := /home/xjy/fuzz/else/llvm-project/build/bin/clang++
#---------------------------------------------------------
OLD_BASE_PATH := /home/fdl/diff-fuzz/examples/builds/libs/zlib_oldest
NEW_BASE_PATH := /home/fdl/diff-fuzz/examples/builds/libs/zlib_newest

OLD_INC_PATH := -I$(OLD_BASE_PATH)/include
OLD_LIB_PATH := -L$(OLD_BASE_PATH)/lib
OLD_DYN_LIBS := -Wl,-rpath,$(OLD_BASE_PATH)/lib

NEW_INC_PATH := -I$(NEW_BASE_PATH)/include
NEW_LIB_PATH := -L$(NEW_BASE_PATH)/lib
NEW_DYN_LIBS := -Wl,-rpath,$(NEW_BASE_PATH)/lib

FLAGS := -fsanitize=address,fuzzer-no-link -fsanitize-coverage=edge,indirect-calls  
LD_FLAGS := -rdynamic -shared 
LIBS := -lz

all: genlib test

genlib: zlib_parse.c
	$(CC) $(NEW_INC_PATH) $(FLAGS) -fpic -c zlib_parse.c -o zlib_parse.o
	$(CC) $(FLAGS) $(LD_FLAGS) zlib_parse.o -o libzlib_newest.so $(NEW_DYN_LIBS)  $(NEW_LIB_PATH) $(LIBS)

	$(CC) $(OLD_INC_PATH) $(FLAGS) -fpic -c zlib_parse.c -o zlib_parse_old.o
	$(CC) $(FLAGS) $(LD_FLAGS) zlib_parse_old.o -o libzlib_oldest.so $(OLD_DYN_LIBS)  $(OLD_LIB_PATH) $(LIBS)

test: test.cpp
	$(CXX) -fsanitize=address,fuzzer-no-link -o test test.cpp

clean:
	rm -f test
	rm -f *.o
	rm -f *.so


# ###################################################################


THREAD_BASE_PATH := /home/xjy/fuzz/else/fuzz_project/build
THREAD_INC_PATH = -I$(THREAD_BASE_PATH)/zlib_$(COMMIT_HASH)/install/usr/local/include 
THREAD_LIB_PATH = -L$(THREAD_BASE_PATH)/zlib_$(COMMIT_HASH)/install/usr/local/lib
THREAD_DYN_LIBS = -Wl,-rpath,$(THREAD_BASE_PATH)/zlib_$(COMMIT_HASH)/install/usr/local/lib
# 考虑到这里用了两个不同的parse.cpp文件,发现内容一致，所以改成一个了，相应的diff也做了修改。
genlib_thread: zlib_parse.c
	$(CC) $(THREAD_INC_PATH) $(FLAGS) -fpic -c zlib_parse.c -o zlib_$(COMMIT_HASH).o
	$(CC) $(FLAGS) $(LD_FLAGS) zlib_$(COMMIT_HASH).o -o libzlib_$(COMMIT_HASH).so $(THREAD_LIB_PATH)  $(THREAD_DYN_LIBS) $(LIBS)
DYN_LIBS_DOCKER = -Wl,-Bdynamic,-rpath=/app/zlib/install/usr/local/lib/

genlib_docker: zlib_parse.c
	$(CC) $(THREAD_INC_PATH) $(FLAGS) -fpic -c zlib_parse.c -o zlib_$(COMMIT_HASH)_docker.o
	$(CC) $(FLAGS) $(LD_FLAGS) zlib_$(COMMIT_HASH)_docker.o -o libzlib_$(COMMIT_HASH)_docker.so $(THREAD_LIB_PATH)  $(DYN_LIBS_DOCKER) $(LIBS)

test_thread: test_thread.cpp
	$(CXX) -fsanitize=address,fuzzer-no-link -o test_thread test_thread.cpp
