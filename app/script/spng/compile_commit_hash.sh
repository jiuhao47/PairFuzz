#!/bin/sh

commit_hash=$1

set -e


if [ -z "$commit_hash" ]; then
  echo "Usage: $0 <commit_hash>"
  exit 1
fi


SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" &>/dev/null && pwd)

APP_PATH="${SCRIPT_DIR}/../.."
SPNG_BUILD_PATH=$APP_PATH/build/spng
SPNG_SRC_PATH=$APP_PATH/src/spng
SPNG_INSTALL_PATH=$APP_PATH/build/spng
SPNG_LIB_PATH=$APP_PATH/libs/build_spng_libs/

mkdir -p $SPNG_BUILD_PATH/
rm -rf $SPNG_BUILD_PATH/spng_$commit_hash && rm -rf $SPNG_INSTALL_PATH/$commit_hash
cp -r $SPNG_SRC_PATH $SPNG_BUILD_PATH/spng_$commit_hash
cd $SPNG_BUILD_PATH/spng_$commit_hash
git reset --hard $commit_hash


CC="${SCRIPT_DIR}/../../compiler/target/release/libafl_cc"
CXX="${SCRIPT_DIR}/../../compiler/target/release/libafl_cxx"
CFLAGS="-fsanitize=address"
CXXFLAGS="-fsanitize=address"

# cmake build for spng
mkdir build
cd build
cmake .. -DCMAKE_C_COMPILER="$CC" -DCMAKE_CXX_COMPILER="$CXX" -DCMAKE_C_FLAGS="$CFLAGS" -DCMAKE_CXX_FLAGS="$CXXFLAGS" -DCMAKE_POLICY_VERSION_MINIMUM=3.5
make -j 16

cp $SPNG_BUILD_PATH/spng_$commit_hash/src/spng.h $SPNG_BUILD_PATH/spng_$commit_hash/spng.h || cp $SPNG_BUILD_PATH/spng_$commit_hash/spng/spng.h $SPNG_BUILD_PATH/spng_$commit_hash/spng.h


cd $SPNG_INSTALL_PATH && mkdir -p $commit_hash
cd $SPNG_BUILD_PATH/spng_$commit_hash/build
make install DESTDIR=$SPNG_INSTALL_PATH/$commit_hash
cp $SPNG_BUILD_PATH/spng_$commit_hash/build/libspng_static.a $SPNG_INSTALL_PATH/$commit_hash/usr/local/lib/libspng_static.a

rm -rf $SPNG_BUILD_PATH/spng_$commit_hash

cd $SPNG_LIB_PATH
make genlib_thread COMMIT_HASH=$commit_hash
make genlib_docker COMMIT_HASH=$commit_hash
make test_thread
