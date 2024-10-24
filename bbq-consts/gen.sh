#! /bin/sh

cargo run > consts.rs
mv consts.rs ../bbq-steno/src/dict/emily/consts.rs
