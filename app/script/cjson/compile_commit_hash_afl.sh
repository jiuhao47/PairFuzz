#!/usr/bin/env bash
# #   - libcjson_newest.so: 12c4bf1
# #   - libcjson_oldest.so: 174c629

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
CJSON_BUILD_PATH=$APP_PATH/build/cjson_afl
CJSON_SRC_PATH=$APP_PATH/src/cjson
CJSON_INSTALL_PATH=$APP_PATH/build/cjson_afl
CJSON_LIB_PATH=$APP_PATH/libs/build_cjson_libs/

mkdir -p $CJSON_BUILD_PATH/
rm -rf $CJSON_BUILD_PATH/cjson_$commit_hash && rm -rf $CJSON_INSTALL_PATH/$commit_hash
cp -r $CJSON_SRC_PATH $CJSON_BUILD_PATH/cjson_$commit_hash
cd $CJSON_BUILD_PATH/cjson_$commit_hash
git reset --hard $commit_hash


CC="${SCRIPT_DIR}/../../../fuzzers/AFL/afl-clang-fast"
CXX="${SCRIPT_DIR}/../../../fuzzers/AFL/afl-clang-fast++"
CFLAGS="-fsanitize=address -fPIC"
CXXFLAGS="-fsanitize=address -fPIC"

# If the libafl wrapper compilers are not available, fall back to system compilers
if [ ! -x "$CC" ]; then
  echo "Warning: $CC not found or not executable — falling back to system 'cc'"
  CC="cc"
fi
if [ ! -x "$CXX" ]; then
  echo "Warning: $CXX not found or not executable — falling back to system 'c++'"
  CXX="c++"
fi

# Basic tool checks
command -v "$CC" >/dev/null 2>&1 || { echo "Error: C compiler '$CC' not found in PATH."; exit 1; }
command -v ar >/dev/null 2>&1 || { echo "Error: 'ar' (binutils) not found in PATH."; exit 1; }

# $CC -fsanitize=address  -I./ -DMJSON_ENABLE_MERGE=1 -DMJSON_ENABLE_PRETTY=1 ./cJSON.c  -c -fPIC -o cjson.o 
# $CC -fsanitize=address  -I./ -DMJSON_ENABLE_MERGE=1 -DMJSON_ENABLE_PRETTY=1  ./cJSON_Utils.c -c -fPIC -o cjson_utils.o 
# $CC -rdynamic -shared cjson.o cjson_utils.o -o libcjson.so
# ar rcs libcjson.a cjson.o cjson_utils.o
# mkdir -p build
# cd build
if command -v cmake >/dev/null 2>&1 && [ -f CMakeLists.txt ]; then
  cmake . -D CMAKE_C_COMPILER=$CC -D CMAKE_CXX_COMPILER=$CXX -DCMAKE_C_FLAGS="$CFLAGS" -DCMAKE_CXX_FLAGS="$CXXFLAGS" -DCMAKE_POLICY_VERSION_MINIMUM=3.5 -DBUILD_SHARED_AND_STATIC_LIBS=On
  make -j8

  cd $CJSON_INSTALL_PATH && mkdir -p $commit_hash
  cd $CJSON_INSTALL_PATH/cjson_$commit_hash
  make install DESTDIR=$CJSON_INSTALL_PATH/$commit_hash
  rm -rf $CJSON_BUILD_PATH/cjson_$commit_hash
else
  echo "cmake not found; falling back to manual build as per README"
  # compile sources manually to produce shared and static libs
  objs=""
  if [ -f ./cJSON.c ]; then
    $CC $CFLAGS -I. -DMJSON_ENABLE_MERGE=1 -DMJSON_ENABLE_PRETTY=1 -c -fPIC ./cJSON.c -o cjson.o
    objs="$objs cjson.o"
  fi
  if [ -f ./cJSON_Utils.c ]; then
    $CC $CFLAGS -I. -DMJSON_ENABLE_MERGE=1 -DMJSON_ENABLE_PRETTY=1 -c -fPIC ./cJSON_Utils.c -o cjson_utils.o
    objs="$objs cjson_utils.o"
  fi

  # produce shared library
  # Use -Wl,-soname if available to set soname; keep simple for compatibility
  $CC -rdynamic -shared $objs -o libcjson.so || { echo "Failed to build shared library"; exit 1; }
  # produce static archive
  ar rcs libcjson.a $objs || { echo "Failed to create static archive"; exit 1; }

  # create DESTDIR layout and copy artifacts
  INSTALLDIR="$CJSON_INSTALL_PATH/$commit_hash"
  mkdir -p "$INSTALLDIR/usr/local/lib" "$INSTALLDIR/usr/local/include"
  cp -f libcjson.so libcjson.a "$INSTALLDIR/usr/local/lib/" || { echo "Failed to copy libs to $INSTALLDIR/usr/local/lib"; exit 1; }
  if [ -f ./cJSON.h ]; then
    cp -f ./cJSON.h "$INSTALLDIR/usr/local/include/" || { echo "Failed to copy header to $INSTALLDIR/usr/local/include"; exit 1; }
  else
    echo "Warning: cJSON.h not found in source; header not installed."
  fi

  # cleanup build directory
  rm -rf $CJSON_BUILD_PATH/cjson_$commit_hash
fi
# mkdir -p ../install


# mkdir -p ../install
# make install DESTDIR=$CJSON_INSTALL_PATH/cjson_$commit_hash/install

# cd $CJSON_LIB_PATH
# make genlib_thread COMMIT_HASH=$commit_hash -B
# make genlib_thread_docker COMMIT_HASH=$commit_hash -B
# make test_thread -B
