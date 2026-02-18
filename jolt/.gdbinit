set history save on
set confirm no
set pagination off

target extended-remote :2331

define hook
  monitor exec SetRTTAddr 0x20000410
end

hook

b rust_main

define z
  load
  monitor reset
  maintenance flush register-cache
end
