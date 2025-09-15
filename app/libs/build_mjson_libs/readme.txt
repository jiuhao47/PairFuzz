
### version
newest: b766f34
oldest: 6b0367

### compile 
clang -fsanitize=address,fuzzer-no-link  -I./src/ -DMJSON_ENABLE_MERGE=1 -DMJSON_ENABLE_PRETTY=1 ./src/mjson.c -c -fPIC -o mjson.o
clang -rdynamic -shared mjson.o -o libmjson.so
