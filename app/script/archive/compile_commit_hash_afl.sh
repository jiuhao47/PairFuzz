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

export PATH="/opt/clang+llvm-11.0.0/bin:$PATH"
# check if clang version is 11 (robust extraction of the major version number)
CLANG_VERSION=$(clang --version | head -n 1 | grep -oE '[0-9]+' | head -n1 || true)
if ! [[ "$CLANG_VERSION" =~ ^[0-9]+$ ]]; then
  echo "Error: unable to parse clang version: $CLANG_VERSION"
  exit 1
fi
if [ "$CLANG_VERSION" -ne 11 ]; then
  echo "Error: clang version 11 is required. Current version is $CLANG_VERSION"
  exit 1
fi

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" &>/dev/null && pwd)

APP_PATH="${SCRIPT_DIR}/../.."
archive_BUILD_PATH=$APP_PATH/build/archive_afl
archive_SRC_PATH=$APP_PATH/src/archive
archive_INSTALL_PATH=$APP_PATH/build/archive_afl
archive_LIB_PATH=$APP_PATH/libs/build_archive_libs/



mkdir -p $archive_BUILD_PATH/
rm -rf $archive_BUILD_PATH/archive_$commit_hash && rm -rf $archive_INSTALL_PATH/$commit_hash
cp -r $archive_SRC_PATH $archive_BUILD_PATH/archive_$commit_hash
cd $archive_BUILD_PATH/archive_$commit_hash
git reset --hard $commit_hash
python3 $SCRIPT_DIR/patch_checksum.py $archive_BUILD_PATH/archive_$commit_hash

CC="${SCRIPT_DIR}/../../../AFL/afl-clang-fast"
CXX="${SCRIPT_DIR}/../../../AFL/afl-clang-fast++"
CFLAGS="-fsanitize=address -fPIC"
CXXFLAGS="-fsanitize=address -fPIC"

cd $archive_BUILD_PATH/archive_$commit_hash

./build/autogen.sh
./configure  CXX=$CXX CC=$CC CFLAGS="$CFLAGS" CXXFLAGS="$CXXFLAGS"

make -j16


cd $archive_INSTALL_PATH && mkdir -p $commit_hash
cd $archive_INSTALL_PATH/archive_$commit_hash
make install DESTDIR=$archive_INSTALL_PATH/$commit_hash
rm -rf $archive_BUILD_PATH/archive_$commit_hash



# cd $archive_LIB_PATH
# make genlib_thread COMMIT_HASH=$commit_hash -B
# make genlib_docker COMMIT_HASH=$commit_hash -B
# make test_thread -B



