#!/bin/sh
set -eu

input=$1
output=$2
mkdir -p "$output"
libreoffice --headless --convert-to pdf --outdir "$output" "$input"
test -s "$output/$(basename "${input%.*}").pdf"
