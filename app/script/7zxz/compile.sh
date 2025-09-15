#!/bin/bash
commit_hash=$1

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" &>/dev/null && pwd)

APP_PATH="${SCRIPT_DIR}/../.."
XZ7Z_BUILD_PATH=$APP_PATH/build/7zxz
XZ7Z_SRC_PATH=$APP_PATH/src/xz_7znewest
XZ7Z_LIB_PATH=$APP_PATH/libs/build_7zxz_libs/

rm -rf $XZ7Z_BUILD_PATH/7zxz_count
mkdir -p $XZ7Z_BUILD_PATH/7zxz_count/src_to_build/
cd $XZ7Z_BUILD_PATH/7zxz_count

mkdir -p $XZ7Z_BUILD_PATH/7zxz_count/src_to_build/output


cp -r $XZ7Z_SRC_PATH/* ./src_to_build

cp $XZ7Z_LIB_PATH/Makefile.build ./src_to_build/Makefile


cd $XZ7Z_BUILD_PATH/7zxz_count/src_to_build/
CC="${SCRIPT_DIR}/../../compiler/target/release/libafl_cc"
CXX="${SCRIPT_DIR}/../../compiler/target/release/libafl_cxx"
CFLAGS="-fsanitize=address"
CXXFLAGS="-fsanitize=address"
mkdir $XZ7Z_BUILD_PATH/install
mkdir $XZ7Z_BUILD_PATH/include
make clean
make all CC=$CC CXX=$CXX CFLAGS=$CFLAGS COMMIT_HASH=$commit_hash
cp $XZ7Z_BUILD_PATH/7zxz_count/src_to_build/lib7zxz_.a $XZ7Z_BUILD_PATH/install/lib7zxz.a
cp -r $XZ7Z_SRC_PATH/* $XZ7Z_BUILD_PATH/include/
rm -rf $XZ7Z_BUILD_PATH/7zxz_count

