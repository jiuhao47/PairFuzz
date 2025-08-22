#!/bin/bash
# files=(*)
# for filename in ${files[@]}
rm -f 1-verify_log.txt
# this is used to test whether fuzz hareness is OK
corpus_path=/home/fdl/diff-fuzz/examples/src/apps/6-bmpON/results/epoch_1/1-Jan_10_imGM_JointOutput_24hours/diffs
#  &>> 1-verify_log.txt
for fileName in $(ls $corpus_path/diff_*); do
    echo $fileName &>> 1-verify_log.txt
    ./test $fileName &>> 1-verify_log.txt
done