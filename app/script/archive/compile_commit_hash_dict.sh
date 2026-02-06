#!/usr/bin/env bash
set -e
# archive new: f8a34abf
# archive old: 614110e

# git clone  https://github.com/libarchive/libarchive archive
commit_hash=$1

if [ -z "$commit_hash" ]; then
  echo "Usage: $0 <commit_hash>"
  exit 1
fi


SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" &>/dev/null && pwd)

APP_PATH="${SCRIPT_DIR}/../.."
archive_BUILD_PATH=$APP_PATH/build/archive_dict
archive_SRC_PATH=$APP_PATH/src/archive
archive_INSTALL_PATH=$APP_PATH/build/archive_dict
archive_LIB_PATH=$APP_PATH/libs/build_archive_libs/



mkdir -p $archive_BUILD_PATH/
rm -rf $archive_BUILD_PATH/archive_$commit_hash && rm -rf $archive_INSTALL_PATH/$commit_hash
cp -r $archive_SRC_PATH $archive_BUILD_PATH/archive_$commit_hash
cd $archive_BUILD_PATH/archive_$commit_hash
git reset --hard $commit_hash
python3 $SCRIPT_DIR/patch_checksum.py $archive_BUILD_PATH/archive_$commit_hash

CC="${SCRIPT_DIR}/../../../fuzzers/AFLplusplus/afl-clang-lto"
CXX="${SCRIPT_DIR}/../../../fuzzers/AFLplusplus/afl-clang-lto++"
CFLAGS="-fsanitize=address -fPIC"
CXXFLAGS="-fsanitize=address -fPIC"

cd $archive_BUILD_PATH/archive_$commit_hash

./build/autogen.sh
./configure  CXX=$CXX CC=$CC CFLAGS="$CFLAGS" CXXFLAGS="$CXXFLAGS"

rm -f dict_${commit_hash}.txt
export AFL_LLVM_DICT2FILE="${SCRIPT_DIR}/dict_${commit_hash}.txt"

make -j16


cd $archive_INSTALL_PATH && mkdir -p $commit_hash
cd $archive_INSTALL_PATH/archive_$commit_hash
make install DESTDIR=$archive_INSTALL_PATH/$commit_hash
rm -rf $archive_BUILD_PATH/archive_$commit_hash



# cd $archive_LIB_PATH
# make genlib_thread COMMIT_HASH=$commit_hash -B
# make genlib_docker COMMIT_HASH=$commit_hash -B
# make test_thread -B



