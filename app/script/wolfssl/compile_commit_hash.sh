#!/usr/bin/env bash
# wolfssl new:8f131ff3d
# wolfssl old:7e01af0
set -e
commit_hash=$1


if [ -z "$commit_hash" ]; then
	echo "Usage: $0 <commit_hash>"
	exit 1
fi

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" &>/dev/null && pwd)

APP_PATH="${SCRIPT_DIR}/../.."
WOLFSSL_BUILD_PATH=$APP_PATH/build/wolfssl
WOLFSSL_SRC_PATH=$APP_PATH/src/wolfssl
WOLFSSL_INSTALL_PATH=$APP_PATH/build/wolfssl

mkdir -p $WOLFSSL_BUILD_PATH/
rm -rf $WOLFSSL_BUILD_PATH/WOLFSSL_$commit_hash && rm -rf $WOLFSSL_INSTALL_PATH/$commit_hash
cp -r $WOLFSSL_SRC_PATH $WOLFSSL_BUILD_PATH/WOLFSSL_$commit_hash
cd $WOLFSSL_BUILD_PATH/WOLFSSL_$commit_hash
git reset --hard $commit_hash

CC="${SCRIPT_DIR}/../../compiler/target/release/libafl_cc"
CXX="${SCRIPT_DIR}/../../compiler/target/release/libafl_cxx"

CFLAGS="-fsanitize=address"
CXXFLAGS="-fsanitize=address"

# WOLFSSL's Configure can be picky about how flags are passed. Export
# the environment variables first, then invoke ./Configure with CC/CXX
export CC CXX CFLAGS CXXFLAGS
./autogen.sh
./configure --enable-opensslextra CXX="$CXX" CC="$CC" CFLAGS="$CFLAGS" CXXFLAGS="$CXXFLAGS"

make -j 16


cd $WOLFSSL_INSTALL_PATH && mkdir -p $commit_hash
cd $WOLFSSL_INSTALL_PATH/WOLFSSL_$commit_hash
make install DESTDIR=$WOLFSSL_INSTALL_PATH/$commit_hash
rm -rf $WOLFSSL_BUILD_PATH/WOLFSSL_$commit_hash

# cd $OPENSSL_LIB_PATH
# make genlib_thread COMMIT_HASH=$commit_hash
# make genlib_docker COMMIT_HASH=$commit_hash
# make test_thread
