CC := clang
CXX := clang++

all: test

test: test.cpp
	$(CXX) -fsanitize=address,fuzzer-no-link -o test test.cpp

test_thread: test_thread.cpp
	$(CXX) -fsanitize=address,fuzzer-no-link -o test_thread test_thread.cpp

clean:
	rm -f test

