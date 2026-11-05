cmake_minimum_required(VERSION 3.20.0)

find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(keyboard_firmware)

rust_cargo_application()

target_include_directories(app PRIVATE src)
target_sources(app PRIVATE src/usb.c src/usbd_support.c src/platform.c src/minder_usb.c)
target_sources_ifdef(CONFIG_BT app PRIVATE src/ble.c src/minder_ble.c)
target_sources_ifdef(CONFIG_FUEL_GAUGE app PRIVATE src/battery.c)
target_sources_ifdef(CONFIG_POWEROFF app PRIVATE src/sleep.c)
target_sources_ifdef(CONFIG_NORDIC_QSPI_NOR app PRIVATE src/qspi_off.c)
