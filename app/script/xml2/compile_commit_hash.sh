#!/usr/bin/env bash
# xml2 new:73514f2d
# xml2 old:f8a8c1f
set -e
commit_hash=$1

if [ -z "$commit_hash" ]; then
  echo "Usage: $0 <commit_hash>"
  exit 1
fi

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" &>/dev/null && pwd)

APP_PATH="${SCRIPT_DIR}/../.."
XML2_BUILD_PATH=$APP_PATH/build/xml2
XML2_SRC_PATH=$APP_PATH/src/xml2
XML2_INSTALL_PATH=$APP_PATH/build/xml2
XML2_LIB_PATH=$APP_PATH/libs/build_xml2_libs/

mkdir -p $XML2_BUILD_PATH/
rm -rf $XML2_BUILD_PATH/xml2_$commit_hash && rm -rf $XML2_INSTALL_PATH/$commit_hash
cp -r $XML2_SRC_PATH $XML2_BUILD_PATH/xml2_$commit_hash
cd $XML2_BUILD_PATH/xml2_$commit_hash
git reset --hard $commit_hash


CC="${SCRIPT_DIR}/../../compiler/target/release/libafl_cc"
CXX="${SCRIPT_DIR}/../../compiler/target/release/libafl_cxx"
CFLAGS="-fsanitize=address"
CXXFLAGS="-fsanitize=address"

cd $XML2_BUILD_PATH/xml2_$commit_hash
./autogen.sh
./configure CC="$CC" CXX="$CXX" CFLAGS="$CFLAGS" CXXFLAGS="$CXXFLAGS" --enable-static

make -j16
cd $XML2_INSTALL_PATH && mkdir -p $commit_hash
cd $XML2_INSTALL_PATH/xml2_$commit_hash
# cp $XML2_BUILD_PATH/xml2_$commit_hash $XML2_INSTALL_PATH/$commit_hash
make install DESTDIR=$XML2_INSTALL_PATH/$commit_hash

# cp -r $XML2_BUILD_PATH/xml2_$commit_hash/* $XML2_INSTALL_PATH/$commit_hash/usr/local
cp $XML2_BUILD_PATH/xml2_$commit_hash/libxml.h $XML2_INSTALL_PATH/$commit_hash/usr/local/include/libxml.h
cp $XML2_BUILD_PATH/xml2_$commit_hash/config.h $XML2_INSTALL_PATH/$commit_hash/usr/local/include/libxml.h

rm -rf $XML2_BUILD_PATH/xml2_$commit_hash

# cd $XML2_LIB_PATH
# make genlib_thread COMMIT_HASH=$commit_hash -B
# make genlib_docker COMMIT_HASH=$commit_hash -B
# make test_thread -B

