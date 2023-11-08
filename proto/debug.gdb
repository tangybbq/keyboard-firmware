# Debugging

set history save on
set confirm off

# Path fixing for rustc.  Find hash using `rustc -Vv`
set substitute-path /rustc/e5cfc55477eceed1317a02189fdf77a4a98f2124 \
    /home/davidb/.rustup/toolchains/nightly-2023-10-29-x86_64-unknown-linux-gnu/lib/rustlib/src/rust

# target extended-remote :1337
target extended-remote :2331
