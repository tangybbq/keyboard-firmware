#! /bin/bash

release='--release'
release=''

# Invoke and filter out the file information so that the output is more readable.
cargo run $release |
    grep -v '^└─'
