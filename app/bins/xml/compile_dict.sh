#!/bin/bash
set -euo pipefail

# Usage: compile.sh <libxml2_hash> <expat_hash> <name_suffix>
if [ "$#" -ne 3 ]; then
  echo "Usage: $0 <libxml2_hash> <expat_hash> <name_suffix>"
  exit 1
fi


libxml2_hash=$1
expat_hash=$2
name_suffix=$3

# copy /home/user/workspace/diffray/app/script/libxml2

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" &>/dev/null && pwd)
APP_PATH="${SCRIPT_DIR}/../.."
LIBXML2_DICT_SOURCE_PATH="$APP_PATH/script/libxml2/dict_${libxml2_hash}.txt"
EXPAT_DICT_SOURCE_PATH="$APP_PATH/script/expat/dict_${expat_hash}.txt"

# check if source dict files exist
if [ ! -f "$LIBXML2_DICT_SOURCE_PATH" ]; then
    echo "Error: libxml2 dictionary file '$LIBXML2_DICT_SOURCE_PATH' not found."
    exit 1
fi
if [ ! -f "$EXPAT_DICT_SOURCE_PATH" ]; then
    echo "Error: expat dictionary file '$EXPAT_DICT_SOURCE_PATH' not found."
    exit 1
fi

# combine dicts
cat "$LIBXML2_DICT_SOURCE_PATH" "$EXPAT_DICT_SOURCE_PATH" > "$SCRIPT_DIR/dict_${libxml2_hash}_${expat_hash}.txt"
echo "Combined dictionary created at $SCRIPT_DIR/dict_${libxml2_hash}_${expat_hash}.txt"


echo "Copy combined dictionary to bins/.bin/$name_suffix/"
mkdir -p "$APP_PATH/bins/.bin/$name_suffix"
cp "$SCRIPT_DIR/dict_${libxml2_hash}_${expat_hash}.txt" "$APP_PATH/bins/.bin/$name_suffix/dict.txt"

# cleanup
rm "$SCRIPT_DIR/dict_${libxml2_hash}_${expat_hash}.txt"


