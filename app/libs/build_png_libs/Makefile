ROOT_PATH=/home/user/workspace/workshop/diffray/app/build/png

CC := /home/user/workspace/workshop/diffray/app/compiler/target/release/libafl_cc
CXX := /home/user/workspace/workshop/diffray/app/compiler/target/release/libafl_cxx
FLAGS := -fsanitize=address,fuzzer-no-link
LD_FLAGS := -rdynamic -shared  
LIBS := -lm -lz -lpng

##############################################################################
INSTALL_PATH=$(ROOT_PATH)/$(COMMIT_HASH)/usr/local
INC_PATH = -I$(INSTALL_PATH)/include
LIB_PATH = -L$(INSTALL_PATH)/lib
DYN_LIBS = -Wl,-Bdynamic,-rpath=$(INSTALL_PATH)/lib
DYN_LIBS_DOCKER = -Wl,-Bdynamic,-rpath=/app/png/install/usr/local/lib

genlib_thread: libpngParse.cpp
	$(CXX) $(FLAGS)  $(INC_PATH) -fPIC -c -o libpngParse_$(COMMIT_HASH).o libpngParse.cpp
	$(CXX) $(LD_FLAGS) $(LIB_PATH) $(DYN_LIBS) libpngParse_$(COMMIT_HASH).o -o $(ROOT_PATH)/$(COMMIT_HASH)/libpng_$(COMMIT_HASH).so $(LIBS)
	rm -rf libpngParse_$(COMMIT_HASH).o

genlib_docker: libpngParse.cpp
	$(CXX) $(FLAGS)  $(INC_PATH) -fPIC -c -o libpngParse_$(COMMIT_HASH).o libpngParse.cpp
	$(CXX) $(LD_FLAGS) $(LIB_PATH) $(DYN_LIBS_DOCKER) libpngParse_$(COMMIT_HASH).o -o $(ROOT_PATH)/$(COMMIT_HASH)/libpng_$(COMMIT_HASH)_docker.so $(LIBS)
	rm -rf libpngParse_$(COMMIT_HASH).o

test: test.cpp
	$(CXX) -fsanitize=address test.cpp -o test 

test_thread: test_thread.cpp
	$(CXX) -g -O2 -fsanitize=address test_thread.cpp -o test_thread -ldl

clean:
	rm -f test test_thread
	rm -f *.o
