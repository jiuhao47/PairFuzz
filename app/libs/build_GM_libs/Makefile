

CXX:=/home/xjy/fuzz/else/llvm-project/build/bin/clang++
CC:=/home/xjy/fuzz/else/llvm-project/build/bin/clang
ROOT_PATH=/home/xjy/fuzz/else/fuzz_project
# GraphicsMagick 14303
ORIGIN_LIB_OLD_PATH=$(ROOT_PATH)/libs/build_GM_libs/GraphicsMagick_oldest/

# GraphicsMagick 17300
ORIGIN_LIB_NEW_PATH=$(ROOT_PATH)/libs/build_GM_libs/GraphicsMagick_newest/

#
BUILD_LIB_OLD_PATH=$(ROOT_PATH)/libs/build_GM_libs/GraphicsMagick_oldest_lib/
#
BUILD_LIB_NEW_PATH=$(ROOT_PATH)/libs/build_GM_libs/GraphicsMagick_newest_lib/

BUILD_FLAGS := --disable-static --enable-shared  CC="clang" CXX="clang++" CFLAGS="-g -fPIC -fsanitize=address,fuzzer-no-link -fsanitize-coverage=edge,indirect-calls " CXXFLAGS="-g -fPIC -fsanitize=address,fuzzer-no-link -fsanitize-coverage=edge,indirect-calls "

build_all: clean_build create_folder build_old build_new

configure:
	cd $(ORIGIN_LIB_OLD_PATH); \
	./configure $(BUILD_FLAGS) --prefix=$(BUILD_LIB_OLD_PATH);\
	cd $(ORIGIN_LIB_NEW_PATH); \
	./configure $(BUILD_FLAGS) --prefix=$(BUILD_LIB_NEW_PATH);

create_folder:
	mkdir -p $(BUILD_LIB_OLD_PATH)
	mkdir -p $(BUILD_LIB_NEW_PATH)

build_old: 
	cd $(ORIGIN_LIB_OLD_PATH); \
	make -j8; \
	make install

build_new:
	cd $(ORIGIN_LIB_NEW_PATH); \
	make -j8; \
	make install

clean_build:
	rm -rf $(BUILD_LIB_OLD_PATH)
	rm -rf $(BUILD_LIB_NEW_PATH)


FLAGS := -O2 -Wall -g -ggdb3 -fsanitize=address,fuzzer-no-link 
TESTSAN_FLAGS := -fsanitize=testsan -g -O0
LDFLAGS := -O2 -rdynamic -shared 
COMMONFLAGS := -DMAGICKCORE_HDRI_ENABLE=1 -DMAGICKCORE_QUANTUM_DEPTH=16

OLD_INC_PATH = -I$(BUILD_LIB_OLD_PATH)include/GraphicsMagick
OLD_LIB_PATH = -L$(BUILD_LIB_OLD_PATH)lib
OLD_DYN_LIBS = -Wl,-Bdynamic,-rpath=$(BUILD_LIB_OLD_PATH)lib

NEW_INC_PATH = -I$(BUILD_LIB_NEW_PATH)include/GraphicsMagick
NEW_LIB_PATH = -L$(BUILD_LIB_NEW_PATH)lib
NEW_DYN_LIBS = -Wl,-Bdynamic,-rpath=$(BUILD_LIB_NEW_PATH)lib

LIBS :=  -lGraphicsMagick++ -lGraphicsMagick -lSM -lICE -lX11 -llzma -lz -lm -lpthread 

genlib: parseGM.cpp
	$(CXX) $(OLD_INC_PATH) ${FLAGS} -fPIC -c parseGM.cpp -o parseGM_old.o
	$(CXX) $(LDFLAGS) $(OLD_LIB_PATH) $(OLD_DYN_LIBS) parseGM_old.o -o libGM_oldest.so $(LIBS)
	$(CXX) $(FLAGS) $(NEW_INC_PATH) -fPIC -c -o parseGM_new.o parseGM.cpp
	$(CXX) $(LDFLAGS) $(NEW_LIB_PATH) $(NEW_DYN_LIBS) parseGM_new.o -o libGM_newest.so $(LIBS)
	cp $(ROOT_PATH)/libs/build_GM_libs/*.so $(ROOT_PATH)/libs/

# OLD_INC_PATH_RE = -I ./build_GM_libs/GraphicsMagick_oldest/include/GraphicsMagick
# OLD_LIB_PATH_RE = -L ./build_GM_libs/GraphicsMagick_oldest/lib
OLD_DYN_LIBS_RE = -Wl,-Bdynamic,-rpath=/workspace/libs/build_GM_libs/GraphicsMagick_oldest_lib/lib

# NEW_INC_PATH_RE = -I ./build_GM_libs/GraphicsMagick_newest/include/GraphicsMagick
# NEW_LIB_PATH_RE = -L ./build_GM_libs/GraphicsMagick_newest/lib
NEW_DYN_LIBS_RE = -Wl,-Bdynamic,-rpath=/workspace/libs/build_GM_libs/GraphicsMagick_newest_lib/lib

genlib_relative: parseGM.cpp
	$(CXX) $(OLD_INC_PATH) ${FLAGS} -fPIC -c parseGM.cpp -o parseGM_old.o
	$(CXX) $(LDFLAGS) $(OLD_LIB_PATH) $(OLD_DYN_LIBS_RE) parseGM_old.o -o libGM_oldest_re.so $(LIBS)
	$(CXX) $(FLAGS) $(NEW_INC_PATH) -fPIC -c -o parseGM_new.o parseGM.cpp
	$(CXX) $(LDFLAGS) $(NEW_LIB_PATH) $(NEW_DYN_LIBS_RE) parseGM_new.o -o libGM_newest_re.so $(LIBS)
	cp $(ROOT_PATH)/libs/build_GM_libs/*.so $(ROOT_PATH)/libs/


##############################################################################
BUILD_BASE_PATH := /home/xjy/fuzz/else/fuzz_project/build
INSTALL_LIB_PATH_THREAD = $(BUILD_BASE_PATH)/GM_$(COMMIT_HASH)/install/usr/local
INC_PATH_THREAD = -I$(INSTALL_LIB_PATH_THREAD)/include/GraphicsMagick
LIB_PATH_THREAD = -L$(INSTALL_LIB_PATH_THREAD)/lib
DYN_LIBS_THREAD = -Wl,-Bdynamic,-rpath=$(INSTALL_LIB_PATH_THREAD)/lib

genlib_thread: parseGM.cpp
	$(CXX) $(FLAGS) $(COMMONFLAGS) $(INC_PATH_THREAD) -fPIC -c -o parseGM_$(COMMIT_HASH).o parseGM.cpp
	$(CXX) $(LDFLAGS) $(LIB_PATH_THREAD) $(DYN_LIBS_THREAD) parseGM_$(COMMIT_HASH).o -o libGM_$(COMMIT_HASH).so $(LIBS)

DYN_LIBS_DOCKER = -Wl,-Bdynamic,-rpath=/app/GM/install/usr/local/lib
genlib_docker: parseGM.cpp
	$(CXX) $(FLAGS) $(COMMONFLAGS) $(INC_PATH_THREAD) -fPIC -c -o parseGM_$(COMMIT_HASH).o parseGM.cpp
	$(CXX) $(LDFLAGS) $(LIB_PATH_THREAD) $(DYN_LIBS_DOCKER) parseGM_$(COMMIT_HASH).o -o libGM_$(COMMIT_HASH)_docker.so $(LIBS)

INSTALL_LIB_PATH_COUNT=$(BUILD_BASE_PATH)/GM_count_$(COMMIT_HASH)/install/usr/local
INC_PATH_COUNT = -I$(INSTALL_LIB_PATH_COUNT)/include/GraphicsMagick
LIB_PATH_COUNT = -L$(INSTALL_LIB_PATH_COUNT)/lib
DYN_LIBS_COUNT = -Wl,-Bdynamic,-rpath=$(INSTALL_LIB_PATH_COUNT)/lib

genlib_count: parseGM.cpp
	$(CXX) $(TESTSAN_FLAGS) $(COMMONFLAGS) $(INC_PATH_COUNT) -fPIC -c -o parseGM_count.o parseGM.cpp
	$(CXX) $(LDFLAGS) $(LIB_PATH_COUNT) $(DYN_LIBS_COUNT) parseGM_count.o -o libGM_count.so $(LIBS)




clean_lib:
	rm -f *.so
	rm -f *.o

test: test.cpp
	$(CXX) -g -O2 -fsanitize=address,fuzzer-no-link test.cpp -o test -ldl

test_thread: test_thread.cpp
	$(CXX) -g -O2 -fsanitize=address,fuzzer-no-link test_thread.cpp -o test_thread -ldl
