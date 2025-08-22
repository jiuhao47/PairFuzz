
SCRIPT_DIR := $(dir $(realpath $(lastword $(MAKEFILE_LIST))))

ROOT_PATH="${SCRIPT_DIR}/../../build/png"

CC := ${SCRIPT_DIR}/../../compiler/target/release/libafl_cc
CXX := ${SCRIPT_DIR}/../../compiler/target/release/libafl_cxx
FLAGS := -fsanitize=address,fuzzer-no-link
LD_FLAGS := -rdynamic -shared  
COMMONFLAGS := -DMAGICKCORE_HDRI_ENABLE=1 -DMAGICKCORE_QUANTUM_DEPTH=16

OLD_INC_PATH = -I$(BUILD_LIB_OLD_PATH)include/ImageMagick-7
OLD_LIB_PATH = -L$(BUILD_LIB_OLD_PATH)lib
OLD_DYN_LIBS = -Wl,-Bdynamic,-rpath=$(BUILD_LIB_OLD_PATH)lib

NEW_INC_PATH = -I$(BUILD_LIB_NEW_PATH)include/ImageMagick-7
NEW_LIB_PATH = -L$(BUILD_LIB_NEW_PATH)lib
NEW_DYN_LIBS = -Wl,-Bdynamic,-rpath=$(BUILD_LIB_NEW_PATH)lib

LIBS := -lMagick++-7.Q16HDRI -lMagickWand-7.Q16HDRI -lMagickCore-7.Q16HDRI
OLD_DYN_LIBS_RE = -Wl,-Bdynamic,-rpath=/workspace/libs/build_IM_libs/ImageMagick_oldest_lib/lib
NEW_DYN_LIBS_RE = -Wl,-Bdynamic,-rpath=/workspace/libs/build_IM_libs/ImageMagick_newest_lib/lib

genlib_relative: parseIM.cpp
	$(CXX) $(FLAGS) $(COMMONFLAGS) $(OLD_INC_PATH) -fPIC -c -o parseIM.o parseIM.cpp
	$(CXX) $(LDFLAGS) $(OLD_LIB_PATH) $(OLD_DYN_LIBS_RE) parseIM.o -o libIM_oldest_re.so $(LIBS)
	$(CXX) $(FLAGS) $(COMMONFLAGS) $(NEW_INC_PATH) -fPIC -c -o parseIM_newest.o parseIM.cpp
	$(CXX) $(LDFLAGS) $(NEW_LIB_PATH) $(NEW_DYN_LIBS_RE) parseIM_newest.o -o libIM_newest_re.so $(LIBS)
	cp $(ROOT_PATH)/libs/build_IM_libs/*.so $(ROOT_PATH)/libs/


##################################################################################
INSTALL_LIB_PATH_THREAD=$(ROOT_PATH)/build/IM_$(COMMIT_HASH)/install/usr/local
INC_PATH_THREAD = -I$(INSTALL_LIB_PATH_THREAD)/include/ImageMagick-7
LIB_PATH_THREAD = -L$(INSTALL_LIB_PATH_THREAD)/lib
DYN_LIBS_THREAD = -Wl,-Bdynamic,-rpath=$(INSTALL_LIB_PATH_THREAD)/lib

genlib_thread: parseIM.cpp
	$(CXX) $(FLAGS) $(COMMONFLAGS) $(INC_PATH_THREAD) -fPIC -c -o parseIM_$(COMMIT_HASH).o parseIM.cpp
	$(CXX) $(LDFLAGS) $(LIB_PATH_THREAD) $(DYN_LIBS_THREAD) parseIM_$(COMMIT_HASH).o -o libIM_$(COMMIT_HASH).so $(LIBS)

DYN_LIBS_DOCKER = -Wl,-Bdynamic,-rpath=/app/IM/install/usr/local/lib

genlib_docker: parseIM.cpp
	$(CXX) $(FLAGS) $(COMMONFLAGS) $(INC_PATH_THREAD) -fPIC -c -o parseIM_$(COMMIT_HASH).o parseIM.cpp
	$(CXX) $(LDFLAGS) $(LIB_PATH_THREAD) $(DYN_LIBS_DOCKER) parseIM_$(COMMIT_HASH).o -o libIM_$(COMMIT_HASH)_docker.so $(LIBS)



INSTALL_LIB_PATH_COUNT=$(ROOT_PATH)/build/IM_count_$(COMMIT_HASH)/install/usr/local
INC_PATH_COUNT = -I$(INSTALL_LIB_PATH_COUNT)/include/ImageMagick-7
LIB_PATH_COUNT = -L$(INSTALL_LIB_PATH_COUNT)/lib
DYN_LIBS_COUNT = -Wl,-Bdynamic,-rpath=$(INSTALL_LIB_PATH_COUNT)/lib

genlib_count: parseIM.cpp
	$(CXX) $(FLAGS) $(COMMONFLAGS) $(INC_PATH_COUNT) -fPIC -c -o parseIM_count.o parseIM.cpp
	$(CXX) $(LDFLAGS) $(LIB_PATH_COUNT) $(DYN_LIBS_COUNT) parseIM_count.o -o libIM_count.so $(LIBS)



clean_lib:
	rm -f *.o
	rm -f *.so

test: test.cpp
	$(CXX) -g -O2 -fsanitize=address,fuzzer-no-link test.cpp -o test -ldl

test_thread: test_thread.cpp
	$(CXX) -g -O2 -fsanitize=address,fuzzer-no-link test_thread.cpp -o test_thread -ldl

verify:verify.cpp
	$(CXX) $(FLAGS) $(COMMONFLAGS) $(INC_PATH_THREAD) -o verify_$(COMMIT_HASH) verify.cpp $(LIB_PATH_THREAD) $(DYN_LIBS_THREAD) $(LIBS)
