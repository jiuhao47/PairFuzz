#!/bin/sh

set -e

commit_hash=$1

if [ -z "$commit_hash" ]; then
  echo "Usage: $0 <commit_hash>"
  exit 1
fi

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" &>/dev/null && pwd)

APP_PATH="${SCRIPT_DIR}/../.."
FREETYPE_BUILD_PATH=$APP_PATH/build/freetype
FREETYPE_SRC_PATH=$APP_PATH/src/freetype
FREETYPE_INSTALL_PATH=$APP_PATH/build/freetype
FREETYPE_LIB_PATH=$APP_PATH/libs/build_freetype_libs/

mkdir -p $FREETYPE_BUILD_PATH/
rm -rf $FREETYPE_BUILD_PATH/freetype_$commit_hash && rm -rf $FREETYPE_INSTALL_PATH/$commit_hash
cp -r $FREETYPE_SRC_PATH $FREETYPE_BUILD_PATH/freetype_$commit_hash
cd $FREETYPE_BUILD_PATH/freetype_$commit_hash
if [ "$commit_hash" = "dep" ]; then
  # Try to reset to the official 2.14.1 tag if available; otherwise leave repository as-is.
  if git rev-parse --verify VER-2-14-1 >/dev/null 2>&1; then
    git reset --hard VER-2-14-1
  elif git rev-parse --verify 2.14.1 >/dev/null 2>&1; then
    git reset --hard 2.14.1
  else
    echo "Tag VER-2-14-1 / 2.14.1 not found; leaving repo at current checked-out state for 'dep'"
  fi
else
  git reset --hard $commit_hash
fi



CC="${SCRIPT_DIR}/../../compiler/target/release/libafl_cc"
CXX="${SCRIPT_DIR}/../../compiler/target/release/libafl_cxx"
CFLAGS="-fsanitize=address -fPIC"
CXXFLAGS="-fsanitize=address -fPIC"

# Prefer the shipped configure script; if not executable, try autogen

./autogen.sh

# Configure with wrappers and sanitizer flags. Adjust options as needed.
./configure CC="$CC" CXX="$CXX" CFLAGS="$CFLAGS" CXXFLAGS="$CXXFLAGS"

make -j 16


cd $FREETYPE_INSTALL_PATH && mkdir -p $commit_hash
cd $FREETYPE_INSTALL_PATH/freetype_$commit_hash || true
# Install into a commit-specific DESTDIR
make install DESTDIR=$FREETYPE_INSTALL_PATH/$commit_hash || true
rm -rf $FREETYPE_BUILD_PATH/freetype_$commit_hash

# Optional: hooks to build packaged libs (uncomment and implement Makefile targets if present)
# cd $FREETYPE_LIB_PATH
# make genlib_thread COMMIT_HASH=$commit_hash
# make genlib_docker COMMIT_HASH=$commit_hash
# make test_thread
