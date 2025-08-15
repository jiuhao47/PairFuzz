#!/bin/bash
# files=(*)
# for filename in ${files[@]}
rm -f 1-verify_log.txt
# this is used to test whether fuzz hareness is OK
corpus_path=~/tmp/png_init_corpus
corpus_path=./tmp
#  &>> 1-verify_log.txt
count=1
for fileName in $(ls $corpus_path/*.png); do
    echo $fileName &>> 1-verify_log.txt
    ./test $fileName &>> 1-verify_log.txt
    count=$((count + 1))
    echo $count
    # if [ $count -eq 5 ]; then
    #     echo "eq"
    #     break
    # fi
    # break
done