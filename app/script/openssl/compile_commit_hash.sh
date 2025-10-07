#!/usr/bin/env bash
# openssl new:78d252889b
# openssl old:89cd17a  挑选的时候这个是2021的3.0.0,这个时候应该还有一个CVE-2022（和X509 CRL有关的）
set -e
commit_hash=$1


if [ -z "$commit_hash" ]; then
	echo "Usage: $0 <commit_hash>"
	exit 1
fi

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" &>/dev/null && pwd)

APP_PATH="${SCRIPT_DIR}/../.."
OPENSSL_BUILD_PATH=$APP_PATH/build/openssl
OPENSSL_SRC_PATH=$APP_PATH/src/openssl
OPENSSL_INSTALL_PATH=$APP_PATH/build/openssl

mkdir -p $OPENSSL_BUILD_PATH/
rm -rf $OPENSSL_BUILD_PATH/openssl_$commit_hash && rm -rf $OPENSSL_INSTALL_PATH/$commit_hash
cp -r $OPENSSL_SRC_PATH $OPENSSL_BUILD_PATH/openssl_$commit_hash
cd $OPENSSL_BUILD_PATH/openssl_$commit_hash
git reset --hard $commit_hash

CC="${SCRIPT_DIR}/../../compiler/target/release/libafl_cc"
CXX="${SCRIPT_DIR}/../../compiler/target/release/libafl_cxx"

CFLAGS="-fsanitize=address"
CXXFLAGS="-fsanitize=address"

# OpenSSL's Configure can be picky about how flags are passed. Export
# the environment variables first, then invoke ./Configure with CC/CXX
export CC CXX CFLAGS CXXFLAGS

./Configure CC="$CC" CXX="$CXX" CFLAGS="$CFLAGS" CXXFLAGS="$CXXFLAGS"

make -j 16


cd $OPENSSL_INSTALL_PATH && mkdir -p $commit_hash
cd $OPENSSL_INSTALL_PATH/openssl_$commit_hash
make install DESTDIR=$OPENSSL_INSTALL_PATH/$commit_hash
rm -rf $OPENSSL_BUILD_PATH/openssl_$commit_hash

# cd $OPENSSL_LIB_PATH
# make genlib_thread COMMIT_HASH=$commit_hash
# make genlib_docker COMMIT_HASH=$commit_hash
# make test_thread
