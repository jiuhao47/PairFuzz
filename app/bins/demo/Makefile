all: test.c
	# compile to a shared library
	gcc -shared -fPIC -o libtest.so test.c
	/home/user/workspace/diffray/AFLplusplus/afl-clang-fast -o test ./main.c 
	/home/user/workspace/diffray/AFL/afl-clang-fast -o test_afl ./main.c 