set history save on
set confirm no

target extended-remote :2331

# Instead of automatically loading the app, let's make 'z' do this for us
define z
  load
  monitor reset
  maintenance flush register-cache
end
