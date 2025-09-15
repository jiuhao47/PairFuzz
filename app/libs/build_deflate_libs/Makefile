CC := /home/xjy/fuzz/else/llvm-project/build/bin/clang
CXX := /home/xjy/fuzz/else/llvm-project/build/bin/clang++

FLAGS := -fsanitize=address,fuzzer-no-link -fsanitize-coverage=edge,indirect-calls -Wall 

OLD_BASE_PATH := /home/fdl/diff-fuzz/examples/builds/libs/libdeflate_oldest
NEW_BASE_PATH := /home/fdl/diff-fuzz/examples/builds/libs/libdeflate_newest

OLD_INC_PATH := -I${OLD_BASE_PATH}/include
OLD_LIBS := ${OLD_BASE_PATH}/lib/libdeflate.a


NEW_INC_PATH := -I${NEW_BASE_PATH}/include
NEW_LIB_PATH := -L$(NEW_BASE_PATH)/lib
NEW_DYN_LIBS := -Wl,-rpath,$(NEW_BASE_PATH)/lib
NEW_LIBS := -ldeflate

LDFLAGS := -rdynamic -shared -fsanitize=address,fuzzer-no-link
LIBS := -ldeflate


all: genLib test

# very important here, must not use the name libdeflate.so
# otherwise, it incurs the compiler error with 'undefined symbol: __sancov_lowest_stack'

genLib: libdeflate_parse_new.cpp
	$(CXX) $(NEW_INC_PATH) $(FLAGS) -fpic -c libdeflate_parse_new.cpp -o libdeflate_parse_new.o
	$(CXX) $(LDFLAGS) libdeflate_parse_new.o $(NEW_DYN_LIBS) $(NEW_LIB_PATH) $(NEW_LIBS) -o libdeflate_parse_newest.so

	$(CXX) $(OLD_INC_PATH) $(FLAGS) -fpic -c libdeflate_parse_old.cpp -o libdeflate_parse_old.o
	$(CXX) $(LDFLAGS) libdeflate_parse_old.o $(OLD_LIBS) -o libdeflate_parse_oldest.so

test: test.cpp
	$(CXX) $(FLAGS) test.cpp -o test
	
clean:
	rm -f *.so
	rm -f test
	rm -f *.o


# ###################################################################


THREAD_BASE_PATH := /home/xjy/fuzz/else/fuzz_project/build
THREAD_INC_PATH = -I$(THREAD_BASE_PATH)/deflate_$(COMMIT_HASH)/install/usr/local/include 
THREAD_LIB_PATH = -L$(THREAD_BASE_PATH)/deflate_$(COMMIT_HASH)/install/usr/local/lib
THREAD_DYN_LIBS = -Wl,-rpath,$(THREAD_BASE_PATH)/deflate_$(COMMIT_HASH)/install/usr/local/lib
# new:
genlib_thread: libdeflate_parse_new.cpp
	$(CXX) $(THREAD_INC_PATH) $(FLAGS) -fpic -c libdeflate_parse_new.cpp -o libdeflate_parse_new_$(COMMIT_HASH).o 
	$(CXX) $(FLAGS) $(LDFLAGS) libdeflate_parse_new_$(COMMIT_HASH).o -o libdeflate_$(COMMIT_HASH).so $(THREAD_LIB_PATH)  $(THREAD_DYN_LIBS) $(LIBS)
DYN_LIBS_DOCKER = -Wl,-Bdynamic,-rpath=/app/deflate/install/usr/local/lib/

genlib_docker: libdeflate_parse_new.cpp
	$(CC) $(THREAD_INC_PATH) $(FLAGS) -fpic -c libdeflate_parse_new.cpp -o libdeflate_parse_new_$(COMMIT_HASH)_docker.o
	$(CC) $(FLAGS) $(LDFLAGS) libdeflate_parse_new_$(COMMIT_HASH)_docker.o -o libdeflate_$(COMMIT_HASH)_docker.so $(THREAD_LIB_PATH)  $(DYN_LIBS_DOCKER) $(LIBS)
# old:

THREAD_INC_PATH_OLD = -I$(THREAD_BASE_PATH)/deflate_$(COMMIT_HASH)/src -I$(THREAD_BASE_PATH)/deflate_$(COMMIT_HASH)
THREAD_LIB_PATH_OLD = -L$(THREAD_BASE_PATH)/deflate_$(COMMIT_HASH)
THREAD_DYN_LIBS_OLD = -Wl,-rpath,$(THREAD_BASE_PATH)/deflate_$(COMMIT_HASH)

genlib_thread_old: libdeflate_parse_old.cpp
	$(CXX) $(THREAD_INC_PATH_OLD) $(FLAGS) -fpic -c libdeflate_parse_old.cpp -o libdeflate_parse_old_$(COMMIT_HASH).o 
	$(CXX) $(FLAGS) $(LDFLAGS) libdeflate_parse_old_$(COMMIT_HASH).o -o libdeflate_$(COMMIT_HASH).so $(THREAD_LIB_PATH_OLD)  $(THREAD_DYN_LIBS_OLD) $(LIBS)
DYN_LIBS_DOCKER_OLD = -Wl,-Bdynamic,-rpath=/app/deflate/

genlib_docker_old: libdeflate_parse_old.cpp
	$(CC) $(THREAD_INC_PATH_OLD) $(FLAGS) -fpic -c libdeflate_parse_old.cpp -o libdeflate_parse_old_$(COMMIT_HASH)_docker.o
	$(CC) $(FLAGS) $(LDFLAGS) libdeflate_parse_old_$(COMMIT_HASH)_docker.o -o libdeflate_$(COMMIT_HASH)_docker.so $(THREAD_LIB_PATH_OLD)  $(DYN_LIBS_DOCKER_OLD) $(LIBS)

test_thread: test_thread.cpp
	$(CXX) -fsanitize=address,fuzzer-no-link -o test_thread test_thread.cpp

