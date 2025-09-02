#!/bin/sh

# #   - libcjson_newest.so: 12c4bf1
# #   - libcjson_oldest.so: 174c629
commit_hash=$1

if [ -z "$commit_hash" ]; then
  echo "Usage: $0 <commit_hash>"
  exit 1
fi

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" &>/dev/null && pwd)

APP_PATH="${SCRIPT_DIR}/../.."
CJSON_BUILD_PATH=$APP_PATH/build/cjson
CJSON_SRC_PATH=$APP_PATH/src/cjson
CJSON_INSTALL_PATH=$APP_PATH/build/cjson
CJSON_LIB_PATH=$APP_PATH/libs/build_cjson_libs/

mkdir -p $CJSON_BUILD_PATH/
rm -rf $CJSON_BUILD_PATH/cjson_$commit_hash && rm -rf $CJSON_INSTALL_PATH/$commit_hash
cp -r $CJSON_SRC_PATH $CJSON_BUILD_PATH/cjson_$commit_hash
cd $CJSON_BUILD_PATH/cjson_$commit_hash
git reset --hard $commit_hash


CC="${SCRIPT_DIR}/../../compiler/target/release/libafl_cc"
CXX="${SCRIPT_DIR}/../../compiler/target/release/libafl_cxx"
CFLAGS="-fsanitize=address"
CXXFLAGS="-fsanitize=address"

$CC -fsanitize=address  -I./ -DMJSON_ENABLE_MERGE=1 -DMJSON_ENABLE_PRETTY=1 ./cJSON.c  -c -fPIC -o cjson.o 
$CC -fsanitize=address  -I./ -DMJSON_ENABLE_MERGE=1 -DMJSON_ENABLE_PRETTY=1  ./cJSON_Utils.c -c -fPIC -o cjson_utils.o 
# $CC -rdynamic -shared cjson.o cjson_utils.o -o libcjson.so
ar rcs libcjson.a cjson.o cjson_utils.o
mkdir -p build
cd build
cmake .. -D CMAKE_C_COMPILER=$CC -D CMAKE_CXX_COMPILER=$CXX -DCMAKE_C_FLAGS="-O0 -fsanitize=address,fuzzer-no-link -fsanitize-coverage=edge,indirect-calls -fPIC " -DCMAKE_CXX_FLAGS=" -O0 -fsanitize=address,fuzzer-no-link -fsanitize-coverage=edge,indirect-calls -fPIC "
make -j8


cd $CJSON_INSTALL_PATH && mkdir -p $commit_hash
cd $CJSON_INSTALL_PATH/cjson_$commit_hash
make install DESTDIR=$CJSON_INSTALL_PATH/$commit_hash

cd $CJSON_BUILD_PATH/cjson_$commit_hash
cp $CJSON_BUILD_PATH/cjson_$commit_hash/* $CJSON_INSTALL_PATH/$commit_hash/
cp libcjson.a $CJSON_INSTALL_PATH/$commit_hash/
rm -rf $CJSON_BUILD_PATH/cjson_$commit_hash
# mkdir -p ../install


# mkdir -p ../install
# make install DESTDIR=$CJSON_INSTALL_PATH/cjson_$commit_hash/install

# cd $CJSON_LIB_PATH
# make genlib_thread COMMIT_HASH=$commit_hash -B
# make genlib_thread_docker COMMIT_HASH=$commit_hash -B
# make test_thread -B
