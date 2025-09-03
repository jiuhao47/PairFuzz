CXX := /home/xjy/fuzz/else/llvm-project/build/bin/clang++
CC := /home/xjy/fuzz/else/llvm-project/build/bin/clang

OLD_BASE_PATH := /home/fdl/diff-fuzz/examples/builds/libs/cjson_oldest
NEW_BASE_PATH := /home/fdl/diff-fuzz/examples/builds/libs/cjson_newest

OLD_INC_PATH := -I$(OLD_BASE_PATH)/include
OLD_LIB_PATH := -L$(OLD_BASE_PATH)/lib
OLD_DYN_LIBS := -Wl,-rpath,$(OLD_BASE_PATH)/lib

NEW_INC_PATH := -I$(NEW_BASE_PATH)/include
NEW_LIB_PATH := -L$(NEW_BASE_PATH)/lib
NEW_DYN_LIBS := -Wl,-rpath,$(NEW_BASE_PATH)/lib

FLAGS := -fsanitize=address,fuzzer-no-link -fsanitize-coverage=edge,indirect-calls  
TESTSAN_FLAGS := -fsanitize=testsan -g -O0
LD_FLAGS := -rdynamic -shared 
LIBS := -lcjson

all: genlib test

genlib: parse_cJson.c
	$(CC) $(NEW_INC_PATH) $(FLAGS) -fpic -c parse_cJson.c -o parse_cJson.o
	$(CC) $(FLAGS) $(LD_FLAGS) parse_cJson.o -o libcjson_newest.so $(NEW_DYN_LIBS)  $(NEW_LIB_PATH) $(LIBS)

	$(CC) $(OLD_INC_PATH) $(FLAGS) -fpic -c parse_cJson.c -o parse_cJson_old.o
	$(CC) $(FLAGS) $(LD_FLAGS) parse_cJson_old.o -o libcjson_oldest.so $(OLD_DYN_LIBS)  $(OLD_LIB_PATH) $(LIBS)


THREAD_BASE_PATH := /home/xjy/fuzz/else/fuzz_project/build


INC_PATH_THREAD_OLD := -I$(THREAD_BASE_PATH)/cjson_$(COMMIT_HASH)
LIB_PATH_THREAD_OLD := -L$(THREAD_BASE_PATH)/cjson_$(COMMIT_HASH)
DYN_LIBS_THREAD_OLD := -Wl,-rpath,$(THREAD_BASE_PATH)/cjson_$(COMMIT_HASH)
genlib_thread_old: parse_cJson.c
	$(CC) $(INC_PATH_THREAD_OLD) $(FLAGS) -fpic -c parse_cJson.c -o parse_cJson_$(COMMIT_HASH).o
	$(CC) $(FLAGS) $(LD_FLAGS) parse_cJson_$(COMMIT_HASH).o -o libcjson_$(COMMIT_HASH).so $(DYN_LIBS_THREAD_OLD)  $(LIB_PATH_THREAD_OLD) $(LIBS)

DYN_LIBS_DOCKER_OLD = -Wl,-Bdynamic,-rpath=/app/cjson
genlib_thread_old_docker: parse_cJson.c
	$(CC) $(INC_PATH_THREAD_OLD) $(FLAGS) -fpic -c parse_cJson.c -o parse_cJson_$(COMMIT_HASH).o
	$(CC) $(FLAGS) $(LD_FLAGS) parse_cJson_$(COMMIT_HASH).o -o libcjson_$(COMMIT_HASH)_docker.so $(DYN_LIBS_DOCKER_OLD)  $(LIB_PATH_THREAD_OLD) $(LIBS)


INC_PATH_THREAD := -I$(THREAD_BASE_PATH)/cjson_$(COMMIT_HASH)/install/usr/local/include/cjson
LIB_PATH_THREAD := -L$(THREAD_BASE_PATH)/cjson_$(COMMIT_HASH)/install/usr/local/lib
DYN_LIBS_THREAD := -Wl,-rpath,$(THREAD_BASE_PATH)/cjson_$(COMMIT_HASH)/install/usr/local/lib
genlib_thread: parse_cJson.c
	$(CC) $(INC_PATH_THREAD) $(FLAGS) -fpic -c parse_cJson.c -o parse_cJson_$(COMMIT_HASH).o
	$(CC) $(FLAGS) $(LD_FLAGS) parse_cJson_$(COMMIT_HASH).o -o libcjson_$(COMMIT_HASH).so $(DYN_LIBS_THREAD)  $(LIB_PATH_THREAD) $(LIBS)

DYN_LIBS_DOCKER = -Wl,-Bdynamic,-rpath=/app/cjson/install/usr/local/lib
genlib_thread_docker: parse_cJson.c
	$(CC) $(INC_PATH_THREAD) $(FLAGS) -fpic -c parse_cJson.c -o parse_cJson_$(COMMIT_HASH).o
	$(CC) $(FLAGS) $(LD_FLAGS) parse_cJson_$(COMMIT_HASH).o -o libcjson_$(COMMIT_HASH)_docker.so $(DYN_LIBS_DOCKER)  $(LIB_PATH_THREAD) $(LIBS)

fuzz_target :fuzz_target.cpp
	$(CXX) -fsanitize=address,fuzzer -o fuzz_target fuzz_target.cpp

##########################################################
INC_PATH_COUNT := -I$(THREAD_BASE_PATH)/cjson_count_$(COMMIT_HASH)/install/usr/local/include/cjson
LIB_PATH_COUNT := -L$(THREAD_BASE_PATH)/cjson_count_$(COMMIT_HASH)/install/usr/local/lib
DYN_LIBS_COUNT := -Wl,-rpath,$(THREAD_BASE_PATH)/cjson_count_$(COMMIT_HASH)/install/usr/local/lib

genlib_count:parse_cJson.c 
	$(CC) $(INC_PATH_COUNT) $(TESTSAN_FLAGS) -fpic -c parse_cJson.c -o parse_cJson_count.o
	$(CC) $(TESTSAN_FLAGS) $(LD_FLAGS) parse_cJson_count.o -o libcjson_count.so $(DYN_LIBS_COUNT)  $(LIB_PATH_COUNT) $(LIBS)


test_thread: test_thread.cpp
	$(CXX) -fsanitize=address,fuzzer-no-link -o test_thread test_thread.cpp

test: test.cpp
	$(CXX) -fsanitize=address,fuzzer-no-link -o test test.cpp

len: len.cpp
	$(CXX) -fsanitize=address, -o len len.cpp $(INC_PATH_THREAD) $(DYN_LIBS_THREAD_OLD)  $(LIB_PATH_THREAD_OLD) $(LIBS)
clean:
	rm -f test test_thread
	rm -f *.o
	rm -f *.so
	rm -rf cJSON_*
