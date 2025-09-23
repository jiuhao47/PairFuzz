#!/usr/bin/env bash
set -euo pipefail

# Simplified mjson compile/install script
# Produces libmjson.so + libmjson.a and installs into build/mjson/<commit>/usr/local

commit_hash=${1:-}
if [ -z "$commit_hash" ]; then
  echo "Usage: $0 <commit_hash>"
  exit 1
fi

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" &>/dev/null && pwd)
APP_PATH="$SCRIPT_DIR/../.."
MJSON_BUILD_PATH=$APP_PATH/build/mjson
MJSON_SRC_PATH=$APP_PATH/src/mjson
MJSON_INSTALL_PATH=$APP_PATH/build/mjson

mkdir -p "$MJSON_BUILD_PATH"
rm -rf "$MJSON_BUILD_PATH/mjson_$commit_hash" "$MJSON_INSTALL_PATH/$commit_hash"
cp -r "$MJSON_SRC_PATH" "$MJSON_BUILD_PATH/mjson_$commit_hash"
cd "$MJSON_BUILD_PATH/mjson_$commit_hash"
git reset --hard "$commit_hash"

# compilers and flags (ASan enabled by default)
CC="$SCRIPT_DIR/../../compiler/target/release/libafl_cc"
CXX="$SCRIPT_DIR/../../compiler/target/release/libafl_cxx"
CFLAGS="-fsanitize=address -fPIC"
CXXFLAGS="-fsanitize=address -fPIC"

if [ ! -x "$CC" ]; then
  echo "Warning: $CC not found — falling back to 'cc'"
  CC=cc
fi
if [ ! -x "$CXX" ]; then
  echo "Warning: $CXX not found — falling back to 'c++'"
  CXX=c++
fi

command -v "$CC" >/dev/null 2>&1 || { echo "C compiler '$CC' not found"; exit 1; }
command -v ar >/dev/null 2>&1 || { echo "Required tool 'ar' not found"; exit 1; }

INSTALLDIR="$MJSON_INSTALL_PATH/$commit_hash/usr/local"
mkdir -p "$INSTALLDIR/lib" "$INSTALLDIR/include"

# Helper: install headers from common locations
install_headers() {
  echo "Installing headers to $INSTALLDIR/include"
  # copy any headers from src/ then root
  if [ -d ./src ]; then
    cp -f ./src/*.h "$INSTALLDIR/include/" 2>/dev/null || true
  fi
  cp -f ./*.h "$INSTALLDIR/include/" 2>/dev/null || true
}

# If canonical mjson.c exists in root or src/, compile it
if [ -f ./mjson.c ]; then
  echo "Found ./mjson.c — compiling"
  "$CC" $CFLAGS -I. -DMJSON_ENABLE_MERGE=1 -DMJSON_ENABLE_PRETTY=1 ./mjson.c -c -o mjson.o
  "$CC" -rdynamic -shared mjson.o -o libmjson.so
  ar rcs libmjson.a mjson.o
  cp -f libmjson.so libmjson.a "$INSTALLDIR/lib/"
  install_headers
  rm -rf "$MJSON_BUILD_PATH/mjson_$commit_hash"
  exit 0
fi

# If src/mjson.c exists, use it
if [ -f ./src/mjson.c ]; then
  echo "Found ./src/mjson.c — compiling"
  "$CC" $CFLAGS -I./src -DMJSON_ENABLE_MERGE=1 -DMJSON_ENABLE_PRETTY=1 ./src/mjson.c -c -o mjson.o
  "$CC" -rdynamic -shared mjson.o -o libmjson.so
  ar rcs libmjson.a mjson.o
  cp -f libmjson.so libmjson.a "$INSTALLDIR/lib/"
  install_headers
  rm -rf "$MJSON_BUILD_PATH/mjson_$commit_hash"
  exit 0
fi

# If prebuilt libs exist in the tree, copy them
if compgen -G "./libmjson.*" >/dev/null 2>&1 || compgen -G "./src/libmjson.*" >/dev/null 2>&1; then
  echo "Found prebuilt libmjson.* — copying to install dir"
  cp -f ./libmjson.* ./src/libmjson.* "$INSTALLDIR/lib/" 2>/dev/null || true
  install_headers
  rm -rf "$MJSON_BUILD_PATH/mjson_$commit_hash"
  exit 0
fi

# Header-only fallback: install headers if present
if compgen -G "./src/*.h" >/dev/null 2>&1 || compgen -G "./*.h" >/dev/null 2>&1; then
  echo "No mjson.c found but headers present — installing headers only"
  install_headers
  rm -rf "$MJSON_BUILD_PATH/mjson_$commit_hash"
  exit 0
fi

echo "Error: could not find mjson.c, prebuilt libs, or headers for mjson in this commit."
ls -la . | sed -n '1,200p'
exit 1

