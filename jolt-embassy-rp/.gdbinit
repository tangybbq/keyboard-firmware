set history save on
set confirm no

set substitute-path /rustc/129f3b9964af4d4a709d1383930ade12dfe7c081 \
	/Users/davidb/.rustup/toolchains/stable-aarch64-apple-darwin/lib/rustlib/src/rust

# target extended-remote :1337
target extended-remote :2331
# target remote :3333

# Not sure how stable this is.
monitor exec SetRTTAddr 0x2000015c

b main

define z
  load
  monitor reset
  maintenance flush register-cache
end
