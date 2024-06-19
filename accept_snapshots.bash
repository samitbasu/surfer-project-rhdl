#!/bin/bash

while read -r file; do
    target_filename="$(sed -e "s|\(.*\).new.png|\1.png|g" <<< "$file")"
    diff_filename="$(sed -e "s|\(.*\).new.png|\1.diff.png|g" <<< "$file")"
    mv "$file" "$target_filename"
    rm -f "$diff_filename"
done <<< "$(find "snapshots" -name "*.new.png")"
