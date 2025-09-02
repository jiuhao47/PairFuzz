#!/bin/sh
# #   - libmjson_newest.so: b766f34
# #   - libmjson_oldest.so: 6b0367
commit_hash=$1

if [ -z "$commit_hash" ]; then
  echo "Usage: $0 <commit_hash>"
  exit 1
fi

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" &>/dev/null && pwd)

APP_PATH="${SCRIPT_DIR}/../.."
MJSON_BUILD_PATH=$APP_PATH/build/mjson
MJSON_SRC_PATH=$APP_PATH/src/mjson
MJSON_INSTALL_PATH=$APP_PATH/build/mjson
MJSON_LIB_PATH=$APP_PATH/libs/build_mjson_libs/

mkdir -p $MJSON_BUILD_PATH/
rm -rf $MJSON_BUILD_PATH/mjson_$commit_hash && rm -rf $MJSON_INSTALL_PATH/$commit_hash
cp -r $MJSON_SRC_PATH $MJSON_BUILD_PATH/mjson_$commit_hash
cd $MJSON_BUILD_PATH/mjson_$commit_hash
git reset --hard $commit_hash


CC="${SCRIPT_DIR}/../../compiler/target/release/libafl_cc"
CXX="${SCRIPT_DIR}/../../compiler/target/release/libafl_cxx"
CFLAGS="-fsanitize=address"
CXXFLAGS="-fsanitize=address"

$CC -fsanitize=address  -I./src/ -DMJSON_ENABLE_MERGE=1 -DMJSON_ENABLE_PRETTY=1 ./src/mjson.c -c -fPIC -o mjson.o
# $CC -rdynamic -shared mjson.o -o libmjson.so
# 使用ar命令创建静态库
ar rcs libmjson.a mjson.o

cd $MJSON_INSTALL_PATH && mkdir -p $commit_hash
cd $MJSON_BUILD_PATH/mjson_$commit_hash
cp libmjson.a $MJSON_INSTALL_PATH/$commit_hash/
cp ./src/* $MJSON_INSTALL_PATH/$commit_hash/
cp ./* $MJSON_INSTALL_PATH/$commit_hash/
rm -rf $MJSON_BUILD_PATH/mjson_$commit_hash

# mkdir build
# cd build
# cmake .. -DCMAKE_C_COMPILER=$CC -DCMAKE_CXX_COMPILER=$CXX -DCMAKE_C_FLAGS="-O0 -fsanitize=address,fuzzer-no-link -fsanitize-coverage=edge,indirect-calls -fPIC" -DCMAKE_CXX_FLAGS="-O0 -fsanitize=address,fuzzer-no-link -fsanitize-coverage=edge,indirect-calls -fPIC"
# make -j8

# mkdir -p ../install
# make install DESTDIR=$MJSON_INSTALL_PATH/mjson_$commit_hash/install

# cd $MJSON_LIB_PATH
# make genlib_thread COMMIT_HASH=$commit_hash -B
# make genlib_docker COMMIT_HASH=$commit_hash -B
# make test_thread -B
