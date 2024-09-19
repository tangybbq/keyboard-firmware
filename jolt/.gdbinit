set history save on
set confirm no

set substitute-path /rustc/129f3b9964af4d4a709d1383930ade12dfe7c081 \
	/Users/davidb/.rustup/toolchains/stable-aarch64-apple-darwin/lib/rustlib/src/rust

target extended-remote :2331

define z
  load
  monitor reset
  maintenance flush register-cache
end
