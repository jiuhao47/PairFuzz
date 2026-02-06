#!/usr/bin/env bash
# xml2 new:73514f2d
# xml2 old:f8a8c1f
set -e
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
XML2_BUILD_PATH=$APP_PATH/build/xml2_dict
XML2_SRC_PATH=$APP_PATH/src/libxml2
XML2_INSTALL_PATH=$APP_PATH/build/xml2_dict
XML2_LIB_PATH=$APP_PATH/libs/build_xml2_libs/

mkdir -p $XML2_BUILD_PATH/
rm -rf $XML2_BUILD_PATH/xml2_$commit_hash && rm -rf $XML2_INSTALL_PATH/$commit_hash
cp -r $XML2_SRC_PATH $XML2_BUILD_PATH/xml2_$commit_hash
cd $XML2_BUILD_PATH/xml2_$commit_hash
if [ "$commit_hash" = "dep" ]; then
  git reset --hard cdd2575
  # echo "commit_hash is 'dep' — skipping git reset (using dependency snapshot)"
else
  git reset --hard $commit_hash
fi


CC="${SCRIPT_DIR}/../../../fuzzers/AFLplusplus/afl-clang-lto"
CXX="${SCRIPT_DIR}/../../../fuzzers/AFLplusplus/afl-clang-lto++"
CFLAGS="-fsanitize=address -fPIC"
CXXFLAGS="-fsanitize=address -fPIC"

cd $XML2_BUILD_PATH/xml2_$commit_hash
./autogen.sh
if [ "$commit_hash" = "dep" ]; then
  ./configure CC="$CC" CXX="$CXX" CFLAGS="$CFLAGS" CXXFLAGS="$CXXFLAGS" --enable-static --with-http --with-ftp
else
  ./configure CC="$CC" CXX="$CXX" CFLAGS="$CFLAGS" CXXFLAGS="$CXXFLAGS" --enable-static
fi
# Workaround: disable version script in generated Makefile to avoid
# linker failures with lld when the version script references symbols
# that are not present in this build (e.g., optional modules).
# This is safe for local/instrumented builds (fuzzing) where strict
# symbol versioning isn't required.
if [ -f Makefile ]; then
  if grep -q "^LIBXML2_VERSION_SCRIPT" Makefile; then
    sed -i 's/^LIBXML2_VERSION_SCRIPT =.*$/LIBXML2_VERSION_SCRIPT =/' Makefile
  fi
fi

rm -f dict_${commit_hash}.txt
export AFL_LLVM_DICT2FILE="${SCRIPT_DIR}/dict_${commit_hash}.txt"

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

