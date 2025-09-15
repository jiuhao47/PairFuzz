#!/bin/sh
# openh264 new: 042473ea
# openh264 old: 2610ab1

commit_hash=$1

if [ -z "$commit_hash" ]; then
  echo "Usage: $0 <commit_hash>"
  exit 1
fi

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" &>/dev/null && pwd)

APP_PATH="${SCRIPT_DIR}/../.."
OPENH264_BUILD_PATH=$APP_PATH/build/openh264
OPENH264_SRC_PATH=$APP_PATH/src/openh264
OPENH264_INSTALL_PATH=$APP_PATH/build/openh264
OPENH264_LIB_PATH=$APP_PATH/libs/build_openh264_libs/

mkdir -p $OPENH264_BUILD_PATH/
rm -rf $OPENH264_BUILD_PATH/openh264_$commit_hash && rm -rf $OPENH264_INSTALL_PATH/$commit_hash
cp -r $OPENH264_SRC_PATH $OPENH264_BUILD_PATH/openh264_$commit_hash
cd $OPENH264_BUILD_PATH/openh264_$commit_hash
git reset --hard $commit_hash

CC="${SCRIPT_DIR}/../../compiler/target/release/libafl_cc"
CXX="${SCRIPT_DIR}/../../compiler/target/release/libafl_cxx"
CFLAGS="-fsanitize=address"
CXXFLAGS="-fsanitize=address"

cd $OPENH264_BUILD_PATH/openh264_$commit_hash
CXX=$CXX CC=$CC CFLAGS=$CFLAGS CXXFLAGS=$CXXFLAGS LDFLAGS=$CXXFLAGS make OS=linux ARCH=x86_64 -j16
cd $OPENH264_INSTALL_PATH && mkdir -p $commit_hash
cd $OPENH264_INSTALL_PATH/openh264_$commit_hash
make install DESTDIR=$OPENH264_INSTALL_PATH/$commit_hash
cp $OPENH264_BUILD_PATH/openh264_$commit_hash/codec/console/common/inc/* $OPENH264_INSTALL_PATH/$commit_hash/usr/local/include/
cp $OPENH264_BUILD_PATH/openh264_$commit_hash/codec/common/inc/* $OPENH264_INSTALL_PATH/$commit_hash/usr/local/include/
cp -r $OPENH264_BUILD_PATH/openh264_$commit_hash/codec/api/**/* $OPENH264_INSTALL_PATH/$commit_hash/usr/local/include/
rm -rf $OPENH264_BUILD_PATH/openh264_$commit_hash


# cd $OPENH264_LIB_PATH
# make genlib_thread COMMIT_HASH=$commit_hash -B
# make genlib_docker COMMIT_HASH=$commit_hash -B
# make test_thread -B


