fn main() {
    println!("cargo::rustc-check-cfg=cfg(dt, values(\"chosen::bbq_pwm_leds\"))");
    println!("cargo::rustc-check-cfg=cfg(dt, values(\"chosen::bbq_led_strip\"))");
    println!("cargo::rustc-check-cfg=cfg(dt, values(\"chosen::bbq_mono_leds\"))");
    println!("cargo::rustc-check-cfg=cfg(dt, values(\"aliases::pwm_led0\"))");
    println!("cargo::rustc-check-cfg=cfg(dt, values(\"aliases::pwm_led1\"))");
    println!("cargo::rustc-check-cfg=cfg(dt, values(\"aliases::pwm_led2\"))");
    println!("cargo::rustc-check-cfg=cfg(dt, values(\"labels::gpio1\"))");

    // This call will make make config entries available in the code for every device tree node, to
    // allow conditional compilation based on whether it is present in the device tree.
    // For example, it will be possible to have:
    // ```rust
    // #[cfg(dt = "aliases::led0")]
    // ```
    zephyr_build::dt_cfgs();

    // And a `cfg` for every boolean Kconfig option that is on, such as `#[cfg(CONFIG_BT)]` for the
    // Bluetooth code, which only the boards with a radio build.  Each one tested has to be named
    // here, as on the boards without it the cfg does not exist at all.
    println!("cargo::rustc-check-cfg=cfg(CONFIG_BT)");
    println!("cargo::rustc-check-cfg=cfg(CONFIG_POWEROFF)");
    zephyr_build::export_kconfig_bool_options();
}
