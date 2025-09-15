CC := /home/xjy/fuzz/else/llvm-project/build/bin/clang
CXX := /home/xjy/fuzz/else/llvm-project/build/bin/clang++
#---------------------------------------------------------
OLD_BASE_PATH := /home/fdl/diff-fuzz/examples/builds/libs/mjson_oldest
NEW_BASE_PATH := /home/fdl/diff-fuzz/examples/builds/libs/mjson_newest

OLD_INC_PATH := -I$(OLD_BASE_PATH)/include
OLD_LIB_PATH := -L$(OLD_BASE_PATH)/lib
OLD_DYN_LIBS := -Wl,-rpath,$(OLD_BASE_PATH)/lib

NEW_INC_PATH := -I$(NEW_BASE_PATH)/include
NEW_LIB_PATH := -L$(NEW_BASE_PATH)/lib
NEW_DYN_LIBS := -Wl,-rpath,$(NEW_BASE_PATH)/lib

FLAGS := -fsanitize=address,fuzzer-no-link -fsanitize-coverage=edge,indirect-calls  
TESTSAN_FLAGS := -O0 -fsanitize=testsan -g
LD_FLAGS := -rdynamic -shared 
LIBS := -lmjson

all: genlib test

genlib: parse_mjson.c
	$(CC) $(NEW_INC_PATH) $(FLAGS) -fpic -c parse_mjson.c -o parse_mjson.o
	$(CC) $(FLAGS) $(LD_FLAGS) parse_mjson.o -o libmjson_newest.so $(NEW_DYN_LIBS)  $(NEW_LIB_PATH) $(LIBS)
	$(CC) $(OLD_INC_PATH) $(FLAGS) -fpic -c parse_mjson.c -o parse_mjson_old.o
	$(CC) $(FLAGS) $(LD_FLAGS) parse_mjson_old.o -o libmjson_oldest.so 

test: test.cpp
	$(CXX) -fsanitize=address,fuzzer-no-link -o test test.cpp

THREAD_BASE_PATH := /home/xjy/fuzz/else/fuzz_project/build
THREAD_INC_PATH = -I$(THREAD_BASE_PATH)/mjson_$(COMMIT_HASH)/src
THREAD_LIB_PATH = -L$(THREAD_BASE_PATH)/mjson_$(COMMIT_HASH)/
THREAD_DYN_LIBS = -Wl,-rpath,$(THREAD_BASE_PATH)/mjson_$(COMMIT_HASH)/

genlib_thread: parse_mjson.c
	$(CC) $(THREAD_INC_PATH) $(FLAGS) -fpic -c parse_mjson.c -o parse_mjson_$(COMMIT_HASH).o
	$(CC) $(FLAGS) $(LD_FLAGS) parse_mjson_$(COMMIT_HASH).o -o libmjson_$(COMMIT_HASH).so $(THREAD_LIB_PATH)  $(THREAD_DYN_LIBS) $(LIBS)

DYN_LIBS_DOCKER = -Wl,-Bdynamic,-rpath=/app/mjson
genlib_docker: parse_mjson.c
	$(CC) $(THREAD_INC_PATH) $(FLAGS) -fpic -c parse_mjson.c -o parse_mjson_$(COMMIT_HASH).o
	$(CC) $(FLAGS) $(LD_FLAGS) parse_mjson_$(COMMIT_HASH).o -o libmjson_$(COMMIT_HASH)_docker.so $(THREAD_LIB_PATH)  $(DYN_LIBS_DOCKER) $(LIBS)


THREAD_BASE_PATH_OLD := /home/xjy/fuzz/else/fuzz_project/build
THREAD_INC_PATH_OLD = -I$(THREAD_BASE_PATH)/mjson_$(COMMIT_HASH)
THREAD_LIB_PATH_OLD = -L$(THREAD_BASE_PATH)/mjson_$(COMMIT_HASH)/
THREAD_DYN_LIBS_OLD = -Wl,-rpath,$(THREAD_BASE_PATH)/mjson_$(COMMIT_HASH)/
genlib_thread_old: parse_mjson.c
	$(CC) $(THREAD_INC_PATH_OLD) $(FLAGS) -fpic -c parse_mjson.c -o parse_mjson_$(COMMIT_HASH).o
	$(CC) $(FLAGS) $(LD_FLAGS) parse_mjson_$(COMMIT_HASH).o -o libmjson_$(COMMIT_HASH).so $(THREAD_LIB_PATH_OLD)  $(THREAD_DYN_LIBS_OLD) $(LIBS)
DYN_LIBS_DOCKER_OLD = -Wl,-Bdynamic,-rpath=/app/mjson
genlib_thread_old_docker: parse_mjson.c
	$(CC) $(THREAD_INC_PATH_OLD) $(FLAGS) -fpic -c parse_mjson.c -o parse_mjson_$(COMMIT_HASH).o
	$(CC) $(FLAGS) $(LD_FLAGS) parse_mjson_$(COMMIT_HASH).o -o libmjson_$(COMMIT_HASH)_docker.so $(THREAD_LIB_PATH_OLD)  $(DYN_LIBS_DOCKER_OLD) $(LIBS)

#####################################################################
COUNT_INC_PATH = -I$(THREAD_BASE_PATH)/mjson_$(COMMIT_HASH)/src
COUNT_LIB_PATH = -L$(THREAD_BASE_PATH)/mjson_$(COMMIT_HASH)/
COUNT_DYN_LIBS = -Wl,-rpath,$(THREAD_BASE_PATH)/mjson_$(COMMIT_HASH)/

genlib_count: parse_mjson.c
	$(CC) $(COUNT_INC_PATH) $(TESTSAN_FLAGS) -fpic -c parse_mjson.c -o parse_mjson_count.o
	$(CC) $(TESTSAN_FLAGS) $(LD_FLAGS) parse_mjson_count.o -o libmjson_count.so $(COUNT_LIB_PATH)  $(COUNT_DYN_LIBS) $(LIBS)

test_thread: test_thread.cpp
	$(CXX) -fsanitize=address,fuzzer-no-link -o test_thread test_thread.cpp

fuzz_target :fuzz_target.cpp
	$(CXX) -fsanitize=address,fuzzer -o fuzz_target fuzz_target.cpp
clean:
	rm -f test test_thread
	rm -f *.o
	rm -f *.so
	rm -rf mjson_*
