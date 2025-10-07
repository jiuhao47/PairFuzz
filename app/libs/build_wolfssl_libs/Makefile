CC := /home/user/software/llvm-project/build/bin/clang
CXX := /home/user/software/llvm-project/build/bin/clang++
#---------------------------------------------------------

CONFIG_OLD := /home/fdl/diff-fuzz/examples/builds/libs/wolfssl_oldest
CONFIG_NEW := /home/fdl/diff-fuzz/examples/builds/libs/wolfssl_newest
CONFIG_SRC := /home/fdl/diff-fuzz/examples/src/libs/wolfssl_newest

# CONFIG_OLD := /home/fdl/diff-fuzz/tmp/openssl/openssl
OLD_LIB_PATH := -L$(CONFIG_OLD)/build/src/.libs
NEW_LIB_PATH := -L$(CONFIG_NEW)/build/src/.libs
OLD_INC_PATH := -I$(CONFIG_SRC) -I$(CONFIG_SRC)/build
NEW_INC_PATH := -I$(CONFIG_SRC) -I$(CONFIG_SRC)/build

LIBS := -lwolfssl -lpthread -lm
#这里我去掉了-werror的参数 -Werror在FLAGS
FLAGS := -O2 -Wall -Wextra -Wno-unused  -g -ggdb3 -fPIC  -fsanitize=address,fuzzer-no-link 
LDFLAGS := -O2 -Wall -g -ggdb3 -fPIC -rdynamic -shared -fsanitize=address,fuzzer-no-link
OLD_DYN_LIBS := -Wl,-rpath,$(CONFIG_OLD)/build/src/.libs
NEW_DYN_LIBS := -Wl,-rpath,$(CONFIG_NEW)/build/src/.libs

# -fsanitize=fuzzer-no-link -fsanitize=fuzzer-no-link  -fsanitize=fuzzer-no-link

all: verify_old verify_new test

verify_old: wolfssl_old.cpp 
	$(CXX) -c $(OLD_INC_PATH) $(FLAGS) wolfssl_old.cpp -o verify_old.o  
	$(CXX) $(LDFLAGS) $(OLD_LIB_PATH) $(OLD_DYN_LIBS) $(LIBS) verify_old.o -o libwolfssl_old.so

verify_new: wolfssl_new.cpp
	$(CXX) -c $(NEW_INC_PATH) $(FLAGS) wolfssl_new.cpp -o verify_new.o 
	$(CXX) $(LDFLAGS) $(NEW_LIB_PATH) $(NEW_DYN_LIBS) $(LIBS) verify_new.o -o libwolfssl_new.so

test: test.cpp
	$(CXX) -g -O2 -fsanitize=address,fuzzer-no-link test.cpp -o test -ldl

clean:
	rm -f verify_new*
	rm -f verify_old*
	rm -f test


# ###########################################
THREAD_BASE_PATH := /home/user/software/fuzz_project/build
THREAD_INC_PATH = -I$(THREAD_BASE_PATH)/wolfssl_$(COMMIT_HASH)/install/usr/local/include
THREAD_LIB_PATH = -L$(THREAD_BASE_PATH)/wolfssl_$(COMMIT_HASH)/install/usr/local/lib
THREAD_DYN_LIBS = -Wl,-rpath,$(THREAD_BASE_PATH)/wolfssl_$(COMMIT_HASH)/install/usr/local/lib
# 考虑到这里用了两个不同的parse.cpp文件,发现内容一致，所以改成一个了，相应的diff也做了修改。
genlib_thread: wolfssl_verify_cert.cpp
	$(CXX) $(THREAD_INC_PATH) $(FLAGS) -fpic -c wolfssl_verify_cert.cpp -o wolfssl_$(COMMIT_HASH).o
	$(CXX) $(FLAGS) $(LDFLAGS) wolfssl_$(COMMIT_HASH).o -o libwolfssl_$(COMMIT_HASH).so $(THREAD_LIB_PATH)  $(THREAD_DYN_LIBS) $(LIBS)
DYN_LIBS_DOCKER = -Wl,-Bdynamic,-rpath=/app/wolfssl/install/usr/local/lib/

genlib_docker: wolfssl_verify_cert_docker.cpp
	$(CXX) $(THREAD_INC_PATH) $(FLAGS) -fpic -c wolfssl_verify_cert_docker.cpp -o wolfssl_$(COMMIT_HASH)_docker.o
	$(CXX) $(FLAGS) $(LDFLAGS) wolfssl_$(COMMIT_HASH)_docker.o -o libwolfssl_$(COMMIT_HASH)_docker.so $(THREAD_LIB_PATH)  $(DYN_LIBS_DOCKER) $(LIBS)

THREAD_BASE_PATH := /home/user/software/fuzz_project/build
# COUNT_INC_PATH = -I$(THREAD_BASE_PATH)/openssl_count_$(COMMIT_HASH)/install/usr/local/include
# COUNT_LIB_PATH = -L$(THREAD_BASE_PATH)/openssl_count_$(COMMIT_HASH)/install/usr/local/lib64
# COUNT_DYN_LIBS = -Wl,-rpath,$(THREAD_BASE_PATH)/openssl_count_$(COMMIT_HASH)/install/usr/local/lib64
COUNT_INC_PATH = -I$(THREAD_BASE_PATH)/wolfssl_count_$(COMMIT_HASH)/install/usr/local/include
COUNT_LIB_PATH = -L$(THREAD_BASE_PATH)/wolfssl_count_$(COMMIT_HASH)/install/usr/local/lib
COUNT_DYN_LIBS = -Wl,-rpath,$(THREAD_BASE_PATH)/wolfssl_count_$(COMMIT_HASH)/install/usr/local/lib

genlib_count: wolfssl_verify_cert.cpp
	$(CXX) $(COUNT_INC_PATH) $(FLAGS) -fpic -c wolfssl_verify_cert.cpp -o wolfssl_count_$(COMMIT_HASH).o
	$(CXX) $(FLAGS) $(LDFLAGS) wolfssl_count_$(COMMIT_HASH).o -o libwolfssl_count.so $(COUNT_LIB_PATH)  $(COUNT_DYN_LIBS) $(LIBS)

test_thread: test_thread.cpp
	$(CXX) -fsanitize=address,fuzzer-no-link -o test_thread test_thread.cpp

xjy_thread: xjy_parse.cpp
	$(CXX) $(THREAD_INC_PATH) $(FLAGS) -fpic -c wolfssl_verify_cert.cpp -o xjy_parse.o
	$(CXX) $(FLAGS) $(LDFLAGS) xjy_parse.o -o xjy_parse.so $(THREAD_LIB_PATH)  $(THREAD_DYN_LIBS) $(LIBS)
