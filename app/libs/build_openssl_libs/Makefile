CC := /home/user/software/llvm-project/build/bin/clang
CXX := /home/user/software/llvm-project/build/bin/clang++
#---------------------------------------------------------
CONFIG_OLD := /home/fdl/diff-fuzz/examples/builds/libs/openssl_oldest
CONFIG_NEW := /home/fdl/diff-fuzz/examples/builds/libs/openssl_newest

# CONFIG_OLD := /home/fdl/Desktop/ssl_builds/fadb57
# CONFIG_NEW := /home/fdl/Desktop/ssl_builds/cf6342

# CONFIG_OLD := /home/fdl/diff-fuzz/tmp/openssl/openssl
LIB_PATH := -L$(CONFIG_OLD)/lib
NEW_LIB_PATH := -L$(CONFIG_NEW)/lib64
INC_PATH := -I$(CONFIG_OLD)/include
NEW_INC_PATH := -I$(CONFIG_NEW)/include

LIBS := -lssl -lcrypto -ldl -pthread 
TESTSAN_LIBS:= -lssl -lcrypto -ldl -pthread -lclang_rt.testsan-x86_64
FLAGS := -O2 -Wall -g -ggdb3 -fPIC -fsanitize=fuzzer-no-link
TESTSAN_FLAGS := -fsanitize=testsan -g -O0
LDFLAGS := -O2  -Wall -g -ggdb3 -fPIC -rdynamic -shared -fsanitize=fuzzer-no-link
TESTSAN_LDFLAGS := -O2  -Wall -g -ggdb3 -fPIC -rdynamic -shared
DYN_LIBS := -Wl,-rpath,$(CONFIG_OLD)/lib
NEW_DYN_LIBS := -Wl,-rpath,$(CONFIG_NEW)/lib64

# -fsanitize=fuzzer-no-link -fsanitize=fuzzer-no-link  -fsanitize=fuzzer-no-link

all: verify_old verify_new test

verify_old: openssl_old.cpp 
	$(CXX) -c  $(INC_PATH) $(FLAGS) -o verify_old.o openssl_old.cpp 
	$(CXX) $(LDFLAGS)  $(LIB_PATH) $(DYN_LIBS) $(LIBS) verify_old.o -o libopenssl_old.so

verify_new: openssl_new.cpp
	$(CXX) $(NEW_INC_PATH) $(FLAGS) -c -o verify_new.o openssl_new.cpp 
	$(CXX) $(LDFLAGS)  $(NEW_LIB_PATH) $(NEW_DYN_LIBS) $(LIBS) verify_new.o -o libopenssl_new.so

test: test.cpp
	$(CXX) -g -O2 -fsanitize=address,fuzzer-no-link test.cpp -o test -ldl

clean:
	rm -f verify_new
	rm -f verify_old
	rm -f test

# ###########################################
THREAD_BASE_PATH := /home/user/software/fuzz_project/build
THREAD_INC_PATH = -I$(THREAD_BASE_PATH)/openssl_$(COMMIT_HASH)/install/usr/local/include
THREAD_LIB_PATH = -L$(THREAD_BASE_PATH)/openssl_$(COMMIT_HASH)/install/usr/local/lib64
THREAD_DYN_LIBS = -Wl,-rpath,$(THREAD_BASE_PATH)/openssl_$(COMMIT_HASH)/install/usr/local/lib64
# 考虑到这里用了两个不同的parse.cpp文件,发现内容一致，所以改成一个了，相应的diff也做了修改。
genlib_thread: openssl_verify_cert.cpp
	$(CXX) $(THREAD_INC_PATH) $(FLAGS) -fpic -c openssl_verify_cert.cpp -o openssl_$(COMMIT_HASH).o
	$(CXX) $(FLAGS) $(LDFLAGS) openssl_$(COMMIT_HASH).o -o libopenssl_$(COMMIT_HASH).so $(THREAD_LIB_PATH)  $(THREAD_DYN_LIBS) $(LIBS)

DYN_LIBS_DOCKER = -Wl,-Bdynamic,-rpath=/app/openssl/install/usr/local/lib64
genlib_docker: openssl_verify_cert_docker.cpp
	$(CXX) $(THREAD_INC_PATH) $(FLAGS) -fpic -c openssl_verify_cert_docker.cpp -o openssl_$(COMMIT_HASH).o
	$(CXX) $(FLAGS) $(LDFLAGS) openssl_$(COMMIT_HASH).o -o libopenssl_$(COMMIT_HASH)_docker.so $(THREAD_LIB_PATH)  $(DYN_LIBS_DOCKER) $(LIBS)


THREAD_BASE_PATH := /home/user/software/fuzz_project/build
COUNT_INC_PATH = -I$(THREAD_BASE_PATH)/openssl_count_$(COMMIT_HASH)/install/usr/local/include
COUNT_LIB_PATH = -L$(THREAD_BASE_PATH)/openssl_count_$(COMMIT_HASH)/install/usr/local/lib64
COUNT_DYN_LIBS = -Wl,-rpath,$(THREAD_BASE_PATH)/openssl_count_$(COMMIT_HASH)/install/usr/local/lib64

genlib_count: openssl_verify_cert.cpp
	$(CXX) $(COUNT_INC_PATH) $(TESTSAN_FLAGS) -fpic -c openssl_verify_cert.cpp -o openssl_count_$(COMMIT_HASH).o
	$(CXX) $(TESTSAN_FLAGS) $(TESTSAN_LDFLAGS) -L/home/user/software/llvm-project/build/lib/clang/14.0.0/lib/linux/  openssl_count_$(COMMIT_HASH).o -o libopenssl_count.so $(COUNT_LIB_PATH)  $(COUNT_DYN_LIBS) $(TESTSAN_LIBS)



xjy_parse: xjy_parse.cpp
	$(CXX) $(THREAD_INC_PATH) $(FLAGS) -fpic -c xjy_parse.cpp -o xjy_parse_$(COMMIT_HASH).o
	$(CXX) $(FLAGS) $(LDFLAGS) xjy_parse_$(COMMIT_HASH).o -o xjy_parse_$(COMMIT_HASH).so $(THREAD_LIB_PATH)  $(THREAD_DYN_LIBS) $(LIBS)
test_thread: test_thread.cpp
	$(CXX) -fsanitize=address,fuzzer-no-link -o test_thread test_thread.cpp
