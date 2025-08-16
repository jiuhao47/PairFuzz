SCRIPT_DIR := $(dir $(realpath $(lastword $(MAKEFILE_LIST))))

ROOT_PATH="${SCRIPT_DIR}/../../build/xml2"

CC := ${SCRIPT_DIR}/../../compiler/target/release/libafl_cc
CXX := ${SCRIPT_DIR}/../../compiler/target/release/libafl_cxx
#---------------------------------------------------------
LIBS := -lxml2  -lpthread -ldl
#这里我去掉了-werror的参数 -Werror在FLAGS
FLAGS := -fsanitize=address,fuzzer-no-link
LD_FLAGS := -rdynamic -shared  

# ###########################################
INSTALL_PATH=$(ROOT_PATH)/$(COMMIT_HASH)/usr/local
INC_PATH = -I$(INSTALL_PATH)/include
LIB_PATH = -L$(INSTALL_PATH)/lib
DYN_LIBS = -Wl,-Bdynamic,-rpath=$(INSTALL_PATH)/lib
DYN_LIBS_DOCKER = -Wl,-Bdynamic,-rpath=/app/xml2/install/usr/local/lib
# 考虑到这里用了两个不同的parse.cpp文件,发现内容一致，所以改成一个了，相应的diff也做了修改。
genlib_thread: xml2_parse.cpp
	$(CXX) $(THREAD_INC_PATH) $(FLAGS) -fpic -c xml2_parse.cpp -o xml2_$(COMMIT_HASH).o
	$(CXX) $(FLAGS) $(LDFLAGS) xml2_$(COMMIT_HASH).o -o libxml2_$(COMMIT_HASH).so $(THREAD_LIB_PATH)  $(THREAD_DYN_LIBS) $(LIBS)
DYN_LIBS_DOCKER = -Wl,-Bdynamic,-rpath=/app/xml2/install/usr/local/lib/

genlib_docker: xml2_parse.cpp
	$(CXX) $(THREAD_INC_PATH) $(FLAGS) -fpic -c xml2_parse.cpp -o xml2_$(COMMIT_HASH)_docker.o
	$(CXX) $(FLAGS) $(LDFLAGS) xml2_$(COMMIT_HASH)_docker.o -o libxml2_$(COMMIT_HASH)_docker.so $(THREAD_LIB_PATH)  $(DYN_LIBS_DOCKER) $(LIBS)

test_thread: test_thread.cpp
	$(CXX) -fsanitize=address,fuzzer-no-link -o test_thread test_thread.cpp

