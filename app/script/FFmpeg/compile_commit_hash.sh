#!/bin/sh
# FFmpeg new:c88614662c
# FFmpeg old:4521700

commit_hash=$1

if [ -z "$commit_hash" ]; then
  echo "Usage: $0 <commit_hash>"
  exit 1
fi

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" &>/dev/null && pwd)

APP_PATH="${SCRIPT_DIR}/../.."
FFMPEG_BUILD_PATH=$APP_PATH/build/FFmpeg
FFMPEG_SRC_PATH=$APP_PATH/src/FFmpeg
FFMPEG_INSTALL_PATH=$APP_PATH/build/FFmpeg
FFMPEG_LIB_PATH=$APP_PATH/libs/build_FFmpeg_libs/

mkdir -p $FFMPEG_BUILD_PATH/
rm -rf $FFMPEG_BUILD_PATH/FFmpeg_$commit_hash && rm -rf $FFmpeg_INSTALL_PATH/$commit_hash
cp -r $FFMPEG_SRC_PATH $FFMPEG_BUILD_PATH/FFmpeg_$commit_hash
cd $FFMPEG_BUILD_PATH/FFmpeg_$commit_hash
git reset --hard $commit_hash

CC="${SCRIPT_DIR}/../../compiler/target/release/libafl_cc"
CXX="${SCRIPT_DIR}/../../compiler/target/release/libafl_cxx"
CFLAGS="-fsanitize=address"
CXXFLAGS="-fsanitize=address"

cd $FFMPEG_BUILD_PATH/FFmpeg_$commit_hash

CXXFLAGS=$CXX CFLAGS=$CC  ./configure  --cxx=$CXX --cc=$CC 
# --prefix=$FFMPEG_INSTALL_PATH/FFmpeg_$commit_hash/install

make -j16
# mkdir -p install
cd $FFMPEG_INSTALL_PATH && mkdir -p $commit_hash
cd $FFMPEG_INSTALL_PATH/FFmpeg_$commit_hash
make install DESTDIR=$FFMPEG_INSTALL_PATH/$commit_hash
rm -rf $FFMPEG_BUILD_PATH/FFmpeg_$commit_hash

# cd $FFMPEG_LIB_PATH
# make genlib_thread COMMIT_HASH=$commit_hash -B
# make genlib_docker COMMIT_HASH=$commit_hash -B
# make test_thread -B


