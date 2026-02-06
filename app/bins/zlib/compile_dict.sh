#!/bin/bash
set -euo pipefail

# Usage: compile_dict.sh <hash1> <hash2> <name_suffix>
if [ "$#" -ne 3 ]; then
  echo "Usage: $0 <hash1> <hash2> <name_suffix>"
  exit 1
fi

hash1=$1
hash2=$2
name_suffix=$3

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" &>/dev/null && pwd)
APP_PATH="${SCRIPT_DIR}/../.."
DICT_SOURCE_PATH1="$APP_PATH/script/zlib/dict_${hash1}.txt"
DICT_SOURCE_PATH2="$APP_PATH/script/libdeflate/dict_${hash2}.txt"

# check if source dict files exist
if [ ! -f "$DICT_SOURCE_PATH1" ]; then
    echo "Error: dictionary file '$DICT_SOURCE_PATH1' not found."
    exit 1
fi
if [ ! -f "$DICT_SOURCE_PATH2" ]; then
    echo "Error: dictionary file '$DICT_SOURCE_PATH2' not found."
    exit 1
fi

# combine dicts
cat "$DICT_SOURCE_PATH1" "$DICT_SOURCE_PATH2" > "$SCRIPT_DIR/dict_${hash1}_${hash2}.txt"
echo "Combined dictionary created at $SCRIPT_DIR/dict_${hash1}_${hash2}.txt"

echo "Copy combined dictionary to bins/.bin/$name_suffix/"
mkdir -p "$APP_PATH/bins/.bin/$name_suffix"
cp "$SCRIPT_DIR/dict_${hash1}_${hash2}.txt" "$APP_PATH/bins/.bin/$name_suffix/dict.txt"

# cleanup
rm "$SCRIPT_DIR/dict_${hash1}_${hash2}.txt"
