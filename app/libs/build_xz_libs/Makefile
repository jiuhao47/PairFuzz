CC := /home/xjy/fuzz/else/llvm-project/build/bin/clang
CXX := /home/xjy/fuzz/else/llvm-project/build/bin/clang++

OLD_BASE_PATH := /home/fdl/diff-fuzz/examples/builds/libs/xz_oldest
NEW_BASE_PATH := /home/fdl/diff-fuzz/examples/builds/libs/xz_newest

OLD_INC_PATH := -I$(OLD_BASE_PATH)/include
OLD_LIB_PATH := -L$(OLD_BASE_PATH)/lib
OLD_DYN_LIBS := -Wl,-rpath,$(OLD_BASE_PATH)/lib

NEW_INC_PATH := -I$(NEW_BASE_PATH)/include
NEW_LIB_PATH := -L$(NEW_BASE_PATH)/lib
NEW_DYN_LIBS := -Wl,-rpath,$(NEW_BASE_PATH)/lib

FLAGS := -fsanitize=address,fuzzer-no-link -fsanitize-coverage=edge,indirect-calls  
TESTSAN_FLAGS := -fsanitize=testsan -g -O0
LD_FLAGS := -rdynamic -shared 
LIBS := -llzma

all: genlib test

genlib: xz_parse.cpp
	$(CXX) $(NEW_INC_PATH) $(FLAGS) -fpic -c xz_parse.cpp -o xz_parse.o
	$(CXX) $(FLAGS) $(LD_FLAGS) xz_parse.o -o libxz_newest.so $(NEW_DYN_LIBS)  $(NEW_LIB_PATH) $(LIBS)

	$(CXX) $(OLD_INC_PATH) $(FLAGS) -fpic -c xz_parse.cpp -o xz_parse.o
	$(CXX) $(FLAGS) $(LD_FLAGS) xz_parse.o -o libxz_oldest.so $(OLD_DYN_LIBS)  $(OLD_LIB_PATH) $(LIBS)

test: test.cpp
	$(CXX) -fsanitize=address,fuzzer-no-link -o test test.cpp


##########################################################

BUILD_BASE_PATH := /home/xjy/fuzz/else/fuzz_project/build
INC_PATH_THREAD := -I$(BUILD_BASE_PATH)/xz_$(COMMIT_HASH)/install/usr/local/include
LIB_PATH_THREAD := -L$(BUILD_BASE_PATH)/xz_$(COMMIT_HASH)/install/usr/local/lib
DYN_LIBS_THREAD := -Wl,-rpath,$(BUILD_BASE_PATH)/xz_$(COMMIT_HASH)/install/usr/local/lib


genlib_thread: xz_parse.cpp
	$(CXX) $(INC_PATH_THREAD) $(FLAGS) -fpic -c xz_parse.cpp -o xz_parse.o
	$(CXX) $(FLAGS) $(LD_FLAGS) xz_parse.o -o libxz_$(COMMIT_HASH).so $(LIB_PATH_THREAD)  $(DYN_LIBS_THREAD) $(LIBS)

test_thread: test_thread.cpp
	$(CXX) -fsanitize=address,fuzzer-no-link -o test_thread test_thread.cpp

DYN_LIBS_DOCKER = -Wl,-Bdynamic,-rpath=/app/xz/install/usr/local/lib/

genlib_docker: xz_parse.cpp
	$(CXX) $(THREAD_INC_PATH) $(FLAGS) -fpic -c xz_parse.cpp -o xz_$(COMMIT_HASH)_docker.o
	$(CXX) $(FLAGS) $(LD_FLAGS) xz_$(COMMIT_HASH)_docker.o -o libxz_$(COMMIT_HASH)_docker.so $(THREAD_LIB_PATH)  $(DYN_LIBS_DOCKER) $(LIBS)

##########################################################
INC_PATH_COUNT := -I$(BUILD_BASE_PATH)/xz_count$(COMMIT_HASH)/install/usr/local/include
LIB_PATH_COUNT := -L$(BUILD_BASE_PATH)/xz_count_$(COMMIT_HASH)/install/usr/local/lib
DYN_LIBS_COUNT := -Wl,-rpath,$(BUILD_BASE_PATH)/xz_count_$(COMMIT_HASH)/install/usr/local/lib

genlib_count: xz_parse.cpp
	$(CXX) $(INC_PATH_COUNT) $(TESTSAN_FLAGS) -fpic -c xz_parse.cpp -o xz_parse.o
	$(CXX) $(TESTSAN_FLAGS) $(LD_FLAGS) xz_parse.o -o libxz_count.so $(LIB_PATH_COUNT)  $(DYN_LIBS_COUNT) $(LIBS)



clean:
	rm -f test
	rm -f *.o
	rm -f *.so
