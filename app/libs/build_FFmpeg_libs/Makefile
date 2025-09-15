CC := /home/xjy/fuzz/else/llvm-project/build/bin/clang
CXX := /home/xjy/fuzz/else/llvm-project/build/bin/clang++
#---------------------------------------------------------
LIBS := -lavformat  -lavcodec -lavutil -lswresample -lpthread -ldl
#这里我去掉了-werror的参数 -Werror在FLAGS
FLAGS := -O2 -Wall -Wextra -Wno-unused  -g -ggdb3 -fPIC  -fsanitize=address,fuzzer-no-link 
LDFLAGS := -O2 -Wall -g -ggdb3 -fPIC -rdynamic -shared -fsanitize=address,fuzzer-no-link

# ###########################################
THREAD_BASE_PATH := /home/xjy/fuzz/else/fuzz_project/build
THREAD_INC_PATH = -I$(THREAD_BASE_PATH)/FFmpeg_$(COMMIT_HASH)/install/include 
THREAD_LIB_PATH = -L$(THREAD_BASE_PATH)/FFmpeg_$(COMMIT_HASH)/install/lib
THREAD_DYN_LIBS = -Wl,-rpath,$(THREAD_BASE_PATH)/FFmpeg_$(COMMIT_HASH)/install/lib
# 考虑到这里用了两个不同的parse.cpp文件,发现内容一致，所以改成一个了，相应的diff也做了修改。
genlib_thread: h264_parse.cpp
	$(CXX) $(THREAD_INC_PATH) $(FLAGS) -fpic -c h264_parse.cpp -o FFmpeg_$(COMMIT_HASH).o
	$(CXX) $(FLAGS) $(LDFLAGS) FFmpeg_$(COMMIT_HASH).o -o libFFmpeg_$(COMMIT_HASH).so $(THREAD_LIB_PATH)  $(THREAD_DYN_LIBS) $(LIBS)
DYN_LIBS_DOCKER = -Wl,-Bdynamic,-rpath=/app/FFmpeg/install/lib

genlib_docker: h264_parse.cpp
	$(CXX) $(THREAD_INC_PATH) $(FLAGS) -fpic -c h264_parse.cpp -o FFmpeg_$(COMMIT_HASH)_docker.o
	$(CXX) $(FLAGS) $(LDFLAGS) FFmpeg_$(COMMIT_HASH)_docker.o -o libFFmpeg_$(COMMIT_HASH)_docker.so $(THREAD_LIB_PATH)  $(DYN_LIBS_DOCKER) $(LIBS)

test_thread: test_thread.cpp
	$(CXX) -fsanitize=address,fuzzer-no-link -o test_thread test_thread.cpp

