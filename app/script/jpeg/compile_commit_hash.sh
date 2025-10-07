#!/bin/sh
# jpeg-turbo new:9f
# jpeg-turbo old:9c

#actually,it is not commit_hash, it is version number
#!/bin/sh

commit_hash=$1



if [ -z "$commit_hash" ]; then
	echo "Usage: $0 <commit_hash>"
	exit 1
fi

# check if commit_hash is within allowed set
if ! echo "$commit_hash" | grep -qE '^(dep|8c|9f)$'; then
    echo "Error: commit_hash must be one of 'dep', '8c', or '9f'."
    exit 1
fi 

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" &>/dev/null && pwd)

APP_PATH="${SCRIPT_DIR}/../.."
JPEG_BUILD_PATH=$APP_PATH/build/jpeg
JPEG_SRC_PATH=$APP_PATH/src/jpeg
JPEG_INSTALL_PATH=$APP_PATH/build/jpeg
JPEG_LIB_PATH=$APP_PATH/libs/build_jpeg_libs/

mkdir -p $JPEG_BUILD_PATH/
rm -rf $JPEG_BUILD_PATH/jpeg_$commit_hash && rm -rf $JPEG_INSTALL_PATH/$commit_hash
if [ "$commit_hash" = "dep" ]; then
	cp -r $JPEG_SRC_PATH/jpeg-8c $JPEG_BUILD_PATH/jpeg_$commit_hash
else
	cp -r $JPEG_SRC_PATH/jpeg-$commit_hash $JPEG_BUILD_PATH/jpeg_$commit_hash
fi
cd $JPEG_BUILD_PATH/jpeg_$commit_hash

CC="${SCRIPT_DIR}/../../compiler/target/release/libafl_cc"
CXX="${SCRIPT_DIR}/../../compiler/target/release/libafl_cxx"
CFLAGS="-fsanitize=address"
CXXFLAGS="-fsanitize=address"

./configure CC="$CC" CXX="$CXX" CFLAGS="$CFLAGS" CXXFLAGS="$CXXFLAGS"

make -j 16


cd $JPEG_INSTALL_PATH && mkdir -p $commit_hash
cd $JPEG_INSTALL_PATH/jpeg_$commit_hash
make install DESTDIR=$JPEG_INSTALL_PATH/$commit_hash
rm -rf $JPEG_BUILD_PATH/jpeg_$commit_hash

# cd $JPEG_LIB_PATH
# make genlib_thread COMMIT_HASH=$commit_hash
# make genlib_docker COMMIT_HASH=$commit_hash
# make test_thread
