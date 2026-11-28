#! /bin/bash

PROG="$1"
ROOTDIR=$(git rev-parse --show-toplevel)

cat $ROOTDIR/vendor/progit/*/*/*.markdown | "$PROG" > /dev/null
